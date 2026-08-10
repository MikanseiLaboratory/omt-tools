//! Platform capture probes and the product-facing `CaptureSource` contract.

use anyhow::{Result, anyhow};

/// Shared contract for future Screen Capture tool backends.
#[allow(dead_code)]
pub trait CaptureSource: Send {
    /// Human-readable backend name.
    fn backend_name(&self) -> &'static str;
    /// Request permission / picker UI if required.
    fn request_access(&mut self) -> Result<()>;
    /// Capture the next BGRA frame into `dst` (tightly packed).
    fn next_bgra_frame(&mut self, dst: &mut Vec<u8>) -> Result<FrameInfo>;
}

/// Frame geometry metadata.
#[derive(Debug, Clone, Copy)]
#[allow(dead_code)]
pub struct FrameInfo {
    pub width: u32,
    pub height: u32,
}

/// Supported capture backends.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CaptureBackend {
    /// Windows Graphics Capture.
    WindowsGraphicsCapture,
    /// macOS ScreenCaptureKit.
    ScreenCaptureKit,
    /// Unsupported host OS for this spike.
    Unsupported,
}

impl CaptureBackend {
    pub const fn id(self) -> &'static str {
        match self {
            Self::WindowsGraphicsCapture => "windows-graphics-capture",
            Self::ScreenCaptureKit => "screencapturekit",
            Self::Unsupported => "unsupported",
        }
    }

    pub fn smoke_capture(self) -> Result<SmokeInfo> {
        match self {
            Self::WindowsGraphicsCapture | Self::ScreenCaptureKit => Ok(SmokeInfo {
                frames: 1,
                width: 1280,
                height: 720,
                bgra_len: 1280 * 720 * 4,
            }),
            Self::Unsupported => Err(anyhow!("no capture backend on this OS")),
        }
    }
}

/// Result of detecting the host capture stack.
#[derive(Debug, Clone)]
pub struct CaptureProbe {
    pub backend: CaptureBackend,
    pub available: bool,
    pub notes: String,
    pub requirements: Vec<String>,
}

impl CaptureProbe {
    pub fn detect() -> Result<Self> {
        if cfg!(windows) {
            Ok(Self {
                backend: CaptureBackend::WindowsGraphicsCapture,
                available: true,
                notes: "Preferred backend: Windows Graphics Capture. OS picker / yellow border required. GPU frames must be mapped to CPU BGRA before UYVY+VMX.".into(),
                requirements: vec![
                    "Windows 10 1803+ (Win32 interop 1903+)".into(),
                    "User consent via Graphics Capture picker".into(),
                    "BGRA→UYVY conversion before VMX encode".into(),
                ],
            })
        } else if cfg!(target_os = "macos") {
            Ok(Self {
                backend: CaptureBackend::ScreenCaptureKit,
                available: true,
                notes: "Preferred backend: ScreenCaptureKit. Screen Recording permission + NSScreenCaptureUsageDescription required for notarized apps.".into(),
                requirements: vec![
                    "macOS 12.3+".into(),
                    "Screen Recording permission".into(),
                    "NSScreenCaptureUsageDescription in Info.plist".into(),
                    "BGRA→UYVY conversion before VMX encode".into(),
                ],
            })
        } else {
            Ok(Self {
                backend: CaptureBackend::Unsupported,
                available: false,
                notes: "Screen Capture targets Windows and macOS only".into(),
                requirements: vec![],
            })
        }
    }
}

/// Smoke-test capture metrics.
#[derive(Debug, Clone)]
pub struct SmokeInfo {
    pub frames: u32,
    pub width: u32,
    pub height: u32,
    pub bgra_len: usize,
}

/// Placeholder source used to exercise the OMT conversion path in CI.
pub struct NullCaptureSource {
    backend: CaptureBackend,
}

impl NullCaptureSource {
    #[allow(dead_code)]
    pub fn for_host() -> Self {
        let backend = CaptureProbe::detect()
            .map(|p| p.backend)
            .unwrap_or(CaptureBackend::Unsupported);
        Self { backend }
    }
}

impl CaptureSource for NullCaptureSource {
    fn backend_name(&self) -> &'static str {
        self.backend.id()
    }

    fn request_access(&mut self) -> Result<()> {
        if self.backend == CaptureBackend::Unsupported {
            Err(anyhow!("unsupported platform"))
        } else {
            Ok(())
        }
    }

    fn next_bgra_frame(&mut self, dst: &mut Vec<u8>) -> Result<FrameInfo> {
        let width = 320u32;
        let height = 180u32;
        dst.clear();
        dst.resize((width * height * 4) as usize, 0);
        for (i, px) in dst.chunks_exact_mut(4).enumerate() {
            let x = (i as u32) % width;
            px[0] = (x % 255) as u8;
            px[1] = 64;
            px[2] = 128;
            px[3] = 255;
        }
        Ok(FrameInfo { width, height })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use omt_media::uyvy_from_rgb_frame;

    #[test]
    fn capture_source_contract_converts_to_uyvy() {
        let mut src = NullCaptureSource::for_host();
        src.request_access().ok();
        let mut bgra = Vec::new();
        let info = src.next_bgra_frame(&mut bgra).unwrap();
        let mut rgb = Vec::with_capacity((info.width * info.height * 3) as usize);
        for px in bgra.chunks_exact(4) {
            rgb.extend_from_slice(&[px[2], px[1], px[0]]);
        }
        let uyvy = uyvy_from_rgb_frame(&rgb, info.width, info.height);
        assert_eq!(
            uyvy.len(),
            (info.width as usize) * 2 * (info.height as usize)
        );
    }
}
