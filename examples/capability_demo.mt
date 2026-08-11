// examples/capability_demo.mt
// Capability-Based Security — thatteOS Three-Ring Model
//
// thatteOS uses a three-ring privilege model, mapped naturally to trits:
//   +  =>  Ring + (Kernel)    — full hardware access
//   0  =>  Ring 0 (Service)   — device drivers, system services
//   -  =>  Ring - (User)      — application code
//
// This is not a translation of binary ring levels (0/1/2/3).
// It is the NATIVE ternary model:
//   - Privilege comparison is a single trit comparison (TMIN2 gate)
//   - Permission attenuation is tand (min) — you can only lower privilege
//   - Three levels are exactly what most real systems need
//   - Capability tokens carry their ring level as a trit field
//
// In binary systems (x86 rings 0-3):
//   Ring comparison requires subtraction + sign check (multi-gate)
//   Four rings, but rings 1 and 2 are almost never used
//   Two wasted levels, more complexity for nothing
//
// In thatteOS:
//   Ring comparison = one trit, one gate, one cycle
//   Three rings, all three used, no waste
//
// Demonstrates:
//   - Process creation with ring levels
//   - Capability tokens with trit-encoded permissions
//   - TMIN2 gate permission checking
//   - Capability attenuation (lowering privileges)
//   - Capability delegation (transferring to another process)
//   - Three-ring security boundary enforcement
//
// Authored by: Manish Jagdish Thatte

use std::io;
use std::fmt;
use std::ternary;
use std::collections;

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------
fn heading(title: str) {
    io::println("");
    io::println("================================================");
    io::print("  ");
    io::println(title);
    io::println("================================================");
}

fn ring_name(ring: trit) -> str {
    tif ring {
        + => return "Kernel  (+)",
        0 => return "Service (0)",
        - => return "User    (-)",
    }
}

fn perm_str(p: trit) -> str {
    tif p {
        + => return "GRANT",
        0 => return "CHECK",
        - => return "DENY ",
    }
}

// ---------------------------------------------------------------------------
// 1. Process and Ring Levels
// ---------------------------------------------------------------------------
struct Process {
    pub pid: int,
    pub name: str,
    pub ring: trit,   // privilege level: +, 0, -
}

impl Process {
    fn new(pid: int, name: str, ring: trit) -> Process {
        return Process { pid: pid, name: name, ring: ring };
    }

    fn describe(self) {
        io::print("PID ");
        io::print_int(self.pid);
        io::print(" [");
        io::print(self.name);
        io::print("] ring=");
        io::print_trit(self.ring);
    }
}

fn demo_process_rings() {
    heading("1. Process Ring Levels — The Ternary Privilege Model");

    io::println("  thatteOS three-ring model:");
    io::println("    Ring + (Kernel):  full hardware access, memory management");
    io::println("    Ring 0 (Service): device drivers, system daemons");
    io::println("    Ring - (User):    application code, sandboxed");
    io::println("");

    let kernel_proc  = Process::new(0, "kernel",     +);
    let driver_proc  = Process::new(1, "disk_driver", 0);
    let service_proc = Process::new(2, "netstack",    0);
    let user_proc    = Process::new(3, "editor",      -);
    let shell_proc   = Process::new(4, "shell",       -);

    let procs: [Process] = [kernel_proc, driver_proc, service_proc, user_proc, shell_proc];

    io::println("  Running processes:");
    for p in procs {
        io::print("    ");
        p.describe();
        io::print("  =>  ");
        io::println(ring_name(p.ring));
    }
}

// ---------------------------------------------------------------------------
// 2. Capability Tokens
// ---------------------------------------------------------------------------
// A capability is a transferable token that grants specific permissions.
// Permissions are encoded as trits:
//   + => full access (read + write + execute)
//   0 => partial access (read only)
//   - => no access (revoked)

struct Capability {
    pub id: int,
    pub resource: str,     // what resource this grants access to
    pub read_perm: trit,   // read permission level
    pub write_perm: trit,  // write permission level
    pub exec_perm: trit,   // execute permission level
    pub min_ring: trit,    // minimum ring level required
    pub owner_pid: int,    // process that holds this capability
}

impl Capability {
    fn new(id: int, resource: str, r: trit, w: trit, x: trit,
           min_ring: trit, owner: int) -> Capability {
        return Capability {
            id: id,
            resource: resource,
            read_perm: r,
            write_perm: w,
            exec_perm: x,
            min_ring: min_ring,
            owner_pid: owner,
        };
    }

    fn describe(self) {
        io::print("    Cap#");
        io::print_int(self.id);
        io::print("  resource=\"");
        io::print(self.resource);
        io::print("\"  R=");
        io::print_trit(self.read_perm);
        io::print(" W=");
        io::print_trit(self.write_perm);
        io::print(" X=");
        io::print_trit(self.exec_perm);
        io::print("  min_ring=");
        io::print_trit(self.min_ring);
        io::print("  owner=PID ");
        io::println_int(self.owner_pid);
    }
}

