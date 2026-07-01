//! Host Controller Driver (HCD) abstraction.
//!
//! Defines the hardware-independent interface every USB host controller
//! implements (`HostController`) plus the value types that flow across it:
//! USB setup packets, transfer directions/results, port status and speed.
//! The xHCI model (`super::xhci`) is the concrete backend; enumeration
//! (`super::hub`) and the class drivers (`super::class`) are written purely
//! against this trait so they stay controller agnostic.

/// USB bus signalling speed reported for a port.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UsbSpeed {
    Low,
    Full,
    High,
    Super,
}

impl UsbSpeed {
    /// Default control-endpoint max packet size for the speed.
    pub fn default_max_packet(self) -> u16 {
        match self {
            UsbSpeed::Low => 8,
            UsbSpeed::Full => 64,
            UsbSpeed::High => 64,
            UsbSpeed::Super => 512,
        }
    }
}

/// Direction of a data-stage / bulk / interrupt transfer.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TransferDirection {
    /// Device-to-host (IN).
    In,
    /// Host-to-device (OUT).
    Out,
}

/// 8-byte USB SETUP packet (USB 2.0 §9.3).
#[derive(Debug, Clone, Copy, Default)]
pub struct SetupPacket {
    pub request_type: u8,
    pub request: u8,
    pub value: u16,
    pub index: u16,
    pub length: u16,
}

impl SetupPacket {
    /// True if the data stage moves device→host.
    pub fn is_device_to_host(&self) -> bool {
        self.request_type & 0x80 != 0
    }

    /// Encode into the 8 wire bytes (used as the parameter of a Setup-stage TRB).
    pub fn to_bytes(self) -> [u8; 8] {
        let v = self.value.to_le_bytes();
        let i = self.index.to_le_bytes();
        let l = self.length.to_le_bytes();
        [
            self.request_type,
            self.request,
            v[0],
            v[1],
            i[0],
            i[1],
            l[0],
            l[1],
        ]
    }
}

// ── Standard USB request constants (USB 2.0 §9.4) ───────────────────────

pub const REQ_GET_DESCRIPTOR: u8 = 0x06;
pub const REQ_SET_ADDRESS: u8 = 0x05;
pub const REQ_SET_CONFIGURATION: u8 = 0x09;
pub const REQ_GET_CONFIGURATION: u8 = 0x08;
pub const REQ_SET_INTERFACE: u8 = 0x0B;

pub const DESC_DEVICE: u8 = 0x01;
pub const DESC_CONFIGURATION: u8 = 0x02;
pub const DESC_STRING: u8 = 0x03;
pub const DESC_INTERFACE: u8 = 0x04;
pub const DESC_ENDPOINT: u8 = 0x05;

/// Completion status reported by the controller for a transfer.
///
/// Mirrors the meaningful subset of xHCI completion codes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CompletionCode {
    Success,
    ShortPacket,
    Stall,
    TransactionError,
    Babble,
    TrbError,
}

impl CompletionCode {
    /// xHCI completion-code numeric value (Transfer Event TRB, §6.4.5).
    pub fn as_u8(self) -> u8 {
        match self {
            CompletionCode::Success => 1,
            CompletionCode::TrbError => 5,
            CompletionCode::Stall => 6,
            CompletionCode::ShortPacket => 13,
            CompletionCode::TransactionError => 4,
            CompletionCode::Babble => 3,
        }
    }

    pub fn is_ok(self) -> bool {
        matches!(self, CompletionCode::Success | CompletionCode::ShortPacket)
    }
}

/// Outcome of a single transfer issued through the controller.
#[derive(Debug, Clone, Copy)]
pub struct TransferResult {
    pub completion: CompletionCode,
    /// Number of bytes actually moved.
    pub transferred: usize,
}

impl TransferResult {
    pub fn ok(transferred: usize) -> Self {
        TransferResult {
            completion: CompletionCode::Success,
            transferred,
        }
    }

    pub fn short(transferred: usize) -> Self {
        TransferResult {
            completion: CompletionCode::ShortPacket,
            transferred,
        }
    }

    pub fn stall() -> Self {
        TransferResult {
            completion: CompletionCode::Stall,
            transferred: 0,
        }
    }

    pub fn is_ok(&self) -> bool {
        self.completion.is_ok()
    }
}

/// Status of a single root-hub port.
#[derive(Debug, Clone, Copy)]
pub struct PortStatus {
    pub connected: bool,
    pub enabled: bool,
    pub powered: bool,
    pub reset: bool,
    pub speed: UsbSpeed,
}

impl Default for PortStatus {
    fn default() -> Self {
        PortStatus {
            connected: false,
            enabled: false,
            powered: true,
            reset: false,
            speed: UsbSpeed::High,
        }
    }
}

/// Hardware-independent host-controller interface.
///
/// All transfer methods take `&mut self` because the concrete backend
/// advances ring producer/consumer cycle state as a side effect.
pub trait HostController: Send + Sync {
    /// Human-readable controller name.
    fn name(&self) -> &str;

    /// Number of root-hub ports.
    fn port_count(&self) -> u8;

    /// Read the status of `port` (1-based).
    fn port_status(&self, port: u8) -> Result<PortStatus, &'static str>;

    /// Drive a USB reset on `port` (1-based) and leave it enabled.
    fn reset_port(&mut self, port: u8) -> Result<(), &'static str>;

    /// Allocate a device slot for the device on `port`, returning the slot id.
    fn enable_slot(&mut self, port: u8) -> Result<u8, &'static str>;

    /// Issue a control transfer on endpoint 0 of `slot`.
    fn control_transfer(
        &mut self,
        slot: u8,
        setup: SetupPacket,
        data: Option<&mut [u8]>,
    ) -> Result<TransferResult, &'static str>;

    /// Issue a bulk transfer on `endpoint` of `slot`.
    ///
    /// `endpoint` is the USB endpoint address (bit 7 = IN). `dir` must agree
    /// with the address direction.
    fn bulk_transfer(
        &mut self,
        slot: u8,
        endpoint: u8,
        dir: TransferDirection,
        buffer: &mut [u8],
    ) -> Result<TransferResult, &'static str>;

    /// Issue an interrupt-IN transfer on `endpoint` of `slot`.
    fn interrupt_transfer(
        &mut self,
        slot: u8,
        endpoint: u8,
        buffer: &mut [u8],
    ) -> Result<TransferResult, &'static str>;
}
