//! Linux socket operation APIs
//!
//! This module implements Linux-compatible socket operations including
//! send, recv, socket options, and I/O multiplexing.

use alloc::string::String;
use alloc::vec;
use alloc::vec::Vec;
use core::sync::atomic::{AtomicU64, Ordering};

use super::types::*;
use super::{LinuxError, LinuxResult};
use crate::memory::user_space::UserSpaceMemory;
use crate::net::socket::{SocketAddress, SocketOption, SocketOptionType, SocketType};
use crate::net::{self, NetworkAddress, NetworkError, Protocol};
use crate::vfs::{self, FdKind};

/// Operation counter for statistics
static SOCKET_OPS_COUNT: AtomicU64 = AtomicU64::new(0);
const MAX_SOCKET_RW_CHUNK: usize = 64 * 1024;
const MAX_SOCKET_IOV: usize = 1024;

// Linux socket message flags (from <linux/socket.h>)
/// Send data without routing table lookup (no-op in our stack)
const MSG_DONTROUTE: i32 = 0x4;
/// Peek at incoming data without consuming it
const MSG_PEEK: i32 = 0x2;
/// Non-blocking operation
const MSG_DONTWAIT: i32 = 0x40;
/// Wait for full request amount (loop until all data received)
const MSG_WAITALL: i32 = 0x100;
/// Don't generate SIGPIPE (no-op — we have no SIGPIPE)
const MSG_NOSIGNAL: i32 = 0x4000;
/// Sender will send more data (no-op — no Nagle coalescing yet)
const MSG_MORE: i32 = 0x8000;
/// Return real packet length even if truncated (for recv)
const MSG_TRUNC: i32 = 0x20;

// accept4() flags (from <linux/socket.h>)
/// Set O_NONBLOCK on the new fd
const SOCK_NONBLOCK: i32 = 0o20000;
/// Set O_CLOEXEC on the new fd
const SOCK_CLOEXEC: i32 = 0o2000000;

use crate::net::raw::{self, AF_PACKET};
use crate::net::unix::{self};

pub use crate::net::unix::UnixSocketRole;

/// Pre-bind a Unix socket path for kernel-provided services (D-Bus, Wayland).
pub fn prebind_unix_socket(path: &str, role: UnixSocketRole) -> Result<(), &'static str> {
    unix::prebind_unix_socket(path, role)
}

/// Returns true when a path has been pre-bound or bound by userspace.
pub fn is_prebound(path: &str) -> bool {
    unix::is_prebound(path)
}

fn queue_out_of_band_fds(sockfd: Fd, fds: &[u32]) {
    unix::queue_out_of_band_fds(sockfd, fds);
}

/// Queue file descriptors to deliver on the next recvmsg for a Wayland socket fd.
pub fn queue_wayland_out_of_band_fds(sockfd: Fd, fds: &[u32]) {
    queue_out_of_band_fds(sockfd, fds);
}

/// Queue out-of-band FDs for every userspace fd bound to a Wayland pipe.
pub fn queue_wayland_pipe_out_of_band_fds(pipe_id: u32, fds: &[u32]) {
    unix::queue_wayland_pipe_out_of_band_fds(pipe_id, fds);
}

fn deliver_out_of_band_fds(sockfd: Fd, msg: *mut u8) -> LinuxResult<()> {
    let pending = unix::take_out_of_band_fds(sockfd);
    if pending.is_empty() {
        return Ok(());
    }

    // msghdr layout: name(8), namelen(4), pad(4), iov(8), iovlen(8), control(8), controllen(8), flags(4)
    let mut header = [0u8; 48];
    UserSpaceMemory::copy_from_user(msg as u64, &mut header).map_err(|_| LinuxError::EFAULT)?;
    let msg_control = usize::from_ne_bytes(header[32..40].try_into().unwrap()) as *mut u8;
    let msg_controllen = usize::from_ne_bytes(header[40..48].try_into().unwrap());
    if msg_control.is_null() || msg_controllen == 0 {
        return Ok(());
    }

    let mut delivered = Vec::new();
    for pipe_id in pending {
        let fd = super::special_fd::register_special(
            crate::vfs::FdKind::PipeRead(pipe_id),
            crate::vfs::OpenFlags::RDONLY,
        )?;
        delivered.push(fd);
    }

    // SCM_RIGHTS: cmsghdr { len, level=SOL_SOCKET(1), type=SCM_RIGHTS(1) } + aligned fd array
    let payload_len = delivered.len() * core::mem::size_of::<i32>();
    let cmsg_space = 16 + payload_len; // CMSG_SPACE approximation on Linux/x86_64
    if msg_controllen < cmsg_space {
        return Err(LinuxError::EINVAL);
    }

    let mut control = vec![0u8; cmsg_space];
    let cmsg_len = 12 + payload_len;
    control[0..4].copy_from_slice(&(cmsg_len as u32).to_le_bytes());
    control[4..8].copy_from_slice(&1u32.to_le_bytes()); // SOL_SOCKET
    control[8..12].copy_from_slice(&1u32.to_le_bytes()); // SCM_RIGHTS
    for (idx, fd) in delivered.iter().enumerate() {
        let offset = 16 + idx * core::mem::size_of::<i32>();
        control[offset..offset + 4].copy_from_slice(&fd.to_le_bytes());
    }

    UserSpaceMemory::copy_to_user(
        msg_control as u64,
        &control[..cmsg_space.min(msg_controllen)],
    )
    .map_err(|_| LinuxError::EFAULT)?;
    Ok(())
}

fn unix_err(code: i32) -> LinuxError {
    LinuxError::from_errno(-code)
}

/// Initialize socket operations subsystem
pub fn init_socket_operations() {
    SOCKET_OPS_COUNT.store(0, Ordering::Relaxed);
}

/// Get number of socket operations performed
pub fn get_operation_count() -> u64 {
    SOCKET_OPS_COUNT.load(Ordering::Relaxed)
}

/// Increment operation counter
fn inc_ops() {
    SOCKET_OPS_COUNT.fetch_add(1, Ordering::Relaxed);
}

/// Map a network error to a Linux error code.
pub fn net_err_to_linux(e: NetworkError) -> LinuxError {
    match e {
        NetworkError::ConnectionRefused => LinuxError::ECONNREFUSED,
        NetworkError::ConnectionReset => LinuxError::ECONNRESET,
        NetworkError::NotConnected => LinuxError::ENOTCONN,
        NetworkError::Timeout => LinuxError::ETIMEDOUT,
        NetworkError::InvalidAddress => LinuxError::EINVAL,
        NetworkError::AddressInUse => LinuxError::EADDRINUSE,
        NetworkError::NotSupported => LinuxError::ENOSYS,
        NetworkError::PermissionDenied => LinuxError::EPERM,
        NetworkError::InvalidArgument => LinuxError::EINVAL,
        NetworkError::BufferOverflow => LinuxError::ENOBUFS,
        NetworkError::BufferTooSmall => LinuxError::EINVAL,
        NetworkError::NetworkUnreachable => LinuxError::ENETUNREACH,
        NetworkError::HostUnreachable => LinuxError::EHOSTUNREACH,
        NetworkError::PortUnreachable => LinuxError::ECONNREFUSED,
        NetworkError::NoRoute => LinuxError::EHOSTUNREACH,
        NetworkError::Busy => LinuxError::EBUSY,
        NetworkError::InvalidState => LinuxError::EINVAL,
        NetworkError::InsufficientMemory => LinuxError::ENOMEM,
        _ => LinuxError::EIO,
    }
}

/// Look up the socket ID for a VFS fd. Returns EBADF if not a socket fd.
fn fd_to_socket_id(sockfd: Fd) -> LinuxResult<u32> {
    if sockfd < 0 {
        return Err(LinuxError::EBADF);
    }
    let kind = vfs::vfs_fd_kind(sockfd).map_err(|_| LinuxError::EBADF)?;
    match kind {
        FdKind::Socket(id) => Ok(id),
        _ => Err(LinuxError::ENOTSOCK),
    }
}

