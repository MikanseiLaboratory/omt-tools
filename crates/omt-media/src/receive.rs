//! Background OMT A/V receive worker (Tokio).

use std::collections::VecDeque;
use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use openmediatransport::{
    Codec, FrameType, OmtError, PreferredVideoFormat, Quality, ReceiveFlags, Receiver, Statistics,
};
use parking_lot::Mutex;
use tokio::sync::mpsc;

use crate::audio_out::{AudioLevels, AudioOutput};
use crate::playout::{BufferSettings, Playout};
use crate::runtime;
use crate::stall::StallDetector;

const METADATA_LOG_CAP: usize = 256;

/// Connection / quality options for an inbound receive session.
#[derive(Debug, Clone)]
pub struct ConnectOptions {
    /// Source URL (`omt://…`).
    pub url: String,
    /// Suggested encode quality sent to the peer.
    pub quality: Quality,
    /// Request preview / low-bandwidth path when true.
    pub low_bandwidth: bool,
}

impl ConnectOptions {
    /// Connect with default quality settings.
    pub fn new(url: impl Into<String>) -> Self {
        Self {
            url: url.into(),
            quality: Quality::Default,
            low_bandwidth: false,
        }
    }
}

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

/// One metadata / diagnostic log line from the receive path.
#[derive(Debug, Clone)]
pub struct MetadataLogEntry {
    /// Unix time in milliseconds.
    pub unix_ms: u64,
    /// Short kind label (`metadata`, `frame-meta`, `info`, …).
    pub kind: String,
    /// XML or text payload.
    pub text: String,
}

/// App-level receive counters (latest-wins semantics).
#[derive(Debug, Clone, Copy, Default)]
pub struct ReceiveCounters {
    /// Decoded video frames written into the latest slot.
    pub frames_decoded: u64,
    /// Frames overwritten before the UI took them (source / queue drops).
    pub frames_replaced: u64,
    /// Metadata / XML events received.
    pub metadata_events: u64,
    /// Decoded audio packets played / metered.
    pub audio_frames: u64,
}

/// Shared latest-frame slot + receive stats.
#[derive(Debug, Default)]
pub struct LatestVideo {
    /// Newest decoded frame (replaced, never queued).
    pub frame: Mutex<Option<VideoFrame>>,
    /// Last receiver statistics snapshot.
    pub stats: Mutex<Statistics>,
    /// App-level counters.
    pub counters: Mutex<ReceiveCounters>,
    /// Latest audio peak levels for VU.
    pub audio_levels: Mutex<AudioLevels>,
    /// Effective video buffer delay currently applied (milliseconds).
    pub video_buffer_delay_ms: Mutex<u32>,
    /// Effective audio buffer delay currently applied (milliseconds).
    pub audio_buffer_delay_ms: Mutex<u32>,
    /// Rolling metadata / XML log (oldest front).
    pub metadata_log: Mutex<VecDeque<MetadataLogEntry>>,
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

    /// Drain new metadata log entries after `after_unix_ms` (exclusive).
    pub fn metadata_since(&self, after_unix_ms: u64) -> Vec<MetadataLogEntry> {
        self.metadata_log
            .lock()
            .iter()
            .filter(|e| e.unix_ms > after_unix_ms)
            .cloned()
            .collect()
    }
}

/// Commands for the receive worker.
#[derive(Debug, Clone)]
pub enum ReceiveCommand {
    /// Connect / switch to a source URL (with quality options).
    Connect(ConnectOptions),
    /// Disconnect and idle.
    Disconnect,
    /// Update A/V playout buffer depth.
    SetBuffer(BufferSettings),
    /// Stop the worker task.
    Shutdown,
}

/// Background Tokio task that owns an OMT [`Receiver`].
pub struct ReceiveWorker {
    tx: mpsc::UnboundedSender<ReceiveCommand>,
    latest: Arc<LatestVideo>,
    stall: Arc<Mutex<StallDetector>>,
    audio: Arc<AudioOutput>,
    join: Option<tokio::task::JoinHandle<()>>,
}

