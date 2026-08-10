// emulator/syscall_fs.rs — File-system and network syscalls.
// Syscall ranges: 75-79 (basic file I/O), 500-515 (extended fs), 520-525 (TCP).
#[allow(unused_imports)]
use std::io::{Read as IoRead, Write as IoWrite};
use std::net::{TcpListener, TcpStream};
use super::*;

impl Emulator {
    /// Maximum buffer length a program may request for a read/write/net syscall.
    const MAX_BUF_LEN: i64 = 1 << 20;

    /// Validate a buffer-length syscall argument.  Negative or absurdly large
    /// lengths trap (with R1 = -1) instead of aborting the emulator with a
    /// capacity overflow / OOM.
    fn checked_buf_len(&mut self, num: i64, len: i64) -> Option<usize> {
        if !(0..=Self::MAX_BUF_LEN).contains(&len) {
            self.trap(format!("TRAP: syscall #{}: invalid buffer length {}", num, len));
            self.regs[1] = -1;
            return None;
        }
        Some(len as usize)
    }

    pub(super) fn do_syscall_fs(&mut self, num: i64) {
        match num {
            // ----------------------------------------------------------------
            // Basic file I/O syscalls (75-79)
            // ----------------------------------------------------------------
            75 => {
                // fs_open(path_ptr=R1, mode=R2) → R1 = fd
                // mode: 0=read, 1=write, 2=append
                let path = self.get_string_r1();
                let mode = self.regs[2];
                let file_result = match mode {
                    1 => std::fs::File::create(&path),
                    2 => std::fs::OpenOptions::new().append(true).open(&path),
                    _ => std::fs::File::open(&path),
                };
                let fd = match file_result {
                    Ok(f) => {
                        let fd = self.next_fd;
                        self.next_fd += 1;
                        self.files.insert(fd, f);
                        fd as i64
                    }
                    Err(_) => -1,
                };
                self.regs[1] = fd;
            }
            76 => {
                // fs_read(fd=R1, buf_ptr=R2) → R1 = bytes read
                let fd = self.regs[1] as usize;
                if let Some(file) = self.files.get_mut(&fd) {
                    let mut buf = Vec::new();
                    let n = file.read_to_end(&mut buf).unwrap_or(0);
                    let s = String::from_utf8_lossy(&buf).to_string();
                    let addr = self.heap_alloc_str(s);
                    let buf_ptr = self.regs[2] as usize;
                    if buf_ptr < self.memory.len() {
                        self.memory[buf_ptr] = addr as i64;
                    }
                    self.regs[1] = n as i64;
                } else {
                    self.regs[1] = -1;
                }
            }
            77 => {
                // fs_write(fd=R1, buf_ptr=R2) → R1 = bytes written
                let fd = self.regs[1] as usize;
                let addr = self.regs[2] as usize;
                let s = if let Some(content) = self.string_data.get(&addr) {
                    content.clone()
                } else { self.read_lp_string(addr) };
                if let Some(file) = self.files.get_mut(&fd) {
                    let n = file.write(s.as_bytes()).unwrap_or(0);
                    self.regs[1] = n as i64;
                } else {
                    self.regs[1] = -1;
                }
            }
            78 => {
                // fs_close(fd=R1)
                let fd = self.regs[1] as usize;
                self.files.remove(&fd);
            }
            79 => {
                // fs_exists(path_ptr=R1) → R1 = bool
                let path = self.get_string_r1();
                let exists = std::path::Path::new(&path).exists();
                self.regs[1] = if exists { 1 } else { 0 };
            }

            // ----------------------------------------------------------------
            // Extended fs syscalls (500-515): real std::fs file I/O
            // ----------------------------------------------------------------
            500 => {
                // fs_open(path_addr: R1, mode_addr: R2) -> R1 (fd or -1)
                // mode string: "r"=read, "w"=write/create, "a"=append
                let path_addr = self.regs[1] as usize;
                let path = self.string_data.get(&path_addr).cloned()
                    .unwrap_or_else(|| self.read_lp_string(path_addr));
                let mode_addr = self.regs[2] as usize;
                let mode = self.string_data.get(&mode_addr).cloned()
                    .unwrap_or_else(|| self.read_lp_string(mode_addr));
                let file_result = match mode.trim() {
                    "w" | "wb" => std::fs::File::create(&path),
                    "a" | "ab" => std::fs::OpenOptions::new().append(true).create(true).open(&path),
                    _ => std::fs::File::open(&path),
                };
                self.regs[1] = match file_result {
                    Ok(f) => {
                        let fd = self.next_fd;
                        self.next_fd += 1;
                        self.files.insert(fd, f);
                        fd as i64
                    }
                    Err(_) => -1,
                };
            }
            501 => {
                // fs_read(fd: R1, buf_addr: R2, max_len: R3) -> R1 (bytes read)
                let fd = self.regs[1] as usize;
                let buf_addr = self.regs[2] as usize;
                let Some(max_len) = self.checked_buf_len(num, self.regs[3]) else { return; };
                if let Some(file) = self.files.get_mut(&fd) {
                    let mut buf = vec![0u8; max_len];
                    let n = file.read(&mut buf).unwrap_or(0);
                    for i in 0..n {
                        let a = buf_addr + i;
                        if a < self.memory.len() {
                            self.memory[a] = buf[i] as i64;
                        }
                    }
                    self.regs[1] = n as i64;
                } else {
                    self.regs[1] = -1;
                }
            }
            502 => {
                // fs_write(fd: R1, buf_addr: R2, len: R3) -> R1 (bytes written)
                let fd = self.regs[1] as usize;
                let buf_addr = self.regs[2] as usize;
                let Some(len) = self.checked_buf_len(num, self.regs[3]) else { return; };
                let bytes: Vec<u8> = (0..len)
                    .map(|i| self.memory.get(buf_addr + i).copied().unwrap_or(0) as u8)
                    .collect();
                if let Some(file) = self.files.get_mut(&fd) {
                    self.regs[1] = file.write(&bytes).unwrap_or(0) as i64;
                } else {
                    self.regs[1] = -1;
                }
            }
            503 => {
                // fs_close(fd: R1) -> void
                let fd = self.regs[1] as usize;
                self.files.remove(&fd);
            }
            504 => {
                // fs_exists(path_addr: R1) -> R1 (1 if exists, 0 if not)
                let path_addr = self.regs[1] as usize;
                let path = self.string_data.get(&path_addr).cloned()
                    .unwrap_or_else(|| self.read_lp_string(path_addr));
                self.regs[1] = if std::path::Path::new(&path).exists() { 1 } else { 0 };
            }
            505 => {
                // fs_read_all(path_addr: R1) -> R1 (str ptr or 0)
                let path_addr = self.regs[1] as usize;
                let path = self.string_data.get(&path_addr).cloned()
                    .unwrap_or_else(|| self.read_lp_string(path_addr));
                self.regs[1] = match std::fs::read_to_string(&path) {
                    Ok(s) => self.heap_alloc_str(s) as i64,
                    Err(_) => 0,
                };
            }
            506 => {
                // fs_write_all(path_addr: R1, content_addr: R2) -> R1 (1 on success, 0 on error)
                let path_addr = self.regs[1] as usize;
                let content_addr = self.regs[2] as usize;
                let path = self.string_data.get(&path_addr).cloned()
                    .unwrap_or_else(|| self.read_lp_string(path_addr));
                let content = self.string_data.get(&content_addr).cloned()
                    .unwrap_or_else(|| self.read_lp_string(content_addr));
                self.regs[1] = match std::fs::write(&path, content.as_bytes()) {
                    Ok(_) => 1,
                    Err(_) => 0,
                };
            }
            507 => {
                // fs_append(path_addr: R1, content_addr: R2) -> R1 (1 on success, 0 on error)
                use std::io::Write as W;
                let path_addr = self.regs[1] as usize;
                let content_addr = self.regs[2] as usize;
                let path = self.string_data.get(&path_addr).cloned()
                    .unwrap_or_else(|| self.read_lp_string(path_addr));
                let content = self.string_data.get(&content_addr).cloned()
                    .unwrap_or_else(|| self.read_lp_string(content_addr));
                let result = std::fs::OpenOptions::new()
                    .append(true)
                    .create(true)
                    .open(&path)
                    .and_then(|mut f| f.write_all(content.as_bytes()));
                self.regs[1] = if result.is_ok() { 1 } else { 0 };
            }
            508 => {
                // fs_delete(path_addr: R1) -> R1 (1 on success, 0 on error)
                let path_addr = self.regs[1] as usize;
                let path = self.string_data.get(&path_addr).cloned()
                    .unwrap_or_else(|| self.read_lp_string(path_addr));
                self.regs[1] = match std::fs::remove_file(&path) {
                    Ok(_) => 1,
                    Err(_) => 0,
                };
            }
            509 => {
                // fs_mkdir(path_addr: R1) -> R1 (1 on success, 0 on error)
                let path_addr = self.regs[1] as usize;
                let path = self.string_data.get(&path_addr).cloned()
                    .unwrap_or_else(|| self.read_lp_string(path_addr));
                self.regs[1] = match std::fs::create_dir_all(&path) {
                    Ok(_) => 1,
                    Err(_) => 0,
                };
            }
            510 => {
                // fs_list_dir(path_addr: R1) -> R1 (Vec handle of str ptrs, or 0)
                let path_addr = self.regs[1] as usize;
                let path = self.string_data.get(&path_addr).cloned()
                    .unwrap_or_else(|| self.read_lp_string(path_addr));
                let entries: Vec<i64> = std::fs::read_dir(&path)
                    .map(|rd| {
                        rd.filter_map(|e| {
                            let name = e.ok()?.file_name().to_string_lossy().to_string();
                            Some(self.heap_alloc_str(name) as i64)
                        }).collect()
                    })
                    .unwrap_or_default();
                let h = self.heap_alloc_obj(HeapObj::Vec(entries));
                self.regs[1] = h as i64;
            }
            511 => {
                // fs_is_dir(path_addr: R1) -> R1 (1 if dir, 0 if not)
                let path_addr = self.regs[1] as usize;
                let path = self.string_data.get(&path_addr).cloned()
                    .unwrap_or_else(|| self.read_lp_string(path_addr));
                self.regs[1] = if std::path::Path::new(&path).is_dir() { 1 } else { 0 };
            }
            512 => {
                // fs_rename(src_addr: R1, dst_addr: R2) -> R1 (1 on success, 0 on error)
                let src_addr = self.regs[1] as usize;
                let dst_addr = self.regs[2] as usize;
                let src = self.string_data.get(&src_addr).cloned()
                    .unwrap_or_else(|| self.read_lp_string(src_addr));
                let dst = self.string_data.get(&dst_addr).cloned()
                    .unwrap_or_else(|| self.read_lp_string(dst_addr));
                self.regs[1] = match std::fs::rename(&src, &dst) {
                    Ok(_) => 1,
                    Err(_) => 0,
                };
            }

            // ----------------------------------------------------------------
            // TCP networking syscalls (520-525): std::net::TcpStream/TcpListener
            // ----------------------------------------------------------------
            520 => {
                // net_tcp_connect(host_addr: R1, port: R2) -> R1 (fd or -1)
                let host_addr = self.regs[1] as usize;
                let port = self.regs[2] as u16;
                let host = self.string_data.get(&host_addr).cloned()
                    .unwrap_or_else(|| self.read_lp_string(host_addr));
                let addr = format!("{}:{}", host, port);
                self.regs[1] = match TcpStream::connect(&addr) {
                    Ok(stream) => {
                        let fd = self.next_net_fd;
                        self.next_net_fd += 1;
                        self.tcp_streams.insert(fd, stream);
                        fd as i64
                    }
                    Err(_) => -1,
                };
            }
            521 => {
                // net_tcp_listen(port: R1) -> R1 (fd or -1)
                let port = self.regs[1] as u16;
                let addr = format!("0.0.0.0:{}", port);
                self.regs[1] = match TcpListener::bind(&addr) {
                    Ok(listener) => {
                        let fd = self.next_net_fd;
                        self.next_net_fd += 1;
                        self.tcp_listeners.insert(fd, listener);
                        fd as i64
                    }
                    Err(_) => -1,
                };
            }
            522 => {
                // net_tcp_accept(listener_fd: R1) -> R1 (stream fd or -1)
                let listener_fd = self.regs[1] as usize;
                let accept_result = self.tcp_listeners.get(&listener_fd)
                    .and_then(|l| l.accept().ok());
                self.regs[1] = match accept_result {
                    Some((stream, _addr)) => {
                        let fd = self.next_net_fd;
                        self.next_net_fd += 1;
                        self.tcp_streams.insert(fd, stream);
                        fd as i64
                    }
                    None => -1,
                };
            }
            523 => {
                // net_send(fd: R1, data_addr: R2, len: R3) -> R1 (bytes sent)
                let fd = self.regs[1] as usize;
                let data_addr = self.regs[2] as usize;
                let Some(len) = self.checked_buf_len(num, self.regs[3]) else { return; };
                let bytes: Vec<u8> = (0..len)
                    .map(|i| self.memory.get(data_addr + i).copied().unwrap_or(0) as u8)
                    .collect();
                self.regs[1] = if let Some(stream) = self.tcp_streams.get_mut(&fd) {
                    stream.write(&bytes).unwrap_or(0) as i64
                } else {
                    -1
                };
            }
            524 => {
                // net_recv(fd: R1, buf_addr: R2, max_len: R3) -> R1 (bytes received)
                let fd = self.regs[1] as usize;
                let buf_addr = self.regs[2] as usize;
                let Some(max_len) = self.checked_buf_len(num, self.regs[3]) else { return; };
                if let Some(stream) = self.tcp_streams.get_mut(&fd) {
                    let mut buf = vec![0u8; max_len];
                    let n = stream.read(&mut buf).unwrap_or(0);
                    for i in 0..n {
                        let a = buf_addr + i;
                        if a < self.memory.len() {
                            self.memory[a] = buf[i] as i64;
                        }
                    }
                    self.regs[1] = n as i64;
                } else {
                    self.regs[1] = -1;
                }
            }
            525 => {
                // net_close(fd: R1) — close TCP stream or listener
                let fd = self.regs[1] as usize;
                self.tcp_streams.remove(&fd);
                self.tcp_listeners.remove(&fd);
            }

            // Unassigned numbers inside the claimed ranges (513-519) take the
            // graceful TRAP path, not a panic.
            _ => self.trap_unknown_syscall(num),
        }
    }
}
