// emulator/mod.rs — runtime emulator for T3ISA.
// Split into: profiler.rs (ExecProfile + Task), debugger.rs (debugger + run methods), tests.rs.

pub mod profiler;
pub use profiler::ExecProfile;
pub(crate) use profiler::{Task, DebugAction};

pub mod debugger;
pub use debugger::{run_emulator, run_emulator_debug, run_emulator_debug_argv,
                   run_emulator_profiled, run_emulator_with_exit,
                   run_emulator_with_exit_capped, run_emulator_with_exit_capped_argv,
                   run_emulator_with_exit_capped_argv_profiled,
                   DEFAULT_MAX_STEPS, T3_STEP_LIMIT_EXIT};

mod execute;
mod syscalls;
mod syscall_io;
mod syscall_fs;
mod syscall_proc;
mod ordered;
use ordered::{OrderedMap, OrderedSet};

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
    /// Iteration order is INSERTION order and is part of the language — see ordered.rs.
    Map(OrderedMap),
    Set(OrderedSet),
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

/// The initial stack pointer, and therefore the first address STATIC DATA may
/// not reach.
///
/// The stack grows DOWNWARD from here while code, string literals and float
/// literals grow UPWARD from 0, and **nothing had ever checked that the two do
/// not meet** (report.txt P38). A program of more than 60,000 words simply
/// overwrote its own stack and then executed whatever a `CALL` had pushed —
/// reported as `TRAP: unknown opcode`, which names the symptom and not the
/// cause. The assembler now refuses that layout, and this is what it refuses
/// against; it is `pub` for exactly that reason.
pub const STACK_BASE: usize = 60_000;

/// The first address of the heap, which grows UPWARD from here to the top of
/// memory — so the heap is `memory.len() - HEAP_BASE` words and nothing may be
/// allocated past the end.
///
/// Named for the same reason `STACK_BASE` is: the number was written out three
/// times (the constructor and the profiler's two readouts) and a bound that is
/// spelled rather than named is a bound each caller can get differently.
pub const HEAP_BASE: usize = 63_000;

pub struct Emulator {
    pub regs: [i64; 27],
    pub pc: usize,
    pub memory: Vec<i64>,
    pub flags: i8,
    pub halted: bool,
    /// Set when execution stopped on a TRAP (a runtime fault) rather than a
    /// normal HALT/RET. A5: the process exit status is taken from R1, so
    /// without this a trapped program could still report success.
    pub trapped: bool,
    pub output: Vec<String>,
    pub string_data: HashMap<usize, String>,
    pub float_data: HashMap<usize, i64>,
    pub input_queue: std::collections::VecDeque<i64>,
    call_stack: Vec<usize>,
    /// Bump pointer for runtime heap allocations (structs, strings, arrays).
    ///
    /// The emulator's memory map, which is emulator-defined rather than
    /// architectural — the ISA only fixes the 65,536-word address space:
    ///
    ///   0 ..        code, then string-literal addresses (code_size + 1024 + i)
    ///   .. 60_000   stack, grown DOWNWARD from the initial SP
    ///   61_000      module globals, one word each
    ///   62_000      RESULT_AREA / TUPLE_AREA scratch (see syscall_proc.rs)
    ///   63_000      heap, grown UPWARD to the top of memory
    ///
    /// The heap base moved down from 64_000 when struct allocations became
    /// heap-allocated (syscall #218): 63_000 was the base of a dead
    /// string-literal region and is free, and the extra 1_000 words are worth
    /// having now that every struct costs heap.
    ///
    /// Allocation past the top traps rather than silently dropping writes —
    /// which is what this comment ALREADY CLAIMED while three of the four
    /// allocators did the opposite (report.txt P39). Every one of them now
    /// goes through `heap_reserve`, which is the only place the bound is
    /// spelled.
    heap_ptr: usize,
    /// Heap objects (Vecs, Maps, Sets, Deques, Channels) keyed by handle.
    heap_objs: HashMap<usize, HeapObj>,
    /// Canonical address per string CONTENT, for Map/Set keys: two string
    /// values with equal text must hash/compare as the same key even though
    /// they live at different addresses.
    string_intern: HashMap<String, i64>,
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
    /// Instruction budget for one `run()`. Defaults to
    /// [`debugger::DEFAULT_MAX_STEPS`]; `manitc run-t3 --max-steps N` overrides
    /// it. This is a runaway guard, NOT a correctness limit — see the constant.
    pub max_steps: usize,
    /// The program's command-line arguments, `argv[0]` first, as `env::argc`
    /// and `env::arg` see them (syscalls 552/553).
    ///
    /// A public field set after `new()`, like `debug` and `max_steps`, so that
    /// none of the five `run_emulator*` entry points change signature. Default
    /// is a single empty element rather than an empty vector: every real
    /// process has an `argv[0]`, and a program asking `argc() > 1` must get the
    /// same answer here as it does on LLVM, where `env_argc` counts the binary
    /// path out of /proc/self/cmdline.
    pub argv: Vec<String>,
    /// Set when execution stopped because `max_steps` was reached.
    ///
    /// Kept separate from `trapped` because the two mean opposite things to a
    /// caller: a trap is the program doing something illegal, while this is the
    /// program being *interrupted* while still running legally. Sharing one
    /// flag made a truncated run indistinguishable from a fault, and the
    /// adjudicator downstream scored it as the two backends disagreeing.
    pub step_limited: bool,
}

