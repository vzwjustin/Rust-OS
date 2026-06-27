//! Main loop and event sources matching `gmain.h` / `gmain.c`.
//!
//! Provides the core types for GLib's main event loop. The actual
//! poll/dispatch mechanism requires OS support and is abstracted via
//! a platform trait. Fully `no_std` compatible using `alloc` and `spin`.

use crate::poll::{g_poll, PollFD};
use crate::prelude::*;
use crate::timer::monotonic_time_us;
use alloc::collections::BTreeMap;

/// Source continue/remove constants.
pub const SOURCE_CONTINUE: bool = true;
pub const SOURCE_REMOVE: bool = false;

/// Main context flags (`GMainContextFlags`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum MainContextFlags {
    None,
    OwnerlessPolling,
}

/// Source flags (`GSourceFlags`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct SourceFlags(pub u32);

impl SourceFlags {
    pub const NONE: SourceFlags = SourceFlags(0);
    pub const SKIP: SourceFlags = SourceFlags(1 << 0);
    pub const READY: SourceFlags = SourceFlags(1 << 1);
    pub const CAN_RECURSE: SourceFlags = SourceFlags(1 << 2);

    pub fn contains(self, other: SourceFlags) -> bool {
        self.0 & other.0 == other.0
    }
}

impl core::ops::BitOr for SourceFlags {
    type Output = SourceFlags;
    fn bitor(self, rhs: SourceFlags) -> SourceFlags {
        SourceFlags(self.0 | rhs.0)
    }
}

/// Source function callback (`GSourceFunc`).
pub type SourceFunc = fn() -> bool;

/// Source prepare function (`GSourceFuncsPrepareFunc`).
pub type SourcePrepareFunc = fn(&Source) -> (bool, i32);

/// Source check function (`GSourceFuncsCheckFunc`).
pub type SourceCheckFunc = fn(&Source) -> bool;

/// Source dispatch function (`GSourceFuncsDispatchFunc`).
pub type SourceDispatchFunc = fn(&Source) -> bool;

/// Source finalize function.
pub type SourceFinalizeFunc = fn(&Source);

/// Source callback functions (`GSourceCallbackFuncs`).
pub struct SourceCallbackFuncs {
    pub ref_: fn(),
    pub unref: fn(),
}

/// Source functions table (`GSourceFuncs`).
pub struct SourceFuncs {
    pub prepare: Option<SourcePrepareFunc>,
    pub check: Option<SourceCheckFunc>,
    pub dispatch: Option<SourceDispatchFunc>,
    pub finalize: Option<SourceFinalizeFunc>,
}

/// Source kind for built-in dispatch logic.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum SourceKind {
    Generic,
    Timeout,
    Idle,
}

/// An event source (`GSource`).
pub struct Source {
    pub id: u32,
    pub priority: i32,
    pub flags: SourceFlags,
    pub name: String,
    pub ready_time: Option<i64>,
    kind: SourceKind,
    funcs: SourceFuncs,
    callback: Option<SourceFunc>,
    poll_fds: Vec<PollFD>,
}

impl Source {
    /// Create a new source (`g_source_new`).
    pub fn new(id: u32, funcs: SourceFuncs) -> Self {
        Self {
            id,
            priority: 0,
            flags: SourceFlags::NONE,
            name: String::new(),
            ready_time: None,
            kind: SourceKind::Generic,
            funcs,
            callback: None,
            poll_fds: Vec::new(),
        }
    }

    /// Create a timeout source that fires after `interval_ms` from attach time.
    pub fn new_timeout(interval_ms: u32) -> Self {
        Self {
            id: 0,
            priority: 0,
            flags: SourceFlags::NONE,
            name: "timeout".to_owned(),
            ready_time: None,
            kind: SourceKind::Timeout,
            funcs: SourceFuncs {
                prepare: None,
                check: None,
                dispatch: None,
                finalize: None,
            },
            callback: None,
            poll_fds: Vec::new(),
        }
        .with_deadline(monotonic_time_ms() + interval_ms as i64)
    }

