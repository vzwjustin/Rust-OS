//! # VirtIO RNG (Random Number Generator) Driver
//!
//! Implements virtio-rng for paravirtualized random number generation in QEMU/KVM.
//! Uses a single virtqueue where the driver submits empty buffers and the device
//! fills them with random data.

use super::*;
use spin::Mutex;

/// Queue index for virtio-rng (single request queue)
const RNG_QUEUE: u16 = 0;

/// Default queue size
const QUEUE_SIZE: u16 = 32;

/// Buffer size for each RNG read request
const RNG_BUF_SIZE: usize = 64;

/// Number of static receive buffers
const RNG_BUF_COUNT: usize = 32;

/// Static buffer pool — buffers must persist until the device fills them.
static RNG_BUFFERS: Mutex<[[u8; RNG_BUF_SIZE]; RNG_BUF_COUNT]> =
    Mutex::new([[0u8; RNG_BUF_SIZE]; RNG_BUF_COUNT]);

/// Track which buffers are in flight (submitted to device, not yet consumed)
static RNG_BUF_IN_FLIGHT: Mutex<[bool; RNG_BUF_COUNT]> = Mutex::new([false; RNG_BUF_COUNT]);

/// Completed random data, ready for consumption
static RNG_READY_DATA: Mutex<alloc::vec::Vec<u8>> = Mutex::new(alloc::vec::Vec::new());

/// VirtIO RNG driver state
pub struct VirtioRng {
    transport: VirtioTransport,
    queue: Option<VirtQueue>,
}

impl VirtioRng {
    /// Create and initialize a virtio-rng device
    pub fn new(transport: VirtioTransport) -> Result<Self, &'static str> {
        // virtio-rng has no special feature bits to negotiate
        transport.init_device(0)?;

        // Set up the request queue
        let q_size = transport.select_queue(RNG_QUEUE);
        let q_size = if q_size == 0 {
            QUEUE_SIZE
        } else {
            q_size.min(QUEUE_SIZE)
        };
        let notify_off =
            unsafe { core::ptr::read_volatile((transport.common_base + 30) as *const u16) };
        let mut queue = VirtQueue::new(q_size, notify_off)?;
        transport.setup_queue(&queue);

        // Set DRIVER_OK
        transport.set_driver_ok();

        crate::serial_println!("virtio-rng: initialized (queue_size={})", q_size);

        // Submit initial fill requests
        for i in 0..q_size.min(RNG_BUF_COUNT as u16) as usize {
            let buf_phys = super::virt_to_phys(RNG_BUFFERS.lock()[i].as_ptr() as usize);
            let desc_idx = i as u16;
            queue.set_desc(
                desc_idx,
                buf_phys,
                RNG_BUF_SIZE as u32,
                desc_flags::WRITE,
                0,
            );
            queue.submit(desc_idx);
            RNG_BUF_IN_FLIGHT.lock()[i] = true;
        }
        transport.notify(&queue);

        Ok(VirtioRng {
            transport,
            queue: Some(queue),
        })
    }

    /// Poll the device for completed random data and move it to the ready queue.
    /// Returns the number of bytes newly available.
    fn poll_completed(&mut self) -> usize {
        let queue = match self.queue.as_mut() {
            Some(q) => q,
            None => return 0,
        };

        let mut total = 0;
        while queue.has_used() {
            let (id, len) = match queue.pop_used() {
                Some(v) => v,
                None => break,
            };
            let desc_idx = id as u16;
            let buf_idx = desc_idx as usize;

            if buf_idx < RNG_BUF_COUNT {
                // Copy data from the static buffer to the ready queue
                let data_len = len as usize;
                if data_len > 0 {
                    let buffers = RNG_BUFFERS.lock();
                    let mut ready = RNG_READY_DATA.lock();
                    ready.extend_from_slice(&buffers[buf_idx][..data_len.min(RNG_BUF_SIZE)]);
                    total += data_len.min(RNG_BUF_SIZE);
                }

                // Mark buffer as no longer in flight
                RNG_BUF_IN_FLIGHT.lock()[buf_idx] = false;

                // Re-submit the buffer for more random data
                let buf_phys = super::virt_to_phys(RNG_BUFFERS.lock()[buf_idx].as_ptr() as usize);
                queue.set_desc(
                    desc_idx,
                    buf_phys,
                    RNG_BUF_SIZE as u32,
                    desc_flags::WRITE,
                    0,
                );
                queue.submit(desc_idx);
                RNG_BUF_IN_FLIGHT.lock()[buf_idx] = true;
            }
        }
        self.transport.notify(queue);
        total
    }

    /// Read random bytes from the device.
    /// Polls the virtqueue for completed buffers and returns data from the ready queue.
    pub fn read(&mut self, buf: &mut [u8]) -> Result<usize, &'static str> {
        // First, poll for any newly completed data
        self.poll_completed();

        // Drain from the ready queue
        let mut ready = RNG_READY_DATA.lock();
        let n = buf.len().min(ready.len());
        if n == 0 {
            return Err("virtio-rng: no entropy available");
        }
        buf[..n].copy_from_slice(&ready[..n]);
        ready.drain(..n);
        Ok(n)
    }

    /// Check if random data is available without consuming it
    pub fn data_available(&mut self) -> bool {
        self.poll_completed();
        !RNG_READY_DATA.lock().is_empty()
    }
}

/// Global virtio-rng driver instance
static VIRTIO_RNG: Mutex<Option<VirtioRng>> = Mutex::new(None);

/// Initialize virtio-rng from a transport
pub fn init_virtio_rng(transport: VirtioTransport) -> Result<(), &'static str> {
    let rng = VirtioRng::new(transport)?;
    *VIRTIO_RNG.lock() = Some(rng);
    Ok(())
}

/// Read random bytes from the virtio-rng device
pub fn read_random(buf: &mut [u8]) -> Result<usize, &'static str> {
    match VIRTIO_RNG.lock().as_mut() {
        Some(rng) => rng.read(buf),
        None => Err("virtio-rng: not initialized"),
    }
}

/// Check if virtio-rng is available
pub fn is_available() -> bool {
    VIRTIO_RNG.lock().is_some()
}

/// Check if data is available
pub fn data_available() -> bool {
    match VIRTIO_RNG.lock().as_mut() {
        Some(rng) => rng.data_available(),
        None => false,
    }
}