impl Emulator {
    pub fn new() -> Self {
        let mut regs = [0i64; 27];
        regs[26] = STACK_BASE as i64; // SP
        Emulator {
            regs,
            pc: 0,
            memory: vec![0i64; 65536],
            flags: 0,
            halted: false,
            trapped: false,
            output: Vec::new(),
            string_data: HashMap::new(),
            float_data: HashMap::new(),
            input_queue: std::collections::VecDeque::new(),
            call_stack: Vec::new(),
            heap_ptr: HEAP_BASE,
            heap_objs: HashMap::new(),
            string_intern: HashMap::new(),
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
            max_steps: debugger::DEFAULT_MAX_STEPS,
            argv: vec![String::new()],
            step_limited: false,
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
        let saved_call_depth = self.call_depth;
        // Simulate the CALL instruction (Ret decrements call_depth, so it must
        // be incremented here to stay balanced)
        self.call_stack.push(saved_pc);
        self.call_depth += 1;
        self.regs[26] -= 1;
        self.regs[1] = arg;
        self.pc = fn_ptr;
        // Run until the function's RET restores us to saved_pc, charging the
        // GLOBAL instruction budget (report.txt P33).
        //
        // This is a RE-ENTRANT emulator loop: a syscall handed a maniT function
        // pointer — a `Vec::filter` predicate, a sort comparator, a scheduled
        // task — drives the callee here rather than returning to `run`. It used
        // to count its own iterations against a private 1,000,000, so every
        // instruction executed inside a callback was RECORDED in the profile
        // and CHARGED TO NOTHING: `--max-steps` bounded only the outer loop's
        // iterations. `concurrency` ran 30,299 instructions under a budget of
        // 26,699 and exited 0, and bisecting the budget for a dynamic
        // instruction count therefore under-reported exactly the programs that
        // use callbacks.
        //
        // `profile.total_instructions` is the right counter because it is the
        // one thing that counts every instruction wherever it was executed.
        while !self.halted
            && self.call_stack.len() > depth
            && self.profile.total_instructions < self.max_steps
        {
            self.step();
        }
        let ret = self.regs[1];
        // On a normal return, Ret already popped our frame and set pc=saved_pc.
        // On step-limit timeout the stale frame(s) are still on the stack:
        // drop them and restore the PC so execution resumes at the call site.
        if self.call_stack.len() > depth {
            self.call_stack.truncate(depth);
            self.call_depth = saved_call_depth;
            self.regs[26] = clamp27(self.regs[26] + 1);
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
        let saved_call_depth = self.call_depth;
        self.call_stack.push(saved_pc);
        self.call_depth += 1;
        self.regs[26] -= 1;
        self.regs[1] = a;
        self.regs[2] = b;
        self.pc = fn_ptr;
        // The global budget, exactly as in `call_fn_ptr` above (P33).
        while !self.halted
            && self.call_stack.len() > depth
            && self.profile.total_instructions < self.max_steps
        {
            self.step();
        }
        let ret = self.regs[1];
        // Same timeout recovery as call_fn_ptr: pop stale frames, restore PC.
        if self.call_stack.len() > depth {
            self.call_stack.truncate(depth);
            self.call_depth = saved_call_depth;
            self.regs[26] = clamp27(self.regs[26] + 1);
            self.pc = saved_pc;
        }
        ret
    }

    /// Read a length-prefixed string out of emulated memory, where
    /// `memory[ptr]` is the length and `memory[ptr+1..ptr+1+len]` the characters.
    ///
    /// Returns the empty string when `ptr` does not address a well-formed
    /// lp-string. It never reads past the end of memory and never invents
    /// characters that are not there.
    ///
    /// The previous version took the length word on trust: `unwrap_or(0) as
    /// usize` turned a negative length into ~1.8e19, and the body then pushed
    /// `char::from_u32(0)` for every word past the end of memory because the
    /// per-character read also used `unwrap_or(0)`. A single bad address
    /// therefore produced gigabytes of NUL. It was not theoretical —
    /// `examples/data_structures.mt` emitted **7.7 GB** on one call to
    /// `fmt::align_left`, and because the run still exited 0 it read as a
    /// truncation rather than a fault.
    ///
    /// Two independent guards, because either alone would have prevented that:
    /// the length must be a plausible length, and the read must stay inside
    /// memory that exists.
    fn read_lp_string(&self, ptr: usize) -> String {
        let Some(&raw_len) = self.memory.get(ptr) else {
            return String::new();
        };
        // A negative length is not a short string, it is a bad address.
        if raw_len < 0 {
            return String::new();
        }
        let len = raw_len as usize;
        // The characters must fit in memory that actually exists. If they do
        // not, this is not an lp-string and guessing at a prefix of it would
        // only make the corruption harder to trace.
        if ptr.saturating_add(1).saturating_add(len) > self.memory.len() {
            return String::new();
        }
        let mut s = String::with_capacity(len);
        for i in 0..len {
            // Bounds already proven above; no unwrap_or fabrication.
            let ch = self.memory[ptr + 1 + i];
            if let Some(c) = u32::try_from(ch).ok().and_then(char::from_u32) {
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
        // len word + chars + null terminator.
        let addr = match self.heap_reserve(len + 2) {
            Some(a) => a,
            None => return 0,
        };
        self.memory[addr] = len as i64;
        for (i, &c) in chars.iter().enumerate() {
            self.memory[addr + 1 + i] = c;
        }
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

    /// The text behind an arbitrary `str` value, wherever it came from.
    ///
    /// A `str` inside a collection is type-erased to an i64, so a collection
    /// that has to compare its elements as TEXT — rather than by identity —
    /// has to come back through here for each one.
    pub(crate) fn str_at(&self, addr: i64) -> String {
        let addr = addr as usize;
        if let Some(s) = self.string_data.get(&addr) {
            return s.clone();
        }
        self.read_lp_string(addr)
    }

    /// Reserve `n` words of heap and return the base address, or trap and
    /// return `None` when the heap cannot hold them.
    ///
    /// **Every heap allocation goes through here** (report.txt P39). Syscall
    /// #218 checked the bound from the day struct allocations moved to the
    /// heap; the other three allocators did not, and two of them wrote through
    /// a per-word `if addr + i < self.memory.len()` that SKIPPED the words
    /// which did not fit. A program that exhausted the heap therefore received
    /// a zeroed array and exited 0 — `math::to_balanced_ternary` returning
    /// `[0,0,0]` rather than the digits of its argument — while the emulator's
    /// own comment claimed "Allocation past the top traps".
    ///
    /// It returns `None` HAVING ALREADY TRAPPED, the same shape as `checked`
    /// in `execute.rs`, so a caller cannot forget to report it. On that path
    /// the address handed back is 0 and the emulator is already halted.
    fn heap_reserve(&mut self, n: usize) -> Option<usize> {
        let base = self.heap_ptr;
        if base + n > self.memory.len() {
            self.trap(format!(
                "TRAP: heap exhausted allocating {} word(s) at {} (limit {})",
                n,
                base,
                self.memory.len()
            ));
            return None;
        }
        self.heap_ptr += n;
        Some(base)
    }

    /// Allocate a runtime string, store it in `string_data`, and return its address.
    ///
    /// One word per string whatever its length: the TEXT lives in `string_data`
    /// on the host side, and this reserves only the address that identifies it.
    /// The charge is still a charge — the address space is finite — so it is
    /// bounded like every other allocation.
    fn heap_alloc_str(&mut self, s: String) -> usize {
        let addr = match self.heap_reserve(1) {
            Some(a) => a,
            None => return 0,
        };
        self.string_data.insert(addr, s);
        addr
    }

    /// Canonicalize a Map/Set key: values that are string addresses map to
    /// one canonical address per distinct CONTENT (first address seen), so
    /// key comparison behaves like string comparison. Non-string values
    /// pass through unchanged.
    pub(crate) fn intern_key(&mut self, k: i64) -> i64 {
        if let Some(s) = self.string_data.get(&(k as usize)) {
            let s = s.clone();
            return *self.string_intern.entry(s).or_insert(k);
        }
        k
    }

    /// Allocate `len` words of memory at the heap, write `values`, and return the address.
    ///
    /// The reservation is `max(1)` because a zero-length array still needs a
    /// distinct address; the writes below need no bounds test of their own,
    /// since `heap_reserve` has already established that the whole range
    /// exists. It is the per-word test that used to drop them silently.
    fn heap_alloc_array(&mut self, values: &[i64]) -> usize {
        let addr = match self.heap_reserve(values.len().max(1)) {
            Some(a) => a,
            None => return 0,
        };
        for (i, &v) in values.iter().enumerate() {
            self.memory[addr + i] = v;
        }
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

    // NOTE: a former `load_strings` helper registered strings at base 63_000,
    // which disagreed with the assembler's placement (code_size + 1024).  It
    // was dead code with no callers and has been removed; string data flows
    // in through `string_data` (see run_emulator).
}
