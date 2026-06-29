//! IPv4 routing table with netlink-style internal API and ioctl support.
//!
//! Routes are matched using longest-prefix semantics. The routing table is
//! consulted by the IP layer for forwarding and by outbound packet builders
//! when selecting an egress interface and next-hop gateway.

use super::{NetworkAddress, NetworkError, NetworkResult};
use alloc::string::String;
use alloc::vec::Vec;
use spin::RwLock;

/// Routing table entry (Linux `struct rtentry` equivalent, simplified).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RouteEntry {
    /// Destination network (host bits cleared by netmask).
    pub destination: NetworkAddress,
    /// Network mask.
    pub netmask: NetworkAddress,
    /// Optional gateway (next hop).
    pub gateway: Option<NetworkAddress>,
    /// Output interface name.
    pub interface: String,
    /// Route metric (lower is preferred).
    pub metric: u32,
}

/// Netlink-style route message types for in-kernel route management.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RouteMsgType {
    NewRoute,
    DelRoute,
    GetRoute,
    DumpRoutes,
}

/// Netlink-style route request.
#[derive(Debug, Clone)]
pub struct RouteRequest {
    pub msg_type: RouteMsgType,
    pub route: RouteEntry,
}

/// IPv4 routing table.
pub struct RoutingTable {
    routes: RwLock<Vec<RouteEntry>>,
}

impl RoutingTable {
    pub fn new() -> Self {
        Self {
            routes: RwLock::new(Vec::new()),
        }
    }

    /// Add a route (append; use `add_validated` for conflict resolution).
    pub fn add(&self, route: RouteEntry) -> NetworkResult<()> {
        self.routes.write().push(route);
        Ok(())
    }

    /// Add route with validation and replace conflicting entries.
    pub fn add_validated(
        &self,
        route: RouteEntry,
        interface_exists: impl Fn(&str) -> bool,
        gateway_reachable: impl Fn(&str, &NetworkAddress) -> bool,
    ) -> NetworkResult<()> {
        validate_route(&route, &interface_exists, &gateway_reachable)?;

        let mut routes = self.routes.write();

        routes.retain(|existing| {
            !(existing.destination == route.destination
                && existing.netmask == route.netmask
                && existing.interface == route.interface)
        });

        let insert_pos = routes
            .iter()
            .position(|existing| {
                let existing_prefix = prefix_length(&existing.netmask);
                let new_prefix = prefix_length(&route.netmask);
                existing_prefix < new_prefix
                    || (existing_prefix == new_prefix && existing.metric > route.metric)
            })
            .unwrap_or(routes.len());

        routes.insert(insert_pos, route);
        Ok(())
    }

    /// Delete a route matching destination, netmask, and interface.
    pub fn delete(
        &self,
        destination: &NetworkAddress,
        netmask: &NetworkAddress,
        interface: &str,
    ) -> NetworkResult<()> {
        let mut routes = self.routes.write();
        let before = routes.len();
        routes.retain(|r| {
            !(r.destination == *destination && r.netmask == *netmask && r.interface == interface)
        });
        if routes.len() == before {
            return Err(NetworkError::NotFound);
        }
        Ok(())
    }

    /// Delete by exact route entry match.
    pub fn delete_entry(&self, route: &RouteEntry) -> NetworkResult<()> {
        self.delete(&route.destination, &route.netmask, &route.interface)
    }

    /// List all routes (copy).
    pub fn list(&self) -> Vec<RouteEntry> {
        self.routes.read().clone()
    }

    /// Route count.
    pub fn len(&self) -> usize {
        self.routes.read().len()
    }

    /// Longest-prefix match for `destination`.
    pub fn find(&self, destination: &NetworkAddress) -> Option<RouteEntry> {
        let routes = self.routes.read();
        let mut best: Option<RouteEntry> = None;
        let mut best_prefix = 0u32;

        for route in routes.iter() {
            if address_matches_route(destination, &route.destination, &route.netmask) {
                let plen = prefix_length(&route.netmask);
                if best.is_none()
                    || plen > best_prefix
                    || (plen == best_prefix && route.metric < best.as_ref().unwrap().metric)
                {
                    best = Some(route.clone());
                    best_prefix = plen;
                }
            }
        }
        best
    }

