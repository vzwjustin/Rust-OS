//! GDataInputStream matching `gio/gdatainputstream.h` / `gio/gdatainputstream.c`.
//!
//! Provides the `DataInputStream` wrapper to read structured binary data and
//! text lines from an underlying `InputStream`.
//!
//! Fully `no_std` compatible using `alloc`.

use crate::error::Error;
use crate::gcancellable::GCancellable;
use crate::ginputstream::InputStream;
use crate::gioerror::IOErrorEnum;
use crate::prelude::*;
use spin::Mutex;

/// Endianness byte order matching `GDataStreamByteOrder`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[repr(i32)]
pub enum DataStreamByteOrder {
    /// Big Endian (`G_DATA_STREAM_BYTE_ORDER_BIG_ENDIAN`).
    BigEndian = 0,
    /// Little Endian (`G_DATA_STREAM_BYTE_ORDER_LITTLE_ENDIAN`).
    LittleEndian,
    /// Host Endian (`G_DATA_STREAM_BYTE_ORDER_HOST_ENDIAN`).
    HostEndian,
}

/// Newline types for line parsing matching `GDataStreamNewlineType`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[repr(i32)]
pub enum DataStreamNewlineType {
    /// Line Feed (`G_DATA_STREAM_NEWLINE_TYPE_LF`).
    Lf = 0,
    /// Carriage Return (`G_DATA_STREAM_NEWLINE_TYPE_CR`).
    Cr,
    /// Carriage Return + Line Feed (`G_DATA_STREAM_NEWLINE_TYPE_CR_LF`).
    CrLf,
    /// Any newline representation (`G_DATA_STREAM_NEWLINE_TYPE_ANY`).
    Any,
}

struct DataInputStreamState {
    byte_order: DataStreamByteOrder,
    newline_type: DataStreamNewlineType,
}

/// An input stream wrapper to read structured data (`GDataInputStream`).
pub struct DataInputStream {
    base_stream: InputStream,
    state: Mutex<DataInputStreamState>,
}

impl DataInputStream {
    /// Create a new `DataInputStream` wrapping `base_stream`.
    ///
    /// Mirrors `g_data_input_stream_new`.
    pub fn new(base_stream: InputStream) -> Self {
        Self {
            base_stream,
            state: Mutex::new(DataInputStreamState {
                byte_order: DataStreamByteOrder::BigEndian,
                newline_type: DataStreamNewlineType::Lf,
            }),
        }
    }

    /// Gets the configured byte order.
    ///
    /// Mirrors `g_data_input_stream_get_byte_order`.
    pub fn get_byte_order(&self) -> DataStreamByteOrder {
        self.state.lock().byte_order
    }

    /// Sets the byte order.
    ///
    /// Mirrors `g_data_input_stream_set_byte_order`.
    pub fn set_byte_order(&self, order: DataStreamByteOrder) {
        self.state.lock().byte_order = order;
    }

    /// Gets the configured newline type.
    ///
    /// Mirrors `g_data_input_stream_get_newline_type`.
    pub fn get_newline_type(&self) -> DataStreamNewlineType {
        self.state.lock().newline_type
    }

    /// Sets the newline type.
    ///
    /// Mirrors `g_data_input_stream_set_newline_type`.
    pub fn set_newline_type(&self, type_: DataStreamNewlineType) {
        self.state.lock().newline_type = type_;
    }

    /// Gets the base input stream.
    ///
    /// Mirrors `g_data_input_stream_get_base_stream`.
    pub fn get_base_stream(&self) -> InputStream {
        self.base_stream.clone()
    }

    fn read_bytes(
        &self,
        buffer: &mut [u8],
        cancellable: Option<&GCancellable>,
    ) -> Result<(), Error> {
        let (n, err) = self.base_stream.read_all(buffer, cancellable)?;
        if n < buffer.len() {
            if let Some(e) = err {
                return Err(e);
            } else {
                return Err(Error::new(
                    crate::gioerror::io_error_quark(),
                    IOErrorEnum::Failed.to_code(),
                    "Unexpected EOF",
                ));
            }
        }
        Ok(())
    }

