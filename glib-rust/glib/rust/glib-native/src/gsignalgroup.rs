//! Signal group support (`gsignalgroup.c`).

use crate::gsignal::{
    signal_connect_by_name, signal_handler_disconnect, ConnectFlags, SignalCallback,
};
use crate::gtype::GType;
use alloc::string::String;
use alloc::vec::Vec;

#[derive(Clone, Default)]
pub struct SignalGroup {
    target_type: GType,
    handlers: Vec<u32>,
    names: Vec<String>,
}

impl SignalGroup {
    #[must_use]
    pub fn new(target_type: GType) -> Self {
        Self {
            target_type,
            handlers: Vec::new(),
            names: Vec::new(),
        }
    }

    pub fn connect(
        &mut self,
        signal_name: &str,
        callback: SignalCallback,
        flags: ConnectFlags,
    ) -> u32 {
        let id = signal_connect_by_name(self.target_type, signal_name, callback, flags);
        self.handlers.push(id);
        self.names.push(String::from(signal_name));
        id
    }

    pub fn disconnect_all(&mut self) {
        for id in self.handlers.drain(..) {
            signal_handler_disconnect(id);
        }
        self.names.clear();
    }

    #[must_use]
    pub fn len(&self) -> usize {
        self.handlers.len()
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.handlers.is_empty()
    }
}
