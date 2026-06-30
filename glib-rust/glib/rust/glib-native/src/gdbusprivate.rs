//! gdbusprivate matching `gio/gdbusprivate.c`.
//!
//! Private D-Bus utilities including hexdump, worker thread management,
//! and message read/write helpers. The C implementation manages GDBusWorker
//! threads for reading/writing D-Bus messages over sockets with support for
//! ancillary messages (file descriptors).
//!
//! In this no_std port, we implement the hexdump utility and model the
//! worker state machine. Actual socket I/O and threading are deferred.
//!
//! Fully `no_std` compatible using `alloc`.

use alloc::string::{String, ToString};
use alloc::vec::Vec;
use spin::Mutex;

/// Hexdump a byte buffer with the given indentation.
///
/// Produces output like:
/// ```text
///   0000: 48 65 6c 6c 6f 20 57 6f  72 6c 64 0a 00 00 00 00    Hello World.....
/// ```
///
/// Mirrors `_g_dbus_hexdump`.
pub fn hexdump(data: &[u8], indent: usize) -> String {
    let mut result = String::new();
    let indent_str: String = (0..indent).map(|_| ' ').collect();

    let mut n = 0;
    while n < data.len() {
        // Offset
        result.push_str(&indent_str);
        result.push_str(&format!("{:04x}: ", n));

        // Hex bytes
        for m in n..n + 16 {
            if m > n && (m % 4) == 0 {
                result.push(' ');
            }
            if m < data.len() {
                result.push_str(&format!("{:02x} ", data[m]));
            } else {
                result.push_str("   ");
            }
        }

        result.push_str("   ");

        // ASCII representation
        for m in n..core::cmp::min(n + 16, data.len()) {
            let c = data[m];
            if c >= 32 && c <= 126 {
                result.push(c as char);
            } else {
                result.push('.');
            }
        }

        result.push('\n');
        n += 16;
    }

    result
}

/// D-Bus worker state.
///
/// Models the `GDBusWorker` struct. In the C implementation, this manages
/// a D-Bus connection's read/write loop, including message framing,
/// authentication, and ancillary message handling.
pub struct DBusWorker {
    /// The unique D-Bus address being communicated with.
    address: String,
    /// Whether the worker has been closed.
    closed: Mutex<bool>,
    /// Whether authentication has completed.
    authenticated: Mutex<bool>,
    /// Pending messages to be sent.
    pending_write: Mutex<Vec<Vec<u8>>>,
    /// Buffer for incomplete reads.
    read_buffer: Mutex<Vec<u8>>,
}

impl DBusWorker {
    /// Creates a new D-Bus worker for the given address.
    pub fn new(address: &str) -> Self {
        Self {
            address: address.to_string(),
            closed: Mutex::new(false),
            authenticated: Mutex::new(false),
            pending_write: Mutex::new(Vec::new()),
            read_buffer: Mutex::new(Vec::new()),
        }
    }

    /// Returns the D-Bus address.
    pub fn address(&self) -> &str {
        &self.address
    }

    /// Returns whether the worker is closed.
    pub fn is_closed(&self) -> bool {
        *self.closed.lock()
    }

    /// Closes the worker.
    pub fn close(&self) {
        *self.closed.lock() = true;
    }

    /// Returns whether authentication is complete.
    pub fn is_authenticated(&self) -> bool {
        *self.authenticated.lock()
    }

    /// Marks authentication as complete.
    pub fn set_authenticated(&self) {
        *self.authenticated.lock() = true;
    }

    /// Queues a message for writing.
    pub fn queue_write(&self, data: Vec<u8>) {
        self.pending_write.lock().push(data);
    }

    /// Drains pending write messages.
    pub fn drain_pending_writes(&self) -> Vec<Vec<u8>> {
        core::mem::take(&mut *self.pending_write.lock())
    }

    /// Appends data to the read buffer.
    pub fn append_read(&self, data: &[u8]) {
        self.read_buffer.lock().extend_from_slice(data);
    }

    /// Returns the current read buffer contents.
    pub fn read_buffer(&self) -> Vec<u8> {
        self.read_buffer.lock().clone()
    }

    /// Consumes `n` bytes from the read buffer.
    pub fn consume_read(&self, n: usize) {
        let mut buf = self.read_buffer.lock();
        if n <= buf.len() {
            buf.drain(..n);
        } else {
            buf.clear();
        }
    }
}

/// Reads from a socket with control messages.
///
/// In the C implementation, this uses `g_socket_receive_message` to read
/// data and ancillary messages (file descriptors). In this no_std port,
/// we model it as a simple buffer read.
///
/// Mirrors `_g_socket_read_with_control_messages`.
pub fn socket_read_with_control_messages(
    worker: &DBusWorker,
    buffer: &mut [u8],
) -> Result<usize, String> {
    if worker.is_closed() {
        return Err("worker is closed".to_string());
    }
    let read_buf = worker.read_buffer();
    let n = core::cmp::min(buffer.len(), read_buf.len());
    buffer[..n].copy_from_slice(&read_buf[..n]);
    worker.consume_read(n);
    Ok(n)
}

/// Writes data to the D-Bus connection.
///
/// Mirrors the write path of `GDBusWorker`.
pub fn socket_write(worker: &DBusWorker, data: &[u8]) -> Result<(), String> {
    if worker.is_closed() {
        return Err("worker is closed".to_string());
    }
    worker.queue_write(data.to_vec());
    Ok(())
}

