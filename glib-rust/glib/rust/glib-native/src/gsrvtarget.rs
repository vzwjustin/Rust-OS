//! GIO SRV record target matching `gio/gsrvtarget.h` / `gio/gsrvtarget.c`.
//!
//! `SrvTarget` is a boxed type representing a single host/port that a
//! network service is running on, with priority and weight fields
//! matching RFC 2782 SRV records. Provides construction, copy, accessors,
//! and `srv_target_list_sort` which implements the RFC 2782
//! priority/weight sorting algorithm (including the weighted-random
//! selection within a priority group).
//!
//! Fully `no_std` compatible using `alloc`.

use crate::prelude::*;
use crate::rand::{random_int_range, Rand};
use alloc::string::String;
use alloc::vec::Vec;

// ──────────────────────────── SrvTarget ───────────────────────────────────

/// A single SRV record target (`GSrvTarget`).
///
/// Mirrors `struct _GSrvTarget`: hostname, port, priority, weight.
/// Upstream uses a boxed type with manual malloc/free; we use a plain
/// `pub struct` with `Clone` (Rust's ownership model handles the
/// "copy/free" semantics).
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct SrvTarget {
    hostname: String,
    port: u16,
    priority: u16,
    weight: u16,
}

impl SrvTarget {
    /// Create a new SRV target (`g_srv_target_new`).
    pub fn new(hostname: &str, port: u16, priority: u16, weight: u16) -> Self {
        Self {
            hostname: hostname.to_owned(),
            port,
            priority,
            weight,
        }
    }

    /// Hostname (ASCII form; may be Punycode-encoded for IDN)
    /// (`g_srv_target_get_hostname`).
    pub fn hostname(&self) -> &str {
        &self.hostname
    }

    /// Port (`g_srv_target_get_port`).
    pub fn port(&self) -> u16 {
        self.port
    }

    /// Priority (`g_srv_target_get_priority`).
    pub fn priority(&self) -> u16 {
        self.priority
    }

    /// Weight (`g_srv_target_get_weight`).
    pub fn weight(&self) -> u16 {
        self.weight
    }
}

// ─────────────────────── srv_target_list_sort ─────────────────────────────

/// Sort a list of SRV targets according to RFC 2782
/// (`g_srv_target_list_sort`).
///
/// The algorithm:
/// 1. If a single target with hostname `"."` is present, return an
///    empty list (the service is decidedly not available at this
///    domain — RFC 2782).
/// 2. Sort by priority ascending; within a priority group, targets
///    with weight 0 come first.
/// 3. For each priority group, repeatedly pick a target at random
///    with probability proportional to its weight, remove it from the
///    group, and append it to the output. This implements the
///    weighted-random ordering from RFC 2782 §3.
///
/// Returns a new sorted `Vec<SrvTarget>`; the input is consumed.
///
/// **Note**: upstream mutates the input `GList` in place and returns
/// the new head. We return a new `Vec` (the input `Vec` is taken by
/// value and consumed), which is the idiomatic Rust equivalent.
pub fn srv_target_list_sort(targets: Vec<SrvTarget>) -> Vec<SrvTarget> {
    if targets.is_empty() {
        return Vec::new();
    }

    // Single-target special case: hostname "." means the service is
    // not available; return empty.
    if targets.len() == 1 && targets[0].hostname == "." {
        return Vec::new();
    }

    // Sort by (priority, weight) ascending. Within a priority group,
    // weight-0 targets come first (matching the upstream
    // compare_target helper).
    let mut sorted = targets;
    sorted.sort_by(|a, b| {
        if a.priority == b.priority {
            a.weight.cmp(&b.weight)
        } else {
            a.priority.cmp(&b.priority)
        }
    });

    let mut out: Vec<SrvTarget> = Vec::with_capacity(sorted.len());
    let mut remaining = sorted;

    // Process each priority group.
    while !remaining.is_empty() {
        let priority = remaining[0].priority;

        // Count the targets at this priority level and sum their
        // weights. RFC 2782: "If there is precisely one SRV RR with a
        // priority of 0, it's the only one used." Otherwise,
        // weighted-random selection within the group.
        let mut group_end = 0usize;
        let mut sum: u32 = 0;
        while group_end < remaining.len() && remaining[group_end].priority == priority {
            sum += remaining[group_end].weight as u32;
            group_end += 1;
        }

        // Repeatedly pick a target from this priority group with
        // probability proportional to weight, remove it, append to
        // output.
        while group_end > 0 {
            // val in [0, sum]. With sum == 0 (all weights 0), pick
            // the first remaining target deterministically.
            let val = if sum == 0 {
                0u32
            } else {
                random_int_range(0, (sum + 1) as i32) as u32
            };

            let mut picked = 0usize;
            let mut acc = 0u32;
            for i in 0..group_end {
                let weight = remaining[i].weight as u32;
                if weight >= val - acc {
                    picked = i;
                    break;
                }
                acc += weight;
            }

            let target = remaining.remove(picked);
            sum -= target.weight as u32;
            group_end -= 1;
            out.push(target);
        }
    }

    out
}