impl ReceiveWorker {
    /// Spawn a worker on the shared media runtime.
    pub fn spawn() -> Self {
        let (tx, rx) = mpsc::unbounded_channel();
        let latest = Arc::new(LatestVideo::default());
        let stall = Arc::new(Mutex::new(StallDetector::default()));
        let audio = Arc::new(AudioOutput::new());
        let latest_c = Arc::clone(&latest);
        let stall_c = Arc::clone(&stall);
        let audio_c = Arc::clone(&audio);

        let join = runtime::spawn(async move {
            worker_loop(rx, latest_c, stall_c, audio_c).await;
        });

        Self {
            tx,
            latest,
            stall,
            audio,
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

    /// Shared audio output (levels / boost).
    pub fn audio(&self) -> Arc<AudioOutput> {
        Arc::clone(&self.audio)
    }

    /// Set playback boost in dB.
    pub fn set_audio_boost_db(&self, db: i32) {
        self.audio.set_boost_db(db);
    }

    /// Select system audio output (`None` = default device).
    pub fn set_audio_output_device(&self, name: Option<String>) {
        self.audio.set_output_device(name);
    }

    /// Currently selected audio output device name (`None` = default).
    pub fn audio_output_device(&self) -> Option<String> {
        self.audio.selected_device_name()
    }

    /// Set A/V playout buffer (milliseconds or frames).
    pub fn set_buffer(&self, settings: BufferSettings) {
        let _ = self.tx.send(ReceiveCommand::SetBuffer(settings));
    }

    /// Ask the worker to connect to `url` with default quality.
    pub fn connect(&self, url: impl Into<String>) {
        let _ = self
            .tx
            .send(ReceiveCommand::Connect(ConnectOptions::new(url)));
    }

    /// Connect with explicit quality / bandwidth options.
    pub fn connect_with(&self, options: ConnectOptions) {
        let _ = self.tx.send(ReceiveCommand::Connect(options));
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
            join.abort();
        }
    }
}

fn now_unix_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

fn push_log(latest: &LatestVideo, kind: &str, text: impl Into<String>) {
    let mut q = latest.metadata_log.lock();
    if q.len() >= METADATA_LOG_CAP {
        q.pop_front();
    }
    q.push_back(MetadataLogEntry {
        unix_ms: now_unix_ms(),
        kind: kind.to_string(),
        text: text.into(),
    });
    drop(q);
    {
        let mut c = latest.counters.lock();
        c.metadata_events = c.metadata_events.saturating_add(1);
    }
}

async fn worker_loop(
    mut rx: mpsc::UnboundedReceiver<ReceiveCommand>,
    latest: Arc<LatestVideo>,
    stall: Arc<Mutex<StallDetector>>,
    audio: Arc<AudioOutput>,
) {
    let mut receiver: Option<Receiver> = None;
    let mut playout = Playout::default();
    publish_buffer_delays(&latest, &playout);

    loop {
        while let Ok(cmd) = rx.try_recv() {
            match cmd {
                ReceiveCommand::Connect(opts) => {
                    apply_connect(opts, &mut receiver, &latest, &stall, &audio, &mut playout)
                        .await;
                }
                ReceiveCommand::Disconnect => {
                    apply_disconnect(&mut receiver, &latest, &stall, &audio, &mut playout);
                }
                ReceiveCommand::SetBuffer(settings) => {
                    playout.set_settings(settings);
                    publish_buffer_delays(&latest, &playout);
                }
                ReceiveCommand::Shutdown => return,
            }
        }

        let Some(recv) = receiver.as_mut() else {
            match rx.recv().await {
                Some(ReceiveCommand::Connect(opts)) => {
                    apply_connect(opts, &mut receiver, &latest, &stall, &audio, &mut playout)
                        .await;
                }
                Some(ReceiveCommand::Disconnect) => {
                    apply_disconnect(&mut receiver, &latest, &stall, &audio, &mut playout);
                }
                Some(ReceiveCommand::SetBuffer(settings)) => {
                    playout.set_settings(settings);
                    publish_buffer_delays(&latest, &playout);
                }
                Some(ReceiveCommand::Shutdown) | None => return,
            }
            continue;
        };

        let result = tokio::task::block_in_place(|| recv.receive(5));
        match result {
            Ok(Some(frame)) if frame.frame_type.contains(FrameType::VIDEO) => {
                if let Some(meta) = frame.metadata.as_ref().filter(|s| !s.is_empty()) {
                    push_log(&latest, "frame-meta", meta.clone());
                }
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
                    playout.push_video(video);
                    *latest.stats.lock() = recv.statistics();
                }
            }
            Ok(Some(frame)) if frame.frame_type.contains(FrameType::AUDIO) => {
                let m = &frame.media;
                playout.push_audio(
                    frame.timestamp,
                    frame.data,
                    m.channels,
                    m.samples_per_channel,
                    m.sample_rate,
                );
                *latest.stats.lock() = recv.statistics();
            }
            Ok(Some(frame)) if frame.frame_type.contains(FrameType::METADATA) => {
                let text = frame
                    .metadata
                    .clone()
                    .or_else(|| String::from_utf8(frame.data.clone()).ok())
                    .unwrap_or_else(|| format!("<binary metadata {} bytes>", frame.data.len()));
                push_log(&latest, "metadata", text);
                *latest.stats.lock() = recv.statistics();
            }
            Ok(Some(_)) => {
                *latest.stats.lock() = recv.statistics();
            }
            Ok(None) => {
                stall.lock().tick();
            }
            Err(e) => {
                *latest.error.lock() = Some(e.to_string());
                push_log(&latest, "error", e.to_string());
                audio.clear();
                playout.reset();
                receiver = None;
            }
        }

        playout.release(&latest, &audio);
        publish_buffer_delays(&latest, &playout);
    }
}

