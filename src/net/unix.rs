//! AF_UNIX stream and datagram socket implementation.
//!
//! Stream sockets use the kernel IPC pipe layer for byte-stream I/O.
//! Datagram sockets use per-path mailboxes with message boundaries preserved.
//! Pre-bound paths (D-Bus, Wayland) integrate with their respective servers.

use crate::process::ipc::get_ipc_manager;
use alloc::collections::{BTreeMap, VecDeque};
use alloc::string::String;
use alloc::sync::Arc;
use alloc::vec::Vec;
use core::sync::atomic::{AtomicBool, Ordering};
use spin::{Mutex, RwLock};

/// Role of a pre-bound runtime socket used by the GNOME overlay.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UnixSocketRole {
    Generic,
    DbusSession,
    DbusSystem,
    WaylandDisplay,
}

/// Unix socket type.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UnixSocketType {
    Stream,
    Datagram,
}

/// A datagram message with optional sender path.
#[derive(Debug, Clone)]
struct UnixDgramMessage {
    data: Vec<u8>,
    sender: Option<String>,
}

/// Listener state for a bound Unix domain socket path.
struct UnixPathListener {
    path: String,
    role: UnixSocketRole,
    sock_type: UnixSocketType,
    listening: AtomicBool,
    /// Stream: pending connection pipe ids.
    pending_stream: Mutex<Vec<u32>>,
    /// Datagram: inbound message queue.
    dgram_inbox: Mutex<VecDeque<UnixDgramMessage>>,
}

impl UnixPathListener {
    fn new(
        path: String,
        role: UnixSocketRole,
        sock_type: UnixSocketType,
        listening: bool,
    ) -> Arc<Self> {
        Arc::new(Self {
            path,
            role,
            sock_type,
            listening: AtomicBool::new(listening),
            pending_stream: Mutex::new(Vec::new()),
            dgram_inbox: Mutex::new(VecDeque::new()),
        })
    }

    fn is_listening(&self) -> bool {
        self.listening.load(Ordering::Acquire)
    }

    fn set_listening(&self, listening: bool) {
        self.listening.store(listening, Ordering::Release);
    }

    fn push_stream_connection(&self, pipe_id: u32) {
        self.pending_stream.lock().push(pipe_id);
    }

    fn pop_stream_connection(&self) -> Option<u32> {
        let mut pending = self.pending_stream.lock();
        if pending.is_empty() {
            None
        } else {
            Some(pending.remove(0))
        }
    }

    fn push_dgram(&self, msg: UnixDgramMessage) {
        let mut inbox = self.dgram_inbox.lock();
        if inbox.len() >= 256 {
            inbox.pop_front();
        }
        inbox.push_back(msg);
    }

    fn pop_dgram(&self) -> Option<UnixDgramMessage> {
        self.dgram_inbox.lock().pop_front()
    }

    fn has_dgram(&self) -> bool {
        !self.dgram_inbox.lock().is_empty()
    }
}

/// Per-fd Unix socket state.
#[derive(Clone)]
pub struct UnixSocketEnd {
    pub pipe_id: u32,
    pub is_listener: bool,
    pub path: Option<String>,
    pub role: UnixSocketRole,
    pub sock_type: UnixSocketType,
    pub pending_out_of_band_fds: Arc<Mutex<Vec<u32>>>,
    /// Datagram: local bound path.
    pub bound_path: Option<String>,
    /// Datagram: outbound default peer path (connect).
    pub connected_path: Option<String>,
}

static UNIX_SOCKET_REGISTRY: RwLock<BTreeMap<String, Arc<UnixPathListener>>> =
    RwLock::new(BTreeMap::new());

static UNIX_SOCKET_FDS: RwLock<BTreeMap<i32, UnixSocketEnd>> = RwLock::new(BTreeMap::new());
static UNIX_LISTENER_FDS: RwLock<BTreeMap<i32, String>> = RwLock::new(BTreeMap::new());