/// Register a socket ID as a VFS fd and return the fd.
fn register_socket_fd(socket_id: u32) -> LinuxResult<Fd> {
    let inode = vfs::get_vfs().lookup("/").map_err(|_| LinuxError::ENOMEM)?;
    vfs::vfs_open_special(inode, vfs::OpenFlags::RDWR, FdKind::Socket(socket_id))
        .map_err(|_| LinuxError::EMFILE)
}

fn copy_iovec_from_user(iov_ptr: *const IoVec, index: usize) -> LinuxResult<IoVec> {
    let mut iov = IoVec {
        iov_base: core::ptr::null_mut(),
        iov_len: 0,
    };
    let iov_bytes = unsafe {
        core::slice::from_raw_parts_mut(
            &mut iov as *mut IoVec as *mut u8,
            core::mem::size_of::<IoVec>(),
        )
    };
    UserSpaceMemory::copy_from_user(
        (iov_ptr as u64) + (index * core::mem::size_of::<IoVec>()) as u64,
        iov_bytes,
    )
    .map_err(|_| LinuxError::EFAULT)?;
    Ok(iov)
}

fn read_msghdr_fields(msg: *const u8) -> LinuxResult<(*const u8, u32, *const IoVec, usize)> {
    let mut header = [0u8; 32];
    UserSpaceMemory::copy_from_user(msg as u64, &mut header).map_err(|_| LinuxError::EFAULT)?;

    let msg_name =
        usize::from_ne_bytes(header[0..8].try_into().map_err(|_| LinuxError::EINVAL)?) as *const u8;
    let msg_namelen = u32::from_ne_bytes(header[8..12].try_into().map_err(|_| LinuxError::EINVAL)?);
    let msg_iov = usize::from_ne_bytes(header[16..24].try_into().map_err(|_| LinuxError::EINVAL)?)
        as *const IoVec;
    let msg_iovlen =
        usize::from_ne_bytes(header[24..32].try_into().map_err(|_| LinuxError::EINVAL)?);

    if msg_iovlen > MAX_SOCKET_IOV {
        return Err(LinuxError::EINVAL);
    }

    Ok((msg_name, msg_namelen, msg_iov, msg_iovlen))
}

fn parse_unix_path(addr: *const SockAddr, addrlen: u32) -> LinuxResult<String> {
    let mut family_bytes = [0u8; 2];
    UserSpaceMemory::copy_from_user(addr as u64, &mut family_bytes)
        .map_err(|_| LinuxError::EFAULT)?;
    let family = u16::from_ne_bytes(family_bytes);
    if family != 1 {
        return Err(LinuxError::EAFNOSUPPORT);
    }
    if (addrlen as usize) < 2 {
        return Err(LinuxError::EINVAL);
    }
    let path_len = (addrlen as usize - 2).min(108);
    let mut path_buf = [0u8; 108];
    UserSpaceMemory::copy_from_user((addr as u64) + 2, &mut path_buf[..path_len])
        .map_err(|_| LinuxError::EFAULT)?;
    let path_str = String::from(
        core::str::from_utf8(&path_buf[..path_len])
            .map_err(|_| LinuxError::EINVAL)?
            .trim_end_matches('\0'),
    );
    if path_str.is_empty() {
        return Err(LinuxError::EINVAL);
    }
    Ok(path_str)
}

/// Parse a SockAddr into a SocketAddress.
fn parse_sockaddr(addr: *const SockAddr, _addrlen: u32) -> LinuxResult<SocketAddress> {
    if addr.is_null() {
        return Err(LinuxError::EFAULT);
    }
    let mut sa = SockAddr {
        sa_family: 0,
        sa_data: [0; 14],
    };
    let sa_bytes = unsafe {
        core::slice::from_raw_parts_mut(
            &mut sa as *mut SockAddr as *mut u8,
            core::mem::size_of::<SockAddr>(),
        )
    };
    UserSpaceMemory::copy_from_user(addr as u64, sa_bytes).map_err(|_| LinuxError::EFAULT)?;

    match sa.sa_family {
        1 => {
            // AF_UNIX — handled separately via UNIX_SOCKET_FDS and
            // UNIX_SOCKET_REGISTRY, not through the network stack.
            Err(LinuxError::EAFNOSUPPORT)
        }
        2 => {
            // AF_INET (sockaddr_in)
            // sa_data layout: [port_be:2, addr:4, zero:8]
            let port = u16::from_be_bytes([sa.sa_data[0], sa.sa_data[1]]);
            let addr_bytes = [sa.sa_data[2], sa.sa_data[3], sa.sa_data[4], sa.sa_data[5]];
            Ok(SocketAddress::new(NetworkAddress::IPv4(addr_bytes), port))
        }
        10 => {
            // AF_INET6 - not enough data in generic SockAddr (14 bytes)
            // Need the full sockaddr_in6 which is 28 bytes
            Err(LinuxError::EAFNOSUPPORT)
        }
        _ => Err(LinuxError::EAFNOSUPPORT),
    }
}

/// Write a SocketAddress back into a SockAddr buffer.
fn write_sockaddr(addr: *mut SockAddr, addrlen: *mut u32, sa: &SocketAddress) -> LinuxResult<()> {
    if addr.is_null() || addrlen.is_null() {
        return Err(LinuxError::EFAULT);
    }
    match sa.address {
        NetworkAddress::IPv4(ref ip) => {
            let needed = 16u32; // sizeof(sockaddr_in)
            let mut len_bytes = [0u8; core::mem::size_of::<u32>()];
            UserSpaceMemory::copy_from_user(addrlen as u64, &mut len_bytes)
                .map_err(|_| LinuxError::EFAULT)?;
            let avail = u32::from_ne_bytes(len_bytes);
            if avail < needed {
                UserSpaceMemory::copy_to_user(addrlen as u64, &needed.to_ne_bytes())
                    .map_err(|_| LinuxError::EFAULT)?;
                return Err(LinuxError::EINVAL);
            }

            let mut out = SockAddr {
                sa_family: 2,
                sa_data: [0; 14],
            };
            out.sa_data[0] = (sa.port >> 8) as u8;
            out.sa_data[1] = sa.port as u8;
            out.sa_data[2] = ip[0];
            out.sa_data[3] = ip[1];
            out.sa_data[4] = ip[2];
            out.sa_data[5] = ip[3];

            let out_bytes = unsafe {
                core::slice::from_raw_parts(
                    &out as *const SockAddr as *const u8,
                    core::mem::size_of::<SockAddr>(),
                )
            };
            UserSpaceMemory::copy_to_user(addr as u64, out_bytes)
                .map_err(|_| LinuxError::EFAULT)?;
            UserSpaceMemory::copy_to_user(addrlen as u64, &needed.to_ne_bytes())
                .map_err(|_| LinuxError::EFAULT)?;
            Ok(())
        }
        _ => Err(LinuxError::EAFNOSUPPORT),
    }
}

