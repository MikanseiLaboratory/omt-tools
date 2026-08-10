//! Lightweight FPS counter.

use std::time::{Duration, Instant};

/// Sliding one-second FPS estimate.
#[derive(Debug, Clone)]
pub struct FpsCounter {
    window_start: Instant,
    count: u32,
    last_fps: f32,
}

impl Default for FpsCounter {
    fn default() -> Self {
        Self {
            window_start: Instant::now(),
            count: 0,
            last_fps: 0.0,
        }
    }
}

impl FpsCounter {
    /// Record one displayed / sent frame.
    pub fn tick(&mut self) -> f32 {
        self.count += 1;
        let elapsed = self.window_start.elapsed();
        if elapsed >= Duration::from_secs(1) {
            self.last_fps = self.count as f32 / elapsed.as_secs_f32();
            self.count = 0;
            self.window_start = Instant::now();
        }
        self.last_fps
    }

    /// Last completed window FPS.
    pub fn fps(&self) -> f32 {
        self.last_fps
    }
}
