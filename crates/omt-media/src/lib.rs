//! Shared OMT media helpers for the tools suite.

#![deny(missing_docs)]

mod audio_out;
mod color;
mod discovery;
mod playout;
mod receive;
mod runtime;
mod send;
mod stall;
mod stats;

pub use audio_out::{list_output_devices, AudioLevels, AudioOutput, AudioOutputDevice};
pub use color::{rgb_to_uyvy_pixel, uyvy_from_rgb_frame};
pub use playout::{BufferSettings, BufferUnit, DelaySetting};
pub use discovery::{DiscoveredSource, SourceBrowser, discover_sources, spawn_discover};
pub use receive::{
    ConnectOptions, LatestVideo, MetadataLogEntry, ReceiveCounters, ReceiveWorker, VideoFrame,
};
pub use send::{AudioToneConfig, SendSession, SendSessionConfig, SendStats};
pub use stall::{StallDetector, StallState};
pub use stats::FpsCounter;

pub use openmediatransport::{
    bgra_alpha_mask, bgra_to_rgba, bgra_to_rgba_into, Codec, ColorSpace, DecodedAudioFrame,
    DecodedVideoFrame, Discovery, FrameType, MediaFrame, MetadataFrame, Quality, ReceiverConfig,
    ReceiverSession, Sender, SenderConfig, SenderInfo, SessionState, SessionStatistics, Statistics,
};
pub use vmx::{Codec as VmxCodec, Config as VmxConfig, Profile as VmxProfile};
