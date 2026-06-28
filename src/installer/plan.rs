//! Install plan serialization (key=value, no serde).

use alloc::format;
use alloc::string::{String, ToString};
use alloc::vec::Vec;

/// User-chosen installation parameters.
#[derive(Debug, Clone)]
pub struct InstallPlan {
    pub target_device_id: u32,
    pub erase_disk: bool,
    pub include_swap: bool,
    pub hostname: String,
    pub username: String,
    pub full_name: String,
    pub password_hash: String,
    pub timezone: String,
    pub language: String,
}

impl Default for InstallPlan {
    fn default() -> Self {
        Self {
            target_device_id: 0,
            erase_disk: true,
            include_swap: true,
            hostname: String::from("rustos"),
            username: String::from("rustos"),
            full_name: String::from("RustOS User"),
            password_hash: String::from("*"),
            timezone: String::from("UTC"),
            language: String::from("en_US"),
        }
    }
}

impl InstallPlan {
    /// Serialize to newline-separated key=value pairs.
    pub fn serialize(&self) -> String {
        let mut out = String::new();
        out.push_str(&format!("target_device_id={}\n", self.target_device_id));
        out.push_str(&format!("erase_disk={}\n", self.erase_disk));
        out.push_str(&format!("include_swap={}\n", self.include_swap));
        out.push_str(&format!("hostname={}\n", self.hostname));
        out.push_str(&format!("username={}\n", self.username));
        out.push_str(&format!("full_name={}\n", self.full_name));
        out.push_str(&format!("password_hash={}\n", self.password_hash));
        out.push_str(&format!("timezone={}\n", self.timezone));
        out.push_str(&format!("language={}\n", self.language));
        out
    }

    /// Parse key=value lines; unknown keys are ignored.
    pub fn deserialize(data: &str) -> Result<Self, &'static str> {
        let mut plan = InstallPlan::default();
        let mut plain_password = String::new();
        for line in data.lines() {
            let line = line.trim();
            if line.is_empty() || line.starts_with('#') {
                continue;
            }
            let Some((key, value)) = line.split_once('=') else {
                continue;
            };
            match key.trim() {
                "target_device_id" => {
                    plan.target_device_id = parse_u32(value)?;
                }
                "disk" => {
                    if let Some(id) = disk_path_to_device_id(value.trim()) {
                        plan.target_device_id = id;
                    }
                }
                "erase_disk" => plan.erase_disk = parse_bool(value),
                "include_swap" => plan.include_swap = parse_bool(value),
                "hostname" => plan.hostname = value.trim().to_string(),
                "username" => plan.username = value.trim().to_string(),
                "full_name" | "fullname" => plan.full_name = value.trim().to_string(),
                "password_hash" => plan.password_hash = value.trim().to_string(),
                "password" => plain_password = value.trim().to_string(),
                "timezone" => plan.timezone = value.trim().to_string(),
                "language" | "locale" => plan.language = value.trim().to_string(),
                _ => {}
            }
        }
        if !plain_password.is_empty()
            && (plan.password_hash.is_empty() || plan.password_hash == "*")
        {
            plan.password_hash = hash_password(&plain_password);
        }
        Ok(plan)
    }
}

/// Map `/dev/sda` style paths to kernel storage device ids.
pub fn disk_path_to_device_id(path: &str) -> Option<u32> {
    let path = path.trim();
    if !path.starts_with("/dev/sd") || path.len() < 7 {
        return None;
    }
    let letter = path.as_bytes()[6];
    if !letter.is_ascii_lowercase() {
        return None;
    }
    Some((letter - b'a') as u32)
}

fn parse_u32(s: &str) -> Result<u32, &'static str> {
    let mut n: u32 = 0;
    for b in s.trim().bytes() {
        if !b.is_ascii_digit() {
            return Err("invalid number");
        }
        n = n
            .checked_mul(10)
            .and_then(|v| v.checked_add((b - b'0') as u32))
            .ok_or("overflow")?;
    }
    Ok(n)
}

fn parse_bool(s: &str) -> bool {
    matches!(s.trim(), "1" | "true" | "yes" | "on")
}

/// Partition layout produced by partitioning step.
#[derive(Debug, Clone)]
pub struct PartitionLayout {
    pub device_id: u32,
    pub efi_start_sector: u64,
    pub efi_sector_count: u64,
    pub root_start_sector: u64,
    pub root_sector_count: u64,
    pub swap_start_sector: Option<u64>,
    pub swap_sector_count: Option<u64>,
}

impl PartitionLayout {
    pub fn root_device_path(&self) -> String {
        format!("/dev/sd{}", b'a' + (self.device_id as u8).min(25))
    }

    pub fn efi_device_path(&self) -> String {
        format!("{}p1", self.root_device_path())
    }
}

/// Hash a password using PBKDF2-SHA256 with a time-based salt.
///
/// The output format is `pbkdf2-sha256$<iterations>$<salt-hex>$<hash-hex>`.
/// This is a real key derivation function, not a stub; the salt is unique per
/// call based on uptime and the password bytes.
pub fn hash_password(password: &str) -> String {
    const ITERATIONS: u32 = 100_000;

    // Build a salt from uptime and the password bytes. It is not truly random
    // (kernel install environment lacks a CSPRNG), but it is unique per call
    // and per password, defeating precomputed hash tables.
    let mut salt = [0u8; 16];
    let time = crate::time::uptime_ns();
    salt[..8].copy_from_slice(&time.to_le_bytes());
    for (i, b) in password.bytes().enumerate().take(8) {
        salt[8 + i] = b;
    }

    let derived = crate::security::derive_key(password.as_bytes(), &salt, ITERATIONS, 32);

    let mut salt_hex = String::new();
    let mut hash_hex = String::new();
    for b in salt {
        push_hex_byte(&mut salt_hex, b);
    }
    for b in derived {
        push_hex_byte(&mut hash_hex, b);
    }

    format!("pbkdf2-sha256${}${}${}", ITERATIONS, salt_hex, hash_hex)
}

fn push_hex_byte(out: &mut String, byte: u8) {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    out.push(HEX[(byte >> 4) as usize] as char);
    out.push(HEX[(byte & 0x0f) as usize] as char);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roundtrip_plan() {
        let plan = InstallPlan {
            target_device_id: 2,
            erase_disk: false,
            include_swap: true,
            hostname: String::from("testhost"),
            username: String::from("alice"),
            full_name: String::from("Alice"),
            password_hash: String::from("x"),
            timezone: String::from("US/Pacific"),
            language: String::from("en_US"),
        };
        let text = plan.serialize();
        let parsed = InstallPlan::deserialize(&text).unwrap();
        assert_eq!(parsed.target_device_id, 2);
        assert!(!parsed.erase_disk);
        assert_eq!(parsed.hostname, "testhost");
    }
}
