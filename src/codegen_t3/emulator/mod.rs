// emulator/mod.rs — runtime emulator for T3ISA.
// Split into: profiler.rs (ExecProfile + Task), debugger.rs (debugger + run methods), tests.rs.

pub mod profiler;
pub use profiler::ExecProfile;
pub(crate) use profiler::{Task, DebugAction};

pub mod debugger;
pub use debugger::{run_emulator, run_emulator_debug, run_emulator_profiled};

mod execute;
mod syscalls;
mod syscall_io;
mod syscall_fs;
mod syscall_proc;

#[cfg(test)]
mod tests;

use super::isa::*;
use std::collections::{HashMap, HashSet};
use std::io::{Write as IoWrite, BufRead};
use std::net::{TcpListener, TcpStream};


// ---------------------------------------------------------------------------
// HeapObj — runtime collection objects
// ---------------------------------------------------------------------------

pub(super) enum HeapObj {
    Vec(Vec<i64>),
    Map(std::collections::BTreeMap<i64, i64>),
    Set(std::collections::BTreeSet<i64>),
    Deque(std::collections::VecDeque<i64>),
    Channel(std::collections::VecDeque<i64>),
    ClosedChannel(std::collections::VecDeque<i64>),
    /// TernaryTrie: keys are Vec<i64> (sequence of trit values), values are i64
    Trie(std::collections::BTreeMap<Vec<i64>, i64>),
    /// Mutex: holds a single value
    Mutex(i64),
    /// AtomicTrit: holds a trit value (-1, 0, or 1)
    AtomicTrit(i64),
    /// Barrier: (needed, arrived_count)
    Barrier(i64, i64),
    /// Semaphore: permit count
    Semaphore(#[allow(dead_code)] i64),
    /// Task result: stores a completed future's return value
    TaskResult(i64),
}

pub struct Emulator {
    pub regs: [i64; 27],
    pub pc: usize,
    pub memory: Vec<i64>,
    pub flags: i8,
    pub halted: bool,
    pub output: Vec<String>,
    pub string_data: HashMap<usize, String>,
    pub float_data: HashMap<usize, i64>,
    pub input_queue: std::collections::VecDeque<i64>,
    call_stack: Vec<usize>,
    /// Bump pointer for runtime heap allocations (strings, arrays).
    /// Starts at 64_000, grows upward (well above the string-literal region at 63_000).
    heap_ptr: usize,
    /// Heap objects (Vecs, Maps, Sets, Deques, Channels) keyed by handle.
    heap_objs: HashMap<usize, HeapObj>,
    /// Open file handles.
    files: HashMap<usize, std::fs::File>,
    /// Next file descriptor counter (starts at 3 to avoid stdin/stdout/stderr).
    next_fd: usize,
    /// TCP streams keyed by fd.
    tcp_streams: HashMap<usize, TcpStream>,
    /// TCP listeners keyed by fd.
    tcp_listeners: HashMap<usize, TcpListener>,
    /// Next network fd counter.
    next_net_fd: usize,
    /// Call depth counter for stack overflow detection.
    call_depth: usize,
    /// Cooperative scheduler tasks.
    #[allow(dead_code)]
    tasks: Vec<Task>,
    /// Currently-executing task index.
    #[allow(dead_code)]
    current_task: usize,
    /// Execution profile (always collected).
    pub profile: ExecProfile,
    /// Interactive debug mode.
    pub debug: bool,
    /// Breakpoint addresses for debug mode.
    pub breakpoints: HashSet<usize>,
}

impl Emulator {
    pub fn new() -> Self {
        let mut regs = [0i64; 27];
        regs[26] = 60_000; // SP
        Emulator {
            regs,
            pc: 0,
            memory: vec![0i64; 65536],
            flags: 0,
            halted: false,
            output: Vec::new(),
            string_data: HashMap::new(),
            float_data: HashMap::new(),
            input_queue: std::collections::VecDeque::new(),
            call_stack: Vec::new(),
            heap_ptr: 64_000,
            heap_objs: HashMap::new(),
            files: HashMap::new(),
            next_fd: 3,
            tcp_streams: HashMap::new(),
            tcp_listeners: HashMap::new(),
            next_net_fd: 100,
            call_depth: 0,
            tasks: Vec::new(),
            current_task: 0,
            profile: ExecProfile::new(),
            debug: false,
            breakpoints: HashSet::new(),
        }
    }

    /// Allocate a new heap object and return its handle.
    fn heap_alloc_obj(&mut self, obj: HeapObj) -> usize {
        let handle = 0x8000_0000usize + self.heap_objs.len();
        self.heap_objs.insert(handle, obj);
        handle
    }

    /// Call a function pointer from within a syscall handler.
    /// Simulates CALL fn_ptr with R1=arg, runs until the function returns, then
    /// restores the original PC. Returns R1 (the callee's return value).
    fn call_fn_ptr(&mut self, fn_ptr: usize, arg: i64) -> i64 {
        let saved_pc = self.pc;
        let depth = self.call_stack.len();
        // Simulate the CALL instruction
        self.call_stack.push(saved_pc);
        self.regs[26] -= 1;
        self.regs[1] = arg;
        self.pc = fn_ptr;
        // Run until the function's RET restores us to saved_pc
        let mut steps = 0;
        while !self.halted && self.call_stack.len() > depth && steps < 1_000_000 {
            self.step();
            steps += 1;
        }
        let ret = self.regs[1];
        // Ensure PC is correctly restored even if we timed out
        if self.call_stack.len() <= depth {
            self.pc = saved_pc;
        }
        ret
    }