/// send - send message on socket
pub fn send(sockfd: Fd, buf: *const u8, len: usize, flags: i32) -> LinuxResult<isize> {
    inc_ops();

    if buf.is_null() && len > 0 {
        return Err(LinuxError::EFAULT);
    }

    // Raw / AF_PACKET sockets
    if raw::is_raw_fd(sockfd) || raw::is_packet_fd(sockfd) {
        if len == 0 {
            return Ok(0);
        }
        let copy_len = len.min(MAX_SOCKET_RW_CHUNK);
        let mut data = vec![0u8; copy_len];
        UserSpaceMemory::copy_from_user(buf as u64, &mut data).map_err(|_| LinuxError::EFAULT)?;
        let _ = flags & (MSG_NOSIGNAL | MSG_DONTROUTE | MSG_MORE | MSG_DONTWAIT);
        return raw::send(sockfd, &data)
            .map_err(net_err_to_linux)
            .map(|n| n as isize);
    }

    // AF_UNIX socket
    if unix::is_unix_fd(sockfd) {
        if len == 0 {
            return Ok(0);
        }
        let copy_len = len.min(MAX_SOCKET_RW_CHUNK);
        let mut data = vec![0u8; copy_len];
        UserSpaceMemory::copy_from_user(buf as u64, &mut data).map_err(|_| LinuxError::EFAULT)?;
        let _ = flags & (MSG_NOSIGNAL | MSG_DONTROUTE | MSG_MORE | MSG_DONTWAIT);
        return unix::send(sockfd, &data, None)
            .map_err(unix_err)
            .map(|n| n as isize);
    }

    let socket_id = fd_to_socket_id(sockfd)?;
    if len == 0 {
        return Ok(0);
    }
    let copy_len = len.min(MAX_SOCKET_RW_CHUNK);
    let mut data = vec![0u8; copy_len];
    UserSpaceMemory::copy_from_user(buf as u64, &mut data).map_err(|_| LinuxError::EFAULT)?;

    let mut sock = net::network_stack()
        .get_socket(socket_id)
        .ok_or(LinuxError::EBADF)?;

    // MSG_NOSIGNAL, MSG_DONTROUTE, MSG_MORE, MSG_DONTWAIT are all effectively
    // no-ops in our network stack (no SIGPIPE, no routing table, no Nagle
    // coalescing, operations already return immediately).
    let _ = flags & (MSG_NOSIGNAL | MSG_DONTROUTE | MSG_MORE | MSG_DONTWAIT);
    sock.send(&data)
        .map_err(net_err_to_linux)
        .map(|n| n as isize)
}

/// sendto - send message to specific destination
pub fn sendto(
    sockfd: Fd,
    buf: *const u8,
    len: usize,
    flags: i32,
    dest_addr: *const SockAddr,
    addrlen: u32,
) -> LinuxResult<isize> {
    inc_ops();

    if buf.is_null() && len > 0 {
        return Err(LinuxError::EFAULT);
    }

    if len == 0 {
        return Ok(0);
    }

    let copy_len = len.min(MAX_SOCKET_RW_CHUNK);
    let mut data = vec![0u8; copy_len];
    UserSpaceMemory::copy_from_user(buf as u64, &mut data).map_err(|_| LinuxError::EFAULT)?;
    let _ = flags & (MSG_NOSIGNAL | MSG_DONTROUTE | MSG_MORE | MSG_DONTWAIT);

    if raw::is_raw_fd(sockfd) || raw::is_packet_fd(sockfd) {
        return raw::send(sockfd, &data)
            .map_err(net_err_to_linux)
            .map(|n| n as isize);
    }

    if unix::is_unix_fd(sockfd) {
        let dest_path = if dest_addr.is_null() {
            None
        } else {
            Some(parse_unix_path(dest_addr, addrlen)?)
        };
        return unix::send(sockfd, &data, dest_path.as_deref())
            .map_err(unix_err)
            .map(|n| n as isize);
    }

    let socket_id = fd_to_socket_id(sockfd)?;

    if dest_addr.is_null() {
        // No destination - use connected send
        let mut sock = net::network_stack()
            .get_socket(socket_id)
            .ok_or(LinuxError::EBADF)?;
        return sock
            .send(&data)
            .map_err(net_err_to_linux)
            .map(|n| n as isize);
    }

    let dest = parse_sockaddr(dest_addr, addrlen)?;
    let mut sock = net::network_stack()
        .get_socket(socket_id)
        .ok_or(LinuxError::EBADF)?;
    sock.send_to(&data, dest)
        .map_err(net_err_to_linux)
        .map(|n| n as isize)
}

/// sendmsg - send message using message structure
pub fn sendmsg(sockfd: Fd, msg: *const u8, flags: i32) -> LinuxResult<isize> {
    inc_ops();

    if msg.is_null() {
        return Err(LinuxError::EFAULT);
    }

    // msghdr layout: { void *msg_name, socklen_t msg_namelen,
    //   struct iovec *msg_iov, size_t msg_iovlen, void *msg_control, size_t msg_controllen, int msg_flags }
    let _ = flags & (MSG_NOSIGNAL | MSG_DONTROUTE | MSG_MORE | MSG_DONTWAIT);

    let (msg_name, msg_namelen, msg_iov, msg_iovlen) = read_msghdr_fields(msg)?;
    if msg_iov.is_null() && msg_iovlen > 0 {
        return Err(LinuxError::EFAULT);
    }

    // AF_UNIX (D-Bus / Wayland bridge). libwayland flushes its outbound buffer
    // exclusively through sendmsg, so this is the primary client→compositor
    // write path. Gather all iovecs into one contiguous buffer (Wayland wire
    // messages must be delivered intact) and hand it to the unix transport.
    //
    // NOTE: SCM_RIGHTS ancillary fds in msg_control (e.g. the wl_shm.create_pool
    // buffer fd) are not yet imported into the compositor — that is the next
    // milestone (client-buffer rendering). Handshake, registry bind, surface
    // creation and configure all work without it.
    if unix::is_unix_fd(sockfd) {
        let mut data = Vec::new();
        for i in 0..msg_iovlen {
            let iov = copy_iovec_from_user(msg_iov, i)?;
            if iov.iov_base.is_null() && iov.iov_len > 0 {
                return Err(LinuxError::EFAULT);
            }
            if iov.iov_len == 0 {
                continue;
            }
            let copy_len = iov.iov_len.min(MAX_SOCKET_RW_CHUNK);
            let mut chunk = vec![0u8; copy_len];
            UserSpaceMemory::copy_from_user(iov.iov_base as u64, &mut chunk)
                .map_err(|_| LinuxError::EFAULT)?;
            data.extend_from_slice(&chunk);
        }
        let dest_path = if msg_name.is_null() {
            None
        } else {
            Some(parse_unix_path(msg_name as *const SockAddr, msg_namelen)?)
        };
        return unix::send(sockfd, &data, dest_path.as_deref())
            .map_err(unix_err)
            .map(|n| n as isize);
    }

    let socket_id = fd_to_socket_id(sockfd)?;
    let mut total_sent = 0usize;
    for i in 0..msg_iovlen {
        let iov = copy_iovec_from_user(msg_iov, i)?;
        if iov.iov_base.is_null() && iov.iov_len > 0 {
            return Err(LinuxError::EFAULT);
        }
        if iov.iov_len == 0 {
            continue;
        }

        let copy_len = iov.iov_len.min(MAX_SOCKET_RW_CHUNK);
        let mut data = vec![0u8; copy_len];
        UserSpaceMemory::copy_from_user(iov.iov_base as u64, &mut data)
            .map_err(|_| LinuxError::EFAULT)?;

        let mut sock = net::network_stack()
            .get_socket(socket_id)
            .ok_or(LinuxError::EBADF)?;
        if !msg_name.is_null() && i == 0 {
            let dest = parse_sockaddr(msg_name as *const SockAddr, msg_namelen)?;
            let n = sock.send_to(&data, dest).map_err(net_err_to_linux)?;
            total_sent += n;
        } else {
            let n = sock.send(&data).map_err(net_err_to_linux)?;
            total_sent += n;
        }
    }
    Ok(total_sent as isize)
}