/// Returns true when `fd` is an AF_UNIX socket.
pub fn is_unix_fd(fd: i32) -> bool {
    UNIX_SOCKET_FDS.read().contains_key(&fd)
}

/// Lookup endpoint state for an fd.
pub fn get_endpoint(fd: i32) -> Option<UnixSocketEnd> {
    UNIX_SOCKET_FDS.read().get(&fd).cloned()
}

/// Returns true when a Unix socket fd has data available to read. Used by the
/// poll/select layer, which otherwise treats these fds as plain regular files
/// (always-ready) and would spin clients against a perpetual POLLIN.
pub fn poll_readable(fd: i32) -> bool {
    let Some(end) = UNIX_SOCKET_FDS.read().get(&fd).cloned() else {
        return false;
    };
    match end.sock_type {
        // Connected stream fds carry a real pipe. Listener fds have pipe_id == 0
        // and will read as "no data" here; that's fine while the compositor is
        // in-kernel and accepts synchronously, but would need accept-readiness
        // wiring if the server ever moved to a separate accept loop.
        UnixSocketType::Stream => get_ipc_manager().pipe_has_data(end.pipe_id),
        UnixSocketType::Datagram => end
            .bound_path
            .as_deref()
            .or(end.path.as_deref())
            .and_then(lookup_listener)
            .map(|listener| listener.has_dgram())
            .unwrap_or(false),
    }
}

/// Pre-bind a Unix socket path for kernel-provided services (D-Bus, Wayland).
pub fn prebind_unix_socket(path: &str, role: UnixSocketRole) -> Result<(), &'static str> {
    if path.is_empty() {
        return Err("Unix socket path must not be empty");
    }

    let mut registry = UNIX_SOCKET_REGISTRY.write();
    if registry.contains_key(path) {
        return Ok(());
    }

    registry.insert(
        String::from(path),
        UnixPathListener::new(String::from(path), role, UnixSocketType::Stream, true),
    );
    Ok(())
}

/// Returns true when a path has been pre-bound or bound by userspace.
pub fn is_prebound(path: &str) -> bool {
    UNIX_SOCKET_REGISTRY.read().contains_key(path)
}

fn lookup_listener(path: &str) -> Option<Arc<UnixPathListener>> {
    UNIX_SOCKET_REGISTRY.read().get(path).cloned()
}

fn allocate_connection_pipe() -> Result<u32, ()> {
    let ipc = get_ipc_manager();
    let (pipe_id, _) = ipc.create_pipe().map_err(|_| ())?;
    Ok(pipe_id)
}

fn register_connector(
    sockfd: i32,
    pipe_id: u32,
    path: &str,
    role: UnixSocketRole,
    sock_type: UnixSocketType,
) {
    if role == UnixSocketRole::WaylandDisplay {
        let _ = crate::wayland::server::attach_connection(pipe_id);
    } else if role == UnixSocketRole::DbusSession {
        crate::dbus::register_session_pipe(pipe_id);
    } else if role == UnixSocketRole::DbusSystem {
        crate::dbus::register_system_pipe(pipe_id);
    }

    UNIX_SOCKET_FDS.write().insert(
        sockfd,
        UnixSocketEnd {
            pipe_id,
            is_listener: false,
            path: Some(String::from(path)),
            role,
            sock_type,
            pending_out_of_band_fds: Arc::new(Mutex::new(Vec::new())),
            bound_path: None,
            connected_path: None,
        },
    );
}

