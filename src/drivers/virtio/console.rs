//! # VirtIO Console Driver
//!
//! Implements virtio-console for paravirtualized console access in QEMU/KVM.
//! Uses two virtqueues: receive (queue 0) for input from the host and
//! transmit (queue 1) for output to the host. This provides an alternative
//! to the 8250 UART for console I/O.

use super::*;
use spin::Mutex;

/// Queue indices for virtio-console
const RX_QUEUE: u16 = 0;
const TX_QUEUE: u16 = 1;

/// Default queue size
const QUEUE_SIZE: u16 = 64;

/// Buffer size for console data
const CON_BUF_SIZE: usize = 128;

/// Number of static RX buffers
const RX_BUF_COUNT: usize = 32;

/// Number of static TX buffers
const TX_BUF_COUNT: usize = 32;

/// Static RX buffer pool — device writes data into these via DMA
static RX_BUFFERS: Mutex<[[u8; CON_BUF_SIZE]; RX_BUF_COUNT]> =
    Mutex::new([[0u8; CON_BUF_SIZE]; RX_BUF_COUNT]);

/// Track which RX buffers are in flight
static RX_BUF_IN_FLIGHT: Mutex<[bool; RX_BUF_COUNT]> = Mutex::new([false; RX_BUF_COUNT]);

/// Static TX buffer pool — must persist until device has consumed them
static TX_BUFFERS: Mutex<[[u8; CON_BUF_SIZE]; TX_BUF_COUNT]> =
    Mutex::new([[0u8; CON_BUF_SIZE]; TX_BUF_COUNT]);

/// Track which TX buffers are in use
static TX_BUF_USED: Mutex<[bool; TX_BUF_COUNT]> = Mutex::new([false; TX_BUF_COUNT]);

/// Completed RX data, ready for consumption by the TTY layer
static RX_READY_DATA: Mutex<alloc::vec::Vec<u8>> = Mutex::new(alloc::vec::Vec::new());

/// Allocate a TX buffer from the static pool
fn alloc_tx_buffer() -> Option<usize> {
    let mut used = TX_BUF_USED.lock();
    for i in 0..TX_BUF_COUNT {
        if !used[i] {
            used[i] = true;
            return Some(i);
        }
    }
    None
}

/// VirtIO console driver state
pub struct VirtioConsole {
    transport: VirtioTransport,
    rx_queue: Option<VirtQueue>,
    tx_queue: Option<VirtQueue>,
}

impl VirtioConsole {
    /// Create and initialize a virtio-console device
    pub fn new(transport: VirtioTransport) -> Result<Self, &'static str> {
        // virtio-console has no special feature bits to negotiate
        transport.init_device(0)?;

        // Set up RX queue (receive from host)
        let rx_size = transport.select_queue(RX_QUEUE);
        let rx_size = if rx_size == 0 {
            QUEUE_SIZE
        } else {
            rx_size.min(QUEUE_SIZE)
        };
        let rx_notify_off =
            unsafe { core::ptr::read_volatile((transport.common_base + 30) as *const u16) };
        let mut rx_queue = VirtQueue::new(rx_size, rx_notify_off)?;
        transport.setup_queue(&rx_queue);

        // Set up TX queue (transmit to host)
        let tx_size = transport.select_queue(TX_QUEUE);
        let tx_size = if tx_size == 0 {
            QUEUE_SIZE
        } else {
            tx_size.min(QUEUE_SIZE)
        };
        let tx_notify_off =
            unsafe { core::ptr::read_volatile((transport.common_base + 30) as *const u16) };
        let tx_queue = VirtQueue::new(tx_size, tx_notify_off)?;
        transport.setup_queue(&tx_queue);

        // Fill RX queue with empty buffers for the device to write into
        for i in 0..rx_size.min(RX_BUF_COUNT as u16) as usize {
            let buf_phys = super::virt_to_phys(RX_BUFFERS.lock()[i].as_ptr() as usize);
            let desc_idx = i as u16;
            rx_queue.set_desc(
                desc_idx,
                buf_phys,
                CON_BUF_SIZE as u32,
                desc_flags::WRITE,
                0,
            );
            rx_queue.submit(desc_idx);
            RX_BUF_IN_FLIGHT.lock()[i] = true;
        }
        transport.notify(&rx_queue);

        // Set DRIVER_OK
        transport.set_driver_ok();

        crate::serial_println!(
            "virtio-console: initialized (rx_queue={}, tx_queue={})",
            rx_size,
            tx_size
        );

