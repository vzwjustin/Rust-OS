#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AcpicaError {
    Unavailable,
    InvalidCString,
    Status(u32),
}

pub fn available() -> bool {
    imp::available()
}

pub fn initialize(rsdp_physical: u64) -> Result<(), AcpicaError> {
    imp::initialize(rsdp_physical)
}

pub fn evaluate_integer(path: &[u8], method: &[u8]) -> Result<u64, AcpicaError> {
    if !is_c_string(path) || !is_c_string(method) {
        return Err(AcpicaError::InvalidCString);
    }
    imp::evaluate_integer(path, method)
}

fn is_c_string(bytes: &[u8]) -> bool {
    matches!(bytes.last(), Some(0))
        && bytes[..bytes.len().saturating_sub(1)]
            .iter()
            .all(|&byte| byte != 0)
}

#[cfg(rustos_acpica)]
mod imp {
    use super::AcpicaError;
    use core::ffi::{c_char, c_int};

    unsafe extern "C" {
        fn rustos_acpica_available() -> c_int;
        fn rustos_acpica_initialize(rsdp_physical: u64) -> u32;
        fn rustos_acpica_evaluate_integer(
            path: *const c_char,
            method: *const c_char,
            out_value: *mut u64,
        ) -> u32;
    }

    pub fn available() -> bool {
        unsafe { rustos_acpica_available() != 0 }
    }

    pub fn initialize(rsdp_physical: u64) -> Result<(), AcpicaError> {
        match unsafe { rustos_acpica_initialize(rsdp_physical) } {
            0 => Ok(()),
            status => Err(AcpicaError::Status(status)),
        }
    }

    pub fn evaluate_integer(path: &[u8], method: &[u8]) -> Result<u64, AcpicaError> {
        let mut out = 0;
        let status = unsafe {
            rustos_acpica_evaluate_integer(
                path.as_ptr().cast::<c_char>(),
                method.as_ptr().cast::<c_char>(),
                &mut out,
            )
        };

        match status {
            0 => Ok(out),
            status => Err(AcpicaError::Status(status)),
        }
    }
}

#[cfg(not(rustos_acpica))]
mod imp {
    use super::AcpicaError;

    pub fn available() -> bool {
        false
    }

    pub fn initialize(_rsdp_physical: u64) -> Result<(), AcpicaError> {
        Err(AcpicaError::Unavailable)
    }

    pub fn evaluate_integer(_path: &[u8], _method: &[u8]) -> Result<u64, AcpicaError> {
        Err(AcpicaError::Unavailable)
    }
}