    /// Create an idle source (always ready when iterated).
    pub fn new_idle() -> Self {
        Self {
            id: 0,
            priority: 0,
            flags: SourceFlags::NONE,
            name: "idle".to_owned(),
            ready_time: None,
            kind: SourceKind::Idle,
            funcs: SourceFuncs {
                prepare: None,
                check: None,
                dispatch: None,
                finalize: None,
            },
            callback: None,
            poll_fds: Vec::new(),
        }
    }

    fn with_deadline(mut self, deadline_ms: i64) -> Self {
        self.ready_time = Some(deadline_ms);
        self
    }

    fn monotonic_deadline_ms(&self) -> Option<i64> {
        self.ready_time
    }

    /// Set the callback (`g_source_set_callback`).
    pub fn set_callback(&mut self, callback: SourceFunc) {
        self.callback = Some(callback);
    }

    /// Set priority (`g_source_set_priority`).
    pub fn set_priority(&mut self, priority: i32) {
        self.priority = priority;
    }

    /// Set name (`g_source_set_name`).
    pub fn set_name(&mut self, name: &str) {
        self.name = name.to_owned();
    }

    /// Set ready time (`g_source_set_ready_time`).
    pub fn set_ready_time(&mut self, ready_time: i64) {
        self.ready_time = Some(ready_time);
    }

    /// Set flags (`g_source_set_flags`).
    pub fn set_flags(&mut self, flags: SourceFlags) {
        self.flags = flags;
    }

    /// Add a poll FD (`g_source_add_poll`).
    pub fn add_poll(&mut self, fd: PollFD) {
        self.poll_fds.push(fd);
    }

    /// Remove a poll FD (`g_source_remove_poll`).
    pub fn remove_poll(&mut self, fd: &PollFD) {
        self.poll_fds.retain(|p| p.fd != fd.fd);
    }

    /// Get the source ID (`g_source_get_id`).
    pub fn get_id(&self) -> u32 {
        self.id
    }

    /// Get the name (`g_source_get_name`).
    pub fn get_name(&self) -> &str {
        &self.name
    }

    /// Get priority (`g_source_get_priority`).
    pub fn get_priority(&self) -> i32 {
        self.priority
    }

    /// Get flags (`g_source_get_flags`).
    pub fn get_flags(&self) -> SourceFlags {
        self.flags
    }

    /// Get ready time (`g_source_get_ready_time`).
    pub fn get_ready_time(&self) -> Option<i64> {
        self.ready_time
    }

    /// Check if source is ready (calls prepare + check).
    pub fn check(&self) -> bool {
        if self.kind == SourceKind::Timeout {
            return timeout_is_ready(self);
        }
        if self.kind == SourceKind::Idle {
            return true;
        }
        if !self.poll_fds.is_empty()
            && self
                .poll_fds
                .iter()
                .any(|pfd| pfd.revents & pfd.events != 0)
        {
            return true;
        }
        if let Some(check_fn) = self.funcs.check {
            return check_fn(self);
        }
        false
    }

    /// Dispatch the source (calls dispatch with callback).
    pub fn dispatch(&self) -> bool {
        if let Some(dispatch_fn) = self.funcs.dispatch {
            return dispatch_fn(self);
        }
        if let Some(callback) = self.callback {
            return callback();
        }
        SOURCE_REMOVE
    }

    /// Prepare the source.
    pub fn prepare(&self) -> (bool, i32) {
        if self.kind == SourceKind::Timeout {
            return timeout_prepare(self);
        }
        if self.kind == SourceKind::Idle {
            return (true, 0);
        }
        if let Some(prepare_fn) = self.funcs.prepare {
            return prepare_fn(self);
        }
        (false, -1)
    }

    /// Get poll FDs.
    pub fn get_poll_fds(&self) -> &[PollFD] {
        &self.poll_fds
    }
}

fn monotonic_time_ms() -> i64 {
    monotonic_time_us() / 1000
}

fn timeout_prepare(source: &Source) -> (bool, i32) {
    let deadline = source.monotonic_deadline_ms().unwrap_or(0);
    let now = monotonic_time_ms();
    if now >= deadline {
        return (true, 0);
    }
    (false, (deadline - now) as i32)
}

fn timeout_is_ready(source: &Source) -> bool {
    let deadline = source.monotonic_deadline_ms().unwrap_or(0);
    monotonic_time_ms() >= deadline
}