/// recv - receive message from socket
pub fn recv(sockfd: Fd, buf: *mut u8, len: usize, flags: i32) -> LinuxResult<isize> {
    inc_ops();

    if buf.is_null() && len > 0 {
        return Err(LinuxError::EFAULT);
    }

    if raw::is_raw_fd(sockfd) || raw::is_packet_fd(sockfd) {
        if len == 0 {
            return Ok(0);
        }
        let copy_len = len.min(MAX_SOCKET_RW_CHUNK);
        let mut buffer = vec![0u8; copy_len];
        let _ = flags;
        let n = raw::recv(sockfd, &mut buffer).map_err(net_err_to_linux)?;
        UserSpaceMemory::copy_to_user(buf as u64, &buffer[..n]).map_err(|_| LinuxError::EFAULT)?;
        return Ok(n as isize);
    }

    if unix::is_unix_fd(sockfd) {
        if len == 0 {
            return Ok(0);
        }
        let copy_len = len.min(MAX_SOCKET_RW_CHUNK);
        let mut buffer = vec![0u8; copy_len];
        let _ = flags & (MSG_DONTWAIT | MSG_WAITALL | MSG_NOSIGNAL | MSG_TRUNC | MSG_PEEK);
        return match unix::recv(sockfd, &mut buffer) {
            Ok((n, _sender)) => {
                UserSpaceMemory::copy_to_user(buf as u64, &buffer[..n])
                    .map_err(|_| LinuxError::EFAULT)?;
                Ok(n as isize)
            }
            Err(e) => Err(unix_err(e)),
        };
    }

    let socket_id = fd_to_socket_id(sockfd)?;
    if len == 0 {
        return Ok(0);
    }
    let copy_len = len.min(MAX_SOCKET_RW_CHUNK);
    let mut buffer = vec![0u8; copy_len];

    let mut sock = net::network_stack()
        .get_socket(socket_id)
        .ok_or(LinuxError::EBADF)?;

    let want_peek = (flags & MSG_PEEK) != 0;
    let want_trunc = (flags & MSG_TRUNC) != 0;
    let _ = flags & (MSG_DONTWAIT | MSG_WAITALL | MSG_NOSIGNAL);

    if want_peek {
        match sock.recv_peek(&mut buffer) {
            Ok(n) => {
                UserSpaceMemory::copy_to_user(buf as u64, &buffer[..n])
                    .map_err(|_| LinuxError::EFAULT)?;
                Ok(n as isize)
            }
            Err(NetworkError::Timeout) => Ok(0),
            Err(e) => Err(net_err_to_linux(e)),
        }
    } else {
        match sock.recv(&mut buffer) {
            Ok(n) => {
                UserSpaceMemory::copy_to_user(buf as u64, &buffer[..n])
                    .map_err(|_| LinuxError::EFAULT)?;
                if want_trunc && n == copy_len {
                    // Report the actual available bytes even if buffer was too small
                    Ok(sock.available_bytes() as isize + n as isize)
                } else {
                    Ok(n as isize)
                }
            }
            Err(NetworkError::Timeout) => Ok(0), // Non-blocking, no data
            Err(e) => Err(net_err_to_linux(e)),
        }
    }
}

/// recvfrom - receive message from socket with source address
pub fn recvfrom(
    sockfd: Fd,
    buf: *mut u8,
    len: usize,
    flags: i32,
    src_addr: *mut SockAddr,
    addrlen: *mut u32,
) -> LinuxResult<isize> {
    inc_ops();

    if buf.is_null() && len > 0 {
        return Err(LinuxError::EFAULT);
    }

    let socket_id = fd_to_socket_id(sockfd)?;
    if len == 0 {
        return Ok(0);
    }
    let copy_len = len.min(MAX_SOCKET_RW_CHUNK);
    let mut buffer = vec![0u8; copy_len];

    let want_peek = (flags & MSG_PEEK) != 0;
    let _ = flags & (MSG_DONTWAIT | MSG_WAITALL | MSG_NOSIGNAL | MSG_TRUNC);

    let mut sock = net::network_stack()
        .get_socket(socket_id)
        .ok_or(LinuxError::EBADF)?;

    if !src_addr.is_null() {
        match sock.recv_from(&mut buffer) {
            Ok((n, source)) => {
                write_sockaddr(src_addr, addrlen, &source)?;
                UserSpaceMemory::copy_to_user(buf as u64, &buffer[..n])
                    .map_err(|_| LinuxError::EFAULT)?;
                Ok(n as isize)
            }
            Err(NetworkError::Timeout) => Ok(0),
            Err(e) => Err(net_err_to_linux(e)),
        }
    } else if want_peek {
        match sock.recv_peek(&mut buffer) {
            Ok(n) => {
                UserSpaceMemory::copy_to_user(buf as u64, &buffer[..n])
                    .map_err(|_| LinuxError::EFAULT)?;
                Ok(n as isize)
            }
            Err(NetworkError::Timeout) => Ok(0),
            Err(e) => Err(net_err_to_linux(e)),
        }
    } else {
        match sock.recv(&mut buffer) {
            Ok(n) => {
                UserSpaceMemory::copy_to_user(buf as u64, &buffer[..n])
                    .map_err(|_| LinuxError::EFAULT)?;
                Ok(n as isize)
            }
            Err(NetworkError::Timeout) => Ok(0),
            Err(e) => Err(net_err_to_linux(e)),
        }
    }
}

/// recvmsg - receive message using message structure
pub fn recvmsg(sockfd: Fd, msg: *mut u8, flags: i32) -> LinuxResult<isize> {
    inc_ops();

    if msg.is_null() {
        return Err(LinuxError::EFAULT);
    }

    if unix::is_unix_fd(sockfd) {
        let _ = flags & (MSG_DONTWAIT | MSG_WAITALL | MSG_NOSIGNAL | MSG_TRUNC | MSG_PEEK);
        let (_, _, msg_iov, msg_iovlen) = read_msghdr_fields(msg)?;
        if msg_iov.is_null() && msg_iovlen > 0 {
            return Err(LinuxError::EFAULT);
        }

        let mut total_read = 0usize;
        for i in 0..msg_iovlen {
            let iov = copy_iovec_from_user(msg_iov, i)?;
            if iov.iov_base.is_null() && iov.iov_len > 0 {
                return Err(LinuxError::EFAULT);
            }
            if iov.iov_len == 0 {
                continue;
            }

            let copy_len = iov.iov_len.min(MAX_SOCKET_RW_CHUNK);
            let mut buffer = vec![0u8; copy_len];
            match unix::recv(sockfd, &mut buffer) {
                Ok((0, _)) => break,
                Ok((n, _)) => {
                    UserSpaceMemory::copy_to_user(iov.iov_base as u64, &buffer[..n])
                        .map_err(|_| LinuxError::EFAULT)?;
                    total_read += n;
                }
                // No data this round. If we have not copied anything yet,
                // surface the error (EAGAIN for the non-blocking bridge) so the
                // client retries rather than seeing a 0-byte read as EOF.
                Err(e) => {
                    if total_read == 0 {
                        return Err(unix_err(e));
                    }
                    break;
                }
            }
        }

        if unix::endpoint_role(sockfd) == Some(UnixSocketRole::WaylandDisplay) {
            let _ = deliver_out_of_band_fds(sockfd, msg);
        }

        return Ok(total_read as isize);
    }

    let socket_id = fd_to_socket_id(sockfd)?;
    let want_peek = (flags & MSG_PEEK) != 0;
    let _ = flags & (MSG_DONTWAIT | MSG_WAITALL | MSG_NOSIGNAL | MSG_TRUNC);

    let (_, _, msg_iov, msg_iovlen) = read_msghdr_fields(msg)?;
    if msg_iov.is_null() && msg_iovlen > 0 {
        return Err(LinuxError::EFAULT);
    }

    let mut total_read = 0usize;
    for i in 0..msg_iovlen {
        let iov = copy_iovec_from_user(msg_iov, i)?;
        if iov.iov_base.is_null() && iov.iov_len > 0 {
            return Err(LinuxError::EFAULT);
        }
        if iov.iov_len == 0 {
            continue;
        }

        let copy_len = iov.iov_len.min(MAX_SOCKET_RW_CHUNK);
        let mut buffer = vec![0u8; copy_len];
        let sock = net::network_stack()
            .get_socket(socket_id)
            .ok_or(LinuxError::EBADF)?;
        if want_peek {
            match sock.recv_peek(&mut buffer) {
                Ok(n) => {
                    UserSpaceMemory::copy_to_user(iov.iov_base as u64, &buffer[..n])
                        .map_err(|_| LinuxError::EFAULT)?;
                    total_read += n;
                }
                Err(NetworkError::Timeout) => break,
                Err(e) => return Err(net_err_to_linux(e)),
            }
        } else {
            let mut sock = sock;
            match sock.recv(&mut buffer) {
                Ok(n) => {
                    UserSpaceMemory::copy_to_user(iov.iov_base as u64, &buffer[..n])
                        .map_err(|_| LinuxError::EFAULT)?;
                    total_read += n;
                }
                Err(NetworkError::Timeout) => break,
                Err(e) => return Err(net_err_to_linux(e)),
            }
        }
    }
    Ok(total_read as isize)
}