    /// Read a single byte.
    ///
    /// Mirrors `g_data_input_stream_read_byte`.
    pub fn read_byte(&self, cancellable: Option<&GCancellable>) -> Result<u8, Error> {
        let mut buf = [0u8; 1];
        self.read_bytes(&mut buf, cancellable)?;
        Ok(buf[0])
    }

    /// Read a 16-bit signed integer.
    ///
    /// Mirrors `g_data_input_stream_read_int16`.
    pub fn read_int16(&self, cancellable: Option<&GCancellable>) -> Result<i16, Error> {
        let mut buf = [0u8; 2];
        self.read_bytes(&mut buf, cancellable)?;
        let val = u16::from_ne_bytes(buf);
        let byte_order = self.get_byte_order();
        let decoded = match byte_order {
            DataStreamByteOrder::BigEndian => u16::from_be(val),
            DataStreamByteOrder::LittleEndian => u16::from_le(val),
            DataStreamByteOrder::HostEndian => val,
        };
        Ok(decoded as i16)
    }

    /// Read a 16-bit unsigned integer.
    ///
    /// Mirrors `g_data_input_stream_read_uint16`.
    pub fn read_uint16(&self, cancellable: Option<&GCancellable>) -> Result<u16, Error> {
        let mut buf = [0u8; 2];
        self.read_bytes(&mut buf, cancellable)?;
        let val = u16::from_ne_bytes(buf);
        let byte_order = self.get_byte_order();
        let decoded = match byte_order {
            DataStreamByteOrder::BigEndian => u16::from_be(val),
            DataStreamByteOrder::LittleEndian => u16::from_le(val),
            DataStreamByteOrder::HostEndian => val,
        };
        Ok(decoded)
    }

    /// Read a 32-bit signed integer.
    ///
    /// Mirrors `g_data_input_stream_read_int32`.
    pub fn read_int32(&self, cancellable: Option<&GCancellable>) -> Result<i32, Error> {
        let mut buf = [0u8; 4];
        self.read_bytes(&mut buf, cancellable)?;
        let val = u32::from_ne_bytes(buf);
        let byte_order = self.get_byte_order();
        let decoded = match byte_order {
            DataStreamByteOrder::BigEndian => u32::from_be(val),
            DataStreamByteOrder::LittleEndian => u32::from_le(val),
            DataStreamByteOrder::HostEndian => val,
        };
        Ok(decoded as i32)
    }

    /// Read a 32-bit unsigned integer.
    ///
    /// Mirrors `g_data_input_stream_read_uint32`.
    pub fn read_uint32(&self, cancellable: Option<&GCancellable>) -> Result<u32, Error> {
        let mut buf = [0u8; 4];
        self.read_bytes(&mut buf, cancellable)?;
        let val = u32::from_ne_bytes(buf);
        let byte_order = self.get_byte_order();
        let decoded = match byte_order {
            DataStreamByteOrder::BigEndian => u32::from_be(val),
            DataStreamByteOrder::LittleEndian => u32::from_le(val),
            DataStreamByteOrder::HostEndian => val,
        };
        Ok(decoded)
    }

    /// Read a 64-bit signed integer.
    ///
    /// Mirrors `g_data_input_stream_read_int64`.
    pub fn read_int64(&self, cancellable: Option<&GCancellable>) -> Result<i64, Error> {
        let mut buf = [0u8; 8];
        self.read_bytes(&mut buf, cancellable)?;
        let val = u64::from_ne_bytes(buf);
        let byte_order = self.get_byte_order();
        let decoded = match byte_order {
            DataStreamByteOrder::BigEndian => u64::from_be(val),
            DataStreamByteOrder::LittleEndian => u64::from_le(val),
            DataStreamByteOrder::HostEndian => val,
        };
        Ok(decoded as i64)
    }

