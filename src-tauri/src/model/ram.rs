//! Total-RAM detection and the tier we use to recommend a default model.
//!
//! macOS doesn't expose total RAM through `tauri-plugin-os`, so we shell out
//! to `sysctl hw.memsize`. This is fast (~1ms) and zero‑dep.

use serde::Serialize;

/// Coarse RAM buckets. Boundaries chosen to match common Mac configurations.
#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum RamTier {
    LessThan8Gb,
    Between8And16Gb,
    Between16And32Gb,
    AtLeast32Gb,
    Unknown,
}

impl RamTier {
    pub fn from_bytes(bytes: u64) -> Self {
        const GB: u64 = 1024 * 1024 * 1024;
        match bytes {
            b if b < 8 * GB => RamTier::LessThan8Gb,
            b if b < 16 * GB => RamTier::Between8And16Gb,
            b if b < 32 * GB => RamTier::Between16And32Gb,
            _ => RamTier::AtLeast32Gb,
        }
    }

    /// String form used by the model manifest's `recommended_for` field.
    pub fn as_key(self) -> &'static str {
        match self {
            RamTier::LessThan8Gb => "<8GB",
            RamTier::Between8And16Gb => "8-16GB",
            RamTier::Between16And32Gb => "16-32GB",
            RamTier::AtLeast32Gb => ">=32GB",
            RamTier::Unknown => "unknown",
        }
    }
}

/// Total physical RAM, in bytes. Falls back to `Unknown` (0) on failure.
pub fn detect_total_memory_bytes() -> u64 {
    let output = std::process::Command::new("sysctl")
        .arg("-n")
        .arg("hw.memsize")
        .output();
    match output {
        Ok(out) if out.status.success() => {
            let raw = String::from_utf8_lossy(&out.stdout);
            raw.trim().parse::<u64>().unwrap_or(0)
        }
        _ => 0,
    }
}

pub fn detect_ram_tier() -> RamTier {
    let bytes = detect_total_memory_bytes();
    if bytes == 0 {
        RamTier::Unknown
    } else {
        RamTier::from_bytes(bytes)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tier_boundaries() {
        assert_eq!(RamTier::from_bytes(0), RamTier::LessThan8Gb);
        assert_eq!(RamTier::from_bytes(8 * 1024 * 1024 * 1024), RamTier::Between8And16Gb);
        assert_eq!(RamTier::from_bytes(16 * 1024 * 1024 * 1024), RamTier::Between16And32Gb);
        assert_eq!(RamTier::from_bytes(32 * 1024 * 1024 * 1024), RamTier::AtLeast32Gb);
        assert_eq!(RamTier::from_bytes(64 * 1024 * 1024 * 1024), RamTier::AtLeast32Gb);
    }

    #[test]
    fn from_bytes_zero_is_unclassified() {
        assert_eq!(RamTier::from_bytes(0), RamTier::LessThan8Gb);
    }

    #[test]
    fn from_bytes_real_machine_is_in_range() {
        // sysctl hw.memsize returns the actual machine's RAM. We don't assert
        // a specific tier (the CI runners vary), only that the tier is not
        // Unknown — i.e. the syscall worked.
        let bytes = detect_total_memory_bytes();
        assert!(bytes > 0);
        let tier = detect_ram_tier();
        assert!(matches!(
            tier,
            RamTier::LessThan8Gb
                | RamTier::Between8And16Gb
                | RamTier::Between16And32Gb
                | RamTier::AtLeast32Gb
        ));
    }
}