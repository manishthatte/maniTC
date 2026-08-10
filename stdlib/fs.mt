// stdlib/std/fs.mt
// Filesystem access for maniT.
//
// Provides file reading/writing, directory traversal, and path manipulation.
// All paths are UTF-8 strings.  Relative paths are resolved against the
// process working directory (see std::env::cwd).
//
// Usage:
//   use std::fs;
//   let data = fs::read_file("config.txt");

// ---------------------------------------------------------------------------
// Path utilities
// ---------------------------------------------------------------------------

// Represents a filesystem path.  Thin wrapper around str with path-aware
// helpers.  All raw-string path arguments in this module accept either
// a str or a Path interchangeably.
struct Path {
    raw: str,
}

impl Path {
    // Construct a Path from a string.
    fn new(s: str) -> Path ;  // native

    // Return the raw string representation.
    fn to_str(self) -> str ;  // native

    // Join this path with a child segment (appending a separator as needed).
    fn join(self, child: str) -> Path ;  // native

    // Return the parent directory, or the empty path if there is no parent.
    fn parent(self) -> Path ;  // native

    // Return the final component of the path (the file or directory name).
    fn file_name(self) -> str ;  // native

    // Return the file extension (without the leading dot), or "" if absent.
    fn extension(self) -> str ;  // native

    // Return the file stem (file name without extension).
    fn stem(self) -> str ;  // native

    // Return true if the path is absolute.
    fn is_absolute(self) -> bool ;  // native

    // Return the absolute (canonicalised) form of the path.  Panics if the
    // path does not exist in the filesystem.
    fn canonicalize(self) -> Path ;  // native

    // Return true if the path components indicate a directory (trailing /).
    fn is_dir_path(self) -> bool ;  // native
}

// Convenience free function: join two path strings.
fn path_join(base: str, child: str) -> str ;  // native

// Return the file name component of a path string.
fn path_file_name(p: str) -> str ;  // native

// Return the extension of a path string.
fn path_extension(p: str) -> str ;  // native

// Return the parent directory of a path string.
fn path_parent(p: str) -> str ;  // native

// ---------------------------------------------------------------------------
// File metadata
// ---------------------------------------------------------------------------

// Metadata about a filesystem entry.
struct Metadata {
    // True if the entry is a regular file.
    is_file: bool,
    // True if the entry is a directory.
    is_dir: bool,
    // True if the entry is a symbolic link.
    is_symlink: bool,
    // File size in bytes.
    size: int,
    // Last-modified timestamp as seconds since Unix epoch.
    modified: int,
    // Creation timestamp, or 0 if not available on this OS.
    created: int,
    // POSIX permission bits (octal), or 0 on Windows.
    permissions: int,
}

// Return metadata for the entry at `path`.  Panics if not found.
fn metadata(path: str) -> Metadata ;  // native

// Return true if a file or directory exists at `path`.
fn exists(path: str) -> bool ;  // native

// Return true if `path` is an existing regular file.
fn is_file(path: str) -> bool ;  // native

// Return true if `path` is an existing directory.
fn is_dir(path: str) -> bool ;  // native

// ---------------------------------------------------------------------------
// File read / write
// ---------------------------------------------------------------------------

// Read the entire contents of `path` as a UTF-8 string.  Panics on I/O error.
fn read_file(path: str) -> str ;  // native

// Read the entire contents of `path` as a raw byte array.
fn read_bytes(path: str) -> [int] ;  // native

// Write `content` to `path`, creating or truncating the file.
fn write_file(path: str, content: str) ;  // native

// Write a raw byte array to `path`.
fn write_bytes(path: str, data: [int]) ;  // native

// Append `content` to `path`, creating the file if it does not exist.
fn append_file(path: str, content: str) ;  // native

// Copy the file at `src` to `dst`.  Overwrites `dst` if it exists.
fn copy_file(src: str, dst: str) ;  // native

// Move (rename) the entry at `src` to `dst`.
fn rename(src: str, dst: str) ;  // native

// Delete the file at `path`.  Panics if not found or if `path` is a dir.
fn remove_file(path: str) ;  // native

// ---------------------------------------------------------------------------
// Directory operations
// ---------------------------------------------------------------------------

// Represents an open directory.
struct Dir {
    path: str,
}

impl Dir {
    // Open the directory at `path`.  Panics if not found.
    fn open(path: str) -> Dir ;  // native

    // Return a Vec of the names (not full paths) of the immediate children.
    fn entries(self) -> Vec<str> ;  // native

    // Return a Vec of full paths of the immediate children.
    fn entry_paths(self) -> Vec<str> ;  // native

    // Return a Vec of full paths of all descendants (recursive).
    fn walk(self) -> Vec<str> ;  // native

    // Return the path this Dir was opened from.
    fn path(self) -> str ;  // native

    // Close the directory handle.
    fn close(self) ;  // native
}

// List the immediate children of `path`.  Returns names, not full paths.
fn list_dir(path: str) -> Vec<str> ;  // native

// List full paths of all entries under `path` recursively.
fn walk_dir(path: str) -> Vec<str> ;  // native

// Create the directory at `path`.  Panics if it already exists.
fn create_dir(path: str) ;  // native

// Create `path` and all missing ancestor directories.
fn create_dir_all(path: str) ;  // native

// Remove the empty directory at `path`.  Panics if not empty.
fn remove_dir(path: str) ;  // native

// Remove `path` and all of its contents recursively.  Use with caution.
fn remove_dir_all(path: str) ;  // native

// ---------------------------------------------------------------------------
// File — buffered read/write handle
// ---------------------------------------------------------------------------

// Open mode for File::open.
enum OpenMode {
    // Open for reading only.  File must exist.
    Read,
    // Open for writing only.  Creates or truncates the file.
    Write,
    // Open for appending.  Creates file if absent.
    Append,
    // Open for both reading and writing.  File must exist.
    ReadWrite,
    // Create new file for reading and writing.  Fails if file exists.
    CreateNew,
}

// A buffered file handle supporting sequential reads and writes.
struct File {
    // native OS file descriptor + buffer
}

impl File {
    // Open `path` with the given mode.  Panics on error.
    fn open(path: str, mode: OpenMode) -> File ;  // native

    // Read up to `n` bytes from the current position.  Returns fewer at EOF.
    fn read(self, n: int) -> str ;  // native

    // Read a complete line (up to and including '\n').  Returns "" at EOF.
    fn read_line(self) -> str ;  // native

    // Read all remaining content from the current position to EOF.
    fn read_all(self) -> str ;  // native

    // Write `s` to the file at the current position.
    fn write(self, s: str) ;  // native

    // Write `s` followed by a newline.
    fn writeln(self, s: str) ;  // native

    // Move the file cursor to byte offset `pos` from the start.
    fn seek(self, pos: int) ;  // native

    // Return the current byte offset of the file cursor.
    fn tell(self) -> int ;  // native

    // Return the total file size in bytes.
    fn size(self) -> int ;  // native

    // Flush buffered data to the OS.
    fn flush(self) ;  // native

    // Close the file handle, flushing any pending writes.
    fn close(self) ;  // native

    // Return the path this file was opened from.
    fn path(self) -> str ;  // native
}

// ---------------------------------------------------------------------------
// Temporary files
// ---------------------------------------------------------------------------

// Create a temporary file and return its path.  The file is empty.
// The caller is responsible for deleting it when done.
fn temp_file() -> str ;  // native

// Create a temporary directory and return its path.
fn temp_dir() -> str ;  // native