/// Create a Unix domain socket fd (returns fd via VFS; state registered on bind/connect).
pub fn create_socket_fd(
    sock_type: UnixSocketType,
    nonblock: bool,
    cloexec: bool,
) -> Result<i32, ()> {
    let inode = crate::vfs::get_vfs().lookup("/").map_err(|_| ())?;
    let mut fd_flags: u32 = 0;
    if nonblock {
        fd_flags |= crate::vfs::OpenFlags::NONBLOCK;
    }
    if cloexec {
        fd_flags |= crate::vfs::OpenFlags::CLOEXEC;
    }
    let fd = crate::vfs::vfs_open_special(inode, fd_flags, crate::vfs::FdKind::Regular)
        .map_err(|_| ())?;

    UNIX_SOCKET_FDS.write().insert(
        fd,
        UnixSocketEnd {
            pipe_id: 0,
            is_listener: false,
            path: None,
            role: UnixSocketRole::Generic,
            sock_type,
            pending_out_of_band_fds: Arc::new(Mutex::new(Vec::new())),
            bound_path: None,
            connected_path: None,
        },
    );
    Ok(fd)
}

/// Bind a Unix socket to `path`.
pub fn bind(fd: i32, path: &str) -> Result<(), i32> {
    if path.is_empty() {
        return Err(-22); // EINVAL
    }

    let sock_type = UNIX_SOCKET_FDS
        .read()
        .get(&fd)
        .map(|e| e.sock_type)
        .ok_or(-9)?; // EBADF

    if UNIX_SOCKET_REGISTRY.read().contains_key(path) {
        return Err(-98); // EADDRINUSE
    }

    let listener = UnixPathListener::new(
        String::from(path),
        UnixSocketRole::Generic,
        sock_type,
        false,
    );
    UNIX_SOCKET_REGISTRY
        .write()
        .insert(String::from(path), listener);

    if let Some(end) = UNIX_SOCKET_FDS.write().get_mut(&fd) {
        end.is_listener = true;
        end.path = Some(String::from(path));
        end.bound_path = Some(String::from(path));
    }

    if sock_type == UnixSocketType::Stream {
        UNIX_LISTENER_FDS.write().insert(fd, String::from(path));
    }

    Ok(())
}

/// Connect a Unix socket to `path`.
pub fn connect(fd: i32, path: &str) -> Result<(), i32> {
    let end = UNIX_SOCKET_FDS.read().get(&fd).cloned().ok_or(-9)?;

    let listener = {
        let registry = UNIX_SOCKET_REGISTRY.read();
        registry.get(path).cloned().ok_or(-111)? // ECONNREFUSED
    };

    match end.sock_type {
        UnixSocketType::Stream => {
            if !listener.is_listening() {
                return Err(-111);
            }
            let pipe_id = allocate_connection_pipe().map_err(|_| -24)?; // EMFILE
            listener.push_stream_connection(pipe_id);
            register_connector(fd, pipe_id, path, listener.role, UnixSocketType::Stream);
        }
        UnixSocketType::Datagram => {
            if let Some(e) = UNIX_SOCKET_FDS.write().get_mut(&fd) {
                e.connected_path = Some(String::from(path));
                e.path = Some(String::from(path));
            }
        }
    }

    Ok(())
}

/// Put a stream socket into listening state.
pub fn listen(fd: i32, _backlog: i32) -> Result<(), i32> {
    if let Some(path) = UNIX_LISTENER_FDS.read().get(&fd).cloned() {
        if let Some(listener) = lookup_listener(&path) {
            listener.set_listening(true);
            return Ok(());
        }
        return Err(-22);
    }
    Err(-22)
}

/// Accept a stream connection.
pub fn accept(fd: i32) -> Result<i32, i32> {
    let path = UNIX_LISTENER_FDS.read().get(&fd).cloned().ok_or(-22)?;
    let listener = lookup_listener(&path).ok_or(-22)?;
    let pipe_id = listener.pop_stream_connection().ok_or(-11)?; // EAGAIN

    let inode = crate::vfs::get_vfs().lookup("/").map_err(|_| -12)?;
    let client_fd =
        crate::vfs::vfs_open_special(inode, 0, crate::vfs::FdKind::Regular).map_err(|_| -24)?;

    register_connector(
        client_fd,
        pipe_id,
        &path,
        listener.role,
        UnixSocketType::Stream,
    );
    Ok(client_fd)
}

