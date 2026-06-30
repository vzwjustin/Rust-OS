//! Basic iptables-style netfilter hook points (accept/drop).
//!
//! Hooks are invoked from the IP and Ethernet receive paths before packets
//! are delivered locally or forwarded. Rules are evaluated in insertion order;
//! the first matching rule wins. If no rule matches, the packet is accepted.

use super::NetworkAddress;
use alloc::string::String;
use alloc::vec::Vec;
use lazy_static::lazy_static;
use spin::RwLock;

/// Netfilter hook points (subset of Linux netfilter).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Hook {
    /// Before routing decision on ingress (PREROUTING).
    PreRouting,
    /// Local delivery (INPUT).
    Input,
    /// Forwarding between interfaces (FORWARD).
    Forward,
    /// Locally generated traffic (OUTPUT).
    Output,
    /// After routing on egress (POSTROUTING).
    PostRouting,
}

/// Verdict returned by a hook chain.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Verdict {
    Accept,
    Drop,
}

/// Match criteria for a filter rule.
#[derive(Debug, Clone)]
pub struct RuleMatch {
    pub hook: Hook,
    pub src: Option<NetworkAddress>,
    pub dst: Option<NetworkAddress>,
    pub protocol: Option<u8>,
    pub in_interface: Option<String>,
    pub out_interface: Option<String>,
}

/// Filter rule action.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RuleAction {
    Accept,
    Drop,
}

/// A single filter rule.
#[derive(Debug, Clone)]
pub struct FilterRule {
    pub id: u32,
    pub match_fields: RuleMatch,
    pub action: RuleAction,
}

/// Packet metadata passed to netfilter hooks.
#[derive(Debug, Clone)]
pub struct PacketInfo {
    pub hook: Hook,
    pub src: NetworkAddress,
    pub dst: NetworkAddress,
    pub protocol: u8,
    pub in_interface: Option<String>,
    pub out_interface: Option<String>,
    pub is_local: bool,
}

struct NetfilterState {
    rules: Vec<FilterRule>,
    next_id: u32,
    dropped: u64,
    accepted: u64,
}

impl NetfilterState {
    fn new() -> Self {
        Self {
            rules: Vec::new(),
            next_id: 1,
            dropped: 0,
            accepted: 0,
        }
    }
}

lazy_static! {
    static ref NETFILTER: RwLock<NetfilterState> = RwLock::new(NetfilterState::new());
}

/// Initialize netfilter (install default-accept policy — no rules).
pub fn init() {
    let mut state = NETFILTER.write();
    state.rules.clear();
    state.next_id = 1;
    state.dropped = 0;
    state.accepted = 0;
}

/// Add a filter rule; returns the assigned rule id.
pub fn add_rule(match_fields: RuleMatch, action: RuleAction) -> u32 {
    let mut state = NETFILTER.write();
    let id = state.next_id;
    state.next_id = state.next_id.saturating_add(1);
    state.rules.push(FilterRule {
        id,
        match_fields,
        action,
    });
    id
}

/// Delete a rule by id.
pub fn delete_rule(id: u32) -> bool {
    let mut state = NETFILTER.write();
    let before = state.rules.len();
    state.rules.retain(|r| r.id != id);
    state.rules.len() != before
}

/// List all rules.
pub fn list_rules() -> Vec<FilterRule> {
    NETFILTER.read().rules.clone()
}

/// Evaluate hook chain for a packet.
pub fn check(info: &PacketInfo) -> Verdict {
    let rules = NETFILTER.read();
    for rule in rules.rules.iter() {
        if rule.match_fields.hook != info.hook {
            continue;
        }
        if let Some(src) = rule.match_fields.src {
            if src != info.src {
                continue;
            }
        }
        if let Some(dst) = rule.match_fields.dst {
            if dst != info.dst {
                continue;
            }
        }
        if let Some(proto) = rule.match_fields.protocol {
            if proto != info.protocol {
                continue;
            }
        }
        if let Some(ref iface) = rule.match_fields.in_interface {
            if info.in_interface.as_deref() != Some(iface.as_str()) {
                continue;
            }
        }
        if let Some(ref iface) = rule.match_fields.out_interface {
            if info.out_interface.as_deref() != Some(iface.as_str()) {
                continue;
            }
        }

        let verdict = match rule.action {
            RuleAction::Accept => Verdict::Accept,
            RuleAction::Drop => Verdict::Drop,
        };
        record_verdict(verdict);
        return verdict;
    }

    record_verdict(Verdict::Accept);
    Verdict::Accept
}

fn record_verdict(verdict: Verdict) {
    let mut state = NETFILTER.write();
    match verdict {
        Verdict::Accept => state.accepted += 1,
        Verdict::Drop => state.dropped += 1,
    }
}

/// Returns (accepted, dropped) packet counts.
pub fn stats() -> (u64, u64) {
    let state = NETFILTER.read();
    (state.accepted, state.dropped)
}

/// Convenience: drop all traffic to `dst` on INPUT.
pub fn drop_input_to(dst: NetworkAddress) -> u32 {
    add_rule(
        RuleMatch {
            hook: Hook::Input,
            src: None,
            dst: Some(dst),
            protocol: None,
            in_interface: None,
            out_interface: None,
        },
        RuleAction::Drop,
    )
}