/// A main context (`GMainContext`).
///
/// Holds a collection of sources and manages their dispatch.
pub struct MainContext {
    sources: BTreeMap<u32, Source>,
    next_id: u32,
    flags: MainContextFlags,
}

impl MainContext {
    /// Create a new main context (`g_main_context_new`).
    pub fn new() -> Self {
        Self {
            sources: BTreeMap::new(),
            next_id: 1,
            flags: MainContextFlags::None,
        }
    }

    /// Create with flags (`g_main_context_new_with_flags`).
    pub fn new_with_flags(flags: MainContextFlags) -> Self {
        Self {
            sources: BTreeMap::new(),
            next_id: 1,
            flags,
        }
    }

    /// Add a source to the context (`g_source_attach`).
    ///
    /// Returns the source ID.
    pub fn attach(&mut self, mut source: Source) -> u32 {
        let id = self.next_id;
        self.next_id += 1;
        source.id = id;
        self.sources.insert(id, source);
        id
    }

    /// Remove a source by ID (`g_source_remove`).
    pub fn remove(&mut self, source_id: u32) -> bool {
        self.sources.remove(&source_id).is_some()
    }

    /// Find a source by ID (`g_main_context_find_source_by_id`).
    pub fn find_source_by_id(&self, source_id: u32) -> Option<&Source> {
        self.sources.get(&source_id)
    }

    /// Find a source by name (`g_main_context_find_source_by_name`).
    pub fn find_source_by_name(&self, name: &str) -> Option<&Source> {
        self.sources.values().find(|s| s.name == name)
    }

    /// Get the number of sources.
    pub fn source_count(&self) -> usize {
        self.sources.len()
    }

    /// Add a timeout source and return its ID.
    pub fn timeout_add(&mut self, interval_ms: u32, callback: SourceFunc) -> u32 {
        let mut source = Source::new_timeout(interval_ms);
        source.set_callback(callback);
        self.attach(source)
    }

    /// Add an idle source and return its ID.
    pub fn idle_add(&mut self, callback: SourceFunc) -> u32 {
        let mut source = Source::new_idle();
        source.set_callback(callback);
        self.attach(source)
    }

    /// Check if any source is pending dispatch (`g_main_context_pending`).
    pub fn pending(&self) -> bool {
        self.prepare().0 || self.sources.values().any(|s| s.check())
    }

    /// Dispatch all ready sources.
    ///
    /// Returns the number of sources dispatched.
    pub fn dispatch(&mut self) -> usize {
        let ready_ids: Vec<u32> = self
            .sources
            .iter()
            .filter(|(_, s)| s.prepare().0 || s.check())
            .map(|(id, _)| *id)
            .collect();

        let mut dispatched = 0;
        let mut to_remove = Vec::new();
        for id in ready_ids {
            if let Some(source) = self.sources.get(&id) {
                if !(source.prepare().0 || source.check()) {
                    continue;
                }
                dispatched += 1;
                if source.dispatch() == SOURCE_REMOVE {
                    to_remove.push(id);
                }
            }
        }

        for id in to_remove {
            self.sources.remove(&id);
        }
        dispatched
    }

    /// Prepare all sources and get the minimum timeout.
    ///
    /// Returns (is_ready, min_timeout_ms).
    pub fn prepare(&self) -> (bool, i32) {
        let mut ready = false;
        let mut min_timeout: i32 = -1;
        for source in self.sources.values() {
            let (s_ready, s_timeout) = source.prepare();
            if s_ready {
                ready = true;
            }
            if s_timeout >= 0 && (min_timeout < 0 || s_timeout < min_timeout) {
                min_timeout = s_timeout;
            }
        }
        (ready, min_timeout)
    }

    /// Iterate the context once (`g_main_context_iteration`).
    ///
    /// Returns `true` if any sources were dispatched.
    pub fn iteration(&mut self, may_block: bool) -> bool {
        let (ready, timeout) = self.prepare();
        if !ready && !may_block {
            return false;
        }
        if !ready && may_block {
            let (mut poll_fds, poll_map) = self.gather_poll_fds();
            let has_poll_fds = !poll_fds.is_empty();
            if has_poll_fds || timeout >= 0 {
                g_poll(&mut poll_fds, timeout);
                self.scatter_poll_results(&poll_fds, &poll_map);
            }
        }
        self.dispatch() > 0
    }