    /// Read a 64-bit unsigned integer.
    ///
    /// Mirrors `g_data_input_stream_read_uint64`.
    pub fn read_uint64(&self, cancellable: Option<&GCancellable>) -> Result<u64, Error> {
        let mut buf = [0u8; 8];
        self.read_bytes(&mut buf, cancellable)?;
        let val = u64::from_ne_bytes(buf);
        let byte_order = self.get_byte_order();
        let decoded = match byte_order {
            DataStreamByteOrder::BigEndian => u64::from_be(val),
            DataStreamByteOrder::LittleEndian => u64::from_le(val),
            DataStreamByteOrder::HostEndian => val,
        };
        Ok(decoded)
    }

    /// Read a line of text, validating it as UTF-8.
    ///
    /// Mirrors `g_data_input_stream_read_line_utf8`.
    pub fn read_line_utf8(
        &self,
        cancellable: Option<&GCancellable>,
    ) -> Result<Option<String>, Error> {
        self.read_line(cancellable)
    }

    /// Read a line of text.
    ///
    /// Mirrors `g_data_input_stream_read_line`.
    pub fn read_line(&self, cancellable: Option<&GCancellable>) -> Result<Option<String>, Error> {
        let newline_type = self.get_newline_type();
        let mut line_bytes = Vec::new();
        let mut eof = true;

        loop {
            let mut byte_buf = [0u8; 1];
            match self.base_stream.read(&mut byte_buf, cancellable) {
                Ok(0) => {
                    break;
                }
                Ok(_) => {
                    eof = false;
                    let b = byte_buf[0];
                    line_bytes.push(b);

                    match newline_type {
                        DataStreamNewlineType::Lf => {
                            if b == b'\n' {
                                line_bytes.pop();
                                break;
                            }
                        }
                        DataStreamNewlineType::Cr => {
                            if b == b'\r' {
                                line_bytes.pop();
                                break;
                            }
                        }
                        DataStreamNewlineType::CrLf => {
                            if line_bytes.len() >= 2
                                && line_bytes[line_bytes.len() - 2] == b'\r'
                                && b == b'\n'
                            {
                                line_bytes.pop();
                                line_bytes.pop();
                                break;
                            }
                        }
                        DataStreamNewlineType::Any => {
                            if b == b'\n' {
                                line_bytes.pop();
                                break;
                            } else if b == b'\r' {
                                line_bytes.pop();
                                if self.base_stream.can_seek() {
                                    let mut next_buf = [0u8; 1];
                                    if let Ok(1) = self.base_stream.read(&mut next_buf, cancellable)
                                    {
                                        if next_buf[0] != b'\n' {
                                            self.base_stream.seek(
                                                -1,
                                                crate::gseekable::SeekType::Cur,
                                                cancellable,
                                            )?;
                                        }
                                    }
                                }
                                break;
                            }
                        }
                    }
                }
                Err(e) => {
                    return Err(e);
                }
            }
        }

        if eof && line_bytes.is_empty() {
            Ok(None)
        } else {
            let s = String::from_utf8(line_bytes).map_err(|_| {
                Error::new(
                    crate::gioerror::io_error_quark(),
                    IOErrorEnum::Failed.to_code(),
                    "Invalid UTF-8 sequence",
                )
            })?;
            Ok(Some(s))
        }
    }

    /// Read until any character in `stop_chars` is found.
    ///
    /// Mirrors `g_data_input_stream_read_upto`.
    pub fn read_upto(
        &self,
        stop_chars: &str,
        cancellable: Option<&GCancellable>,
    ) -> Result<Option<String>, Error> {
        let mut line_bytes = Vec::new();
        let mut eof = true;

        loop {
            if !self.base_stream.can_seek() {
                return Err(Error::new(
                    crate::gioerror::io_error_quark(),
                    IOErrorEnum::NotSupported.to_code(),
                    "read_upto requires a seekable stream",
                ));
            }

            let mut byte_buf = [0u8; 1];
            match self.base_stream.read(&mut byte_buf, cancellable) {
                Ok(0) => {
                    break;
                }
                Ok(_) => {
                    let b = byte_buf[0];
                    if stop_chars.as_bytes().contains(&b) {
                        self.base_stream
                            .seek(-1, crate::gseekable::SeekType::Cur, cancellable)?;
                        break;
                    }
                    eof = false;
                    line_bytes.push(b);
                }
                Err(e) => {
                    return Err(e);
                }
            }
        }

        if eof && line_bytes.is_empty() {
            Ok(None)
        } else {
            let s = String::from_utf8(line_bytes).map_err(|_| {
                Error::new(
                    crate::gioerror::io_error_quark(),
                    IOErrorEnum::Failed.to_code(),
                    "Invalid UTF-8 sequence",
                )
            })?;
            Ok(Some(s))
        }
    }
}