    /// Process a netlink-style route request.
    pub fn handle_request(
        &self,
        req: RouteRequest,
        interface_exists: impl Fn(&str) -> bool,
        gateway_reachable: impl Fn(&str, &NetworkAddress) -> bool,
    ) -> NetworkResult<Vec<RouteEntry>> {
        match req.msg_type {
            RouteMsgType::NewRoute => {
                self.add_validated(req.route, interface_exists, gateway_reachable)?;
                Ok(Vec::new())
            }
            RouteMsgType::DelRoute => {
                self.delete_entry(&req.route)?;
                Ok(Vec::new())
            }
            RouteMsgType::GetRoute => {
                if let Some(found) = self.find(&req.route.destination) {
                    Ok(alloc::vec![found])
                } else {
                    Err(NetworkError::NoRoute)
                }
            }
            RouteMsgType::DumpRoutes => Ok(self.list()),
        }
    }
}

impl Default for RoutingTable {
    fn default() -> Self {
        Self::new()
    }
}

fn prefix_length(netmask: &NetworkAddress) -> u32 {
    match netmask {
        NetworkAddress::IPv4(mask) => {
            let mask_u32 = ((mask[0] as u32) << 24)
                | ((mask[1] as u32) << 16)
                | ((mask[2] as u32) << 8)
                | (mask[3] as u32);
            mask_u32.leading_ones()
        }
        _ => 0,
    }
}

fn address_matches_route(
    addr: &NetworkAddress,
    dest: &NetworkAddress,
    mask: &NetworkAddress,
) -> bool {
    match (addr, dest, mask) {
        (NetworkAddress::IPv4(a), NetworkAddress::IPv4(d), NetworkAddress::IPv4(m)) => {
            for i in 0..4 {
                if (a[i] & m[i]) != (d[i] & m[i]) {
                    return false;
                }
            }
            true
        }
        _ => false,
    }
}

fn validate_route(
    route: &RouteEntry,
    interface_exists: &impl Fn(&str) -> bool,
    gateway_reachable: &impl Fn(&str, &NetworkAddress) -> bool,
) -> NetworkResult<()> {
    if !interface_exists(&route.interface) {
        return Err(NetworkError::InvalidAddress);
    }

    match (&route.destination, &route.netmask) {
        (NetworkAddress::IPv4(dest), NetworkAddress::IPv4(mask)) => {
            for i in 0..4 {
                if (dest[i] & mask[i]) != dest[i] {
                    return Err(NetworkError::InvalidAddress);
                }
            }
        }
        _ => return Err(NetworkError::NotSupported),
    }

    if let Some(gateway) = &route.gateway {
        if !gateway_reachable(&route.interface, gateway) {
            return Err(NetworkError::NoRoute);
        }
    }

    Ok(())
}

/// Build an IPv4 netmask from prefix length.
pub fn netmask_from_prefix(prefix: u8) -> NetworkAddress {
    if prefix == 0 {
        return NetworkAddress::ipv4(0, 0, 0, 0);
    }
    let bits = if prefix >= 32 {
        0xFFFF_FFFFu32
    } else {
        !0u32 << (32 - prefix)
    };
    NetworkAddress::ipv4(
        ((bits >> 24) & 0xFF) as u8,
        ((bits >> 16) & 0xFF) as u8,
        ((bits >> 8) & 0xFF) as u8,
        (bits & 0xFF) as u8,
    )
}

/// Apply destination netmask to an address.
pub fn apply_netmask(addr: &NetworkAddress, mask: &NetworkAddress) -> NetworkAddress {
    match (addr, mask) {
        (NetworkAddress::IPv4(a), NetworkAddress::IPv4(m)) => {
            NetworkAddress::ipv4(a[0] & m[0], a[1] & m[1], a[2] & m[2], a[3] & m[3])
        }
        _ => *addr,
    }
}

// ---------------------------------------------------------------------------
// Linux ioctl route control (SIOCADDRT / SIOCDELRT)
// ---------------------------------------------------------------------------