/// getsockopt - get socket option
pub fn getsockopt(
    sockfd: Fd,
    level: i32,
    optname: i32,
    optval: *mut u8,
    optlen: *mut u32,
) -> LinuxResult<i32> {
    inc_ops();

    if optval.is_null() || optlen.is_null() {
        return Err(LinuxError::EFAULT);
    }

    let socket_id = fd_to_socket_id(sockfd)?;
    let sock = net::network_stack()
        .get_socket(socket_id)
        .ok_or(LinuxError::EBADF)?;

    // SOL_SOCKET = 1
    if level != 1 {
        return Err(LinuxError::ENOPROTOOPT);
    }

    // Map common socket options
    let opt_type = match optname {
        2 => SocketOptionType::ReuseAddr,      // SO_REUSEADDR
        15 => SocketOptionType::ReusePort,     // SO_REUSEPORT
        9 => SocketOptionType::KeepAlive,      // SO_KEEPALIVE
        1 => SocketOptionType::NoDelay,        // SO_NODELAY (approx)
        8 => SocketOptionType::RecvBufferSize, // SO_RCVBUF
        7 => SocketOptionType::SendBufferSize, // SO_SNDBUF
        20 => SocketOptionType::RecvTimeout,   // SO_RCVTIMEO
        21 => SocketOptionType::SendTimeout,   // SO_SNDTIMEO
        _ => return Err(LinuxError::ENOPROTOOPT),
    };

    let opt = sock.get_option(opt_type).map_err(net_err_to_linux)?;
    let (bytes, len) = match opt {
        SocketOption::ReuseAddr(v)
        | SocketOption::ReusePort(v)
        | SocketOption::KeepAlive(v)
        | SocketOption::NoDelay(v) => ((v as i32).to_ne_bytes(), 4),
        SocketOption::RecvBufferSize(s) | SocketOption::SendBufferSize(s) => {
            ((s as i32).to_ne_bytes(), 4)
        }
        SocketOption::RecvTimeout(t) | SocketOption::SendTimeout(t) => {
            let val = t.unwrap_or(0);
            (val.to_ne_bytes(), 4)
        }
    };

    let avail = unsafe { *optlen };
    if (avail as usize) < len {
        return Err(LinuxError::EINVAL);
    }
    unsafe {
        core::ptr::copy_nonoverlapping(bytes.as_ptr(), optval, len);
        *optlen = len as u32;
    }
    Ok(0)
}

/// setsockopt - set socket option
pub fn setsockopt(
    sockfd: Fd,
    level: i32,
    optname: i32,
    optval: *const u8,
    optlen: u32,
) -> LinuxResult<i32> {
    inc_ops();

    if optval.is_null() {
        return Err(LinuxError::EFAULT);
    }

    let socket_id = fd_to_socket_id(sockfd)?;

    // SOL_SOCKET = 1
    if level != 1 {
        return Err(LinuxError::ENOPROTOOPT);
    }

    let val = if optlen >= 4 {
        unsafe { *(optval as *const i32) }
    } else {
        return Err(LinuxError::EINVAL);
    };

    let opt = match optname {
        2 => SocketOption::ReuseAddr(val != 0),  // SO_REUSEADDR
        15 => SocketOption::ReusePort(val != 0), // SO_REUSEPORT
        9 => SocketOption::KeepAlive(val != 0),  // SO_KEEPALIVE
        1 => SocketOption::NoDelay(val != 0),    // SO_NODELAY (approx)
        8 => SocketOption::RecvBufferSize(val as usize), // SO_RCVBUF
        7 => SocketOption::SendBufferSize(val as usize), // SO_SNDBUF
        20 => SocketOption::RecvTimeout(if val > 0 { Some(val as u32) } else { None }), // SO_RCVTIMEO
        21 => SocketOption::SendTimeout(if val > 0 { Some(val as u32) } else { None }), // SO_SNDTIMEO
        _ => return Err(LinuxError::ENOPROTOOPT),
    };

    // The network stack stores sockets in a map, so we get a clone, modify it,
    // and write it back so the option persists for subsequent operations.
    let mut sock = net::network_stack()
        .get_socket(socket_id)
        .ok_or(LinuxError::EBADF)?;
    sock.set_option(opt).map_err(net_err_to_linux)?;
    net::network_stack()
        .update_socket(socket_id, sock)
        .map_err(|_| LinuxError::EBADF)?;
    Ok(0)
}

/// getpeername - get peer socket address
pub fn getpeername(sockfd: Fd, addr: *mut SockAddr, addrlen: *mut u32) -> LinuxResult<i32> {
    inc_ops();

    if addr.is_null() || addrlen.is_null() {
        return Err(LinuxError::EFAULT);
    }

    let socket_id = fd_to_socket_id(sockfd)?;
    let sock = net::network_stack()
        .get_socket(socket_id)
        .ok_or(LinuxError::EBADF)?;

    let remote = sock.remote_address.ok_or(LinuxError::ENOTCONN)?;
    write_sockaddr(addr, addrlen, &remote)?;
    Ok(0)
}

/// getsockname - get socket address
pub fn getsockname(sockfd: Fd, addr: *mut SockAddr, addrlen: *mut u32) -> LinuxResult<i32> {
    inc_ops();

    if addr.is_null() || addrlen.is_null() {
        return Err(LinuxError::EFAULT);
    }

    let socket_id = fd_to_socket_id(sockfd)?;
    let sock = net::network_stack()
        .get_socket(socket_id)
        .ok_or(LinuxError::EBADF)?;

    let local = sock.local_address.ok_or(LinuxError::EINVAL)?;
    write_sockaddr(addr, addrlen, &local)?;
    Ok(0)
}

/// shutdown - shut down part of full-duplex connection
pub fn shutdown(sockfd: Fd, how: i32) -> LinuxResult<i32> {
    inc_ops();

    const SHUT_RD: i32 = 0;
    const SHUT_WR: i32 = 1;
    const SHUT_RDWR: i32 = 2;

    match how {
        SHUT_RD | SHUT_WR | SHUT_RDWR => {
            let socket_id = fd_to_socket_id(sockfd)?;
            let mut sock = net::network_stack()
                .get_socket(socket_id)
                .ok_or(LinuxError::EBADF)?;
            sock.shutdown(how).map_err(net_err_to_linux)?;
            net::network_stack()
                .update_socket(socket_id, sock)
                .map_err(|_| LinuxError::EBADF)?;
            Ok(0)
        }
        _ => Err(LinuxError::EINVAL),
    }
}

/// poll - wait for events on file descriptors
pub fn poll(fds: *mut PollFd, nfds: u64, timeout: i32) -> LinuxResult<i32> {
    super::special_fd::poll(fds, nfds, timeout)
}

