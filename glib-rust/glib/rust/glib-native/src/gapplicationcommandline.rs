//! GApplicationCommandLine matching `gio/gapplicationcommandline.h`.
//!
//! A command-line invocation of a `GApplication`. In this no_std port
//! we model it with arguments, environment, and stdin/stdout buffers.
//!
//! Fully `no_std` compatible using `alloc`.

use alloc::collections::BTreeMap;
use alloc::string::{String, ToString};
use alloc::vec::Vec;
use spin::Mutex;

/// A command-line invocation (`GApplicationCommandLine`).
pub struct ApplicationCommandLine {
    arguments: Mutex<Vec<String>>,
    environ: Mutex<BTreeMap<String, String>>,
    stdin_data: Mutex<Vec<u8>>,
    stdout_data: Mutex<Vec<u8>>,
    is_remote: Mutex<bool>,
    done: Mutex<bool>,
}

impl ApplicationCommandLine {
    /// Creates a new command-line from arguments.
    pub fn new(arguments: Vec<String>) -> Self {
        Self {
            arguments: Mutex::new(arguments),
            environ: Mutex::new(BTreeMap::new()),
            stdin_data: Mutex::new(Vec::new()),
            stdout_data: Mutex::new(Vec::new()),
            is_remote: Mutex::new(false),
            done: Mutex::new(false),
        }
    }

    /// Gets the arguments.
    pub fn get_arguments(&self) -> Vec<String> {
        self.arguments.lock().clone()
    }

    /// Sets an environment variable.
    pub fn setenv(&self, key: &str, value: &str) {
        self.environ
            .lock()
            .insert(key.to_string(), value.to_string());
    }

    /// Gets an environment variable.
    pub fn getenv(&self, key: &str) -> Option<String> {
        self.environ.lock().get(key).cloned()
    }

    /// Gets the stdin data.
    pub fn get_stdin_data(&self) -> Vec<u8> {
        self.stdin_data.lock().clone()
    }

    /// Sets stdin data.
    pub fn set_stdin_data(&self, data: &[u8]) {
        *self.stdin_data.lock() = data.to_vec();
    }

    /// Prints to stdout.
    pub fn print(&self, text: &str) {
        self.stdout_data.lock().extend_from_slice(text.as_bytes());
    }

    /// Gets the stdout data.
    pub fn get_stdout_data(&self) -> Vec<u8> {
        self.stdout_data.lock().clone()
    }

    /// Checks if this is a remote invocation.
    pub fn get_is_remote(&self) -> bool {
        *self.is_remote.lock()
    }

    /// Sets whether this is a remote invocation.
    pub fn set_is_remote(&self, is_remote: bool) {
        *self.is_remote.lock() = is_remote;
    }

    /// Marks the command-line as done.
    pub fn done(&self) {
        *self.done.lock() = true;
    }

    /// Checks if the command-line is done.
    pub fn is_done(&self) -> bool {
        *self.done.lock()
    }
}

// ──────────────────────────── Tests ───────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_new() {
        let cl = ApplicationCommandLine::new(vec!["myapp".to_string(), "--verbose".to_string()]);
        assert_eq!(
            cl.get_arguments(),
            vec!["myapp".to_string(), "--verbose".to_string()]
        );
        assert!(!cl.get_is_remote());
    }

    #[test]
    fn test_environ() {
        let cl = ApplicationCommandLine::new(vec!["app".to_string()]);
        cl.setenv("HOME", "/root");
        assert_eq!(cl.getenv("HOME"), Some("/root".to_string()));
        assert!(cl.getenv("PATH").is_none());
    }

    #[test]
    fn test_print() {
        let cl = ApplicationCommandLine::new(vec!["app".to_string()]);
        cl.print("Hello, world!\n");
        assert_eq!(cl.get_stdout_data(), b"Hello, world!\n".to_vec());
    }

    #[test]
    fn test_done() {
        let cl = ApplicationCommandLine::new(vec![]);
        assert!(!cl.is_done());
        cl.done();
        assert!(cl.is_done());
    }
}
