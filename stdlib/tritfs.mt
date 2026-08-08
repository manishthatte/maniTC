// stdlib/tritfs.mt
// TritFS — Persistent Ternary Page-Mapped File System (in-memory simulation)
//
// © Manish Jagdish Thatte, 2026. All rights reserved.
//
// TritFS is a balanced ternary file system layer designed for the T3ISA
// architecture.  It maps file names (trit strings) to ternary page numbers
// and stores page data as t27 words.
//
// Design constants:
//   TRIT_PAGE_SIZE  : 3^8 = 6561 trits per page  (ternary analogue of 4 KB)
//   MAX_PAGES       : 3^4 = 81 pages in this simulation
//   Page numbers    : encoded as int in the range [0, MAX_PAGES)
//   Page 0          : reserved (null / unallocated sentinel)
//
// File attributes encoded as trit:
//   +1 (trit +) : readable / active
//    0 (trit 0) : empty / unused slot
//   -1 (trit -) : write-locked / deleted
//
// All filesystem state lives inside a TritFS struct so it can be created,
// passed around, and tested without global mutable state.
//
// Usage:
//   use std::collections;
//   use std::io;
//   use std::fmt;
//   // (tritfs.mt is included directly — it defines the TritFS struct)
//
//   let fs = TritFS::new();
//   let pg = fs.tritfs_create("alpha");
//   fs.tritfs_write(pg, 42);
//   let v  = fs.tritfs_read(pg);
//   fs.tritfs_list();

use std::io;
use std::fmt;
use std::str;
use std::collections;

// ---------------------------------------------------------------------------
// Internal constants (expressed as plain ints for emulator compatibility)
// ---------------------------------------------------------------------------

// MAX_PAGES: 3^4 = 81 page slots (page 0 is reserved)
fn tritfs_max_pages() -> int { 81 }

// TRIT_PAGE_SIZE: 3^8 = 6561 (informational; actual data stored as one t27
// word per page in this simulation — the full page backing store would be
// 6561 trits wide but is abstracted to a single data word here)
fn tritfs_page_size_trits() -> int { 6561 }

// ---------------------------------------------------------------------------
// TritFS struct
//
// Fields:
//   page_data   : Vec<int>  — one data word per page slot (index = page number)
//   page_attr   : Vec<int>  — attribute trit per page (+1 active, 0 free, -1 locked)
//   dir_names   : Vec<str>  — directory: parallel array of file names
//   dir_pages   : Vec<int>  — directory: parallel array of page numbers
//   next_page   : int       — next free page number to allocate
// ---------------------------------------------------------------------------

struct TritFS {
    pub page_data: Vec<int>,
    pub page_attr: Vec<int>,
    pub dir_names: Vec<str>,
    pub dir_pages: Vec<int>,
    pub next_page: int,
}

impl TritFS {

    // -----------------------------------------------------------------------
    // Constructor: tritfs_new()
    // Initialise an empty TritFS with all pages zeroed and free.
    // Page 0 is the null/reserved page — never allocated to a file.
    // -----------------------------------------------------------------------
    fn new() -> TritFS {
        let max = tritfs_max_pages();

        // page_data[i] holds the data word stored in page i
        let pdata: Vec<int> = Vec::new();
        // page_attr[i] holds the attribute: +1 = active, 0 = free, -1 = locked
        let pattr: Vec<int> = Vec::new();

        let mut i = 0;
        while i < max {
            pdata.push(0);
            pattr.push(0);
            i = i + 1;
        }

        // Mark page 0 as locked (reserved sentinel — never a valid file page)
        pattr.set(0, -1);

        TritFS {
            page_data: pdata,
            page_attr: pattr,
            dir_names: Vec::new(),
            dir_pages: Vec::new(),
            next_page: 1,
        }
    }

    // -----------------------------------------------------------------------
    // tritfs_create(name) -> int
    // Allocate a fresh page, register the name in the directory, and return
    // the page number.  Returns 0 on failure (filesystem full or name exists).
    // -----------------------------------------------------------------------
    fn tritfs_create(self, name: str) -> int {
        // Check for duplicate name
        let mut di = 0;
        let dlen = self.dir_names.len();
        while di < dlen {
            if self.dir_names.get(di) == name {
                io::print("[TritFS] ERROR: name already exists: ");
                io::println(name);
                return 0;
            }
            di = di + 1;
        }

        // Check capacity
        if self.next_page >= tritfs_max_pages() {
            io::println("[TritFS] ERROR: filesystem full");
            return 0;
        }

        let pg = self.next_page;
        self.next_page = self.next_page + 1;

        // Mark page as active (+1)
        self.page_attr.set(pg, 1);
        // Data starts at 0
        self.page_data.set(pg, 0);

        // Register in directory
        self.dir_names.push(name);
        self.dir_pages.push(pg);

        pg
    }