fn demo_capabilities() {
    heading("2. Capability Tokens — Trit-Encoded Permissions");

    io::println("  Permission encoding:");
    io::println("    + => full access   (read/write/execute)");
    io::println("    0 => partial       (read-only / check)");
    io::println("    - => no access     (denied / revoked)");
    io::println("");

    // Create capabilities for different resources.
    let cap_mem = Capability::new(1, "/dev/mem",    +, +, -, +, 0);   // kernel only
    let cap_disk = Capability::new(2, "/dev/disk",  +, +, -, 0, 1);   // service+
    let cap_file = Capability::new(3, "/home/data", +, +, -, -, 3);   // any ring
    let cap_net = Capability::new(4, "/dev/net",    +, 0, -, 0, 2);   // service, read+, write partial

    io::println("  Allocated capabilities:");
    cap_mem.describe();
    cap_disk.describe();
    cap_file.describe();
    cap_net.describe();
}

// ---------------------------------------------------------------------------
// 3. TMIN2 Permission Checking
// ---------------------------------------------------------------------------
// The TMIN2 gate (ternary minimum of two inputs) is the hardware
// primitive for permission checking:
//
//   access = process_ring tand required_ring
//
// If the result equals the required ring, access is granted.
// If the result is lower, access is denied.
//
// This is ONE GATE, ONE CYCLE. No comparison tree, no subtraction.

fn check_access(process_ring: trit, required_ring: trit) -> trit {
    // TMIN2 gate: min(process_ring, required_ring)
    let result = process_ring tand required_ring;
    // Access granted if result == required_ring.
    if result == required_ring {
        return +;   // GRANT
    }
    return -;       // DENY
}

fn demo_permission_check() {
    heading("3. TMIN2 Gate Permission Checking");

    io::println("  TMIN2 gate: access = process_ring tand required_ring");
    io::println("  If result == required_ring: GRANT.  Otherwise: DENY.");
    io::println("  One gate. One cycle. Complete security check.");
    io::println("");

    // Show the full access matrix.
    io::println("  Access matrix (process ring vs required ring):");
    io::println("");
    io::println("                     Required Ring");
    io::println("                     Kernel(+)  Service(0)  User(-)");

    let rings: [trit] = [+, 0, -];
    let ring_labels: [str] = ["Kernel (+)", "Service(0)", "User   (-)"];

    let mut ri = 0;
    for proc_ring in rings {
        io::print("    ");
        io::print(ring_labels[ri]);
        io::print(":");

        for req_ring in rings {
            let result = check_access(proc_ring, req_ring);
            io::print("    ");
            io::print(perm_str(result));
        }
        io::println("");
        ri = ri + 1;
    }

    io::println("");
    io::println("  Key insight: kernel (+) can access everything (TMIN2 always passes).");
    io::println("  User (-) can only access user-level resources.");
    io::println("  Service (0) can access service and user, but not kernel.");
    io::println("  This is exactly the principle of least privilege, in one trit.");
}

// ---------------------------------------------------------------------------
// 4. Capability Attenuation
// ---------------------------------------------------------------------------
// A process can LOWER the permissions on a capability before passing
// it to another process, but can NEVER raise them.
// This is enforced by tand (min) — you can only go down, never up.
//
// attenuation(cap_perm, desired_perm) = cap_perm tand desired_perm
// Since tand = min, the result is always <= original.

fn attenuate_cap(cap: Capability, new_read: trit, new_write: trit,
                 new_exec: trit, new_owner: int) -> Capability {
    return Capability::new(
        cap.id,
        cap.resource,
        cap.read_perm  tand new_read,    // can only lower
        cap.write_perm tand new_write,   // can only lower
        cap.exec_perm  tand new_exec,    // can only lower
        cap.min_ring,                     // ring level doesn't change
        new_owner,
    );
}

fn demo_attenuation() {
    heading("4. Capability Attenuation — Privileges Can Only Decrease");

    io::println("  Attenuation uses tand (min) — the privilege can only go down.");
    io::println("  A kernel process can give a service process a read-only view.");
    io::println("  The service process CANNOT escalate it back to read-write.");
    io::println("");

    // Start with a full-access capability.
    let full_cap = Capability::new(10, "/dev/mem", +, +, +, +, 0);
    io::println("  Original capability (kernel, full access):");
    full_cap.describe();

    // Attenuate: kernel gives service a read-only view.
    let service_cap = attenuate_cap(full_cap, +, -, -, 1);
    io::println("  After attenuation (read-only, given to service):");
    service_cap.describe();

    // Attenuate further: service gives user even less.
    let user_cap = attenuate_cap(service_cap, 0, -, -, 3);
    io::println("  After further attenuation (partial read, given to user):");
    user_cap.describe();

    io::println("");
    io::println("  Attempting to ESCALATE (user tries to grant write):");
    // Even if user tries to set write to +, tand clamps it.
    let escalated = attenuate_cap(user_cap, +, +, +, 3);
    io::println("  Result (tand prevents escalation):");
    escalated.describe();
    io::println("  Write permission is still -, because tand(-, +) = -.");
    io::println("  Escalation is MATHEMATICALLY IMPOSSIBLE with tand.");
}

