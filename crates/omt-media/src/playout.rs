//! PTS-based A/V playout with linked or independent delay buffers.

use std::collections::VecDeque;
use std::time::Instant;

use crate::audio_out::AudioOutput;
use crate::receive::{LatestVideo, VideoFrame};

const TICKS_PER_SECOND: f64 = 10_000_000.0;
const TICKS_PER_MS: i64 = 10_000;
const VIDEO_Q_CAP: usize = 90;
const AUDIO_Q_CAP: usize = 300;

/// Whether buffer depth is measured in milliseconds or video frames.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum BufferUnit {
    /// Wall-clock milliseconds of pre-roll.
    #[default]
    Milliseconds,
    /// Source frame intervals (uses last known FPS, default 30).
    Frames,
}

/// One stream's delay setting (amount + unit).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DelaySetting {
    /// Magnitude in [`BufferUnit`] units.
    pub amount: u32,
    /// Unit for [`amount`](Self::amount).
    pub unit: BufferUnit,
}

impl DelaySetting {
    /// Convert to a clamped delay in milliseconds.
    pub fn delay_ms(self, fps_n: i32, fps_d: i32) -> u32 {
        let ms = match self.unit {
            BufferUnit::Milliseconds => self.amount as f64,
            BufferUnit::Frames => {
                let fps = fps_n.max(1) as f64 / fps_d.max(1) as f64;
                self.amount as f64 * 1000.0 / fps.max(1.0)
            }
        };
        ms.round().clamp(0.0, 2_000.0) as u32
    }

    /// Build a setting in `unit` that matches approximately `ms` at the given FPS.
    pub fn from_ms(ms: u32, unit: BufferUnit, fps_n: i32, fps_d: i32) -> Self {
        let ms = (ms as f64).clamp(0.0, 2_000.0);
        match unit {
            BufferUnit::Milliseconds => Self {
                amount: ms.round() as u32,
                unit,
            },
            BufferUnit::Frames => {
                let fps = fps_n.max(1) as f64 / fps_d.max(1) as f64;
                let frames = (ms * fps.max(1.0) / 1000.0).round().clamp(0.0, 120.0) as u32;
                Self {
                    amount: frames,
                    unit,
                }
            }
        }
    }
}

/// User-facing A/V buffer depth (linked or independent).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BufferSettings {
    /// When true, video/audio delays stay matched via the current frame rate.
    pub linked: bool,
    /// Video playout delay.
    pub video: DelaySetting,
    /// Audio playout delay.
    pub audio: DelaySetting,
}

impl Default for BufferSettings {
    fn default() -> Self {
        // 3 frames @ 30 fps ≈ 100 ms — linked by default.
        Self {
            linked: true,
            video: DelaySetting {
                amount: 3,
                unit: BufferUnit::Frames,
            },
            audio: DelaySetting {
                amount: 100,
                unit: BufferUnit::Milliseconds,
            },
        }
    }
}

impl BufferSettings {
    /// Effective video delay in milliseconds.
    pub fn video_delay_ms(self, fps_n: i32, fps_d: i32) -> u32 {
        self.video.delay_ms(fps_n, fps_d)
    }

    /// Effective audio delay in milliseconds.
    pub fn audio_delay_ms(self, fps_n: i32, fps_d: i32) -> u32 {
        if self.linked {
            self.video.delay_ms(fps_n, fps_d)
        } else {
            self.audio.delay_ms(fps_n, fps_d)
        }
    }

    /// Update video delay; when linked, refresh audio to the rounded equivalent.
    pub fn set_video(&mut self, video: DelaySetting, fps_n: i32, fps_d: i32) {
        self.video = video;
        if self.linked {
            let ms = video.delay_ms(fps_n, fps_d);
            self.audio = DelaySetting::from_ms(ms, self.audio.unit, fps_n, fps_d);
        }
    }

    /// Update audio delay; when linked, refresh video to the rounded equivalent.
    pub fn set_audio(&mut self, audio: DelaySetting, fps_n: i32, fps_d: i32) {
        self.audio = audio;
        if self.linked {
            let ms = audio.delay_ms(fps_n, fps_d);
            self.video = DelaySetting::from_ms(ms, self.video.unit, fps_n, fps_d);
        }
    }