    fn gather_poll_fds(&self) -> (Vec<PollFD>, Vec<(u32, usize)>) {
        let mut fds = Vec::new();
        let mut map = Vec::new();
        for (id, source) in &self.sources {
            for (idx, pfd) in source.poll_fds.iter().enumerate() {
                map.push((*id, idx));
                fds.push(pfd.clone());
            }
        }
        (fds, map)
    }

    fn scatter_poll_results(&mut self, fds: &[PollFD], map: &[(u32, usize)]) {
        for (pfd, (id, idx)) in fds.iter().zip(map.iter()) {
            if let Some(source) = self.sources.get_mut(id) {
                if *idx < source.poll_fds.len() {
                    source.poll_fds[*idx].revents = pfd.revents;
                }
            }
        }
    }

    /// Get all source IDs.
    pub fn source_ids(&self) -> Vec<u32> {
        self.sources.keys().copied().collect()
    }
}

impl Default for MainContext {
    fn default() -> Self {
        Self::new()
    }
}

/// A main loop (`GMainLoop`).
///
/// Wraps a `MainContext` and provides run/quit semantics.
pub struct MainLoop {
    context: MainContext,
    running: bool,
    should_quit: bool,
}

impl MainLoop {
    /// Create a new main loop (`g_main_loop_new`).
    pub fn new(context: MainContext) -> Self {
        Self {
            context,
            running: false,
            should_quit: false,
        }
    }

    /// Run the main loop (`g_main_loop_run`).
    ///
    /// In no_std, this is a single-pass iteration since we can't block.
    /// Real implementations would call `iteration(true)` in a loop.
    pub fn run(&mut self) {
        self.running = true;
        self.should_quit = false;
        while !self.should_quit {
            let dispatched = self.context.iteration(true);
            if !dispatched {
                // No sources ready, would block in real impl
                break;
            }
        }
        self.running = false;
    }

    /// Quit the main loop (`g_main_loop_quit`).
    pub fn quit(&mut self) {
        self.should_quit = true;
    }

    /// Check if the loop is running (`g_main_loop_is_running`).
    pub fn is_running(&self) -> bool {
        self.running
    }

    /// Get the context (`g_main_loop_get_context`).
    pub fn get_context(&self) -> &MainContext {
        &self.context
    }

    /// Get mutable context.
    pub fn get_context_mut(&mut self) -> &mut MainContext {
        &mut self.context
    }
}

/// Default main context (global, for convenience).
static DEFAULT_CONTEXT: spin::Mutex<Option<MainContext>> = spin::Mutex::new(None);

fn with_default_context_mut<R>(f: impl FnOnce(&mut MainContext) -> R) -> R {
    let mut guard = DEFAULT_CONTEXT.lock();
    if guard.is_none() {
        *guard = Some(MainContext::new());
    }
    f(guard.as_mut().unwrap())
}

/// Get the default main context (`g_main_context_default`).
///
/// Returns a new empty context; global sources use the internal default
/// context via [`timeout_add`] and [`idle_add`].
pub fn default_context() -> MainContext {
    MainContext::new()
}

/// Add a timeout source (`g_timeout_add`).
///
/// Returns a source ID. The callback will be called after `interval` ms.
pub fn timeout_add(interval_ms: u32, callback: SourceFunc) -> u32 {
    with_default_context_mut(|ctx| ctx.timeout_add(interval_ms, callback))
}

/// Add an idle source (`g_idle_add`).
///
/// Returns a source ID. The callback will be called when the loop is idle.
pub fn idle_add(callback: SourceFunc) -> u32 {
    with_default_context_mut(|ctx| ctx.idle_add(callback))
}

