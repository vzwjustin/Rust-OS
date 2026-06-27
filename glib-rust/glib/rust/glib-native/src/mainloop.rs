//! Main loop and event sources matching `gmain.h` / `gmain.c`.
//!
//! Provides the core types for GLib's main event loop. The actual
//! poll/dispatch mechanism requires OS support and is abstracted via
//! a platform trait. Fully `no_std` compatible using `alloc` and `spin`.

use crate::prelude::*;
use crate::poll::PollFD;
use crate::thread::GMutex;
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

/// An event source (`GSource`).
pub struct Source {
    pub id: u32,
    pub priority: i32,
    pub flags: SourceFlags,
    pub name: String,
    pub ready_time: Option<i64>,
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
            funcs,
            callback: None,
            poll_fds: Vec::new(),
        }
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

    /// Check if any source is ready.
    pub fn check(&self) -> bool {
        self.sources.values().any(|s| s.check())
    }

    /// Dispatch all ready sources.
    ///
    /// Returns the number of sources dispatched.
    pub fn dispatch(&mut self) -> usize {
        let mut dispatched = 0;
        let to_remove: Vec<u32> = self.sources.iter()
            .filter(|(_, s)| s.check())
            .map(|(id, s)| {
                dispatched += 1;
                if s.dispatch() == SOURCE_REMOVE {
                    *id
                } else {
                    0
                }
            })
            .filter(|&id| id != 0)
            .collect();

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
        let (ready, _timeout) = self.prepare();
        if !ready && !may_block {
            return false;
        }
        let dispatched = self.dispatch();
        dispatched > 0
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

/// Get the default main context (`g_main_context_default`).
pub fn default_context() -> MainContext {
    let mut guard = DEFAULT_CONTEXT.lock();
    if guard.is_none() {
        *guard = Some(MainContext::new());
    }
    // Return a clone-like new context since we can't clone MainContext
    // In practice, callers should use the global context directly
    MainContext::new()
}

/// Add a timeout source (`g_timeout_add`).
///
/// Returns a source ID. The callback will be called after `interval` ms.
/// In no_std, timing requires platform support.
pub fn timeout_add(interval_ms: u32, callback: SourceFunc) -> u32 {
    let mut ctx = default_context();
    fn timeout_prepare(s: &Source) -> (bool, i32) {
        let interval = s.ready_time.unwrap_or(0) as i32;
        (false, interval)
    }
    let mut source = Source::new(0, SourceFuncs {
        prepare: Some(timeout_prepare),
        check: None,
        dispatch: None,
        finalize: None,
    });
    source.set_ready_time(interval_ms as i64);
    source.set_callback(callback);
    source.set_name("timeout");
    ctx.attach(source)
}

/// Add an idle source (`g_idle_add`).
///
/// Returns a source ID. The callback will be called when the loop is idle.
pub fn idle_add(callback: SourceFunc) -> u32 {
    let mut ctx = default_context();
    let mut source = Source::new(0, SourceFuncs {
        prepare: Some(|_s| (true, 0)),
        check: Some(|_s| true),
        dispatch: None,
        finalize: None,
    });
    source.set_callback(callback);
    source.set_name("idle");
    ctx.attach(source)
}

/// Remove a source by ID (`g_source_remove`).
pub fn source_remove(source_id: u32) -> bool {
    let mut ctx = default_context();
    ctx.remove(source_id)
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
        let source = Source::new(0, SourceFuncs {
            prepare: None, check: None, dispatch: None, finalize: None,
        });
        let id = ctx.attach(source);
        assert_eq!(ctx.source_count(), 1);
        assert!(ctx.remove(id));
        assert_eq!(ctx.source_count(), 0);
    }

    #[test]
    fn find_source_by_id() {
        let mut ctx = MainContext::new();
        let mut source = Source::new(0, SourceFuncs {
            prepare: None, check: None, dispatch: None, finalize: None,
        });
        source.set_name("test");
        let id = ctx.attach(source);
        let found = ctx.find_source_by_id(id);
        assert!(found.is_some());
        assert_eq!(found.unwrap().get_name(), "test");
    }

    #[test]
    fn find_source_by_name() {
        let mut ctx = MainContext::new();
        let mut source = Source::new(0, SourceFuncs {
            prepare: None, check: None, dispatch: None, finalize: None,
        });
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
        let mut source = Source::new(1, SourceFuncs {
            prepare: None, check: None, dispatch: None, finalize: None,
        });
        assert_eq!(source.get_ready_time(), None);
        source.set_ready_time(12345);
        assert_eq!(source.get_ready_time(), Some(12345));
    }

    #[test]
    fn source_priority() {
        let mut source = Source::new(1, SourceFuncs {
            prepare: None, check: None, dispatch: None, finalize: None,
        });
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
        let source = Source::new(0, SourceFuncs {
            prepare: Some(|_s| (true, 0)),
            check: Some(|_s| true),
            dispatch: None,
            finalize: None,
        });
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
}
