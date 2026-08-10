//! Viewer settings shared across UI modules.

use omt_media::{BufferSettings, Quality};

/// Receive quality preset (suggested encode quality sent to the peer).
///
/// Official OMT "low bandwidth" is 1/8 preview mode (`OMTReceiveFlags.Preview`).
/// [`omt_media::ReceiverSession`] does not request or decode preview frames yet,
/// so this enum only covers quality suggestions.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VideoQualityPreset {
    Default,
    Low,
    Medium,
    High,
}

impl VideoQualityPreset {
    pub fn to_quality(self) -> Quality {
        match self {
            Self::Default => Quality::Default,
            Self::Low => Quality::Low,
            Self::Medium => Quality::Medium,
            Self::High => Quality::High,
        }
    }
}

/// Viewer settings toggled from the context menu / preferences.
#[derive(Debug, Clone)]
pub struct MonitorSettings {
    pub show_alpha: bool,
    /// SMPTE ST 2046-1 action/title safe guides over the picture.
    pub safe_area: bool,
    pub vu_meter: bool,
    pub quality: VideoQualityPreset,
    pub audio_boost_db: i32,
    /// Linked or independent A/V playout buffers (PTS gate).
    pub buffer: BufferSettings,
}

impl Default for MonitorSettings {
    fn default() -> Self {
        Self {
            show_alpha: false,
            safe_area: false,
            vu_meter: true,
            quality: VideoQualityPreset::Default,
            audio_boost_db: 0,
            buffer: BufferSettings::default(),
        }
    }
}