/// Linux `SIOCADDRT` — add an IPv4 route.
pub const SIOCADDRT: u64 = 0x890B;
/// Linux `SIOCDELRT` — delete an IPv4 route.
pub const SIOCDELRT: u64 = 0x890C;
/// Linux `SIOCRTMSG` route table changed notification.
pub const SIOCRTMSG: u64 = 0x890D;

/// Parse a Linux `struct rtentry`-like blob from userspace.
///
/// Layout used (x86_64, simplified):
///   offset 16: r_dst   (sockaddr, 16 bytes for AF_INET)
///   offset 32: r_gateway
///   offset 48: r_genmask
///   offset 64: r_flags (u16)
///   offset 68: r_metric (i16)
///   offset 72: interface name (16 bytes, null-terminated)
pub fn parse_rtentry_from_user(
    copy_from_user: impl Fn(u64, &mut [u8]) -> Result<(), ()>,
    argp: u64,
) -> Result<RouteEntry, NetworkError> {
    let mut buf = [0u8; 128];
    copy_from_user(argp, &mut buf).map_err(|_| NetworkError::InvalidArgument)?;

    let dst = parse_sockaddr_in(&buf[16..32])?;
    let gateway = parse_sockaddr_in(&buf[32..48]).ok();
    let mask = parse_sockaddr_in(&buf[48..64])?;
    let metric = i16::from_ne_bytes([buf[68], buf[69]]).max(0) as u32;

    let mut ifname = [0u8; 16];
    ifname.copy_from_slice(&buf[72..88]);
    let iface_end = ifname.iter().position(|&b| b == 0).unwrap_or(ifname.len());
    let interface = alloc::string::String::from(
        core::str::from_utf8(&ifname[..iface_end]).map_err(|_| NetworkError::InvalidArgument)?,
    );

    Ok(RouteEntry {
        destination: dst,
        netmask: mask,
        gateway,
        interface,
        metric,
    })
}

fn parse_sockaddr_in(data: &[u8]) -> Result<NetworkAddress, NetworkError> {
    if data.len() < 8 {
        return Err(NetworkError::InvalidArgument);
    }
    let family = u16::from_ne_bytes([data[0], data[1]]);
    if family != 2 && family != 0 {
        return Err(NetworkError::NotSupported);
    }
    if family == 0 {
        return Ok(NetworkAddress::ipv4(0, 0, 0, 0));
    }
    Ok(NetworkAddress::ipv4(data[4], data[5], data[6], data[7]))
}

/// Handle routing ioctls against the global routing table.
pub fn handle_route_ioctl(
    request: u64,
    argp: u64,
    table: &RoutingTable,
    interface_exists: impl Fn(&str) -> bool,
    gateway_reachable: impl Fn(&str, &NetworkAddress) -> bool,
    copy_from_user: impl Fn(u64, &mut [u8]) -> Result<(), ()>,
) -> Result<i32, NetworkError> {
    match request {
        SIOCADDRT => {
            let route = parse_rtentry_from_user(&copy_from_user, argp)?;
            table.add_validated(route, interface_exists, gateway_reachable)?;
            Ok(0)
        }
        SIOCDELRT => {
            let route = parse_rtentry_from_user(&copy_from_user, argp)?;
            table.delete_entry(&route)?;
            Ok(0)
        }
        SIOCRTMSG => Ok(0),
        _ => Err(NetworkError::NotSupported),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn longest_prefix_match() {
        let table = RoutingTable::new();
        table
            .add(RouteEntry {
                destination: NetworkAddress::ipv4(10, 0, 0, 0),
                netmask: NetworkAddress::ipv4(255, 0, 0, 0),
                gateway: None,
                interface: "eth0".into(),
                metric: 0,
            })
            .unwrap();
        table
            .add(RouteEntry {
                destination: NetworkAddress::ipv4(10, 1, 0, 0),
                netmask: NetworkAddress::ipv4(255, 255, 0, 0),
                gateway: None,
                interface: "eth0".into(),
                metric: 0,
            })
            .unwrap();

        let hit = table.find(&NetworkAddress::ipv4(10, 1, 2, 3)).unwrap();
        assert_eq!(hit.destination, NetworkAddress::ipv4(10, 1, 0, 0));
    }
}
