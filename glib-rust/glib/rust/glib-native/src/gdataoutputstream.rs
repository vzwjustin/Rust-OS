//! GDataOutputStream matching `gio/gdataoutputstream.h` / `gio/gdataoutputstream.c`.
//!
//! Provides the `DataOutputStream` wrapper to write structured binary data and
//! text strings to an underlying `OutputStream`.
//!
//! Fully `no_std` compatible using `alloc`.

use crate::error::Error;
use crate::gcancellable::GCancellable;
use crate::gdatainputstream::DataStreamByteOrder;
use crate::goutputstream::OutputStream;
use crate::prelude::*;
use spin::Mutex;

struct DataOutputStreamState {
    byte_order: DataStreamByteOrder,
}

/// An output stream wrapper to write structured data (`GDataOutputStream`).
pub struct DataOutputStream {
    base_stream: OutputStream,
    state: Mutex<DataOutputStreamState>,
}

impl DataOutputStream {
    /// Create a new `DataOutputStream` wrapping `base_stream`.
    ///
    /// Mirrors `g_data_output_stream_new`.
    pub fn new(base_stream: OutputStream) -> Self {
        Self {
            base_stream,
            state: Mutex::new(DataOutputStreamState {
                byte_order: DataStreamByteOrder::BigEndian,
            }),
        }
    }

    /// Gets the configured byte order.
    ///
    /// Mirrors `g_data_output_stream_get_byte_order`.
    pub fn get_byte_order(&self) -> DataStreamByteOrder {
        self.state.lock().byte_order
    }

    /// Sets the byte order.
    ///
    /// Mirrors `g_data_output_stream_set_byte_order`.
    pub fn set_byte_order(&self, order: DataStreamByteOrder) {
        self.state.lock().byte_order = order;
    }

    /// Gets the base output stream.
    ///
    /// Mirrors `g_data_output_stream_get_base_stream`.
    pub fn get_base_stream(&self) -> OutputStream {
        self.base_stream.clone()
    }

    fn write_bytes(&self, buffer: &[u8], cancellable: Option<&GCancellable>) -> Result<(), Error> {
        let (n, err) = self.base_stream.write_all(buffer, cancellable)?;
        if n < buffer.len() {
            if let Some(e) = err {
                return Err(e);
            } else {
                return Err(Error::new(
                    crate::gioerror::io_error_quark(),
                    crate::gioerror::IOErrorEnum::Failed.to_code(),
                    "Incomplete write",
                ));
            }
        }
        Ok(())
    }

    /// Write a single byte.
    ///
    /// Mirrors `g_data_output_stream_put_byte`.
    pub fn put_byte(&self, data: u8, cancellable: Option<&GCancellable>) -> Result<(), Error> {
        self.write_bytes(&[data], cancellable)
    }

    /// Write a 16-bit signed integer.
    ///
    /// Mirrors `g_data_output_stream_put_int16`.
    pub fn put_int16(&self, data: i16, cancellable: Option<&GCancellable>) -> Result<(), Error> {
        let val = data as u16;
        let byte_order = self.get_byte_order();
        let encoded = match byte_order {
            DataStreamByteOrder::BigEndian => val.to_be(),
            DataStreamByteOrder::LittleEndian => val.to_le(),
            DataStreamByteOrder::HostEndian => val,
        };
        self.write_bytes(&encoded.to_ne_bytes(), cancellable)
    }

    /// Write a 16-bit unsigned integer.
    ///
    /// Mirrors `g_data_output_stream_put_uint16`.
    pub fn put_uint16(&self, data: u16, cancellable: Option<&GCancellable>) -> Result<(), Error> {
        let byte_order = self.get_byte_order();
        let encoded = match byte_order {
            DataStreamByteOrder::BigEndian => data.to_be(),
            DataStreamByteOrder::LittleEndian => data.to_le(),
            DataStreamByteOrder::HostEndian => data,
        };
        self.write_bytes(&encoded.to_ne_bytes(), cancellable)
    }

    /// Write a 32-bit signed integer.
    ///
    /// Mirrors `g_data_output_stream_put_int32`.
    pub fn put_int32(&self, data: i32, cancellable: Option<&GCancellable>) -> Result<(), Error> {
        let val = data as u32;
        let byte_order = self.get_byte_order();
        let encoded = match byte_order {
            DataStreamByteOrder::BigEndian => val.to_be(),
            DataStreamByteOrder::LittleEndian => val.to_le(),
            DataStreamByteOrder::HostEndian => val,
        };
        self.write_bytes(&encoded.to_ne_bytes(), cancellable)
    }