// ──────────────────────────── Tests ───────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::bytes::Bytes;
    use crate::ginputstream::MemoryInputStream;

    #[test]
    fn test_data_input_stream_binary_big_endian() {
        let data = vec![0x12, 0x34, 0x00, 0x00, 0x00, 0x0A];
        let base = InputStream::from(MemoryInputStream::new_from_bytes(Bytes::from_vec(data)));
        let stream = DataInputStream::new(base);
        stream.set_byte_order(DataStreamByteOrder::BigEndian);

        assert_eq!(stream.read_uint16(None).unwrap(), 0x1234);
        assert_eq!(stream.read_uint32(None).unwrap(), 0x0000000A);
    }

    #[test]
    fn test_data_input_stream_binary_little_endian() {
        let data = vec![0x34, 0x12, 0x0A, 0x00, 0x00, 0x00];
        let base = InputStream::from(MemoryInputStream::new_from_bytes(Bytes::from_vec(data)));
        let stream = DataInputStream::new(base);
        stream.set_byte_order(DataStreamByteOrder::LittleEndian);

        assert_eq!(stream.read_uint16(None).unwrap(), 0x1234);
        assert_eq!(stream.read_uint32(None).unwrap(), 0x0000000A);
    }

    #[test]
    fn test_data_input_stream_read_line_lf() {
        let data = b"line 1\nline 2\n".to_vec();
        let base = InputStream::from(MemoryInputStream::new_from_bytes(Bytes::from_vec(data)));
        let stream = DataInputStream::new(base);
        stream.set_newline_type(DataStreamNewlineType::Lf);

        assert_eq!(stream.read_line(None).unwrap(), Some("line 1".to_owned()));
        assert_eq!(stream.read_line(None).unwrap(), Some("line 2".to_owned()));
        assert_eq!(stream.read_line(None).unwrap(), None);
    }

    #[test]
    fn test_data_input_stream_read_line_cr_lf() {
        let data = b"line 1\r\nline 2\r\n".to_vec();
        let base = InputStream::from(MemoryInputStream::new_from_bytes(Bytes::from_vec(data)));
        let stream = DataInputStream::new(base);
        stream.set_newline_type(DataStreamNewlineType::CrLf);

        assert_eq!(stream.read_line(None).unwrap(), Some("line 1".to_owned()));
        assert_eq!(stream.read_line(None).unwrap(), Some("line 2".to_owned()));
        assert_eq!(stream.read_line(None).unwrap(), None);
    }

    #[test]
    fn test_data_input_stream_read_line_any() {
        let data = b"line 1\nline 2\rline 3\r\n".to_vec();
        let base = InputStream::from(MemoryInputStream::new_from_bytes(Bytes::from_vec(data)));
        let stream = DataInputStream::new(base);
        stream.set_newline_type(DataStreamNewlineType::Any);

        assert_eq!(stream.read_line(None).unwrap(), Some("line 1".to_owned()));
        assert_eq!(stream.read_line(None).unwrap(), Some("line 2".to_owned()));
        assert_eq!(stream.read_line(None).unwrap(), Some("line 3".to_owned()));
    }

    #[test]
    fn test_data_input_stream_read_upto() {
        let data = b"some data;other data".to_vec();
        let base = InputStream::from(MemoryInputStream::new_from_bytes(Bytes::from_vec(data)));
        let stream = DataInputStream::new(base);

        assert_eq!(
            stream.read_upto(";", None).unwrap(),
            Some("some data".to_owned())
        );
        // The delimiter semicolon is NOT consumed, so it should be the next read byte.
        assert_eq!(stream.read_byte(None).unwrap(), b';');
        assert_eq!(
            stream.read_upto(";", None).unwrap(),
            Some("other data".to_owned())
        );
    }
}