// ---------------------------------------------------------------------------
// 5. Capability Delegation
// ---------------------------------------------------------------------------
fn demo_delegation() {
    heading("5. Capability Delegation — Transferring Access");

    io::println("  A process can delegate a capability to another process.");
    io::println("  The delegated capability can only have EQUAL or FEWER permissions.");
    io::println("");

    let cap_table: Vec<Capability> = Vec::new();

    // Kernel creates a network capability.
    let net_cap = Capability::new(20, "/dev/net", +, +, -, +, 0);
    io::println("  Step 1: Kernel (PID 0) creates network capability:");
    net_cap.describe();
    cap_table.push(net_cap);

    // Kernel delegates to network service (attenuated: no write).
    let svc_cap = attenuate_cap(net_cap, +, 0, -, 2);
    io::println("  Step 2: Kernel delegates to netstack (PID 2), read-full, write-partial:");
    svc_cap.describe();
    cap_table.push(svc_cap);

    // Network service delegates to user browser (read-only).
    let user_cap = attenuate_cap(svc_cap, +, -, -, 4);
    io::println("  Step 3: Netstack delegates to browser (PID 4), read-only:");
    user_cap.describe();
    cap_table.push(user_cap);

    io::println("");
    io::println("  Delegation chain:");
    io::println("    Kernel (+, +, -)  --attenuate-->  Service (+, 0, -)  --attenuate-->  User (+, -, -)");
    io::println("    Permissions can only decrease at each step.");
    io::println("    The tand gate enforces this at the hardware level.");

    // Show the capability table.
    io::println("");
    io::println("  System capability table:");
    let mut i = 0;
    while i < cap_table.len() {
        cap_table.get(i).describe();
        i = i + 1;
    }
}

// ---------------------------------------------------------------------------
// 6. Revocation
// ---------------------------------------------------------------------------
fn demo_revocation() {
    heading("6. Capability Revocation — Setting All Trits to -");

    io::println("  To revoke a capability, set all permission trits to -.");
    io::println("  This is a single trit-fill operation (tnot of all-+ is all-).");
    io::println("");

    let cap = Capability::new(30, "/home/secret", +, +, -, -, 3);
    io::println("  Before revocation:");
    cap.describe();

    // Revoke: set all permissions to -.
    let revoked = Capability::new(cap.id, cap.resource, -, -, -, cap.min_ring, cap.owner_pid);
    io::println("  After revocation:");
    revoked.describe();

    io::println("");
    io::println("  Verification: all access checks now fail.");
    let rings: [trit] = [+, 0, -];
    let ring_labels: [str] = ["Kernel ", "Service", "User   "];
    let mut ri = 0;
    for ring in rings {
        let read_ok = check_access(ring, revoked.min_ring);
        // Even if ring check passes, the permission trits are all -.
        let effective = revoked.read_perm tand ring;
        io::print("    ");
        io::print(ring_labels[ri]);
        io::print(" tries read: perm=");
        io::print_trit(revoked.read_perm);
        io::print(", effective=");
        io::print_trit(effective);
        io::print("  => ");
        tif effective {
            + => io::println("granted"),
            0 => io::println("granted"),
            - => io::println("DENIED"),
        }
        ri = ri + 1;
    }
}

// ---------------------------------------------------------------------------
// 7. Why Three Rings, Not Four
// ---------------------------------------------------------------------------
fn demo_why_three() {
    heading("7. Why Three Rings are Sufficient");

    io::println("  x86 has four rings (0, 1, 2, 3).");
    io::println("  In practice, only rings 0 and 3 are used.");
    io::println("  Rings 1 and 2 are wasted — complexity for nothing.");
    io::println("");
    io::println("  thatteOS has three rings (+, 0, -).");
    io::println("  ALL THREE are actively used:");
    io::println("    + (Kernel):  memory management, interrupt handling");
    io::println("    0 (Service): device drivers, file system, network stack");
    io::println("    - (User):    applications, shells, user code");
    io::println("");
    io::println("  Three levels map directly to three operational concerns:");
    io::println("    + = hardware control  (must not be accessible from outside)");
    io::println("    0 = system services   (mediate between hardware and apps)");
    io::println("    - = user computation  (sandboxed, least privilege)");
    io::println("");
    io::println("  In hardware:");
    io::println("    Binary ring check:   subtract + compare (multi-cycle)");
    io::println("    Ternary ring check:  one trit comparison (one TMIN2 gate)");
    io::println("    The security boundary is enforced in a single clock cycle.");
    io::println("");
    io::println("  Capability attenuation: tand(current, desired) = min");
    io::println("  Privilege can only flow downward: + -> 0 -> -");
    io::println("  Escalation is impossible without a new grant from a higher ring.");
}

// ---------------------------------------------------------------------------
// Main
// ---------------------------------------------------------------------------
fn main() {
    io::println("maniT Capability-Based Security Demo");
    io::println("=====================================");
    io::println("Authored by: Manish Jagdish Thatte");

    demo_process_rings();
    demo_capabilities();
    demo_permission_check();
    demo_attenuation();
    demo_delegation();
    demo_revocation();
    demo_why_three();

    io::println("");
    io::println("Demo complete.");
}