    // -----------------------------------------------------------------------
    // tritfs_write(page, data) -> int
    // Write `data` (as int) to page `page`.
    // Returns +1 on success, -1 on failure (page not active / out of range).
    // -----------------------------------------------------------------------
    fn tritfs_write(self, page: int, data: int) -> int {
        if page <= 0 || page >= tritfs_max_pages() {
            io::print("[TritFS] ERROR: write — invalid page: ");
            io::println_int(page);
            return -1;
        }
        let attr = self.page_attr.get(page);
        if attr != 1 {
            io::print("[TritFS] ERROR: write — page not active: ");
            io::println_int(page);
            return -1;
        }
        self.page_data.set(page, data);
        1
    }

    // -----------------------------------------------------------------------
    // tritfs_read(page) -> int
    // Read the data word stored in page `page`.
    // Returns 0 on invalid / inactive page.
    // -----------------------------------------------------------------------
    fn tritfs_read(self, page: int) -> int {
        if page <= 0 || page >= tritfs_max_pages() {
            io::print("[TritFS] ERROR: read — invalid page: ");
            io::println_int(page);
            return 0;
        }
        let attr = self.page_attr.get(page);
        if attr != 1 {
            io::print("[TritFS] ERROR: read — page not active: ");
            io::println_int(page);
            return 0;
        }
        self.page_data.get(page)
    }

    // -----------------------------------------------------------------------
    // tritfs_delete(page) -> int
    // Mark page as deleted (attribute set to -1 = write-locked / freed).
    // Removes the directory entry for this page.
    // Returns +1 on success, -1 on failure.
    // -----------------------------------------------------------------------
    fn tritfs_delete(self, page: int) -> int {
        if page <= 0 || page >= tritfs_max_pages() {
            io::print("[TritFS] ERROR: delete — invalid page: ");
            io::println_int(page);
            return -1;
        }
        let attr = self.page_attr.get(page);
        if attr != 1 {
            io::print("[TritFS] ERROR: delete — page not active: ");
            io::println_int(page);
            return -1;
        }

        // Set attribute to -1 (write-locked / deleted)
        self.page_attr.set(page, -1);
        // Clear data
        self.page_data.set(page, 0);

        // Remove directory entry (find index where dir_pages[i] == page)
        let mut found = -1;
        let mut di = 0;
        let dlen = self.dir_pages.len();
        while di < dlen {
            if self.dir_pages.get(di) == page {
                found = di;
            }
            di = di + 1;
        }
        if found >= 0 {
            self.dir_names.remove(found);
            self.dir_pages.remove(found);
        }

        1
    }

    // -----------------------------------------------------------------------
    // tritfs_list() -> void
    // Print a table of all active pages: page number, name, and data value.
    // Also prints trit attribute symbol: + active, 0 free, - deleted.
    // -----------------------------------------------------------------------
    fn tritfs_list(self) {
        io::println("[TritFS] Directory listing:");
        io::println("  page | attr | name                 | data");
        io::println("  -----|------|----------------------|----------");

        let dlen = self.dir_names.len();
        if dlen == 0 {
            io::println("  (empty)");
            return;
        }

        let mut di = 0;
        while di < dlen {
            let pg   = self.dir_pages.get(di);
            let name = self.dir_names.get(di);
            let attr = self.page_attr.get(pg);
            let data = self.page_data.get(pg);

            io::print("  ");
            io::print(fmt::align_right(fmt::show_int(pg), 4, ' '));
            io::print(" | ");

            // Print attribute trit as symbol
            if attr == 1 {
                io::print(" +  ");
            } elif attr == 0 {
                io::print(" 0  ");
            } else {
                io::print(" -  ");
            }

            io::print(" | ");
            io::print(fmt::align_left(name, 20, ' '));
            io::print(" | ");
            io::println_int(data);

            di = di + 1;
        }
        io::println("");
    }

    // -----------------------------------------------------------------------
    // tritfs_find_page(name) -> int
    // Look up the page number for a given file name.
    // Returns 0 if not found.
    // -----------------------------------------------------------------------
    fn tritfs_find_page(self, name: str) -> int {
        let mut di = 0;
        let dlen = self.dir_names.len();
        while di < dlen {
            if self.dir_names.get(di) == name {
                return self.dir_pages.get(di);
            }
            di = di + 1;
        }
        0
    }

    // -----------------------------------------------------------------------
    // tritfs_page_count() -> int
    // Return the number of active (allocated) pages.
    // -----------------------------------------------------------------------
    fn tritfs_page_count(self) -> int {
        self.dir_names.len()
    }

    // -----------------------------------------------------------------------
    // tritfs_attr_str(page) -> str
    // Return the attribute trit as a string: "+", "0", or "-".
    // -----------------------------------------------------------------------
    fn tritfs_attr_str(self, page: int) -> str {
        if page <= 0 || page >= tritfs_max_pages() {
            return "?";
        }
        let attr = self.page_attr.get(page);
        if attr == 1  { return "+"; }
        if attr == 0  { return "0"; }
        return "-";
    }
}