    /// Read a raw (non-length-prefixed) trit array from memory starting at ptr, of given length.
    #[allow(dead_code)]
    fn read_raw_trit_array(&self, ptr: usize, len: usize) -> Vec<i64> {
        (0..len).map(|i| self.memory.get(ptr + i).copied().unwrap_or(0)).collect()
    }

    /// Read a TernaryTrie key from a register value:
    /// - If the value is >= 0x8000_0000, treat it as a Vec handle and return its elements.
    /// - Otherwise, treat it as a length-prefixed memory array pointer.
    fn read_trie_key(&self, val: i64) -> Vec<i64> {
        let v = val as usize;
        if v >= 0x8000_0000 {
            if let Some(HeapObj::Vec(elems)) = self.heap_objs.get(&v) {
                return elems.clone();
            }
            return vec![];
        }
        let klen = self.memory.get(v).copied().unwrap_or(0) as usize;
        if klen > 256 { return vec![]; } // sanity check
        (0..klen).map(|i| self.memory.get(v + 1 + i).copied().unwrap_or(0)).collect()
    }

    /// Wrap a Vec<i64> key as a Vec heap object and return its handle.
    #[allow(dead_code)]
    fn key_to_vec_handle(&mut self, key: Vec<i64>) -> usize {
        self.heap_alloc_obj(HeapObj::Vec(key))
    }

    /// Call a 2-argument function pointer: fn(R1=a, R2=b) -> R1.
    fn call_fn_ptr_2arg(&mut self, fn_ptr: usize, a: i64, b: i64) -> i64 {
        let saved_pc = self.pc;
        let depth = self.call_stack.len();
        self.call_stack.push(saved_pc);
        self.regs[26] -= 1;
        self.regs[1] = a;
        self.regs[2] = b;
        self.pc = fn_ptr;
        let mut steps = 0;
        while !self.halted && self.call_stack.len() > depth && steps < 1_000_000 {
            self.step();
            steps += 1;
        }
        let ret = self.regs[1];
        if self.call_stack.len() <= depth {
            self.pc = saved_pc;
        }
        ret
    }

    /// Read a length-prefixed string from memory: memory[ptr]=len, memory[ptr+1..ptr+1+len]=chars.
    fn read_lp_string(&self, ptr: usize) -> String {
        let len = self.memory.get(ptr).copied().unwrap_or(0) as usize;
        let mut s = String::new();
        for i in 0..len {
            let ch = self.memory.get(ptr + 1 + i).copied().unwrap_or(0);
            if let Some(c) = char::from_u32(ch as u32) {
                s.push(c);
            }
        }
        s
    }

    /// Write a length-prefixed string into the heap and return its address.
    #[allow(dead_code)]
    fn write_lp_string(&mut self, s: &str) -> usize {
        let chars: Vec<i64> = s.chars().map(|c| c as i64).collect();
        let len = chars.len();
        let addr = self.heap_ptr;
        if addr < self.memory.len() {
            self.memory[addr] = len as i64;
        }
        for (i, &c) in chars.iter().enumerate() {
            let a = addr + 1 + i;
            if a < self.memory.len() {
                self.memory[a] = c;
            }
        }
        self.heap_ptr += len + 2; // len word + chars + null terminator
        addr
    }

    /// Get a string from R1 — try string_data first, then lp-string in memory.
    fn get_string_r1(&self) -> String {
        let addr = self.regs[1] as usize;
        if let Some(s) = self.string_data.get(&addr) {
            return s.clone();
        }
        self.read_lp_string(addr)
    }

    /// Allocate a runtime string, store it in `string_data`, and return its address.
    fn heap_alloc_str(&mut self, s: String) -> usize {
        let addr = self.heap_ptr;
        self.string_data.insert(addr, s);
        self.heap_ptr += 1;
        addr
    }

    /// Allocate `len` words of memory at the heap, write `values`, and return the address.
    fn heap_alloc_array(&mut self, values: &[i64]) -> usize {
        let addr = self.heap_ptr;
        for (i, &v) in values.iter().enumerate() {
            if addr + i < self.memory.len() {
                self.memory[addr + i] = v;
            }
        }
        self.heap_ptr += values.len().max(1);
        addr
    }

    /// Convert a t27 value to a balanced ternary string using "+", "0", "-" (MST first).
    fn t27_to_ternary_str(mut val: i64) -> String {
        if val == 0 { return "0".to_string(); }
        let mut digits = Vec::new();
        while val != 0 {
            let rem = val.rem_euclid(3); // 0, 1, or 2 (always non-negative)
            let d = if rem <= 1 { rem as i64 } else { -1i64 }; // 2 → -1 in balanced
            digits.push(d);
            val = (val - d) / 3;
        }
        digits.iter().rev().map(|&d| if d > 0 { "+" } else if d == 0 { "0" } else { "-" }).collect()
    }

    pub fn load_program(&mut self, words: Vec<i64>) {
        self.profile.program_words = words.len();
        for (i, w) in words.iter().enumerate() {
            if i < self.memory.len() {
                self.memory[i] = *w;
            }
        }
    }

    /// Register string content at sequential high-memory addresses.
    /// Assembler resolves symbolic string labels to those addresses.
    pub fn load_strings(&mut self, strings: Vec<(String, String)>) {
        let base: usize = 63_000;
        for (i, (_label, content)) in strings.iter().enumerate() {
            self.string_data.insert(base + i, content.clone());
        }
    }

}
