// stdlib/std/env.mt
// Process environment and runtime information for maniT.
//
// Provides access to command-line arguments, environment variables, the
// working directory, and basic process control.
//
// Usage:
//   use std::env;
//   let argv = env::args();
//   let home = env::get_env("HOME");

// ---------------------------------------------------------------------------
// Command-line arguments
// ---------------------------------------------------------------------------

// Return the command-line arguments as a Vec of strings.
// Index 0 is the program name; subsequent indices are the arguments passed
// by the user.
fn args() -> Vec<str> { /* native */ }

// Return the number of command-line arguments (including argv[0]).
fn argc() -> int { /* native */ }

// Return argument at position `i`.  Panics if i >= argc().
fn arg(i: int) -> str { /* native */ }

// ---------------------------------------------------------------------------
// Environment variables
// ---------------------------------------------------------------------------

// Return the value of environment variable `key`.
// Returns Ok(value) if the variable is set, Err("not found") otherwise.
fn get_env(key: str) -> Result<str, str> { /* native */ }

// Return the value of `key`, or `default` if the variable is not set.
fn get_env_or(key: str, default: str) -> str { /* native */ }

// Set the environment variable `key` to `value` for this process.
// Does not affect the parent process's environment.
fn set_env(key: str, value: str) { /* native */ }

// Remove the environment variable `key`.  No-op if not set.
fn unset_env(key: str) { /* native */ }

// Return true if the environment variable `key` is currently set.
fn has_env(key: str) -> bool { /* native */ }

// Return a Map of all environment variables.
fn env_vars() -> Map<str, str> { /* native */ }

// ---------------------------------------------------------------------------
// Working directory
// ---------------------------------------------------------------------------

// Return the current working directory as an absolute path string.
fn cwd() -> str { /* native */ }

// Change the current working directory to `path`.  Panics on error.
fn set_cwd(path: str) { /* native */ }

// ---------------------------------------------------------------------------
// Process information
// ---------------------------------------------------------------------------

// Return the process ID (PID) of the current process.
fn pid() -> int { /* native */ }

// Return the parent process ID (PPID).
fn ppid() -> int { /* native */ }

// Return the name of the host operating system ("linux", "macos", "windows").
fn os_name() -> str { /* native */ }

// Return the CPU architecture ("x86_64", "aarch64", "ternary", …).
fn arch() -> str { /* native */ }

// Return the number of logical CPU cores available to this process.
fn cpu_count() -> int { /* native */ }

// Return the total physical RAM in bytes.
fn total_memory() -> int { /* native */ }

// Return the approximate memory currently used by this process in bytes.
fn used_memory() -> int { /* native */ }

// Return the maniT compiler / runtime version string (e.g. "0.1.0").
fn manit_version() -> str { /* native */ }

// Return the path to the currently executing binary.
fn executable_path() -> str { /* native */ }

// ---------------------------------------------------------------------------
// Process control
// ---------------------------------------------------------------------------

// Exit the process immediately with the given exit code.
// Exit code 0 conventionally indicates success.
fn exit(code: int) { /* native */ }

// Abort the process with a non-zero exit code and a message on stderr.
fn abort(message: str) { /* native */ }

// Register a function to be called when the process exits normally.
// Multiple handlers are called in reverse registration order.
fn on_exit(handler: fn()) { /* native */ }

// ---------------------------------------------------------------------------
// Standard paths
// ---------------------------------------------------------------------------

// Return the user's home directory (e.g. "/home/user").
fn home_dir() -> str { /* native */ }

// Return a suitable directory for temporary files.
fn temp_dir() -> str { /* native */ }

// Return the path to the maniT package cache (where trit downloads packages).
fn cache_dir() -> str { /* native */ }

// Return the path to the user's config directory.
fn config_dir() -> str { /* native */ }
