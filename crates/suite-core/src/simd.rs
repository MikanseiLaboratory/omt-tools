//! Runtime SIMD capability summary for diagnostics UI.
//!
//! Labels mirror the instruction families used by `vmx-rs` / `openmediatransport-rs`
//! (SSE2 / SSSE3 convert, SSE4.2 FDCT, AVX2+BMI2 codec path, NEON on aarch64).

/// Detected CPU features relevant to the OMT media stack.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SimdCapabilities {
    /// SSE2 (x86_64 color convert).
    pub sse2: bool,
    /// SSSE3 (x86_64 color convert / swizzle).
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

    /// Preferred encode/decode SIMD path label (matches `vmx::simd::dispatch`).
    pub fn preferred_path_label(&self) -> &'static str {
        if self.avx2_path() {
            "AVX2"
        } else if self.sse42 {
            "SSE4.2"
        } else if self.neon {
            "NEON"
        } else {
            "Scalar"
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
    /// Example: `SSE2, SSSE3, SSE4.2, AVX2, BMI2 (path: AVX2)`.
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
        assert!(matches!(path, "AVX2" | "SSE4.2" | "NEON" | "Scalar"));
        if caps.avx2_path() {
            assert_eq!(path, "AVX2");
        }
        let summary = caps.summary();
        assert!(summary.contains("path:"));
    }
}
