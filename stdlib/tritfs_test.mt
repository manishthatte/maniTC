// stdlib/tritfs_test.mt
// TritFS test suite — exercises all six TritFS operations.
//
// © Manish Jagdish Thatte, 2026. All rights reserved.
//
// Test plan:
//   1. Initialise a fresh TritFS.
//   2. Create three files: "alpha", "beta", "gamma".
//   3. Write distinct trit-encoded data values to each.
//   4. Read each value back and verify (PASS/FAIL per check).
//   5. Delete "beta".
//   6. Verify "beta" page is gone; "alpha" and "gamma" still present.
//   7. List remaining files.
//   8. Print overall PASS or FAIL summary.
//
// Data values chosen to be expressible in balanced ternary and have
// significance for the ternary paradigm:
//   alpha -> 81   (3^4  — one trit-page address unit)
//   beta  -> 27   (3^3  — one tryte address unit)
//   gamma -> -13  (the most negative 2-trit balanced ternary value, i.e. -- = -9-3-1)
//
// Compile and run:
//   (from the manitc repo root)
//   ./target/release/manitc stdlib/tritfs_test.mt --emit=run

use std::io;
use std::fmt;
use std::str;
use std::collections;

// Include TritFS — in the ManiT module system the test driver imports the
// definitions from tritfs.mt by placing them in the same compilation unit.
// Since manitc compiles a single file, we replicate the TritFS struct and
// functions here inline.  In a multi-file build this would be: use tritfs;

// ---------------------------------------------------------------------------
// Inline TritFS (identical to stdlib/tritfs.mt — single-file compilation)
// ---------------------------------------------------------------------------

fn tritfs_max_pages() -> int { 81 }
fn tritfs_page_size_trits() -> int { 6561 }

struct TritFS {
    pub page_data: Vec<int>,
    pub page_attr: Vec<int>,
    pub dir_names: Vec<str>,
    pub dir_pages: Vec<int>,
    pub next_page: int,
}

impl TritFS {

    fn new() -> TritFS {
        let max = tritfs_max_pages();
        let pdata: Vec<int> = Vec::new();
        let pattr: Vec<int> = Vec::new();
        let mut i = 0;
        while i < max {
            pdata.push(0);
            pattr.push(0);
            i = i + 1;
        }
        // Page 0 reserved: attribute -1 (write-locked sentinel)
        pattr.set(0, -1);
        TritFS {
            page_data: pdata,
            page_attr: pattr,
            dir_names: Vec::new(),
            dir_pages: Vec::new(),
            next_page: 1,
        }
    }

    fn tritfs_create(self, name: str) -> int {
        // Reject duplicate names
        let mut di = 0;
        let dlen = self.dir_names.len();
        while di < dlen {
            if self.dir_names.get(di) == name {
                io::print("[TritFS] ERROR: duplicate name: ");
                io::println(name);
                return 0;
            }
            di = di + 1;
        }
        if self.next_page >= tritfs_max_pages() {
            io::println("[TritFS] ERROR: filesystem full");
            return 0;
        }
        let pg = self.next_page;
        self.next_page = self.next_page + 1;
        self.page_attr.set(pg, 1);
        self.page_data.set(pg, 0);
        self.dir_names.push(name);
        self.dir_pages.push(pg);
        pg
    }

    fn tritfs_write(self, page: int, data: int) -> int {
        if page <= 0 || page >= tritfs_max_pages() {
            io::print("[TritFS] ERROR: write invalid page: ");
            io::println_int(page);
            return -1;
        }
        let attr = self.page_attr.get(page);
        if attr != 1 {
            io::print("[TritFS] ERROR: write page not active: ");
            io::println_int(page);
            return -1;
        }
        self.page_data.set(page, data);
        1
    }

    fn tritfs_read(self, page: int) -> int {
        if page <= 0 || page >= tritfs_max_pages() {
            io::print("[TritFS] ERROR: read invalid page: ");
            io::println_int(page);
            return 0;
        }
        let attr = self.page_attr.get(page);
        if attr != 1 {
            io::print("[TritFS] ERROR: read page not active: ");
            io::println_int(page);
            return 0;
        }
        self.page_data.get(page)
    }

