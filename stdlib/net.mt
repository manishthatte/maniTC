// stdlib/std/net.mt
// Networking primitives for maniT.
//
// Provides TCP socket I/O and a basic HTTP client.  All blocking operations
// have async counterparts (prefixed with `async`).  See std::async for the
// runtime that drives async tasks.
//
// Usage:
//   use std::net;
//   use std::async;
//   let listener = net::TcpListener::bind("0.0.0.0:8080");

// ---------------------------------------------------------------------------
// Address types
// ---------------------------------------------------------------------------

// A resolved IPv4 or IPv6 socket address (host + port).
struct SocketAddr {
    host: str,
    port: int,
}

impl SocketAddr {
    // Parse a "host:port" string.  Panics on invalid format.
    fn parse(addr: str) -> SocketAddr { /* native */ }

    // Construct from explicit host and port.
    fn new(host: str, port: int) -> SocketAddr { /* native */ }

    // Return the string representation "host:port".
    fn to_str(self) -> str { /* native */ }
}

// ---------------------------------------------------------------------------
// TcpListener — accept incoming TCP connections
// ---------------------------------------------------------------------------

// A TCP server socket bound to a local address.
struct TcpListener {
    // native OS socket handle
}

impl TcpListener {
    // Bind to `addr` and start listening.  Panics on OS error.
    fn bind(addr: str) -> TcpListener { /* native */ }

    // Block until a client connects, returning the stream and remote address.
    fn accept(self) -> (TcpStream, SocketAddr) { /* native */ }

    // Asynchronously wait for a client connection.
    async fn accept_async(self) -> (TcpStream, SocketAddr) { /* native */ }

    // Return the local address this listener is bound to.
    fn local_addr(self) -> SocketAddr { /* native */ }

    // Close the listener, releasing the bound port.
    fn close(self) { /* native */ }
}

// ---------------------------------------------------------------------------
// TcpStream — bidirectional TCP connection
// ---------------------------------------------------------------------------

// An established TCP connection to a remote peer.
struct TcpStream {
    // native OS socket handle
}

impl TcpStream {
    // Connect to `addr`, blocking until the connection is established.
    fn connect(addr: str) -> TcpStream { /* native */ }

    // Asynchronously connect to `addr`.
    async fn connect_async(addr: str) -> TcpStream { /* native */ }

    // Read up to `n` bytes from the stream.  Returns fewer bytes at EOF.
    fn read(self, n: int) -> [int] { /* native */ }

    // Read exactly `n` bytes, blocking until all are available.  Panics at EOF.
    fn read_exact(self, n: int) -> [int] { /* native */ }

    // Read until `delim` byte is encountered; returns the data including delim.
    fn read_until(self, delim: int) -> [int] { /* native */ }

    // Read a complete UTF-8 line (reads until '\n').
    fn read_line(self) -> str { /* native */ }

    // Asynchronously read up to `n` bytes.
    async fn read_async(self, n: int) -> [int] { /* native */ }

    // Write `data` bytes to the stream.  Blocks until all bytes are sent.
    fn write(self, data: [int]) { /* native */ }

    // Write a UTF-8 string to the stream.
    fn write_str(self, s: str) { /* native */ }

    // Asynchronously write `data` bytes.
    async fn write_async(self, data: [int]) { /* native */ }

    // Flush any OS-level send buffer.
    fn flush(self) { /* native */ }

    // Return the remote peer address.
    fn peer_addr(self) -> SocketAddr { /* native */ }

    // Return the local address used for this connection.
    fn local_addr(self) -> SocketAddr { /* native */ }

    // Set the read/write timeout (milliseconds, 0 = no timeout).
    fn set_timeout(self, ms: int) { /* native */ }

    // Shut down one or both directions.  direction: 0=read, 1=write, 2=both.
    fn shutdown(self, direction: int) { /* native */ }

    // Close the connection.
    fn close(self) { /* native */ }
}

// ---------------------------------------------------------------------------
// HttpClient — simple HTTP/1.1 client
// ---------------------------------------------------------------------------

// Configuration for an HttpClient instance.
struct HttpClientConfig {
    // Timeout for the entire request in milliseconds (0 = no timeout).
    timeout_ms: int,
    // Maximum number of redirects to follow (default 10).
    max_redirects: int,
    // User-Agent header value.
    user_agent: str,
    // Whether to verify TLS certificates (default true).
    verify_tls: bool,
}

impl HttpClientConfig {
    // Create a config with sensible defaults.
    fn default() -> HttpClientConfig { /* native */ }
}

// An HTTP client capable of making GET, POST, PUT, DELETE, etc. requests.
struct HttpClient {
    // native connection pool
}

impl HttpClient {
    // Create a client with default configuration.
    fn new() -> HttpClient { /* native */ }

    // Create a client with the given configuration.
    fn with_config(cfg: HttpClientConfig) -> HttpClient { /* native */ }

    // Perform a synchronous GET request.
    fn get(self, url: str) -> HttpResponse { /* native */ }

    // Perform a synchronous POST request with a string body.
    fn post(self, url: str, body: str) -> HttpResponse { /* native */ }

    // Perform a synchronous POST request with a JSON body.
    fn post_json(self, url: str, json_body: str) -> HttpResponse { /* native */ }

    // Perform a synchronous PUT request.
    fn put(self, url: str, body: str) -> HttpResponse { /* native */ }

    // Perform a synchronous DELETE request.
    fn delete(self, url: str) -> HttpResponse { /* native */ }

    // Perform a request with a custom method, headers map, and optional body.
    fn request(self, method: str, url: str, headers: Map<str, str>, body: str) -> HttpResponse { /* native */ }

    // Async variants.
    async fn get_async(self, url: str) -> HttpResponse { /* native */ }
    async fn post_async(self, url: str, body: str) -> HttpResponse { /* native */ }

    // Add a persistent header sent with every request.
    fn set_default_header(self, key: str, val: str) { /* native */ }

    // Remove a persistent header.
    fn remove_default_header(self, key: str) { /* native */ }
}

// ---------------------------------------------------------------------------
// HttpResponse
// ---------------------------------------------------------------------------

// The response returned by HttpClient methods.
struct HttpResponse {
    // HTTP status code (e.g. 200, 404).
    status: int,
    // Response headers as a key→value map.
    headers: Map<str, str>,
    // Response body as a UTF-8 string.
    body: str,
}

impl HttpResponse {
    // Return true if the status code indicates success (200–299).
    fn is_ok(self) -> bool { /* native */ }

    // Return the value of a specific response header, or "" if absent.
    fn header(self, key: str) -> str { /* native */ }

    // Return the Content-Type header value.
    fn content_type(self) -> str { /* native */ }

    // Return the body length in bytes.
    fn body_len(self) -> int { /* native */ }
}

// ---------------------------------------------------------------------------
// Convenience free functions
// ---------------------------------------------------------------------------

// Perform a quick one-off GET and return the body string.  Panics on error.
fn get(url: str) -> str { /* native */ }

// Perform a quick one-off POST and return the response body.
fn post(url: str, body: str) -> str { /* native */ }

// Resolve a hostname to its IP addresses.
fn resolve(host: str) -> Vec<str> { /* native */ }