/// Normalizes a D-Bus interface name.
///
/// D-Bus interface names must contain at least one dot and consist of
/// alphanumeric characters and underscores.
pub fn is_valid_interface_name(name: &str) -> bool {
    if name.is_empty() || name.len() > 255 {
        return false;
    }
    if !name.contains('.') {
        return false;
    }
    for part in name.split('.') {
        if part.is_empty() {
            return false;
        }
        if !part.chars().next().unwrap().is_ascii_alphabetic()
            && part.chars().next().unwrap() != '_'
        {
            return false;
        }
        if !part.chars().all(|c| c.is_ascii_alphanumeric() || c == '_') {
            return false;
        }
    }
    true
}

/// Normalizes a D-Bus bus name.
///
/// Bus names follow similar rules to interface names but can also
/// start with ':' for unique names.
pub fn is_valid_bus_name(name: &str) -> bool {
    if name.is_empty() || name.len() > 255 {
        return false;
    }
    if name.starts_with(':') {
        // Unique name
        let rest = &name[1..];
        if !rest.contains('.') {
            return false;
        }
        for part in rest.split('.') {
            if part.is_empty() {
                return false;
            }
            if !part.chars().all(|c| c.is_ascii_alphanumeric() || c == '_') {
                return false;
            }
        }
        true
    } else {
        // Well-known name
        is_valid_interface_name(name)
    }
}

/// Checks if a string is a valid D-Bus object path.
///
/// Object paths must start with '/', consist of alphanumeric characters
/// and underscores separated by '/'.
pub fn is_valid_object_path(path: &str) -> bool {
    if path.is_empty() || !path.starts_with('/') {
        return false;
    }
    if path == "/" {
        return true;
    }
    if path.ends_with('/') {
        return false;
    }
    for part in path[1..].split('/') {
        if part.is_empty() {
            return false;
        }
        if !part.chars().all(|c| c.is_ascii_alphanumeric() || c == '_') {
            return false;
        }
    }
    true
}

/// Checks if a string is a valid D-Bus member name.
///
/// Member names must not be empty, must start with an alphabetic character
/// or underscore, and consist of alphanumeric characters and underscores.
pub fn is_valid_member_name(name: &str) -> bool {
    if name.is_empty() || name.len() > 255 {
        return false;
    }
    let first = name.chars().next().unwrap();
    if !first.is_ascii_alphabetic() && first != '_' {
        return false;
    }
    name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')
}

// ──────────────────────────── Tests ───────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_hexdump() {
        let data = b"Hello World";
        let result = hexdump(data, 0);
        assert!(result.contains("0000:"));
        assert!(result.contains("48 65 6c 6c"));
        assert!(result.contains("Hello World"));
    }

    #[test]
    fn test_hexdump_with_indent() {
        let data = b"AB";
        let result = hexdump(data, 2);
        assert!(result.starts_with("  0000:"));
    }

    #[test]
    fn test_hexdump_empty() {
        let result = hexdump(&[], 0);
        assert!(result.is_empty());
    }

    #[test]
    fn test_worker_basic() {
        let worker = DBusWorker::new("unix:abstract=/tmp/dbus-test");
        assert_eq!(worker.address(), "unix:abstract=/tmp/dbus-test");
        assert!(!worker.is_closed());
        assert!(!worker.is_authenticated());

        worker.set_authenticated();
        assert!(worker.is_authenticated());

        worker.close();
        assert!(worker.is_closed());
    }

    #[test]
    fn test_worker_read_write() {
        let worker = DBusWorker::new("unix:path=/tmp/dbus");
        worker.append_read(b"hello");
        let mut buf = [0u8; 10];
        let n = socket_read_with_control_messages(&worker, &mut buf).unwrap();
        assert_eq!(n, 5);
        assert_eq!(&buf[..5], b"hello");

        socket_write(&worker, b"world").unwrap();
        let pending = worker.drain_pending_writes();
        assert_eq!(pending.len(), 1);
        assert_eq!(pending[0], b"world");
    }

    #[test]
    fn test_worker_closed_errors() {
        let worker = DBusWorker::new("unix:path=/tmp/dbus");
        worker.close();
        let mut buf = [0u8; 10];
        assert!(socket_read_with_control_messages(&worker, &mut buf).is_err());
        assert!(socket_write(&worker, b"data").is_err());
    }

    #[test]
    fn test_valid_interface_name() {
        assert!(is_valid_interface_name("org.freedesktop.DBus"));
        assert!(is_valid_interface_name("org.gtk.Application"));
        assert!(!is_valid_interface_name("invalid"));
        assert!(!is_valid_interface_name(""));
        assert!(!is_valid_interface_name("org..DBus"));
        assert!(!is_valid_interface_name(".org.DBus"));
        assert!(!is_valid_interface_name("org.DBus."));
        assert!(!is_valid_interface_name("org.123DBus"));
    }

    #[test]
    fn test_valid_bus_name() {
        assert!(is_valid_bus_name("org.freedesktop.DBus"));
        assert!(is_valid_bus_name(":1.42"));
        assert!(!is_valid_bus_name(""));
        assert!(!is_valid_bus_name("invalid"));
    }

    #[test]
    fn test_valid_object_path() {
        assert!(is_valid_object_path("/"));
        assert!(is_valid_object_path("/org/freedesktop/DBus"));
        assert!(!is_valid_object_path(""));
        assert!(!is_valid_object_path("org"));
        assert!(!is_valid_object_path("/org/"));
        assert!(!is_valid_object_path("/org//DBus"));
    }

    #[test]
    fn test_valid_member_name() {
        assert!(is_valid_member_name("Activate"));
        assert!(is_valid_member_name("_private_method"));
        assert!(!is_valid_member_name(""));
        assert!(!is_valid_member_name("123method"));
        assert!(!is_valid_member_name("method-name"));
    }
}
