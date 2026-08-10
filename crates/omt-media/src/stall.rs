//! Video stall / signal-loss detection.

use std::time::{Duration, Instant};

/// Stall detector state.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StallState {
    /// No frames have arrived yet.
    Waiting,
    /// Frames are arriving within the deadline.
    Live,
    /// Frames have stopped arriving.
    Stalled,
}

/// Detects when video stops arriving.
#[derive(Debug, Clone)]
pub struct StallDetector {
    last_frame: Option<Instant>,
    fps_n: i32,
    fps_d: i32,
    /// Absolute maximum silence before stall, regardless of FPS.
    absolute_timeout: Duration,
    /// Multiplier over the nominal frame interval.
    frame_timeout_multiplier: f32,
    state: StallState,
}

impl Default for StallDetector {
    fn default() -> Self {
        Self {
            last_frame: None,
            fps_n: 30,
            fps_d: 1,
            absolute_timeout: Duration::from_secs(2),
            frame_timeout_multiplier: 3.0,
            state: StallState::Waiting,
        }
    }
}

impl StallDetector {
    /// Reset to waiting.
    pub fn reset(&mut self) {
        self.last_frame = None;
        self.state = StallState::Waiting;
    }

    /// Record a newly received frame.
    pub fn on_frame(&mut self, fps_n: i32, fps_d: i32) {
        if fps_n > 0 {
            self.fps_n = fps_n;
            self.fps_d = fps_d.max(1);
        }
        self.last_frame = Some(Instant::now());
        self.state = StallState::Live;
    }

    /// Re-evaluate stall state without a new frame.
    pub fn tick(&mut self) -> StallState {
        let Some(last) = self.last_frame else {
            self.state = StallState::Waiting;
            return self.state;
        };
        let elapsed = last.elapsed();
        let frame_interval = Duration::from_secs_f64(self.fps_d as f64 / self.fps_n.max(1) as f64);
        let dynamic = frame_interval.mul_f32(self.frame_timeout_multiplier);
        let deadline = dynamic
            .max(Duration::from_millis(200))
            .min(self.absolute_timeout);
        if elapsed > deadline || elapsed > self.absolute_timeout {
            self.state = StallState::Stalled;
        } else {
            self.state = StallState::Live;
        }
        self.state
    }

    /// Current state.
    pub fn state(&self) -> StallState {
        self.state
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::thread;

    #[test]
    fn detects_stall_after_silence() {
        let mut d = StallDetector {
            absolute_timeout: Duration::from_millis(50),
            frame_timeout_multiplier: 1.0,
            ..Default::default()
        };
        d.on_frame(60, 1);
        assert_eq!(d.state(), StallState::Live);
        thread::sleep(Duration::from_millis(80));
        assert_eq!(d.tick(), StallState::Stalled);
    }
}
