//! RxRPC call/connection state (mirrors Linux `net/rxrpc/`)

use spin::RwLock;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RxrpcCallState {
    Idle,
    ClientSending,
    ServerReceiving,
    Complete,
}

static CALL_STATE: RwLock<RxrpcCallState> = RwLock::new(RxrpcCallState::Idle);

pub fn set_call_state(s: RxrpcCallState) {
    *CALL_STATE.write() = s;
}

pub fn init() -> Result<(), &'static str> {
    crate::serial_println!("rxrpc: call state machine initialized");
    Ok(())
}
