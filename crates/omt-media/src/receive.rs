//! Background OMT video receive worker.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread;
use std::time::Duration;

use openmediatransport::{
    Codec, FrameType, OmtError, PreferredVideoFormat, Receiver, Statistics,
};
use parking_lot::Mutex;

use crate::stall::StallDetector;

/// Decoded video frame ready for UI upload.
#[derive(Debug, Clone)]
pub struct VideoFrame {
    /// Pixel width.
    pub width: u32,
    /// Pixel height.
    pub height: u32,
    /// BGRA8 tightly packed pixels.
    pub bgra: Vec<u8>,
    /// OMT timestamp (100 ns ticks).
    pub timestamp: i64,
    /// Declared frame rate numerator.
    pub fps_n: i32,
    /// Declared frame rate denominator.
    pub fps_d: i32,
}

/// Shared latest-frame slot + receive stats.
#[derive(Debug, Default)]
pub struct LatestVideo {
    /// Newest decoded frame (replaced, never queued).
    pub frame: Mutex<Option<VideoFrame>>,
    /// Last receiver statistics snapshot.
    pub stats: Mutex<Statistics>,
    /// Last error message, if any.
    pub error: Mutex<Option<String>>,
    /// Connected URL.
    pub url: Mutex<Option<String>>,
}

impl LatestVideo {
    /// Take the newest frame if present.
    pub fn take(&self) -> Option<VideoFrame> {
        self.frame.lock().take()
    }

    /// Peek without removing.
    pub fn peek(&self) -> Option<VideoFrame> {
        self.frame.lock().clone()
    }
}

/// Commands for the receive worker.
#[derive(Debug, Clone)]
pub enum ReceiveCommand {
    /// Connect / switch to a source URL.
    Connect(String),
    /// Disconnect and idle.
    Disconnect,
    /// Stop the worker thread.
    Shutdown,
}

/// Background thread that owns an OMT [`Receiver`].
pub struct ReceiveWorker {
    tx: std::sync::mpsc::Sender<ReceiveCommand>,
    latest: Arc<LatestVideo>,
    stall: Arc<Mutex<StallDetector>>,
    join: Option<thread::JoinHandle<()>>,
}

impl ReceiveWorker {
    /// Spawn a worker thread.
    pub fn spawn() -> Self {
        let (tx, rx) = std::sync::mpsc::channel();
        let latest = Arc::new(LatestVideo::default());
        let stall = Arc::new(Mutex::new(StallDetector::default()));
        let latest_c = Arc::clone(&latest);
        let stall_c = Arc::clone(&stall);
        let running = Arc::new(AtomicBool::new(true));
        let running_c = Arc::clone(&running);

        let join = thread::Builder::new()
            .name("omt-receive".into())
            .spawn(move || {
                worker_loop(rx, latest_c, stall_c, running_c);
            })
            .expect("spawn receive worker");

        Self {
            tx,
            latest,
            stall,
            join: Some(join),
        }
    }

    /// Shared latest frame slot.
    pub fn latest(&self) -> Arc<LatestVideo> {
        Arc::clone(&self.latest)
    }

    /// Shared stall detector.
    pub fn stall(&self) -> Arc<Mutex<StallDetector>> {
        Arc::clone(&self.stall)
    }

    /// Ask the worker to connect to `url`.
    pub fn connect(&self, url: impl Into<String>) {
        let _ = self.tx.send(ReceiveCommand::Connect(url.into()));
    }

    /// Disconnect.
    pub fn disconnect(&self) {
        let _ = self.tx.send(ReceiveCommand::Disconnect);
    }

    /// Stop the worker.
    pub fn shutdown(&self) {
        let _ = self.tx.send(ReceiveCommand::Shutdown);
    }
}

impl Drop for ReceiveWorker {
    fn drop(&mut self) {
        let _ = self.tx.send(ReceiveCommand::Shutdown);
        if let Some(join) = self.join.take() {
            let _ = join.join();
        }
    }
}

fn worker_loop(
    rx: std::sync::mpsc::Receiver<ReceiveCommand>,
    latest: Arc<LatestVideo>,
    stall: Arc<Mutex<StallDetector>>,
    running: Arc<AtomicBool>,
) {
    let mut receiver: Option<Receiver> = None;

    while running.load(Ordering::Relaxed) {
        while let Ok(cmd) = rx.try_recv() {
            match cmd {
                ReceiveCommand::Connect(url) => {
                    *latest.error.lock() = None;
                    *latest.url.lock() = Some(url.clone());
                    match open_receiver(&url) {
                        Ok(r) => {
                            receiver = Some(r);
                            stall.lock().reset();
                        }
                        Err(e) => {
                            receiver = None;
                            *latest.error.lock() = Some(e.to_string());
                        }
                    }
                }
                ReceiveCommand::Disconnect => {
                    receiver = None;
                    *latest.url.lock() = None;
                    *latest.frame.lock() = None;
                    stall.lock().reset();
                }
                ReceiveCommand::Shutdown => {
                    running.store(false, Ordering::Relaxed);
                    return;
                }
            }
        }

        let Some(recv) = receiver.as_mut() else {
            thread::sleep(Duration::from_millis(50));
            continue;
        };

        match recv.receive(100) {
            Ok(Some(frame)) if frame.frame_type.contains(FrameType::VIDEO) => {
                let m = &frame.media;
                let codec = Codec::from_i32(m.codec);
                let expected = (m.width as usize) * 4 * (m.height as usize);
                if codec == Some(Codec::Bgra) && frame.data.len() == expected && m.width > 0 {
                    let video = VideoFrame {
                        width: m.width as u32,
                        height: m.height as u32,
                        bgra: frame.data,
                        timestamp: frame.timestamp,
                        fps_n: m.frame_rate_n,
                        fps_d: m.frame_rate_d.max(1),
                    };
                    stall.lock().on_frame(video.fps_n, video.fps_d);
                    *latest.frame.lock() = Some(video);
                    *latest.stats.lock() = recv.statistics();
                }
            }
            Ok(Some(_)) => {}
            Ok(None) => {
                stall.lock().tick();
            }
            Err(e) => {
                *latest.error.lock() = Some(e.to_string());
                receiver = None;
            }
        }
    }
}

fn open_receiver(url: &str) -> Result<Receiver, OmtError> {
    let mut receiver = Receiver::create(url, FrameType::VIDEO)?;
    receiver.set_preferred_format(PreferredVideoFormat::Bgra);
    receiver.connect(Some(Duration::from_secs(5)))?;
    Ok(receiver)
}