/// Returns true for roles whose server is an in-kernel dispatcher (D-Bus,
/// Wayland). For these the connection pipe is a *server→client* channel only:
/// the client's request bytes are consumed directly by the dispatcher and must
/// never be written into the readable buffer, otherwise the client reads its
/// own request back as if it were a server event and immediately desyncs.
fn is_bridged_role(role: UnixSocketRole) -> bool {
    matches!(
        role,
        UnixSocketRole::WaylandDisplay | UnixSocketRole::DbusSession | UnixSocketRole::DbusSystem
    )
}

/// Stream send or datagram sendto.
pub fn send(fd: i32, data: &[u8], dest_path: Option<&str>) -> Result<usize, i32> {
    let end = UNIX_SOCKET_FDS.read().get(&fd).cloned().ok_or(-9)?;

    match end.sock_type {
        UnixSocketType::Stream => {
            if data.is_empty() {
                return Ok(0);
            }
            match end.role {
                // Bridged roles: hand the request straight to the in-kernel
                // dispatcher. It writes any reply/events into the pipe, which
                // is the channel the client reads from. The request itself is
                // NOT written into the pipe (no echo).
                UnixSocketRole::WaylandDisplay => {
                    maybe_dispatch_wayland(data, end.pipe_id);
                    Ok(data.len())
                }
                UnixSocketRole::DbusSession => {
                    maybe_dispatch_dbus(data, end.pipe_id);
                    Ok(data.len())
                }
                UnixSocketRole::DbusSystem => {
                    maybe_dispatch_dbus_system(data, end.pipe_id);
                    Ok(data.len())
                }
                // Generic peer-to-peer stream socket: the pipe is the transport.
                UnixSocketRole::Generic => {
                    let ipc = get_ipc_manager();
                    ipc.pipe_write(end.pipe_id, data).map_err(|_| -32)
                }
            }
        }
        UnixSocketType::Datagram => {
            let target = dest_path.or(end.connected_path.as_deref()).ok_or(-57)?; // ENOTCONN

            let listener = lookup_listener(target).ok_or(-111)?;
            listener.push_dgram(UnixDgramMessage {
                data: data.to_vec(),
                sender: end.bound_path.clone(),
            });
            Ok(data.len())
        }
    }
}

/// Stream recv or datagram recvfrom.
pub fn recv(fd: i32, buf: &mut [u8]) -> Result<(usize, Option<String>), i32> {
    let end = UNIX_SOCKET_FDS.read().get(&fd).cloned().ok_or(-9)?;

    match end.sock_type {
        UnixSocketType::Stream => {
            if buf.is_empty() {
                return Ok((0, None));
            }
            let ipc = get_ipc_manager();
            match ipc.pipe_read(end.pipe_id, buf) {
                Ok(0) | Err(_) => {
                    // No server→client bytes buffered. Bridged clients
                    // (libwayland, libdbus) are non-blocking and treat a
                    // zero-byte read as EOF/disconnect; report EAGAIN so they
                    // retry instead of tearing down the connection.
                    if is_bridged_role(end.role) {
                        Err(-11) // EAGAIN
                    } else {
                        Ok((0, None))
                    }
                }
                Ok(n) => Ok((n, None)),
            }
        }
        UnixSocketType::Datagram => {
            let path = end
                .bound_path
                .as_deref()
                .or(end.path.as_deref())
                .ok_or(-22)?;

            let listener = lookup_listener(path).ok_or(-22)?;
            if let Some(msg) = listener.pop_dgram() {
                let n = core::cmp::min(buf.len(), msg.data.len());
                buf[..n].copy_from_slice(&msg.data[..n]);
                Ok((n, msg.sender))
            } else {
                Ok((0, None))
            }
        }
    }
}