    /// Enable/disable link. Turning link on snaps audio to video.
    pub fn set_linked(&mut self, linked: bool, fps_n: i32, fps_d: i32) {
        self.linked = linked;
        if linked {
            let ms = self.video.delay_ms(fps_n, fps_d);
            self.audio = DelaySetting::from_ms(ms, self.audio.unit, fps_n, fps_d);
        }
    }

    /// Keep the linked pair consistent after FPS changes (video is master).
    pub fn resync_linked(&mut self, fps_n: i32, fps_d: i32) {
        if self.linked {
            let ms = self.video.delay_ms(fps_n, fps_d);
            self.audio = DelaySetting::from_ms(ms, self.audio.unit, fps_n, fps_d);
        }
    }
}

struct PendingAudio {
    timestamp: i64,
    data: Vec<u8>,
    channels: i32,
    samples: i32,
    sample_rate: i32,
}

/// Shared media-clock gate for video + audio packets.
pub struct Playout {
    settings: BufferSettings,
    pts_origin: Option<i64>,
    wall_origin: Option<Instant>,
    fps_n: i32,
    fps_d: i32,
    video_q: VecDeque<VideoFrame>,
    audio_q: VecDeque<PendingAudio>,
}

impl Default for Playout {
    fn default() -> Self {
        Self {
            settings: BufferSettings::default(),
            pts_origin: None,
            wall_origin: None,
            fps_n: 30,
            fps_d: 1,
            video_q: VecDeque::new(),
            audio_q: VecDeque::new(),
        }
    }
}

impl Playout {
    /// Replace buffer settings (clock continues; depth changes immediately).
    pub fn set_settings(&mut self, settings: BufferSettings) {
        self.settings = settings;
    }

    /// Effective video delay in milliseconds at the current FPS.
    pub fn video_delay_ms(&self) -> u32 {
        self.settings.video_delay_ms(self.fps_n, self.fps_d)
    }

    /// Effective audio delay in milliseconds at the current FPS.
    pub fn audio_delay_ms(&self) -> u32 {
        self.settings.audio_delay_ms(self.fps_n, self.fps_d)
    }

    /// Clear queues and clock (disconnect / reconnect).
    pub fn reset(&mut self) {
        self.pts_origin = None;
        self.wall_origin = None;
        self.video_q.clear();
        self.audio_q.clear();
    }

    /// Enqueue a decoded video frame.
    pub fn push_video(&mut self, frame: VideoFrame) {
        if frame.fps_n > 0 {
            let changed = self.fps_n != frame.fps_n || self.fps_d != frame.fps_d.max(1);
            self.fps_n = frame.fps_n;
            self.fps_d = frame.fps_d.max(1);
            if changed {
                self.settings.resync_linked(self.fps_n, self.fps_d);
            }
        }
        self.note_clock(frame.timestamp);
        self.video_q.push_back(frame);
        while self.video_q.len() > VIDEO_Q_CAP {
            self.video_q.pop_front();
        }
    }

    /// Enqueue a decoded audio packet (planar f32 bytes).
    pub fn push_audio(
        &mut self,
        timestamp: i64,
        data: Vec<u8>,
        channels: i32,
        samples: i32,
        sample_rate: i32,
    ) {
        self.note_clock(timestamp);
        self.audio_q.push_back(PendingAudio {
            timestamp,
            data,
            channels,
            samples,
            sample_rate,
        });
        while self.audio_q.len() > AUDIO_Q_CAP {
            self.audio_q.pop_front();
        }
    }

