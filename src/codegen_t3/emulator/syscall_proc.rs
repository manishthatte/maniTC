// emulator/syscall_proc.rs — Collections, channels, concurrency, and scheduler syscalls.
// Syscall ranges: 17-59 (Vec/Map/Set/Deque), 70-74 (channels), 80-86 (scheduler + Vec HOF),
//                 87-99 (collections extras + TernaryTrie), 100-107 (Vec bulk ops + Set predicates),
//                 108-131 (concurrency).
use super::*;

impl Emulator {
    pub(super) fn do_syscall_proc(&mut self, num: i64) {
        match num {
            // ----------------------------------------------------------------
            // Vec syscalls (17-26)
            // ----------------------------------------------------------------
            17 => {
                // Vec::new() → R1 = handle
                let h = self.heap_alloc_obj(HeapObj::Vec(Vec::new()));
                self.regs[1] = h as i64;
            }
            18 => {
                // Vec::push(handle=R1, value=R2)
                let h = self.regs[1] as usize;
                let v = self.regs[2];
                if let Some(HeapObj::Vec(vec)) = self.heap_objs.get_mut(&h) {
                    vec.push(v);
                }
            }
            19 => {
                // Vec::pop(handle=R1) → R1 = popped value or 0
                let h = self.regs[1] as usize;
                let val = if let Some(HeapObj::Vec(vec)) = self.heap_objs.get_mut(&h) {
                    vec.pop().unwrap_or(0)
                } else { 0 };
                self.regs[1] = val;
            }
            20 => {
                // Vec::get(handle=R1, index=R2) → R1 = value
                let h = self.regs[1] as usize;
                let i = self.regs[2] as usize;
                let val = if let Some(HeapObj::Vec(vec)) = self.heap_objs.get(&h) {
                    vec.get(i).copied().unwrap_or(0)
                } else { 0 };
                self.regs[1] = val;
            }
            21 => {
                // Vec::set(handle=R1, index=R2, value=R3)
                let h = self.regs[1] as usize;
                let i = self.regs[2] as usize;
                let v = self.regs[3];
                if let Some(HeapObj::Vec(vec)) = self.heap_objs.get_mut(&h) {
                    if i < vec.len() { vec[i] = v; }
                }
            }
            22 => {
                // Vec::len(handle=R1) → R1 = len
                let h = self.regs[1] as usize;
                let len = if let Some(HeapObj::Vec(vec)) = self.heap_objs.get(&h) {
                    vec.len() as i64
                } else { 0 };
                self.regs[1] = len;
            }
            23 => {
                // Vec::is_empty(handle=R1) → R1 = bool
                let h = self.regs[1] as usize;
                let empty = if let Some(HeapObj::Vec(vec)) = self.heap_objs.get(&h) {
                    vec.is_empty()
                } else { true };
                self.regs[1] = if empty { 1 } else { 0 };
            }
            24 => {
                // Vec::clear(handle=R1)
                let h = self.regs[1] as usize;
                if let Some(HeapObj::Vec(vec)) = self.heap_objs.get_mut(&h) {
                    vec.clear();
                }
            }
            25 => {
                // Vec::contains(handle=R1, value=R2) → R1 = bool
                let h = self.regs[1] as usize;
                let v = self.regs[2];
                let found = if let Some(HeapObj::Vec(vec)) = self.heap_objs.get(&h) {
                    vec.contains(&v)
                } else { false };
                self.regs[1] = if found { 1 } else { 0 };
            }
            38 => {
                // Vec::contains_str(handle=R1, needle=R2) → R1 = bool.
                // Compares TEXT. The plain Vec::contains above compares the
                // type-erased i64, which for a str is its address, so it can
                // only ever match a value that came from the same place.
                let h = self.regs[1] as usize;
                let needle = self.str_at(self.regs[2]);
                let found = if let Some(HeapObj::Vec(vec)) = self.heap_objs.get(&h) {
                    vec.iter().any(|&e| self.str_at(e) == needle)
                } else { false };
                self.regs[1] = if found { 1 } else { 0 };
            }
            39 => {
                // Vec::sort_str(handle=R1) — sort by TEXT.
                // Sorting the raw i64 orders addresses, which for string
                // literals is source order, so a "sorted" vector came back in
                // the order it was written. Both backends did it, so they
                // agreed on an answer that was not sorted.
                let h = self.regs[1] as usize;
                if let Some(HeapObj::Vec(vec)) = self.heap_objs.get(&h) {
                    let mut keyed: Vec<(String, i64)> =
                        vec.iter().map(|&e| (self.str_at(e), e)).collect();
                    keyed.sort_by(|a, b| a.0.cmp(&b.0));
                    let sorted: Vec<i64> = keyed.into_iter().map(|(_, e)| e).collect();
                    if let Some(HeapObj::Vec(v)) = self.heap_objs.get_mut(&h) {
                        *v = sorted;
                    }
                }
            }
            45 => {
                // Vec::index_of_str(handle=R1, needle=R2) → R1 = index or -1.
                let h = self.regs[1] as usize;
                let needle = self.str_at(self.regs[2]);
                let idx = if let Some(HeapObj::Vec(vec)) = self.heap_objs.get(&h) {
                    vec.iter().position(|&e| self.str_at(e) == needle)
                        .map(|i| i as i64).unwrap_or(-1)
                } else { -1 };
                self.regs[1] = idx;
            }
            26 => {
                // Vec::remove(handle=R1, index=R2)
                // R1 = the REMOVED ELEMENT (report.txt P59). `Vec<T>::remove`
                // is typed `T`; this discarded what it removed and left R1
                // holding whatever the previous operation had put there, so
                // the returned value tracked the PRECEDING statement and the
                // defect looked data-dependent when it was not.
                let h = self.regs[1] as usize;
                let idx = self.regs[2] as usize;
                let mut removed: i64 = 0;
                if let Some(HeapObj::Vec(vec)) = self.heap_objs.get_mut(&h) {
                    if idx < vec.len() {
                        removed = vec.remove(idx);
                    }
                }
                self.regs[1] = removed;
            }

            // ----------------------------------------------------------------
            // Map syscalls (30-37)
            // ----------------------------------------------------------------
            37 => {
                // Map::values(handle) → Vec handle of values, in the SAME order
                // as Map::keys, so a caller can pair them by index.
                let h = self.regs[1] as usize;
                let vals: Vec<i64> = if let Some(HeapObj::Map(map)) = self.heap_objs.get(&h) {
                    map.values()
                } else { vec![] };
                let nh = self.heap_alloc_obj(HeapObj::Vec(vals));
                self.regs[1] = nh as i64;
            }
            30 => {
                // Map::new() → R1 = handle
                let h = self.heap_alloc_obj(HeapObj::Map(crate::codegen_t3::emulator::OrderedMap::new()));
                self.regs[1] = h as i64;
            }
            31 => {
                // Map::insert(handle=R1, key=R2, value=R3)
                let h = self.regs[1] as usize;
                let k = self.intern_key(self.regs[2]);
                let v = self.regs[3];
                if let Some(HeapObj::Map(map)) = self.heap_objs.get_mut(&h) {
                    map.insert(k, v);
                }
            }
            32 => {
                // Map::get(handle=R1, key=R2) → R1 = value or 0
                let h = self.regs[1] as usize;
                let k = self.intern_key(self.regs[2]);
                let val = if let Some(HeapObj::Map(map)) = self.heap_objs.get(&h) {
                    map.get(&k).copied().unwrap_or(0)
                } else { 0 };
                self.regs[1] = val;
            }
            33 => {
                // Map::contains_key(handle=R1, key=R2) → R1 = bool
                let h = self.regs[1] as usize;
                let k = self.intern_key(self.regs[2]);
                let found = if let Some(HeapObj::Map(map)) = self.heap_objs.get(&h) {
                    map.contains_key(&k)
                } else { false };
                self.regs[1] = if found { 1 } else { 0 };
            }
            34 => {
                // Map::remove(handle=R1, key=R2)
                let h = self.regs[1] as usize;
                let k = self.intern_key(self.regs[2]);
                if let Some(HeapObj::Map(map)) = self.heap_objs.get_mut(&h) {
                    map.remove(&k);
                }
            }
            35 => {
                // Map::len(handle=R1) → R1 = len
                let h = self.regs[1] as usize;
                let len = if let Some(HeapObj::Map(map)) = self.heap_objs.get(&h) {
                    map.len() as i64
                } else { 0 };
                self.regs[1] = len;
            }
            36 => {
                // Map::is_empty(handle=R1) → R1 = bool
                let h = self.regs[1] as usize;
                let empty = if let Some(HeapObj::Map(map)) = self.heap_objs.get(&h) {
                    map.is_empty()
                } else { true };
                self.regs[1] = if empty { 1 } else { 0 };
            }

            // ----------------------------------------------------------------
            // Set syscalls (40-44)
            // ----------------------------------------------------------------
            40 => {
                // Set::new() → R1 = handle
                let h = self.heap_alloc_obj(HeapObj::Set(crate::codegen_t3::emulator::OrderedSet::new()));
                self.regs[1] = h as i64;
            }
            41 => {
                // Set::insert(handle=R1, value=R2)
                let h = self.regs[1] as usize;
                let v = self.intern_key(self.regs[2]);
                if let Some(HeapObj::Set(set)) = self.heap_objs.get_mut(&h) {
                    set.insert(v);
                }
            }
            42 => {
                // Set::contains(handle=R1, value=R2) → R1 = bool
                let h = self.regs[1] as usize;
                let v = self.intern_key(self.regs[2]);
                let found = if let Some(HeapObj::Set(set)) = self.heap_objs.get(&h) {
                    set.contains(&v)
                } else { false };
                self.regs[1] = if found { 1 } else { 0 };
            }
            43 => {
                // Set::remove(handle=R1, value=R2)
                let h = self.regs[1] as usize;
                let v = self.intern_key(self.regs[2]);
                if let Some(HeapObj::Set(set)) = self.heap_objs.get_mut(&h) {
                    set.remove(&v);
                }
            }
            44 => {
                // Set::len(handle=R1) → R1 = len
                let h = self.regs[1] as usize;
                let len = if let Some(HeapObj::Set(set)) = self.heap_objs.get(&h) {
                    set.len() as i64
                } else { 0 };
                self.regs[1] = len;
            }

            // ----------------------------------------------------------------
            // Deque syscalls (50-59)
            // ----------------------------------------------------------------
            50 => {
                // Deque::new() → R1 = handle
                let h = self.heap_alloc_obj(HeapObj::Deque(std::collections::VecDeque::new()));
                self.regs[1] = h as i64;
            }
            51 => {
                // Deque::push_front(handle=R1, value=R2)
                let h = self.regs[1] as usize;
                let v = self.regs[2];
                if let Some(HeapObj::Deque(dq)) = self.heap_objs.get_mut(&h) {
                    dq.push_front(v);
                }
            }
            52 => {
                // Deque::push_back(handle=R1, value=R2)
                let h = self.regs[1] as usize;
                let v = self.regs[2];
                if let Some(HeapObj::Deque(dq)) = self.heap_objs.get_mut(&h) {
                    dq.push_back(v);
                }
            }
            53 => {
                // Deque::pop_front(handle=R1) → R1 = value or 0
                let h = self.regs[1] as usize;
                let val = if let Some(HeapObj::Deque(dq)) = self.heap_objs.get_mut(&h) {
                    dq.pop_front().unwrap_or(0)
                } else { 0 };
                self.regs[1] = val;
            }
            54 => {
                // Deque::pop_back(handle=R1) → R1 = value or 0
                let h = self.regs[1] as usize;
                let val = if let Some(HeapObj::Deque(dq)) = self.heap_objs.get_mut(&h) {
                    dq.pop_back().unwrap_or(0)
                } else { 0 };
                self.regs[1] = val;
            }
            55 => {
                // Deque::len(handle=R1) → R1 = len
                let h = self.regs[1] as usize;
                let len = if let Some(HeapObj::Deque(dq)) = self.heap_objs.get(&h) {
                    dq.len() as i64
                } else { 0 };
                self.regs[1] = len;
            }
            56 => {
                // Deque::front(handle=R1) → R1 = front value or 0
                let h = self.regs[1] as usize;
                let val = if let Some(HeapObj::Deque(dq)) = self.heap_objs.get(&h) {
                    dq.front().copied().unwrap_or(0)
                } else { 0 };
                self.regs[1] = val;
            }
            57 => {
                // Deque::back(handle=R1) → R1 = back value or 0
                let h = self.regs[1] as usize;
                let val = if let Some(HeapObj::Deque(dq)) = self.heap_objs.get(&h) {
                    dq.back().copied().unwrap_or(0)
                } else { 0 };
                self.regs[1] = val;
            }
            58 => {
                // Deque::is_empty(handle=R1) → R1 = bool
                let h = self.regs[1] as usize;
                self.regs[1] = if let Some(HeapObj::Deque(dq)) = self.heap_objs.get(&h) {
                    if dq.is_empty() { 1 } else { 0 }
                } else { 1 };
            }
            59 => {
                // Deque::contains(handle=R1, value=R2) → R1 = bool
                let h = self.regs[1] as usize;
                let val = self.regs[2];
                self.regs[1] = if let Some(HeapObj::Deque(dq)) = self.heap_objs.get(&h) {
                    if dq.contains(&val) { 1 } else { 0 }
                } else { 0 };
            }

            // ----------------------------------------------------------------
            // Channel syscalls (70-74)
            // ----------------------------------------------------------------
            70 => {
                // channel_new() → R1 = handle
                let h = self.heap_alloc_obj(HeapObj::Channel(std::collections::VecDeque::new()));
                self.regs[1] = h as i64;
            }
            71 => {
                // channel_send(handle=R1, value=R2)
                let h = self.regs[1] as usize;
                let v = self.regs[2];
                if let Some(HeapObj::Channel(ch)) = self.heap_objs.get_mut(&h) {
                    ch.push_back(v);
                }
            }
            72 => {
                // channel_recv(handle=R1) → R1 = value, or a TRAP on an open
                // empty channel.
                //
                // P81: this returned 0 and carried on, while LLVM blocked
                // forever on a condition variable nothing could signal. Under
                // the current contract `spawn { B }` runs B in place, so an
                // OPEN empty channel has no possible sender and the receive
                // cannot be satisfied. Both backends now say so. A CLOSED empty
                // channel still yields 0 — that is the drain case, and it is
                // what `examples/concurrency.mt` relies on.
                let h = self.regs[1] as usize;
                let val = match self.heap_objs.get_mut(&h) {
                    Some(HeapObj::Channel(ch)) => match ch.pop_front() {
                        Some(v) => v,
                        None => {
                            self.trap(
                                "TRAP: recv on an empty channel that is still \
                                 open: nothing can send to it, because `spawn` \
                                 runs its block in place",
                            );
                            return;
                        }
                    },
                    Some(HeapObj::ClosedChannel(ch)) => ch.pop_front().unwrap_or(0),
                    _ => 0,
                };
                self.regs[1] = val;
            }
            73 => {
                // channel_len(handle=R1) → R1 = len
                let h = self.regs[1] as usize;
                let len = match self.heap_objs.get(&h) {
                    Some(HeapObj::Channel(ch)) => ch.len() as i64,
                    Some(HeapObj::ClosedChannel(ch)) => ch.len() as i64,
                    _ => 0,
                };
                self.regs[1] = len;
            }
            74 => {
                // channel_close(handle=R1) — mark closed
                let h = self.regs[1] as usize;
                if let Some(HeapObj::Channel(q)) = self.heap_objs.remove(&h) {
                    self.heap_objs.insert(h, HeapObj::ClosedChannel(q));
                }
            }

            // ----------------------------------------------------------------
            // Cooperative scheduler syscalls (80-82) — no-op stubs
            // ----------------------------------------------------------------
            80 => {
                // spawn(fn_ptr=R1, arg=R2) — no-op stub
                self.regs[1] = 0;
            }
            81 => {
                // yield() — no-op stub
            }
            82 => {
                // task_exit() — no-op stub
            }

            // ----------------------------------------------------------------
            // Vec higher-order functions (83-86)
            // ----------------------------------------------------------------
            83 => {
                // Vec::for_each(handle=R1, fn_ptr=R2) — call fn(elem) for each element
                let h = self.regs[1] as usize;
                let fp = self.regs[2] as usize;
                let elems: Vec<i64> = if let Some(HeapObj::Vec(v)) = self.heap_objs.get(&h) {
                    v.clone()
                } else { vec![] };
                for elem in elems {
                    self.call_fn_ptr(fp, elem);
                }
            }
            84 => {
                // Vec::map(handle=R1, fn_ptr=R2) → new Vec handle in R1
                let h = self.regs[1] as usize;
                let fp = self.regs[2] as usize;
                let elems: Vec<i64> = if let Some(HeapObj::Vec(v)) = self.heap_objs.get(&h) {
                    v.clone()
                } else { vec![] };
                let mapped: Vec<i64> = elems.iter().map(|&e| self.call_fn_ptr(fp, e)).collect();
                let nh = self.heap_alloc_obj(HeapObj::Vec(mapped));
                self.regs[1] = nh as i64;
            }
            85 => {
                // Vec::filter(handle=R1, fn_ptr=R2) → new Vec handle in R1
                let h = self.regs[1] as usize;
                let fp = self.regs[2] as usize;
                let elems: Vec<i64> = if let Some(HeapObj::Vec(v)) = self.heap_objs.get(&h) {
                    v.clone()
                } else { vec![] };
                let filtered: Vec<i64> = elems.iter().filter(|&&e| self.call_fn_ptr(fp, e) != 0).copied().collect();
                let nh = self.heap_alloc_obj(HeapObj::Vec(filtered));
                self.regs[1] = nh as i64;
            }
            86 => {
                // Vec::slice(handle=R1, start=R2, end=R3) → new Vec handle in R1
                let h = self.regs[1] as usize;
                let start = self.regs[2] as usize;
                let end = self.regs[3] as usize;
                let sliced: Vec<i64> = if let Some(HeapObj::Vec(v)) = self.heap_objs.get(&h) {
                    let end = end.min(v.len());
                    let start = start.min(end);
                    v[start..end].to_vec()
                } else { vec![] };
                let nh = self.heap_alloc_obj(HeapObj::Vec(sliced));
                self.regs[1] = nh as i64;
            }

            // ----------------------------------------------------------------
            // Map extras (87-88)
            // ----------------------------------------------------------------
            87 => {
                // Map::get_or(handle, key, default) → value
                let h = self.regs[1] as usize;
                let key = self.intern_key(self.regs[2]);
                let default = self.regs[3];
                self.regs[1] = if let Some(HeapObj::Map(map)) = self.heap_objs.get(&h) {
                    map.get(&key).copied().unwrap_or(default)
                } else { default };
            }
            88 => {
                // Map::keys(handle) → Vec handle of keys
                let h = self.regs[1] as usize;
                let keys: Vec<i64> = if let Some(HeapObj::Map(map)) = self.heap_objs.get(&h) {
                    map.keys()
                } else { vec![] };
                let nh = self.heap_alloc_obj(HeapObj::Vec(keys));
                self.regs[1] = nh as i64;
            }

            // ----------------------------------------------------------------
            // Set algebra + for_each (89-92)
            // ----------------------------------------------------------------
            89 => {
                // Set::intersection(h1, h2) → new Set handle
                let h1 = self.regs[1] as usize;
                let h2 = self.regs[2] as usize;
                let a = if let Some(HeapObj::Set(s)) = self.heap_objs.get(&h1) { s.clone() } else { Default::default() };
                let b = if let Some(HeapObj::Set(s)) = self.heap_objs.get(&h2) { s.clone() } else { Default::default() };
                let result = a.intersection(&b);
                let nh = self.heap_alloc_obj(HeapObj::Set(result));
                self.regs[1] = nh as i64;
            }
            90 => {
                // Set::union(h1, h2) → new Set handle
                let h1 = self.regs[1] as usize;
                let h2 = self.regs[2] as usize;
                let a = if let Some(HeapObj::Set(s)) = self.heap_objs.get(&h1) { s.clone() } else { Default::default() };
                let b = if let Some(HeapObj::Set(s)) = self.heap_objs.get(&h2) { s.clone() } else { Default::default() };
                let result = a.union(&b);
                let nh = self.heap_alloc_obj(HeapObj::Set(result));
                self.regs[1] = nh as i64;
            }
            91 => {
                // Set::difference(h1, h2) → new Set handle
                let h1 = self.regs[1] as usize;
                let h2 = self.regs[2] as usize;
                let a = if let Some(HeapObj::Set(s)) = self.heap_objs.get(&h1) { s.clone() } else { Default::default() };
                let b = if let Some(HeapObj::Set(s)) = self.heap_objs.get(&h2) { s.clone() } else { Default::default() };
                let result = a.difference(&b);
                let nh = self.heap_alloc_obj(HeapObj::Set(result));
                self.regs[1] = nh as i64;
            }
            92 => {
                // Set::for_each(handle, fn_ptr)
                let h = self.regs[1] as usize;
                let fp = self.regs[2] as usize;
                let elems: Vec<i64> = if let Some(HeapObj::Set(s)) = self.heap_objs.get(&h) {
                    s.iter()
                } else { vec![] };
                for elem in elems {
                    self.call_fn_ptr(fp, elem);
                }
            }

            // ----------------------------------------------------------------
            // TernaryTrie (93-99)
            // ----------------------------------------------------------------
            93 => {
                // TernaryTrie::new() → handle
                let h = self.heap_alloc_obj(HeapObj::Trie(std::collections::BTreeMap::new()));
                self.regs[1] = h as i64;
            }
            94 => {
                // TernaryTrie::insert(handle, key, value)
                // R1=handle, R2=key (Vec handle or lp-array ptr), R3=value
                let h = self.regs[1] as usize;
                let key = self.read_trie_key(self.regs[2]);
                let val = self.regs[3];
                if let Some(HeapObj::Trie(t)) = self.heap_objs.get_mut(&h) {
                    t.insert(key, val);
                }
            }
            95 => {
                // TernaryTrie::get(handle, key) → value (0 if not found)
                let h = self.regs[1] as usize;
                let key = self.read_trie_key(self.regs[2]);
                self.regs[1] = if let Some(HeapObj::Trie(t)) = self.heap_objs.get(&h) {
                    t.get(&key).copied().unwrap_or(0)
                } else { 0 };
            }
            96 => {
                // TernaryTrie::len(handle) → int
                let h = self.regs[1] as usize;
                self.regs[1] = if let Some(HeapObj::Trie(t)) = self.heap_objs.get(&h) {
                    t.len() as i64
                } else { 0 };
            }
            97 => {
                // TernaryTrie::keys(handle) → Vec handle of Vec-handle keys
                let h = self.regs[1] as usize;
                let keys: Vec<Vec<i64>> = if let Some(HeapObj::Trie(t)) = self.heap_objs.get(&h) {
                    t.keys().cloned().collect()
                } else { vec![] };
                let handles: Vec<i64> = keys.into_iter().map(|k| {
                    self.heap_alloc_obj(HeapObj::Vec(k)) as i64
                }).collect();
                let nh = self.heap_alloc_obj(HeapObj::Vec(handles));
                self.regs[1] = nh as i64;
            }
            98 => {
                // TernaryTrie::contains_key(handle, key) → bool
                let h = self.regs[1] as usize;
                let key = self.read_trie_key(self.regs[2]);
                self.regs[1] = if let Some(HeapObj::Trie(t)) = self.heap_objs.get(&h) {
                    if t.contains_key(&key) { 1 } else { 0 }
                } else { 0 };
            }
            99 => {
                // TernaryTrie::for_each(handle, fn_ptr) — iterate (key_ptr, value) pairs
                let h = self.regs[1] as usize;
                let fp = self.regs[2] as usize;
                let pairs: Vec<(Vec<i64>, i64)> = if let Some(HeapObj::Trie(t)) = self.heap_objs.get(&h) {
                    t.iter().map(|(k, &v)| (k.clone(), v)).collect()
                } else { vec![] };
                for (k, v) in pairs {
                    let kptr = {
                        let data: Vec<i64> = std::iter::once(k.len() as i64).chain(k).collect();
                        self.heap_alloc_array(&data) as i64
                    };
                    self.regs[2] = kptr;
                    self.call_fn_ptr(fp, v);
                }
            }

            // ----------------------------------------------------------------
            // Vec bulk ops (100-102)
            // ----------------------------------------------------------------
            100 => {
                // Vec::sort(handle=R1) — in-place stable sort
                let h = self.regs[1] as usize;
                if let Some(HeapObj::Vec(v)) = self.heap_objs.get_mut(&h) {
                    v.sort_unstable();
                }
            }
            101 => {
                // Vec::reverse(handle=R1) — in-place reversal
                let h = self.regs[1] as usize;
                if let Some(HeapObj::Vec(v)) = self.heap_objs.get_mut(&h) {
                    v.reverse();
                }
            }
            102 => {
                // Vec::index_of(handle=R1, value=R2) → index in R1, or -1 if not found
                let h = self.regs[1] as usize;
                let val = self.regs[2];
                self.regs[1] = if let Some(HeapObj::Vec(v)) = self.heap_objs.get(&h) {
                    v.iter().position(|&e| e == val).map(|i| i as i64).unwrap_or(-1)
                } else { -1 };
            }

            // ----------------------------------------------------------------
            // Vec::fold (103) and Set predicates (104-107)
            // ----------------------------------------------------------------
            103 => {
                // Vec::fold(handle, init, fn_ptr) → accumulated value
                let h = self.regs[1] as usize;
                let init = self.regs[2];
                let fp = self.regs[3] as usize;
                let elems: Vec<i64> = if let Some(HeapObj::Vec(v)) = self.heap_objs.get(&h) {
                    v.clone()
                } else { vec![] };
                let mut acc = init;
                for elem in elems {
                    acc = self.call_fn_ptr_2arg(fp, acc, elem);
                }
                self.regs[1] = acc;
            }
            104 => {
                // Set::is_subset(h1, h2) → bool: h1 ⊆ h2
                let h1 = self.regs[1] as usize;
                let h2 = self.regs[2] as usize;
                let a = if let Some(HeapObj::Set(s)) = self.heap_objs.get(&h1) { s.clone() } else { Default::default() };
                let b = if let Some(HeapObj::Set(s)) = self.heap_objs.get(&h2) { s.clone() } else { Default::default() };
                self.regs[1] = if a.is_subset(&b) { 1 } else { 0 };
            }
            105 => {
                // Set::is_superset(h1, h2) → bool: h1 ⊇ h2
                let h1 = self.regs[1] as usize;
                let h2 = self.regs[2] as usize;
                let a = if let Some(HeapObj::Set(s)) = self.heap_objs.get(&h1) { s.clone() } else { Default::default() };
                let b = if let Some(HeapObj::Set(s)) = self.heap_objs.get(&h2) { s.clone() } else { Default::default() };
                self.regs[1] = if a.is_superset(&b) { 1 } else { 0 };
            }
            106 => {
                // Set::disjoint(h1, h2) → bool: h1 ∩ h2 = ∅
                let h1 = self.regs[1] as usize;
                let h2 = self.regs[2] as usize;
                let a = if let Some(HeapObj::Set(s)) = self.heap_objs.get(&h1) { s.clone() } else { Default::default() };
                let b = if let Some(HeapObj::Set(s)) = self.heap_objs.get(&h2) { s.clone() } else { Default::default() };
                self.regs[1] = if a.is_disjoint(&b) { 1 } else { 0 };
            }
            107 => {
                // TernaryTrie::keys_with_prefix(handle, prefix) → Vec handle of Vec-handle keys
                let h = self.regs[1] as usize;
                let prefix = self.read_trie_key(self.regs[2]);
                let matching: Vec<Vec<i64>> = if let Some(HeapObj::Trie(t)) = self.heap_objs.get(&h) {
                    t.keys().filter(|k| k.starts_with(&prefix)).cloned().collect()
                } else { vec![] };
                let handles: Vec<i64> = matching.into_iter().map(|k| {
                    self.heap_alloc_obj(HeapObj::Vec(k)) as i64
                }).collect();
                let nh = self.heap_alloc_obj(HeapObj::Vec(handles));
                self.regs[1] = nh as i64;
            }

            // ----------------------------------------------------------------
            // Concurrency syscalls (108-131)
            // ----------------------------------------------------------------
            108 => {
                // channel_try_recv(handle=R1) → R1 = Result ptr
                // disc: 1=Ok, -1=Err; payload: value or string handle
                const RESULT_AREA: usize = 62000;
                let h = self.regs[1] as usize;
                let (disc, payload) = match self.heap_objs.get_mut(&h) {
                    Some(HeapObj::Channel(ch)) => {
                        if let Some(v) = ch.pop_front() {
                            (1i64, v)
                        } else {
                            let s = self.heap_alloc_str("empty".to_string());
                            (-1i64, s as i64)
                        }
                    }
                    Some(HeapObj::ClosedChannel(ch)) => {
                        if let Some(v) = ch.pop_front() {
                            (1i64, v)
                        } else {
                            let s = self.heap_alloc_str("closed".to_string());
                            (-1i64, s as i64)
                        }
                    }
                    _ => {
                        let s = self.heap_alloc_str("closed".to_string());
                        (-1i64, s as i64)
                    }
                };
                self.memory[RESULT_AREA] = disc;
                self.memory[RESULT_AREA + 1] = payload;
                self.regs[1] = RESULT_AREA as i64;
            }
            109 => {
                // mutex_new(value=R1) → R1 = handle
                let val = self.regs[1];
                let h = self.heap_alloc_obj(HeapObj::Mutex(val));
                self.regs[1] = h as i64;
            }
            110 => {
                // mutex_lock(handle=R1) → R1 = handle (no-op in sequential model)
            }
            111 => {
                // mutex_get(handle=R1) → R1 = value
                let h = self.regs[1] as usize;
                let val = if let Some(HeapObj::Mutex(v)) = self.heap_objs.get(&h) {
                    *v
                } else { 0 };
                self.regs[1] = val;
            }
            112 => {
                // mutex_update(handle=R1, fn_ptr=R2) → void
                let h = self.regs[1] as usize;
                let fn_ptr = self.regs[2] as usize;
                let cur_val = if let Some(HeapObj::Mutex(v)) = self.heap_objs.get(&h) {
                    *v
                } else { 0 };
                let new_val = self.call_fn_ptr(fn_ptr, cur_val);
                if let Some(HeapObj::Mutex(v)) = self.heap_objs.get_mut(&h) {
                    *v = new_val;
                }
            }
            113 => {
                // mutex_unlock(handle=R1) → void — no-op
            }
            114 => {
                // atomic_trit_new(trit=R1) → R1 = handle
                let t = self.regs[1];
                let h = self.heap_alloc_obj(HeapObj::AtomicTrit(t));
                self.regs[1] = h as i64;
            }
            115 => {
                // atomic_trit_load(handle=R1) → R1 = trit
                let h = self.regs[1] as usize;
                let val = if let Some(HeapObj::AtomicTrit(t)) = self.heap_objs.get(&h) {
                    *t
                } else { 0 };
                self.regs[1] = val;
            }
            116 => {
                // atomic_trit_store(handle=R1, value=R2) → void
                let h = self.regs[1] as usize;
                let val = self.regs[2];
                if let Some(HeapObj::AtomicTrit(t)) = self.heap_objs.get_mut(&h) {
                    *t = val;
                }
            }
            117 => {
                // barrier_new(n=R1) → R1 = handle
                let n = self.regs[1];
                let h = self.heap_alloc_obj(HeapObj::Barrier(n, 0));
                self.regs[1] = h as i64;
            }
            118 => {
                // barrier_wait(handle=R1) → R1 = bool (is_leader = last to arrive)
                let h = self.regs[1] as usize;
                let is_leader = if let Some(HeapObj::Barrier(needed, arrived)) = self.heap_objs.get_mut(&h) {
                    *arrived += 1;
                    if *arrived >= *needed {
                        // Cycle complete: reset so the barrier is reusable.
                        *arrived = 0;
                        true
                    } else {
                        false
                    }
                } else { false };
                self.regs[1] = if is_leader { 1 } else { 0 };
            }
            119 => {
                // semaphore_new(n=R1) → R1 = handle
                let n = self.regs[1];
                let h = self.heap_alloc_obj(HeapObj::Semaphore(n));
                self.regs[1] = h as i64;
            }
            120 => {
                // semaphore_acquire(handle=R1) → void (no-op in sequential model)
            }
            121 => {
                // semaphore_release(handle=R1) → void (no-op in sequential model)
            }
            122 => {
                // task_join(handle=R1) → R1 = return value
                let h = self.regs[1] as usize;
                let val = if let Some(HeapObj::TaskResult(v)) = self.heap_objs.get(&h) {
                    *v
                } else { 0 };
                self.regs[1] = val;
            }
            123 => {
                // async_sleep(ms=R1) → void (no-op)
            }
            124 => {
                // async_spawn_task(R1=result) → R1 = task handle
                let result = self.regs[1];
                let h = self.heap_alloc_obj(HeapObj::TaskResult(result));
                self.regs[1] = h as i64;
            }
            125 => {
                // async_select(vec_handle=R1) → R1 = select_result handle
                let h = self.regs[1] as usize;
                let first_val = if let Some(HeapObj::Vec(v)) = self.heap_objs.get(&h) {
                    v.first().copied().unwrap_or(0)
                } else { 0 };
                let task_val = if let Some(HeapObj::TaskResult(v)) = self.heap_objs.get(&(first_val as usize)) {
                    *v
                } else { first_val };
                let result_h = self.heap_alloc_obj(HeapObj::Vec(vec![0, task_val]));
                self.regs[1] = result_h as i64;
            }
            126 => {
                // select_block_on(select_handle=R1) → R1 = tuple ptr (winner_idx, winner_val)
                let h = self.regs[1] as usize;
                let (idx, val) = if let Some(HeapObj::Vec(v)) = self.heap_objs.get(&h) {
                    (v.first().copied().unwrap_or(0), v.get(1).copied().unwrap_or(0))
                } else { (0, 0) };
                const TUPLE_AREA: usize = 62010;
                self.memory[TUPLE_AREA] = idx;
                self.memory[TUPLE_AREA + 1] = val;
                self.regs[1] = TUPLE_AREA as i64;
            }
            131 => {
                // mutex_set_value(handle=R1, value=R2) → void
                let h = self.regs[1] as usize;
                let val = self.regs[2];
                if let Some(HeapObj::Mutex(v)) = self.heap_objs.get_mut(&h) {
                    *v = val;
                }
            }

            // Unassigned numbers inside the claimed ranges (27-29, 37-39,
            // 45-49, 127-130) take the graceful TRAP path, not a panic.
            _ => self.trap_unknown_syscall(num),
        }
    }
}