/// select - synchronous I/O multiplexing
/// Implemented on top of poll.
pub fn select(
    nfds: i32,
    readfds: *mut u64,   // fd_set
    writefds: *mut u64,  // fd_set
    exceptfds: *mut u64, // fd_set
    timeout: *mut TimeVal,
) -> LinuxResult<i32> {
    inc_ops();

    if nfds < 0 {
        return Err(LinuxError::EINVAL);
    }

    // Convert timeout to milliseconds
    let timeout_ms = if timeout.is_null() {
        -1i32 // infinite
    } else {
        let tv = unsafe { &*timeout };
        let ms = tv.tv_sec as i32 * 1000 + tv.tv_usec as i32 / 1000;
        if ms < 0 {
            0
        } else {
            ms
        }
    };

    // FD_SETSIZE is typically 1024, each fd_set is 1024/64 = 16 u64s
    const FD_SETSIZE_DWORDS: usize = 16;

    // Build pollfd array from fd_sets
    let mut pollfds: [PollFd; 128] = [PollFd {
        fd: -1,
        events: 0,
        revents: 0,
    }; 128];
    let mut count = 0usize;

    for fd in 0..nfds {
        let fd_idx = fd as usize;
        let word = fd_idx / 64;
        let bit = fd_idx % 64;
        if word >= FD_SETSIZE_DWORDS {
            break;
        }

        let mut events = 0i16;
        if !readfds.is_null() {
            unsafe {
                if *readfds.add(word) & (1u64 << bit) != 0 {
                    events |= 0x001; // POLLIN
                }
            }
        }
        if !writefds.is_null() {
            unsafe {
                if *writefds.add(word) & (1u64 << bit) != 0 {
                    events |= 0x004; // POLLOUT
                }
            }
        }
        // exceptfds not mapped (rarely used)
        let _ = exceptfds;

        if events != 0 && count < pollfds.len() {
            pollfds[count] = PollFd {
                fd,
                events,
                revents: 0,
            };
            count += 1;
        }
    }

    if count == 0 {
        return Ok(0);
    }

    let _n = super::special_fd::poll(pollfds.as_mut_ptr(), count as u64, timeout_ms)?;

    // Clear fd_sets and set revents
    if !readfds.is_null() {
        unsafe {
            for i in 0..FD_SETSIZE_DWORDS {
                *readfds.add(i) = 0;
            }
        }
    }
    if !writefds.is_null() {
        unsafe {
            for i in 0..FD_SETSIZE_DWORDS {
                *writefds.add(i) = 0;
            }
        }
    }

    let mut ready = 0i32;
    for i in 0..count {
        if pollfds[i].revents != 0 {
            let fd = pollfds[i].fd;
            if fd >= 0 {
                let word = fd as usize / 64;
                let bit = fd as usize % 64;
                if word < FD_SETSIZE_DWORDS {
                    if !readfds.is_null() && pollfds[i].revents & 0x001 != 0 {
                        unsafe {
                            *readfds.add(word) |= 1u64 << bit;
                        }
                    }
                    if !writefds.is_null() && pollfds[i].revents & 0x004 != 0 {
                        unsafe {
                            *writefds.add(word) |= 1u64 << bit;
                        }
                    }
                }
                ready += 1;
            }
        }
    }

    Ok(ready)
}

/// pselect - synchronous I/O multiplexing with signal mask
pub fn pselect(
    nfds: i32,
    readfds: *mut u64,
    writefds: *mut u64,
    exceptfds: *mut u64,
    timeout: *const TimeSpec,
    sigmask: *const SigSet,
) -> LinuxResult<i32> {
    inc_ops();

    if nfds < 0 {
        return Err(LinuxError::EINVAL);
    }

    // Apply sigmask for the duration of the select if provided.
    let mut old_mask: SigSet = 0;
    let applied_mask = !sigmask.is_null();
    if applied_mask {
        super::signal_ops::sigprocmask(
            super::signal_ops::sig_how::SIG_SETMASK,
            sigmask,
            &mut old_mask as *mut SigSet,
        )?;
    }

    // Convert timespec to timeval for select
    let tv = if timeout.is_null() {
        TimeVal {
            tv_sec: 0,
            tv_usec: 0,
        }
    } else {
        let ts = unsafe { &*timeout };
        TimeVal {
            tv_sec: ts.tv_sec,
            tv_usec: (ts.tv_nsec / 1000) as i64,
        }
    };

    // If timeout is null, we want infinite wait. select with null timeout = infinite.
    let tv_ptr = if timeout.is_null() {
        core::ptr::null_mut()
    } else {
        &tv as *const TimeVal as *mut TimeVal
    };

    let result = select(nfds, readfds, writefds, exceptfds, tv_ptr);

    if applied_mask {
        super::signal_ops::sigprocmask(
            super::signal_ops::sig_how::SIG_SETMASK,
            &old_mask as *const SigSet,
            core::ptr::null_mut(),
        )?;
    }

    result
}

#[derive(Clone, Copy, Default)]
#[repr(C)]
struct Pselect6Sigmask {
    ss: u64,
    ss_len: usize,
}

/// pselect6 - Linux syscall ABI wrapper around pselect.
pub fn pselect6(
    nfds: i32,
    readfds: *mut u64,
    writefds: *mut u64,
    exceptfds: *mut u64,
    timeout: *const TimeSpec,
    sigmask: *const u8,
) -> LinuxResult<i32> {
    let mut mask_value: SigSet = 0;
    let mask_ptr = if sigmask.is_null() {
        core::ptr::null()
    } else {
        let mut raw = Pselect6Sigmask::default();
        let raw_bytes = unsafe {
            core::slice::from_raw_parts_mut(
                (&mut raw as *mut Pselect6Sigmask) as *mut u8,
                core::mem::size_of::<Pselect6Sigmask>(),
            )
        };
        UserSpaceMemory::copy_from_user(sigmask as u64, raw_bytes)
            .map_err(|_| LinuxError::EFAULT)?;
        if raw.ss == 0 {
            core::ptr::null()
        } else {
            if raw.ss_len != core::mem::size_of::<SigSet>() {
                return Err(LinuxError::EINVAL);
            }
            let mut mask_bytes = [0u8; core::mem::size_of::<SigSet>()];
            UserSpaceMemory::copy_from_user(raw.ss, &mut mask_bytes)
                .map_err(|_| LinuxError::EFAULT)?;
            mask_value = SigSet::from_ne_bytes(mask_bytes);
            &mask_value as *const SigSet
        }
    };

    pselect(nfds, readfds, writefds, exceptfds, timeout, mask_ptr)
}

/// epoll_create - create an epoll file descriptor
pub fn epoll_create(size: i32) -> LinuxResult<Fd> {
    inc_ops();

    if size <= 0 {
        return Err(LinuxError::EINVAL);
    }

    super::special_fd::epoll_create1(0)
}

/// epoll_create1 - create an epoll file descriptor with flags
pub fn epoll_create1(flags: i32) -> LinuxResult<Fd> {
    inc_ops();
    super::special_fd::epoll_create1(flags)
}

/// epoll_ctl - control an epoll file descriptor
pub fn epoll_ctl(epfd: Fd, op: i32, fd: Fd, event: *mut u8) -> LinuxResult<i32> {
    inc_ops();

    if epfd < 0 || fd < 0 {
        return Err(LinuxError::EBADF);
    }

    // Operation constants
    const EPOLL_CTL_ADD: i32 = 1;
    const EPOLL_CTL_DEL: i32 = 2;
    const EPOLL_CTL_MOD: i32 = 3;

    match op {
        EPOLL_CTL_ADD | EPOLL_CTL_DEL | EPOLL_CTL_MOD => {
            super::special_fd::epoll_ctl(epfd, op, fd, event)
        }
        _ => Err(LinuxError::EINVAL),
    }
}

/// epoll_wait - wait for events on an epoll file descriptor
pub fn epoll_wait(
    epfd: Fd,
    events: *mut u8, // struct epoll_event
    maxevents: i32,
    timeout: i32,
) -> LinuxResult<i32> {
    inc_ops();

    if epfd < 0 {
        return Err(LinuxError::EBADF);
    }

    if events.is_null() || maxevents <= 0 {
        return Err(LinuxError::EINVAL);
    }

    super::special_fd::epoll_wait(epfd, events, maxevents, timeout)
}