    /// Release packets whose PTS is due on the (possibly split) media clock.
    pub fn release(&mut self, latest: &LatestVideo, audio: &AudioOutput) {
        let Some(audio_mt) = self.media_time(self.audio_delay_ms()) else {
            return;
        };
        let Some(video_mt) = self.media_time(self.video_delay_ms()) else {
            return;
        };

        while self
            .audio_q
            .front()
            .is_some_and(|p| p.timestamp <= audio_mt)
        {
            let Some(packet) = self.audio_q.pop_front() else {
                break;
            };
            audio.push_planar_f32(
                &packet.data,
                packet.channels,
                packet.samples,
                packet.sample_rate,
            );
            let levels = audio.levels();
            *latest.audio_levels.lock() = levels;
            let mut counters = latest.counters.lock();
            counters.audio_frames = levels.frames;
        }

        let mut due: Option<VideoFrame> = None;
        let mut replaced = 0u64;
        while self
            .video_q
            .front()
            .is_some_and(|f| f.timestamp <= video_mt)
        {
            if due.is_some() {
                replaced += 1;
            }
            due = self.video_q.pop_front();
        }
        if let Some(video) = due {
            {
                let mut slot = latest.frame.lock();
                let mut counters = latest.counters.lock();
                if slot.is_some() {
                    counters.frames_replaced = counters.frames_replaced.saturating_add(1);
                }
                counters.frames_replaced = counters.frames_replaced.saturating_add(replaced);
                *slot = Some(video);
                counters.frames_decoded = counters.frames_decoded.saturating_add(1);
            }
        }

        // If we are hopelessly behind, snap the clock forward to the oldest queued PTS.
        self.maybe_resnap();
    }

    fn note_clock(&mut self, pts: i64) {
        if self.pts_origin.is_none() {
            self.pts_origin = Some(pts);
            self.wall_origin = Some(Instant::now());
        }
    }

    fn media_time(&self, delay_ms: u32) -> Option<i64> {
        let pts0 = self.pts_origin?;
        let wall0 = self.wall_origin?;
        let elapsed_ticks = (wall0.elapsed().as_secs_f64() * TICKS_PER_SECOND).round() as i64;
        let buffer_ticks = i64::from(delay_ms) * TICKS_PER_MS;
        Some(pts0 + elapsed_ticks - buffer_ticks)
    }

    fn maybe_resnap(&mut self) {
        let delay = self.video_delay_ms().max(self.audio_delay_ms());
        let Some(media_time) = self.media_time(delay) else {
            return;
        };
        let oldest = match (self.video_q.front(), self.audio_q.front()) {
            (Some(v), Some(a)) => Some(v.timestamp.min(a.timestamp)),
            (Some(v), None) => Some(v.timestamp),
            (None, Some(a)) => Some(a.timestamp),
            (None, None) => None,
        };
        let Some(oldest) = oldest else {
            return;
        };
        // More than 750 ms late relative to the playout clock → resync.
        if oldest + 7_500_000 < media_time {
            self.pts_origin = Some(oldest);
            self.wall_origin = Some(Instant::now());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn frames_convert_to_ms() {
        let s = DelaySetting {
            amount: 3,
            unit: BufferUnit::Frames,
        };
        assert_eq!(s.delay_ms(30, 1), 100);
        assert_eq!(s.delay_ms(60, 1), 50);
    }

    #[test]
    fn linked_set_video_updates_audio() {
        let mut s = BufferSettings::default();
        s.set_video(
            DelaySetting {
                amount: 6,
                unit: BufferUnit::Frames,
            },
            30,
            1,
        );
        assert_eq!(s.audio.amount, 200);
        assert_eq!(s.audio.unit, BufferUnit::Milliseconds);
    }

    #[test]
    fn unlinked_keeps_independent_delays() {
        let mut s = BufferSettings::default();
        s.set_linked(false, 30, 1);
        s.set_video(
            DelaySetting {
                amount: 5,
                unit: BufferUnit::Frames,
            },
            30,
            1,
        );
        s.set_audio(
            DelaySetting {
                amount: 0,
                unit: BufferUnit::Milliseconds,
            },
            30,
            1,
        );
        assert_eq!(s.video_delay_ms(30, 1), 167);
        assert_eq!(s.audio_delay_ms(30, 1), 0);
    }

    #[test]
    fn default_buffer_is_linked_100ms() {
        let s = BufferSettings::default();
        assert!(s.linked);
        assert_eq!(s.video_delay_ms(30, 1), 100);
        assert_eq!(s.audio_delay_ms(30, 1), 100);
    }
}