pub fn queue_out_of_band_fds(sockfd: i32, fds: &[u32]) {
    if fds.is_empty() {
        return;
    }
    if let Some(end) = UNIX_SOCKET_FDS.read().get(&sockfd).cloned() {
        end.pending_out_of_band_fds.lock().extend_from_slice(fds);
    }
}

pub fn queue_wayland_out_of_band_fds(sockfd: i32, fds: &[u32]) {
    queue_out_of_band_fds(sockfd, fds);
}

pub fn queue_wayland_pipe_out_of_band_fds(pipe_id: u32, fds: &[u32]) {
    if fds.is_empty() {
        return;
    }
    for (sockfd, end) in UNIX_SOCKET_FDS.read().iter() {
        if end.pipe_id == pipe_id && end.role == UnixSocketRole::WaylandDisplay {
            queue_out_of_band_fds(*sockfd, fds);
        }
    }
}

pub fn take_out_of_band_fds(sockfd: i32) -> Vec<u32> {
    if let Some(end) = UNIX_SOCKET_FDS.read().get(&sockfd).cloned() {
        let mut pending = end.pending_out_of_band_fds.lock();
        return core::mem::take(&mut *pending);
    }
    Vec::new()
}

pub fn endpoint_role(sockfd: i32) -> Option<UnixSocketRole> {
    UNIX_SOCKET_FDS.read().get(&sockfd).map(|e| e.role)
}

fn maybe_dispatch_dbus(data: &[u8], pipe_id: u32) {
    if let Some(reply) = crate::dbus::process_wire_request(data, Some(pipe_id)) {
        let ipc = get_ipc_manager();
        let _ = ipc.pipe_write(pipe_id, &reply);
    }
}

fn maybe_dispatch_dbus_system(data: &[u8], pipe_id: u32) {
    if let Some(reply) = crate::dbus::process_wire_request(data, Some(pipe_id)) {
        let ipc = get_ipc_manager();
        let _ = ipc.pipe_write(pipe_id, &reply);
    }
}

fn maybe_dispatch_wayland(data: &[u8], pipe_id: u32) {
    if let Some(reply) = crate::wayland::server::process_wire_request(data, pipe_id) {
        let ipc = get_ipc_manager();
        let _ = ipc.pipe_write(pipe_id, &reply);
    }
}

/// Create socketpair (stream only).
pub fn socketpair(nonblock: bool, cloexec: bool) -> Result<(i32, i32), i32> {
    let pipefd: [i32; 2] = [0, 0];
    crate::linux_compat::special_fd::pipe(pipefd.as_ptr() as *mut [i32; 2]).map_err(|_| -22)?;

    let mut fd_flags: u32 = 0;
    if nonblock {
        fd_flags |= crate::vfs::OpenFlags::NONBLOCK;
    }
    if cloexec {
        fd_flags |= crate::vfs::OpenFlags::CLOEXEC;
    }
    if fd_flags != 0 {
        let _ = crate::vfs::vfs_set_fd_flags(pipefd[0], fd_flags);
        let _ = crate::vfs::vfs_set_fd_flags(pipefd[1], fd_flags);
    }

    for &fd in &pipefd {
        UNIX_SOCKET_FDS.write().insert(
            fd,
            UnixSocketEnd {
                pipe_id: fd as u32,
                is_listener: false,
                path: None,
                role: UnixSocketRole::Generic,
                sock_type: UnixSocketType::Stream,
                pending_out_of_band_fds: Arc::new(Mutex::new(Vec::new())),
                bound_path: None,
                connected_path: None,
            },
        );
    }

    Ok((pipefd[0], pipefd[1]))
}

/// Map sock_type flag from Linux socket() call.
pub fn map_sock_type(raw: i32) -> UnixSocketType {
    match raw & 0xFF {
        2 => UnixSocketType::Datagram,
        _ => UnixSocketType::Stream,
    }
}