/// socket - create an endpoint for communication
pub fn socket(domain: i32, sock_type: i32, protocol: i32) -> LinuxResult<Fd> {
    inc_ops();

    // Validate domain
    match domain {
        1 | 2 | 10 | 16 | 17 | 18 => {}
        _ => return Err(LinuxError::EINVAL),
    }
    match sock_type & 0xFF {
        1 | 2 | 3 | 5 => {}
        _ => return Err(LinuxError::EINVAL),
    }

    // Map to network stack types
    let net_sock_type = match sock_type & 0xFF {
        1 => SocketType::Stream,   // SOCK_STREAM
        2 => SocketType::Datagram, // SOCK_DGRAM
        3 => SocketType::Raw,      // SOCK_RAW
        5 => SocketType::Datagram, // SOCK_SEQPACKET (approx)
        _ => return Err(LinuxError::EINVAL),
    };

    // Map protocol
    let net_proto = match protocol {
        0 => match sock_type & 0xFF {
            1 => Protocol::TCP,
            2 => Protocol::UDP,
            3 => Protocol::ICMP,
            _ => Protocol::TCP,
        },
        6 => Protocol::TCP,
        17 => Protocol::UDP,
        1 => Protocol::ICMP,
        _ => Protocol::TCP,
    };

    // AF_UNIX (1) - stream or datagram Unix domain socket
    if domain == 1 {
        let unix_type = unix::map_sock_type(sock_type);
        let fd = unix::create_socket_fd(
            unix_type,
            (sock_type & SOCK_NONBLOCK) != 0,
            (sock_type & SOCK_CLOEXEC) != 0,
        )
        .map_err(|_| LinuxError::EMFILE)?;
        return Ok(fd);
    }

    // AF_PACKET (17) - link-layer packet socket
    if domain == AF_PACKET {
        let eth_proto = if protocol == 0 { 0u16 } else { protocol as u16 };
        let socket_id = raw::create_packet_socket(eth_proto).map_err(net_err_to_linux)?;
        let inode = vfs::get_vfs().lookup("/").map_err(|_| LinuxError::ENOMEM)?;
        let mut fd_flags = vfs::OpenFlags::RDWR;
        if (sock_type & SOCK_NONBLOCK) != 0 {
            fd_flags |= vfs::OpenFlags::NONBLOCK;
        }
        if (sock_type & SOCK_CLOEXEC) != 0 {
            fd_flags |= vfs::OpenFlags::CLOEXEC;
        }
        let fd = vfs::vfs_open_special(inode, fd_flags, FdKind::Regular)
            .map_err(|_| LinuxError::EMFILE)?;
        raw::register_packet_fd(fd, socket_id);
        return Ok(fd);
    }

    // AF_INET SOCK_RAW
    if (sock_type & 0xFF) == 3 && domain == 2 {
        let raw_proto = if protocol == 0 { 0u8 } else { protocol as u8 };
        let socket_id = raw::create_raw_socket(raw_proto).map_err(net_err_to_linux)?;
        let inode = vfs::get_vfs().lookup("/").map_err(|_| LinuxError::ENOMEM)?;
        let mut fd_flags = vfs::OpenFlags::RDWR;
        if (sock_type & SOCK_NONBLOCK) != 0 {
            fd_flags |= vfs::OpenFlags::NONBLOCK;
        }
        if (sock_type & SOCK_CLOEXEC) != 0 {
            fd_flags |= vfs::OpenFlags::CLOEXEC;
        }
        let fd = vfs::vfs_open_special(inode, fd_flags, FdKind::Regular)
            .map_err(|_| LinuxError::EMFILE)?;
        raw::register_raw_fd(fd, socket_id);
        return Ok(fd);
    }

    let socket_id = net::network_stack()
        .create_socket(net_sock_type, net_proto)
        .map_err(net_err_to_linux)?;

    let new_fd = register_socket_fd(socket_id)?;

    // Apply SOCK_NONBLOCK / SOCK_CLOEXEC from the type parameter.
    let mut fd_flags: u32 = vfs::OpenFlags::RDWR;
    if (sock_type & SOCK_NONBLOCK) != 0 {
        fd_flags |= vfs::OpenFlags::NONBLOCK;
    }
    if (sock_type & SOCK_CLOEXEC) != 0 {
        fd_flags |= vfs::OpenFlags::CLOEXEC;
    }
    if fd_flags != vfs::OpenFlags::RDWR {
        let _ = vfs::vfs_set_fd_flags(new_fd, fd_flags);
    }

    Ok(new_fd)
}

/// bind - bind a name to a socket
pub fn bind(sockfd: Fd, addr: *const SockAddr, addrlen: u32) -> LinuxResult<i32> {
    inc_ops();

    if addr.is_null() {
        return Err(LinuxError::EFAULT);
    }

    // Check for AF_UNIX bind (sockaddr_un with sun_family == 1)
    let mut family_bytes = [0u8; 2];
    UserSpaceMemory::copy_from_user(addr as u64, &mut family_bytes)
        .map_err(|_| LinuxError::EFAULT)?;
    let family = u16::from_ne_bytes(family_bytes);

    if family == 1 {
        let path_str = parse_unix_path(addr, addrlen)?;
        unix::bind(sockfd, &path_str).map_err(unix_err)?;
        return Ok(0);
    }

    // AF_PACKET bind (sockaddr_ll)
    if family == AF_PACKET as u16 {
        if raw::is_packet_fd(sockfd) {
            // sockaddr_ll: family(2), protocol(2), ifindex(4), hatype(2), pkttype(1), halen(1), addr(8)
            let mut ll = [0u8; 20];
            UserSpaceMemory::copy_from_user(addr as u64, &mut ll)
                .map_err(|_| LinuxError::EFAULT)?;
            let ifindex = u32::from_ne_bytes([ll[4], ll[5], ll[6], ll[7]]);
            let protocol = u16::from_be_bytes([ll[2], ll[3]]);
            let id = raw::packet_id_for_fd_internal(sockfd).ok_or(LinuxError::EBADF)?;
            raw::packet_bind(id, ifindex, protocol).map_err(net_err_to_linux)?;
            return Ok(0);
        }
        return Err(LinuxError::EINVAL);
    }

    if raw::is_raw_fd(sockfd) {
        let sa = parse_sockaddr(addr, addrlen)?;
        let id = raw::raw_id_for_fd_internal(sockfd).ok_or(LinuxError::EBADF)?;
        raw::raw_bind(id, sa.address).map_err(net_err_to_linux)?;
        return Ok(0);
    }

    let socket_id = fd_to_socket_id(sockfd)?;
    let sa = parse_sockaddr(addr, addrlen)?;

    let mut sock = net::network_stack()
        .get_socket(socket_id)
        .ok_or(LinuxError::EBADF)?;

    sock.bind(sa).map_err(net_err_to_linux)?;
    Ok(0)
}

/// connect - initiate a connection on a socket
pub fn connect(sockfd: Fd, addr: *const SockAddr, addrlen: u32) -> LinuxResult<i32> {
    inc_ops();

    if addr.is_null() {
        return Err(LinuxError::EFAULT);
    }

    // Check for AF_UNIX connect
    let mut family_bytes = [0u8; 2];
    UserSpaceMemory::copy_from_user(addr as u64, &mut family_bytes)
        .map_err(|_| LinuxError::EFAULT)?;
    let family = u16::from_ne_bytes(family_bytes);

    if family == 1 {
        let path_str = parse_unix_path(addr, addrlen)?;
        unix::connect(sockfd, &path_str).map_err(unix_err)?;
        return Ok(0);
    }

    let socket_id = fd_to_socket_id(sockfd)?;
    let sa = parse_sockaddr(addr, addrlen)?;

    let mut sock = net::network_stack()
        .get_socket(socket_id)
        .ok_or(LinuxError::EBADF)?;

    sock.connect(sa).map_err(net_err_to_linux)?;
    Ok(0)
}

/// listen - listen for connections on a socket
pub fn listen(sockfd: Fd, backlog: i32) -> LinuxResult<i32> {
    inc_ops();

    if backlog < 0 {
        return Err(LinuxError::EINVAL);
    }

    if unix::is_unix_fd(sockfd) {
        unix::listen(sockfd, backlog).map_err(unix_err)?;
        return Ok(0);
    }

    let socket_id = fd_to_socket_id(sockfd)?;
    let mut sock = net::network_stack()
        .get_socket(socket_id)
        .ok_or(LinuxError::EBADF)?;

    sock.listen(backlog as u32).map_err(net_err_to_linux)?;
    Ok(0)
}

/// accept - accept a connection on a socket
pub fn accept(sockfd: Fd, addr: *mut SockAddr, addrlen: *mut u32) -> LinuxResult<Fd> {
    inc_ops();

    if unix::is_unix_fd(sockfd) {
        let client_fd = unix::accept(sockfd).map_err(unix_err)?;
        if !addr.is_null() && !addrlen.is_null() {
            let _ = (addr, addrlen);
        }
        return Ok(client_fd);
    }

    let socket_id = fd_to_socket_id(sockfd)?;
    let mut sock = net::network_stack()
        .get_socket(socket_id)
        .ok_or(LinuxError::EBADF)?;

    match sock.accept().map_err(net_err_to_linux)? {
        Some(new_socket_id) => {
            // Get the new socket to retrieve its address
            if !addr.is_null() && !addrlen.is_null() {
                if let Some(new_sock) = net::network_stack().get_socket(new_socket_id) {
                    if let Some(remote) = new_sock.remote_address {
                        write_sockaddr(addr, addrlen, &remote)?;
                    }
                }
            }
            register_socket_fd(new_socket_id)
        }
        None => Err(LinuxError::EAGAIN),
    }
}

