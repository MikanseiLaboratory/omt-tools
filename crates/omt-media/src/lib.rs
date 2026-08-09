//! Shared OMT media helpers for the tools suite.

#![deny(missing_docs)]

mod color;
mod discovery;
mod receive;
mod send;
mod stall;
mod stats;

pub use color::{bgra_to_rgba, bgra_alpha_mask, bgra_over_checkerboard, rgb_to_uyvy_pixel, uyvy_from_rgb_frame};
pub use discovery::{DiscoveredSource, SourceBrowser};
pub use receive::{LatestVideo, ReceiveWorker, VideoFrame};
pub use send::{AudioToneConfig, SendSession, SendSessionConfig, SendStats};
pub use stall::{StallDetector, StallState};
pub use stats::FpsCounter;

pub use openmediatransport::{
    Codec, ColorSpace, Discovery, FrameType, MediaFrame, PreferredVideoFormat, Receiver, Sender,
    SenderConfig, SenderInfo,
};
pub use vmx::{Codec as VmxCodec, Config as VmxConfig, Profile as VmxProfile};