fn publish_buffer_delays(latest: &LatestVideo, playout: &Playout) {
    *latest.video_buffer_delay_ms.lock() = playout.video_delay_ms();
    *latest.audio_buffer_delay_ms.lock() = playout.audio_delay_ms();
}

async fn apply_connect(
    opts: ConnectOptions,
    receiver: &mut Option<Receiver>,
    latest: &LatestVideo,
    stall: &Mutex<StallDetector>,
    audio: &AudioOutput,
    playout: &mut Playout,
) {
    *latest.error.lock() = None;
    *latest.url.lock() = Some(opts.url.clone());
    *latest.counters.lock() = ReceiveCounters::default();
    *latest.audio_levels.lock() = AudioLevels::default();
    latest.metadata_log.lock().clear();
    audio.clear();
    playout.reset();
    publish_buffer_delays(latest, playout);
    match open_receiver(opts).await {
        Ok(r) => {
            *receiver = Some(r);
            stall.lock().reset();
            push_log(latest, "info", format!("connected {}", latest.url.lock().as_ref().unwrap()));
        }
        Err(e) => {
            *receiver = None;
            *latest.error.lock() = Some(e.to_string());
            push_log(latest, "error", e.to_string());
        }
    }
}

fn apply_disconnect(
    receiver: &mut Option<Receiver>,
    latest: &LatestVideo,
    stall: &Mutex<StallDetector>,
    audio: &AudioOutput,
    playout: &mut Playout,
) {
    *receiver = None;
    *latest.url.lock() = None;
    *latest.frame.lock() = None;
    *latest.audio_levels.lock() = AudioLevels::default();
    audio.clear();
    playout.reset();
    stall.lock().reset();
    push_log(latest, "info", "disconnected");
}

async fn open_receiver(opts: ConnectOptions) -> Result<Receiver, OmtError> {
    let url = opts.url.clone();
    let quality = opts.quality;
    let low_bandwidth = opts.low_bandwidth;
    tokio::task::spawn_blocking(move || {
        let mut receiver = Receiver::create(
            &url,
            FrameType::VIDEO | FrameType::AUDIO | FrameType::METADATA,
        )?;
        receiver.set_preferred_format(PreferredVideoFormat::Bgra);
        receiver.set_suggested_quality(quality);
        receiver.set_flags(if low_bandwidth {
            ReceiveFlags::PREVIEW
        } else {
            ReceiveFlags::NONE
        });
        receiver.connect(Some(Duration::from_secs(5)))?;
        Ok(receiver)
    })
    .await
    .map_err(|e| OmtError::Network(e.to_string()))?
}