/// accept4 - accept a connection on a socket with flags
pub fn accept4(sockfd: Fd, addr: *mut SockAddr, addrlen: *mut u32, flags: i32) -> LinuxResult<Fd> {
    inc_ops();

    let new_fd = accept(sockfd, addr, addrlen)?;

    // Apply SOCK_NONBLOCK and SOCK_CLOEXEC flags to the new fd.
    let mut fd_flags: u32 = vfs::OpenFlags::RDWR;
    if (flags & SOCK_NONBLOCK) != 0 {
        fd_flags |= vfs::OpenFlags::NONBLOCK;
    }
    if (flags & SOCK_CLOEXEC) != 0 {
        fd_flags |= vfs::OpenFlags::CLOEXEC;
    }

    // Only update if we're adding flags beyond the default RDWR.
    if fd_flags != vfs::OpenFlags::RDWR {
        let _ = vfs::vfs_set_fd_flags(new_fd, fd_flags);
    }
    Ok(new_fd)
}

/// socketpair - create a pair of connected sockets
/// For AF_UNIX, creates a bidirectional pipe.
pub fn socketpair(domain: i32, sock_type: i32, _protocol: i32, sv: *mut i32) -> LinuxResult<i32> {
    inc_ops();

    if sv.is_null() {
        return Err(LinuxError::EFAULT);
    }

    // Only AF_UNIX (1) and AF_LOCAL (1) supported
    if domain != 1 {
        return Err(LinuxError::EAFNOSUPPORT);
    }

    let (fd0, fd1) = unix::socketpair(
        (sock_type & SOCK_NONBLOCK) != 0,
        (sock_type & SOCK_CLOEXEC) != 0,
    )
    .map_err(unix_err)?;

    unsafe {
        *sv = fd0;
        *sv.offset(1) = fd1;
    }
    Ok(0)
}

/// sendmmsg - send multiple messages on a socket
pub fn sendmmsg(sockfd: Fd, msgvec: *mut u8, vlen: u32, flags: i32) -> LinuxResult<i32> {
    inc_ops();

    if sockfd < 0 {
        return Err(LinuxError::EBADF);
    }

    // Each mmsghdr is 32 bytes: { struct msghdr msg_hdr, unsigned int msg_len }
    // Process up to vlen messages
    let mut sent = 0i32;
    for i in 0..vlen {
        let msg_ptr = unsafe { msgvec.add((i as usize) * 32) };
        match sendmsg(sockfd, msg_ptr, flags) {
            Ok(n) => {
                // Store msg_len in the last 4 bytes of mmsghdr
                unsafe {
                    *(msg_ptr.add(24) as *mut u32) = n as u32;
                }
                sent += 1;
            }
            Err(_) => break,
        }
    }
    Ok(sent)
}

/// recvmmsg - receive multiple messages from a socket
pub fn recvmmsg(
    sockfd: Fd,
    msgvec: *mut u8,
    vlen: u32,
    flags: i32,
    timeout: *const u8,
) -> LinuxResult<i32> {
    inc_ops();

    if sockfd < 0 {
        return Err(LinuxError::EBADF);
    }

    let _ = timeout;

    let mut received = 0i32;
    for i in 0..vlen {
        let msg_ptr = unsafe { msgvec.add((i as usize) * 32) };
        match recvmsg(sockfd, msg_ptr, flags) {
            Ok(n) => {
                unsafe {
                    *(msg_ptr.add(24) as *mut u32) = n as u32;
                }
                received += 1;
                if n == 0 {
                    break;
                }
            }
            Err(_) => break,
        }
    }
    Ok(received)
}

/// Convert a timespec pointer to a poll/epoll timeout in milliseconds.
fn timespec_to_timeout_ms(ts: *const TimeSpec) -> i32 {
    if ts.is_null() {
        -1
    } else {
        let timespec = unsafe { &*ts };
        timespec.tv_sec as i32 * 1000 + timespec.tv_nsec as i32 / 1_000_000
    }
}

/// ppoll - poll with timeout and signal mask
pub fn ppoll(
    fds: *mut PollFd,
    nfds: u64,
    ts: *const crate::linux_compat::TimeSpec,
    sigmask: *const u8,
) -> LinuxResult<i32> {
    inc_ops();

    // Apply sigmask for the duration of the poll if provided.
    let mut old_mask: SigSet = 0;
    let applied_mask = !sigmask.is_null();
    if applied_mask {
        super::signal_ops::sigprocmask(
            super::signal_ops::sig_how::SIG_SETMASK,
            sigmask as *const SigSet,
            &mut old_mask as *mut SigSet,
        )?;
    }

    let result = poll(fds, nfds, timespec_to_timeout_ms(ts));

    if applied_mask {
        super::signal_ops::sigprocmask(
            super::signal_ops::sig_how::SIG_SETMASK,
            &old_mask as *const SigSet,
            core::ptr::null_mut(),
        )?;
    }

    result
}

/// epoll_pwait - wait for events with signal mask
pub fn epoll_pwait(
    epfd: Fd,
    events: *mut u8,
    maxevents: i32,
    timeout: i32,
    sigmask: *const u8,
) -> LinuxResult<i32> {
    inc_ops();

    // Apply sigmask for the duration of the wait if provided.
    let mut old_mask: SigSet = 0;
    let applied_mask = !sigmask.is_null();
    if applied_mask {
        super::signal_ops::sigprocmask(
            super::signal_ops::sig_how::SIG_SETMASK,
            sigmask as *const SigSet,
            &mut old_mask as *mut SigSet,
        )?;
    }

    let result = epoll_wait(epfd, events, maxevents, timeout);

    if applied_mask {
        super::signal_ops::sigprocmask(
            super::signal_ops::sig_how::SIG_SETMASK,
            &old_mask as *const SigSet,
            core::ptr::null_mut(),
        )?;
    }

    result
}

/// epoll_pwait2 - wait for events with timeout and signal mask
pub fn epoll_pwait2(
    epfd: Fd,
    events: *mut u8,
    maxevents: i32,
    timeout: *const u8,
    sigmask: *const u8,
) -> LinuxResult<i32> {
    inc_ops();

    // Apply sigmask for the duration of the wait if provided.
    let mut old_mask: SigSet = 0;
    let applied_mask = !sigmask.is_null();
    if applied_mask {
        super::signal_ops::sigprocmask(
            super::signal_ops::sig_how::SIG_SETMASK,
            sigmask as *const SigSet,
            &mut old_mask as *mut SigSet,
        )?;
    }

    let timeout_ms = timespec_to_timeout_ms(timeout as *const TimeSpec);
    let result = epoll_wait(epfd, events, maxevents, timeout_ms);

    if applied_mask {
        super::signal_ops::sigprocmask(
            super::signal_ops::sig_how::SIG_SETMASK,
            &old_mask as *const SigSet,
            core::ptr::null_mut(),
        )?;
    }

    result
}

#[cfg(any())]
mod tests {
    use super::*;

    #[test_case]
    fn test_socket_validation() {
        let buf = [0u8; 1024];
        assert!(send(-1, buf.as_ptr(), 1024, 0).is_err());
        assert!(recv(-1, buf.as_ptr() as *mut u8, 1024, 0).is_err());
    }

    #[test_case]
    fn test_shutdown_modes() {
        assert!(shutdown(3, 0).is_ok()); // SHUT_RD
        assert!(shutdown(3, 1).is_ok()); // SHUT_WR
        assert!(shutdown(3, 2).is_ok()); // SHUT_RDWR
        assert!(shutdown(3, 99).is_err()); // Invalid
    }
}