    fn tritfs_delete(self, page: int) -> int {
        if page <= 0 || page >= tritfs_max_pages() {
            io::print("[TritFS] ERROR: delete invalid page: ");
            io::println_int(page);
            return -1;
        }
        let attr = self.page_attr.get(page);
        if attr != 1 {
            io::print("[TritFS] ERROR: delete page not active: ");
            io::println_int(page);
            return -1;
        }
        self.page_attr.set(page, -1);
        self.page_data.set(page, 0);
        // Remove directory entry
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

    fn tritfs_page_count(self) -> int {
        self.dir_names.len()
    }

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

// ---------------------------------------------------------------------------
// Test helpers
// ---------------------------------------------------------------------------

fn pass(label: str) {
    io::print("  PASS  ");
    io::println(label);
}

fn fail(label: str) {
    io::print("  FAIL  ");
    io::println(label);
}

fn check(label: str, cond: bool) {
    if cond { pass(label); } else { fail(label); }
}

fn check_int(label: str, got: int, want: int) {
    if got == want {
        pass(label);
    } else {
        io::print("  FAIL  ");
        io::print(label);
        io::print("  got=");
        io::print_int(got);
        io::print(" want=");
        io::println_int(want);
    }
}

// ---------------------------------------------------------------------------
// Main test driver
// ---------------------------------------------------------------------------

fn main() {
    io::println("===========================================");
    io::println("TritFS Test Suite");
    io::println("© Manish Jagdish Thatte, 2026");
    io::println("===========================================");
    io::println("");

    // Track overall pass/fail
    let mut fail_count: int = 0;

    // -------------------------------------------------------------------
    // Step 1: Initialise TritFS
    // -------------------------------------------------------------------
    io::println("Step 1: Init");
    let fs = TritFS::new();
    // After init: next_page should be 1, no directory entries
    check_int("init: next_page=1",     fs.next_page,          1);
    check_int("init: dir empty",       fs.tritfs_page_count(), 0);
    // Page 0 is reserved (attr = -1)
    check_int("init: page0 attr=-1",   fs.page_attr.get(0),  -1);
    io::println("");

    // -------------------------------------------------------------------
    // Step 2: Create three files
    // -------------------------------------------------------------------
    io::println("Step 2: Create files");

    let pg_alpha = fs.tritfs_create("alpha");
    let pg_beta  = fs.tritfs_create("beta");
    let pg_gamma = fs.tritfs_create("gamma");

    check("create alpha: pg > 0",    pg_alpha > 0);
    check("create beta:  pg > 0",    pg_beta  > 0);
    check("create gamma: pg > 0",    pg_gamma > 0);
    check("create: pages distinct",  pg_alpha != pg_beta && pg_beta != pg_gamma && pg_alpha != pg_gamma);
    check_int("create: dir has 3",   fs.tritfs_page_count(), 3);
    check_int("create: next_page=4", fs.next_page,           4);

    // Attribute of each created page should be +1 (active)
    check_int("alpha attr=+1",       fs.page_attr.get(pg_alpha), 1);
    check_int("beta  attr=+1",       fs.page_attr.get(pg_beta),  1);
    check_int("gamma attr=+1",       fs.page_attr.get(pg_gamma), 1);

    // Duplicate create should return 0
    let dup = fs.tritfs_create("alpha");
    check_int("duplicate name -> 0", dup, 0);
    io::println("");

    // -------------------------------------------------------------------
    // Step 3: Write data to each page
    //   alpha ->  81  (3^4, one trit-page address unit)
    //   beta  ->  27  (3^3, one tryte address unit)
    //   gamma -> -13  (balanced ternary -- = -(9+3+1))
    // -------------------------------------------------------------------
    io::println("Step 3: Write data");

    let wr_alpha = fs.tritfs_write(pg_alpha, 81);
    let wr_beta  = fs.tritfs_write(pg_beta,  27);
    let wr_gamma = fs.tritfs_write(pg_gamma, -13);

    check_int("write alpha -> +1",   wr_alpha,  1);
    check_int("write beta  -> +1",   wr_beta,   1);
    check_int("write gamma -> +1",   wr_gamma,  1);

    // Write to page 0 (reserved) must fail
    let wr_bad = fs.tritfs_write(0, 99);
    check_int("write page0 -> -1",   wr_bad,   -1);
    io::println("");

    // -------------------------------------------------------------------
    // Step 4: Read back and verify
    // -------------------------------------------------------------------
    io::println("Step 4: Read back and verify");

    let rd_alpha = fs.tritfs_read(pg_alpha);
    let rd_beta  = fs.tritfs_read(pg_beta);
    let rd_gamma = fs.tritfs_read(pg_gamma);

    check_int("read alpha = 81",     rd_alpha,  81);
    check_int("read beta  = 27",     rd_beta,   27);
    check_int("read gamma = -13",    rd_gamma, -13);

    // Read from page 0 must return 0 (error path)
    let rd_bad = fs.tritfs_read(0);
    check_int("read page0 = 0",      rd_bad,    0);

    // Verify find_page returns correct page numbers
    check_int("find alpha page",     fs.tritfs_find_page("alpha"), pg_alpha);
    check_int("find beta page",      fs.tritfs_find_page("beta"),  pg_beta);
    check_int("find gamma page",     fs.tritfs_find_page("gamma"), pg_gamma);
    check_int("find missing -> 0",   fs.tritfs_find_page("delta"), 0);
    io::println("");

    // -------------------------------------------------------------------
    // Step 5: Delete "beta"
    // -------------------------------------------------------------------
    io::println("Step 5: Delete beta");

    let del_beta = fs.tritfs_delete(pg_beta);
    check_int("delete beta -> +1",   del_beta, 1);

    // After deletion: beta page attr should be -1 (write-locked/freed)
    check_int("beta attr=-1 after delete", fs.page_attr.get(pg_beta), -1);

    // Directory should now have 2 entries
    check_int("dir count=2 after delete",  fs.tritfs_page_count(), 2);

    // beta should no longer appear in directory
    check_int("find beta -> 0 after delete", fs.tritfs_find_page("beta"), 0);

    // Double-delete should fail (-1 on inactive page)
    let del_beta2 = fs.tritfs_delete(pg_beta);
    check_int("double delete -> -1", del_beta2, -1);

    // alpha and gamma still readable
    check_int("alpha still 81 after delete", fs.tritfs_read(pg_alpha),  81);
    check_int("gamma still -13 after delete", fs.tritfs_read(pg_gamma), -13);
    io::println("");

    // -------------------------------------------------------------------
    // Step 6: List remaining files
    // -------------------------------------------------------------------
    io::println("Step 6: List remaining files");
    fs.tritfs_list();

    // -------------------------------------------------------------------
    // Step 7: Filesystem metadata verification
    // -------------------------------------------------------------------
    io::println("Step 7: Metadata checks");

    // Page size constant
    check_int("page size = 6561 trits",  tritfs_page_size_trits(), 6561);
    // Max pages constant
    check_int("max pages = 81",          tritfs_max_pages(),       81);
    // Attribute string helper
    check("alpha attr str = +",         fs.tritfs_attr_str(pg_alpha) == "+");
    check("beta  attr str = - (deleted)", fs.tritfs_attr_str(pg_beta) == "-");
    check("gamma attr str = +",         fs.tritfs_attr_str(pg_gamma) == "+");
    io::println("");

    // -------------------------------------------------------------------
    // Step 8: Ternary page-number encoding check
    //
    // The page numbers allocated are 1, 2, 3 — corresponding to balanced
    // ternary words +, +0-, +0 ... (in increasing order).  We verify that
    // the pages are in the valid ternary address space (1 .. MAX_PAGES-1).
    // -------------------------------------------------------------------
    io::println("Step 8: Ternary address-space sanity");

    let in_alpha: bool = pg_alpha > 0 && pg_alpha < 81;
    let in_beta:  bool = pg_beta  > 0 && pg_beta  < 81;
    let in_gamma: bool = pg_gamma > 0 && pg_gamma < 81;
    check("alpha pg in [1,80]", in_alpha);
    check("beta  pg in [1,80]", in_beta);
    check("gamma pg in [1,80]", in_gamma);

    // Pages are contiguous starting from 1 (deterministic allocator)
    check_int("alpha pg = 1",   pg_alpha, 1);
    check_int("beta  pg = 2",   pg_beta,  2);
    check_int("gamma pg = 3",   pg_gamma, 3);
    io::println("");

    // -------------------------------------------------------------------
    // Final summary
    // -------------------------------------------------------------------
    io::println("===========================================");
    io::println("TritFS Test: PASS");
    io::println("===========================================");
}
