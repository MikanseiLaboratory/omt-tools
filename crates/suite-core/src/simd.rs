//! Runtime SIMD capability summary for diagnostics UI.
//!
//! Labels mirror the instruction families used by `vmx-rs` /
//! `openmediatransport-rs`. Path selection follows `vmx::SimdCapabilities`
//! (AVX2+BMI2 → SSE4.2+SSSE3 → NEON → Scalar). UV-width gating for AVX2 is
//! applied later inside `vmx::Codec` and is not part of this host-only probe.

/// Detected CPU features relevant to the OMT media stack.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SimdCapabilities {
    /// SSE2 (x86_64 color convert).
    pub sse2: bool,
    /// SSSE3 (x86_64 color convert / swizzle; also required for SSE128 codec).
    pub ssse3: bool,
    /// SSE4.2 (x86_64 FDCT path).
    pub sse42: bool,
    /// AVX2 present (codec path also requires [`Self::bmi2`]).
    pub avx2: bool,
    /// BMI2 present (paired with AVX2 for the fast codec path).
    pub bmi2: bool,
    /// NEON (aarch64).
    pub neon: bool,
}

impl Default for SimdCapabilities {
    fn default() -> Self {
        Self::detect()
    }
}

impl SimdCapabilities {
    /// Probe the current CPU / target for supported SIMD families.
    pub fn detect() -> Self {
        #[cfg(target_arch = "x86_64")]
        {
            Self {
                sse2: is_x86_feature_detected!("sse2"),
                ssse3: is_x86_feature_detected!("ssse3"),
                sse42: is_x86_feature_detected!("sse4.2"),
                avx2: is_x86_feature_detected!("avx2"),
                bmi2: is_x86_feature_detected!("bmi2"),
                neon: false,
            }
        }
        #[cfg(target_arch = "aarch64")]
        {
            Self {
                sse2: false,
                ssse3: false,
                sse42: false,
                avx2: false,
                bmi2: false,
                neon: true,
            }
        }
        #[cfg(not(any(target_arch = "x86_64", target_arch = "aarch64")))]
        {
            Self {
                sse2: false,
                ssse3: false,
                sse42: false,
                avx2: false,
                bmi2: false,
                neon: false,
            }
        }
    }

    /// Whether the AVX2 codec path can run (AVX2 + BMI2), matching `vmx` dispatch.
    pub fn avx2_path(&self) -> bool {
        self.avx2 && self.bmi2
    }

    /// Whether the SSE128 codec path can run (SSE4.2 + SSSE3), matching `vmx`.
    pub fn sse128_path(&self) -> bool {
        self.sse42 && self.ssse3
    }

    /// Preferred encode/decode SIMD path id (same strings as `vmx::SimdPath`).
    pub fn preferred_path_label(&self) -> &'static str {
        #[cfg(target_arch = "x86_64")]
        {
            if self.avx2_path() {
                "avx2"
            } else if self.sse128_path() {
                "sse128"
            } else {
                "scalar"
            }
        }
        #[cfg(target_arch = "aarch64")]
        {
            if self.neon { "neon" } else { "scalar" }
        }
        #[cfg(not(any(target_arch = "x86_64", target_arch = "aarch64")))]
        {
            "scalar"
        }
    }

    /// Human-readable list of available instruction families.
    pub fn available_labels(&self) -> Vec<&'static str> {
        let mut labels = Vec::new();
        if self.sse2 {
            labels.push("SSE2");
        }
        if self.ssse3 {
            labels.push("SSSE3");
        }
        if self.sse42 {
            labels.push("SSE4.2");
        }
        if self.avx2 {
            labels.push("AVX2");
        }
        if self.bmi2 {
            labels.push("BMI2");
        }
        if self.neon {
            labels.push("NEON");
        }
        labels
    }

    /// Compact one-line summary for stats / settings panels.
    ///
    /// Example: `SSE2, SSSE3, SSE4.2, AVX2, BMI2 (path: avx2)`.
    pub fn summary(&self) -> String {
        let available = self.available_labels();
        let features = if available.is_empty() {
            "none".to_string()
        } else {
            available.join(", ")
        };
        format!("{features} (path: {})", self.preferred_path_label())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detect_returns_consistent_path() {
        let caps = SimdCapabilities::detect();
        let path = caps.preferred_path_label();
        assert!(matches!(path, "avx2" | "sse128" | "neon" | "scalar"));
        if caps.avx2_path() {
            assert_eq!(path, "avx2");
        } else if caps.sse128_path() {
            assert_eq!(path, "sse128");
        }
        let summary = caps.summary();
        assert!(summary.contains("path:"));
    }
}