        Ok(VirtioConsole {
            transport,
            rx_queue: Some(rx_queue),
            tx_queue: Some(tx_queue),
        })
    }

    /// Poll the RX virtqueue for completed data and move it to the ready queue.
    /// Re-submits empty buffers for more data. Returns the number of bytes newly available.
    fn poll_rx(&mut self) -> usize {
        let rx_queue = match self.rx_queue.as_mut() {
            Some(q) => q,
            None => return 0,
        };

        let mut total = 0;
        while rx_queue.has_used() {
            let (id, len) = match rx_queue.pop_used() {
                Some(v) => v,
                None => break,
            };
            let desc_idx = id as u16;
            let buf_idx = desc_idx as usize;

            if buf_idx < RX_BUF_COUNT {
                let data_len = len as usize;
                if data_len > 0 {
                    let buffers = RX_BUFFERS.lock();
                    let mut ready = RX_READY_DATA.lock();
                    ready.extend_from_slice(&buffers[buf_idx][..data_len.min(CON_BUF_SIZE)]);
                    total += data_len.min(CON_BUF_SIZE);
                }

                // Mark buffer as no longer in flight and re-submit
                RX_BUF_IN_FLIGHT.lock()[buf_idx] = false;
                let buf_phys = super::virt_to_phys(RX_BUFFERS.lock()[buf_idx].as_ptr() as usize);
                rx_queue.set_desc(
                    desc_idx,
                    buf_phys,
                    CON_BUF_SIZE as u32,
                    desc_flags::WRITE,
                    0,
                );
                rx_queue.submit(desc_idx);
                RX_BUF_IN_FLIGHT.lock()[buf_idx] = true;
            }
        }
        self.transport.notify(rx_queue);
        total
    }

    /// Send data to the host via the TX virtqueue
    pub fn send(&mut self, data: &[u8]) -> Result<usize, &'static str> {
        if data.is_empty() {
            return Ok(0);
        }

        let tx_queue = self.tx_queue.as_mut().unwrap();

        // Reap any completed TX buffers first
        while tx_queue.has_used() {
            if let Some((id, _len)) = tx_queue.pop_used() {
                let desc_idx = id as u16;
                let buf_idx = (desc_idx as usize) % TX_BUF_COUNT;
                TX_BUF_USED.lock()[buf_idx] = false;
                tx_queue.free_desc(desc_idx);
            }
        }

        let desc_idx = tx_queue
            .alloc_desc()
            .ok_or("virtio-console: no free TX descriptors")?;

        let buf_idx = alloc_tx_buffer().ok_or("virtio-console: no free TX buffers")?;

        // Copy data into the static TX buffer
        let copy_len = data.len().min(CON_BUF_SIZE);
        {
            let mut buffers = TX_BUFFERS.lock();
            buffers[buf_idx][..copy_len].copy_from_slice(&data[..copy_len]);
        }

        let buf_phys = super::virt_to_phys(TX_BUFFERS.lock()[buf_idx].as_ptr() as usize);
        tx_queue.set_desc(desc_idx, buf_phys, copy_len as u32, 0, 0);
        tx_queue.submit(desc_idx);
        self.transport.notify(tx_queue);

        Ok(copy_len)
    }

    /// Read available data from the device. Polls the RX queue first.
    pub fn read(&mut self, buf: &mut [u8]) -> Result<usize, &'static str> {
        self.poll_rx();

        let mut ready = RX_READY_DATA.lock();
        let n = buf.len().min(ready.len());
        if n == 0 {
            return Ok(0);
        }
        buf[..n].copy_from_slice(&ready[..n]);
        ready.drain(..n);
        Ok(n)
    }

    /// Check if data is available without consuming it
    pub fn data_available(&mut self) -> bool {
        self.poll_rx();
        !RX_READY_DATA.lock().is_empty()
    }
}

/// Global virtio-console driver instance
static VIRTIO_CONSOLE: Mutex<Option<VirtioConsole>> = Mutex::new(None);

/// Initialize virtio-console from a transport
pub fn init_virtio_console(transport: VirtioTransport) -> Result<(), &'static str> {
    let console = VirtioConsole::new(transport)?;
    *VIRTIO_CONSOLE.lock() = Some(console);
    Ok(())
}

/// Check if virtio-console is available
pub fn is_available() -> bool {
    VIRTIO_CONSOLE.lock().is_some()
}

/// Read data from the virtio-console device
pub fn read(buf: &mut [u8]) -> Result<usize, &'static str> {
    match VIRTIO_CONSOLE.lock().as_mut() {
        Some(console) => console.read(buf),
        None => Err("virtio-console: not initialized"),
    }
}

/// Send data through the virtio-console device
pub fn send(data: &[u8]) -> Result<usize, &'static str> {
    match VIRTIO_CONSOLE.lock().as_mut() {
        Some(console) => console.send(data),
        None => Err("virtio-console: not initialized"),
    }
}

/// Poll for available data without consuming it
pub fn data_available() -> bool {
    match VIRTIO_CONSOLE.lock().as_mut() {
        Some(console) => console.data_available(),
        None => false,
    }
}

/// Poll virtio-console RX and feed data into the TTY line discipline.
/// Called from the TTY poll_input path so virtio-console input is processed
/// alongside serial port input.
pub fn poll_tty_input() {
    let mut buf = [0u8; 256];
    let mut total = 0;

    // Drain all available data
    loop {
        match VIRTIO_CONSOLE.lock().as_mut() {
            Some(console) => {
                let n = match console.read(&mut buf[total..]) {
                    Ok(n) => n,
                    Err(_) => break,
                };
                if n == 0 {
                    break;
                }
                total += n;
                if total >= buf.len() {
                    break;
                }
            }
            None => return,
        }
    }

    if total == 0 {
        return;
    }

    // Feed into the console TTY line discipline
    let data = &buf[..total];
    let mut echo_opt = Some(alloc::vec::Vec::new());
    let mut console = crate::drivers::tty::CONSOLE_TTY.lock();
    crate::drivers::tty::n_tty::tty_push_input(&mut console, data, &mut echo_opt);
    drop(console);

    // Echo back through virtio-console if needed
    if let Some(echo) = echo_opt {
        if !echo.is_empty() {
            let _ = send(&echo);
        }
    }
}