    /// Write a 32-bit unsigned integer.
    ///
    /// Mirrors `g_data_output_stream_put_uint32`.
    pub fn put_uint32(&self, data: u32, cancellable: Option<&GCancellable>) -> Result<(), Error> {
        let byte_order = self.get_byte_order();
        let encoded = match byte_order {
            DataStreamByteOrder::BigEndian => data.to_be(),
            DataStreamByteOrder::LittleEndian => data.to_le(),
            DataStreamByteOrder::HostEndian => data,
        };
        self.write_bytes(&encoded.to_ne_bytes(), cancellable)
    }

    /// Write a 64-bit signed integer.
    ///
    /// Mirrors `g_data_output_stream_put_int64`.
    pub fn put_int64(&self, data: i64, cancellable: Option<&GCancellable>) -> Result<(), Error> {
        let val = data as u64;
        let byte_order = self.get_byte_order();
        let encoded = match byte_order {
            DataStreamByteOrder::BigEndian => val.to_be(),
            DataStreamByteOrder::LittleEndian => val.to_le(),
            DataStreamByteOrder::HostEndian => val,
        };
        self.write_bytes(&encoded.to_ne_bytes(), cancellable)
    }

    /// Write a 64-bit unsigned integer.
    ///
    /// Mirrors `g_data_output_stream_put_uint64`.
    pub fn put_uint64(&self, data: u64, cancellable: Option<&GCancellable>) -> Result<(), Error> {
        let byte_order = self.get_byte_order();
        let encoded = match byte_order {
            DataStreamByteOrder::BigEndian => data.to_be(),
            DataStreamByteOrder::LittleEndian => data.to_le(),
            DataStreamByteOrder::HostEndian => data,
        };
        self.write_bytes(&encoded.to_ne_bytes(), cancellable)
    }

    /// Write a string.
    ///
    /// Mirrors `g_data_output_stream_put_string`.
    pub fn put_string(&self, data: &str, cancellable: Option<&GCancellable>) -> Result<(), Error> {
        self.write_bytes(data.as_bytes(), cancellable)
    }
}

// ──────────────────────────── Tests ───────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::goutputstream::MemoryOutputStream;

    #[test]
    fn test_data_output_stream_big_endian() {
        let base = OutputStream::from(MemoryOutputStream::new_resizable());
        let stream = DataOutputStream::new(base);
        stream.set_byte_order(DataStreamByteOrder::BigEndian);

        stream.put_uint16(0x1234, None).unwrap();
        stream.put_uint32(0x0000000A, None).unwrap();
        stream.put_string("hello", None).unwrap();

        let underlying = stream.downcast_ref::<MemoryOutputStream>().unwrap();
        assert_eq!(&underlying.get_data()[..2], &[0x12, 0x34]);
        assert_eq!(&underlying.get_data()[2..6], &[0x00, 0x00, 0x00, 0x0A]);
        assert_eq!(&underlying.get_data()[6..], b"hello");
    }

    #[test]
    fn test_data_output_stream_little_endian() {
        let base = OutputStream::from(MemoryOutputStream::new_resizable());
        let stream = DataOutputStream::new(base);
        stream.set_byte_order(DataStreamByteOrder::LittleEndian);

        stream.put_uint16(0x1234, None).unwrap();
        stream.put_uint32(0x0000000A, None).unwrap();

        let underlying = stream.downcast_ref::<MemoryOutputStream>().unwrap();
        assert_eq!(&underlying.get_data()[..2], &[0x34, 0x12]);
        assert_eq!(&underlying.get_data()[2..], &[0x0A, 0x00, 0x00, 0x00]);
    }
}

// Helper trait to allow downcasting implementation back to concrete types for tests.
trait Downcast {
    fn downcast_ref<T: 'static>(&self) -> Option<&T>;
}

impl<I: crate::goutputstream::OutputStreamImpl + Send + Sync + 'static> Downcast for I {
    fn downcast_ref<T: 'static>(&self) -> Option<&T> {
        let any = self as &dyn core::any::Any;
        any.downcast_ref::<T>()
    }
}

impl DataOutputStream {
    /// Downcasts the underlying stream implementation to a concrete type.
    pub fn downcast_ref<T: 'static>(&self) -> Option<&T> {
        self.base_stream.downcast_ref::<T>()
    }
}