// ───────────────────────────── tests ──────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn t(hostname: &str, port: u16, priority: u16, weight: u16) -> SrvTarget {
        SrvTarget::new(hostname, port, priority, weight)
    }

    #[test]
    fn new_and_accessors() {
        let target = SrvTarget::new("example.com", 5223, 10, 60);
        assert_eq!(target.hostname(), "example.com");
        assert_eq!(target.port(), 5223);
        assert_eq!(target.priority(), 10);
        assert_eq!(target.weight(), 60);
    }

    #[test]
    fn clone_preserves_fields() {
        let target = t("example.com", 80, 1, 0);
        let cloned = target.clone();
        assert_eq!(cloned.hostname(), target.hostname());
        assert_eq!(cloned.port(), target.port());
        assert_eq!(cloned.priority(), target.priority());
        assert_eq!(cloned.weight(), target.weight());
        assert_eq!(cloned, target);
    }

    #[test]
    fn eq_and_hash() {
        let a = t("h", 1, 2, 3);
        let b = t("h", 1, 2, 3);
        let c = t("h", 1, 2, 4);
        assert_eq!(a, b);
        assert_ne!(a, c);
    }

    #[test]
    fn list_sort_empty_returns_empty() {
        let result = srv_target_list_sort(Vec::new());
        assert!(result.is_empty());
    }

    #[test]
    fn list_sort_single_dot_hostname_returns_empty() {
        // RFC 2782: a Target of "." means the service is decidedly not
        // available at this domain.
        let input = vec![t(".", 0, 0, 0)];
        let result = srv_target_list_sort(input);
        assert!(result.is_empty());
    }

    #[test]
    fn list_sort_single_normal_target_returns_unchanged() {
        let input = vec![t("example.com", 80, 1, 0)];
        let result = srv_target_list_sort(input);
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].hostname(), "example.com");
    }

    #[test]
    fn list_sort_orders_by_priority() {
        let input = vec![
            t("c.example.com", 80, 30, 0),
            t("a.example.com", 80, 10, 0),
            t("b.example.com", 80, 20, 0),
        ];
        let result = srv_target_list_sort(input);
        assert_eq!(result.len(), 3);
        // Priorities should be ascending: 10, 20, 30.
        assert_eq!(result[0].priority(), 10);
        assert_eq!(result[1].priority(), 20);
        assert_eq!(result[2].priority(), 30);
    }

    #[test]
    fn list_sort_zero_weight_targets_present_in_group() {
        // RFC 2782: within a priority group, the initial sort places
        // weight-0 targets first, but the weighted-random selection
        // then redistributes them. We can only assert that all targets
        // survive and are in the same priority group.
        let input = vec![
            t("heavy.example.com", 80, 10, 100),
            t("zero.example.com", 80, 10, 0),
            t("medium.example.com", 80, 10, 50),
        ];
        let result = srv_target_list_sort(input);
        assert_eq!(result.len(), 3);
        // All same priority (10).
        for r in &result {
            assert_eq!(r.priority(), 10);
        }
        // All three hostnames should be present.
        let hosts: Vec<&str> = result.iter().map(|t| t.hostname()).collect();
        assert!(hosts.contains(&"heavy.example.com"));
        assert!(hosts.contains(&"zero.example.com"));
        assert!(hosts.contains(&"medium.example.com"));
    }

    #[test]
    fn list_sort_preserves_all_targets() {
        let input = vec![
            t("a", 80, 10, 0),
            t("b", 80, 10, 0),
            t("c", 80, 20, 0),
            t("d", 80, 20, 0),
            t("e", 80, 30, 0),
        ];
        let result = srv_target_list_sort(input);
        assert_eq!(result.len(), 5);
        // All 5 targets should be present, sorted by priority.
        assert_eq!(result[0].priority(), 10);
        assert_eq!(result[1].priority(), 10);
        assert_eq!(result[2].priority(), 20);
        assert_eq!(result[3].priority(), 20);
        assert_eq!(result[4].priority(), 30);
    }

    #[test]
    fn list_sort_single_zero_priority_target() {
        // RFC 2782: "If there is precisely one SRV RR with a priority
        // of 0, it's the only one used." We don't special-case this
        // (the general algorithm handles it), but verify a single
        // priority-0 target survives.
        let input = vec![t("only.example.com", 80, 0, 0)];
        let result = srv_target_list_sort(input);
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].hostname(), "only.example.com");
        assert_eq!(result[0].priority(), 0);
    }

    #[test]
    fn list_sort_weighted_selection_distributes_targets() {
        // With many targets of varying weights in the same priority
        // group, the weighted-random selection should eventually
        // produce all of them (statistical sanity check — we don't
        // verify the distribution, just that all targets survive).
        let mut input = Vec::new();
        for i in 0..10u16 {
            input.push(t(&format!("h{i}.example.com"), 80, 10, i * 10));
        }
        let result = srv_target_list_sort(input);
        assert_eq!(result.len(), 10);
        // All hostnames should be present.
        let mut hosts: Vec<String> = result.iter().map(|t| t.hostname().to_owned()).collect();
        hosts.sort();
        for i in 0..10u16 {
            assert!(hosts.contains(&format!("h{i}.example.com")), "missing h{i}");
        }
    }

    #[test]
    fn list_sort_consumes_input() {
        let input = vec![t("a", 80, 1, 0), t("b", 80, 2, 0)];
        // Take ownership; the input vec is moved.
        let result = srv_target_list_sort(input);
        assert_eq!(result.len(), 2);
        // input is no longer accessible (moved).
    }
}