/// Remove a source by ID (`g_source_remove`).
pub fn source_remove(source_id: u32) -> bool {
    with_default_context_mut(|ctx| ctx.remove(source_id))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn main_context_new() {
        let ctx = MainContext::new();
        assert_eq!(ctx.source_count(), 0);
    }

    #[test]
    fn attach_and_remove_source() {
        let mut ctx = MainContext::new();
        let source = Source::new(
            0,
            SourceFuncs {
                prepare: None,
                check: None,
                dispatch: None,
                finalize: None,
            },
        );
        let id = ctx.attach(source);
        assert_eq!(ctx.source_count(), 1);
        assert!(ctx.remove(id));
        assert_eq!(ctx.source_count(), 0);
    }

    #[test]
    fn find_source_by_id() {
        let mut ctx = MainContext::new();
        let mut source = Source::new(
            0,
            SourceFuncs {
                prepare: None,
                check: None,
                dispatch: None,
                finalize: None,
            },
        );
        source.set_name("test");
        let id = ctx.attach(source);
        let found = ctx.find_source_by_id(id);
        assert!(found.is_some());
        assert_eq!(found.unwrap().get_name(), "test");
    }

    #[test]
    fn find_source_by_name() {
        let mut ctx = MainContext::new();
        let mut source = Source::new(
            0,
            SourceFuncs {
                prepare: None,
                check: None,
                dispatch: None,
                finalize: None,
            },
        );
        source.set_name("my-source");
        ctx.attach(source);
        assert!(ctx.find_source_by_name("my-source").is_some());
        assert!(ctx.find_source_by_name("nope").is_none());
    }

    #[test]
    fn source_flags() {
        let flags = SourceFlags::SKIP | SourceFlags::READY;
        assert!(flags.contains(SourceFlags::SKIP));
        assert!(flags.contains(SourceFlags::READY));
        assert!(!flags.contains(SourceFlags::CAN_RECURSE));
    }

    #[test]
    fn source_ready_time() {
        let mut source = Source::new(
            1,
            SourceFuncs {
                prepare: None,
                check: None,
                dispatch: None,
                finalize: None,
            },
        );
        assert_eq!(source.get_ready_time(), None);
        source.set_ready_time(12345);
        assert_eq!(source.get_ready_time(), Some(12345));
    }

    #[test]
    fn source_priority() {
        let mut source = Source::new(
            1,
            SourceFuncs {
                prepare: None,
                check: None,
                dispatch: None,
                finalize: None,
            },
        );
        source.set_priority(-10);
        assert_eq!(source.get_priority(), -10);
    }

    #[test]
    fn main_loop_quit() {
        let ctx = MainContext::new();
        let mut loop_ = MainLoop::new(ctx);
        assert!(!loop_.is_running());
        loop_.quit();
    }

    #[test]
    fn idle_source() {
        let mut ctx = MainContext::new();
        let source = Source::new(
            0,
            SourceFuncs {
                prepare: Some(|_s| (true, 0)),
                check: Some(|_s| true),
                dispatch: None,
                finalize: None,
            },
        );
        ctx.attach(source);
        let (ready, timeout) = ctx.prepare();
        assert!(ready);
        assert_eq!(timeout, 0);
    }

    #[test]
    fn iteration_no_sources() {
        let mut ctx = MainContext::new();
        assert!(!ctx.iteration(false));
    }

    #[test]
    fn pending_idle_source() {
        let mut ctx = MainContext::new();
        ctx.idle_add(|| SOURCE_REMOVE);
        assert!(ctx.pending());
    }

    #[test]
    fn idle_dispatches_on_iteration() {
        use core::sync::atomic::{AtomicU32, Ordering};
        static CALLED: AtomicU32 = AtomicU32::new(0);
        fn on_idle() -> bool {
            CALLED.fetch_add(1, Ordering::Relaxed);
            SOURCE_REMOVE
        }
        let mut ctx = MainContext::new();
        ctx.idle_add(on_idle);
        assert!(ctx.iteration(false));
        assert_eq!(CALLED.load(Ordering::Relaxed), 1);
        assert_eq!(ctx.source_count(), 0);
    }

    #[test]
    fn timeout_not_pending_before_deadline() {
        use crate::timer::set_clock;
        use core::sync::atomic::{AtomicI64, Ordering};
        static NOW_MS: AtomicI64 = AtomicI64::new(0);
        fn mock_clock() -> i64 {
            NOW_MS.load(Ordering::Relaxed) * 1000
        }
        NOW_MS.store(0, Ordering::Relaxed);
        set_clock(mock_clock);
        let mut ctx = MainContext::new();
        ctx.timeout_add(100, || SOURCE_REMOVE);
        assert!(!ctx.pending());
        NOW_MS.store(50, Ordering::Relaxed);
        assert!(!ctx.pending());
    }

    #[test]
    fn timeout_pending_at_deadline() {
        use crate::timer::set_clock;
        use core::sync::atomic::{AtomicI64, Ordering};
        static NOW_MS: AtomicI64 = AtomicI64::new(0);
        fn mock_clock() -> i64 {
            NOW_MS.load(Ordering::Relaxed) * 1000
        }
        NOW_MS.store(0, Ordering::Relaxed);
        set_clock(mock_clock);
        let mut ctx = MainContext::new();
        ctx.timeout_add(100, || SOURCE_REMOVE);
        NOW_MS.store(100, Ordering::Relaxed);
        assert!(ctx.pending());
    }

    #[test]
    fn timeout_dispatches_callback() {
        use crate::timer::set_clock;
        use core::sync::atomic::{AtomicI64, AtomicU32, Ordering};
        static NOW_MS: AtomicI64 = AtomicI64::new(0);
        static CALLED: AtomicU32 = AtomicU32::new(0);
        fn mock_clock() -> i64 {
            NOW_MS.load(Ordering::Relaxed) * 1000
        }
        fn on_timeout() -> bool {
            CALLED.fetch_add(1, Ordering::Relaxed);
            SOURCE_REMOVE
        }
        NOW_MS.store(0, Ordering::Relaxed);
        set_clock(mock_clock);
        let mut ctx = MainContext::new();
        ctx.timeout_add(100, on_timeout);
        NOW_MS.store(100, Ordering::Relaxed);
        assert!(ctx.iteration(false));
        assert_eq!(CALLED.load(Ordering::Relaxed), 1);
    }

    #[test]
    fn timeout_iteration_false_before_ready() {
        use crate::timer::set_clock;
        use core::sync::atomic::{AtomicI64, Ordering};
        static NOW_MS: AtomicI64 = AtomicI64::new(0);
        fn mock_clock() -> i64 {
            NOW_MS.load(Ordering::Relaxed) * 1000
        }
        NOW_MS.store(0, Ordering::Relaxed);
        set_clock(mock_clock);
        let mut ctx = MainContext::new();
        ctx.timeout_add(100, || SOURCE_REMOVE);
        assert!(!ctx.iteration(false));
        assert_eq!(ctx.source_count(), 1);
    }

    #[test]
    fn timeout_removed_after_dispatch() {
        use crate::timer::set_clock;
        use core::sync::atomic::{AtomicI64, Ordering};
        static NOW_MS: AtomicI64 = AtomicI64::new(0);
        fn mock_clock() -> i64 {
            NOW_MS.load(Ordering::Relaxed) * 1000
        }
        NOW_MS.store(0, Ordering::Relaxed);
        set_clock(mock_clock);
        let mut ctx = MainContext::new();
        ctx.timeout_add(50, || SOURCE_REMOVE);
        NOW_MS.store(50, Ordering::Relaxed);
        assert!(ctx.iteration(false));
        assert_eq!(ctx.source_count(), 0);
        assert!(!ctx.pending());
    }

    #[test]
    fn poll_fd_source_dispatches_after_poll() {
        use crate::poll::{
            register_poll_platform, test_poll_clear_fds, test_poll_register_fd, IOCondition,
            PollFD, TestPollPlatform,
        };
        use core::sync::atomic::{AtomicU32, Ordering};
        static DISPATCHED: AtomicU32 = AtomicU32::new(0);
        fn on_poll_ready() -> bool {
            DISPATCHED.fetch_add(1, Ordering::Relaxed);
            SOURCE_REMOVE
        }
        fn poll_check(source: &Source) -> bool {
            source
                .get_poll_fds()
                .iter()
                .any(|pfd| pfd.revents & pfd.events != 0)
        }

        register_poll_platform(&TestPollPlatform);
        test_poll_clear_fds();
        test_poll_register_fd(7);

        let mut ctx = MainContext::new();
        let mut source = Source::new(
            0,
            SourceFuncs {
                prepare: Some(|_| (false, -1)),
                check: Some(poll_check),
                dispatch: None,
                finalize: None,
            },
        );
        source.set_callback(on_poll_ready);
        source.add_poll(PollFD::new(7, IOCondition::In.bits()));
        ctx.attach(source);

        assert!(ctx.iteration(true));
        assert_eq!(DISPATCHED.load(Ordering::Relaxed), 1);
        assert_eq!(ctx.source_count(), 0);
        register_poll_platform(&crate::poll::NoPollPlatform);
        test_poll_clear_fds();
    }

    #[test]
    fn poll_fd_not_ready_without_revents() {
        use crate::poll::{
            register_poll_platform, test_poll_clear_fds, IOCondition, PollFD, TestPollPlatform,
        };

        register_poll_platform(&TestPollPlatform);
        test_poll_clear_fds();

        let mut ctx = MainContext::new();
        let mut source = Source::new(
            0,
            SourceFuncs {
                prepare: Some(|_| (false, 0)),
                check: None,
                dispatch: None,
                finalize: None,
            },
        );
        source.add_poll(PollFD::new(11, IOCondition::In.bits()));
        ctx.attach(source);

        assert!(!ctx.iteration(true));
        assert_eq!(ctx.source_count(), 1);
        register_poll_platform(&crate::poll::NoPollPlatform);
    }

    #[test]
    fn iteration_blocks_until_timeout_expires() {
        use crate::poll::register_poll_platform;
        use crate::timer::set_clock;
        use core::sync::atomic::{AtomicI64, AtomicU32, Ordering};
        static NOW_US: AtomicI64 = AtomicI64::new(0);
        static CALLED: AtomicU32 = AtomicU32::new(0);
        fn mock_clock() -> i64 {
            NOW_US.fetch_add(500, Ordering::Relaxed)
        }
        fn on_timeout() -> bool {
            CALLED.fetch_add(1, Ordering::Relaxed);
            SOURCE_REMOVE
        }

        NOW_US.store(0, Ordering::Relaxed);
        set_clock(mock_clock);
        register_poll_platform(&crate::poll::TimerPollPlatform);

        let mut ctx = MainContext::new();
        ctx.timeout_add(20, on_timeout);
        assert!(ctx.iteration(true));
        assert_eq!(CALLED.load(Ordering::Relaxed), 1);
        register_poll_platform(&crate::poll::NoPollPlatform);
    }

    #[test]
    fn poll_fd_check_without_custom_check_fn() {
        use crate::poll::{
            register_poll_platform, test_poll_clear_fds, test_poll_register_fd, IOCondition,
            PollFD, TestPollPlatform,
        };
        use core::sync::atomic::{AtomicU32, Ordering};
        static DISPATCHED: AtomicU32 = AtomicU32::new(0);
        fn on_ready() -> bool {
            DISPATCHED.fetch_add(1, Ordering::Relaxed);
            SOURCE_REMOVE
        }

        register_poll_platform(&TestPollPlatform);
        test_poll_clear_fds();
        test_poll_register_fd(3);

        let mut ctx = MainContext::new();
        let mut source = Source::new(
            0,
            SourceFuncs {
                prepare: None,
                check: None,
                dispatch: None,
                finalize: None,
            },
        );
        source.set_callback(on_ready);
        source.add_poll(PollFD::new(3, IOCondition::In.bits()));
        ctx.attach(source);

        assert!(ctx.iteration(true));
        assert_eq!(DISPATCHED.load(Ordering::Relaxed), 1);
        register_poll_platform(&crate::poll::NoPollPlatform);
        test_poll_clear_fds();
    }

    #[test]
    fn iteration_may_block_false_skips_poll_wait() {
        use crate::poll::{
            register_poll_platform, test_poll_clear_fds, IOCondition, PollFD, TestPollPlatform,
        };

        register_poll_platform(&TestPollPlatform);
        test_poll_clear_fds();

        let mut ctx = MainContext::new();
        let mut source = Source::new(
            0,
            SourceFuncs {
                prepare: Some(|_| (false, 100)),
                check: None,
                dispatch: None,
                finalize: None,
            },
        );
        source.add_poll(PollFD::new(5, IOCondition::In.bits()));
        ctx.attach(source);

        assert!(!ctx.iteration(false));
        assert_eq!(ctx.source_count(), 1);
        register_poll_platform(&crate::poll::NoPollPlatform);
    }
}
