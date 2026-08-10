//! OMT send session with Tokio-paced video + OS-thread tone audio.

use std::f32::consts::TAU;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread;
use std::time::{Duration, Instant};

use openmediatransport::{
    Codec, ColorSpace, Discovery, FrameType, MediaFrame, NETWORK_ASYNC_COUNT, NETWORK_SEND_BUFFER,
    NETWORK_SEND_RECEIVE_BUFFER, OmtError, Sender, SenderConfig, SenderInfo,
};
use parking_lot::Mutex;
use vmx::{Codec as VmxCodec, Config as VmxConfig, Profile};

use crate::runtime;

const TICKS_PER_SECOND: i64 = 10_000_000;

/// Audio tone configuration.
#[derive(Debug, Clone)]
pub struct AudioToneConfig {
    /// Sample rate (Hz).
    pub sample_rate: i32,
    /// Channel count.
    pub channels: i32,
    /// Tone frequency (Hz).
    pub tone_hz: f32,
    /// Peak level in dBFS (SMPTE alignment tone default: −20).
    pub level_dbfs: f32,
    /// Samples per packet (default 480 = 10 ms @ 48 kHz).
    pub samples: i32,
}

impl Default for AudioToneConfig {
    fn default() -> Self {
        Self {
            sample_rate: 48_000,
            channels: 2,
            tone_hz: 1000.0,
            level_dbfs: -20.0,
            samples: 480,
        }
    }
}

/// Configuration for an outbound test / capture session.
#[derive(Debug, Clone)]
pub struct SendSessionConfig {
    /// Discoverable source name.
    pub name: String,
    /// Frame width.
    pub width: i32,
    /// Frame height.
    pub height: i32,
    /// Frame rate numerator.
    pub fps_n: i32,
    /// Frame rate denominator.
    pub fps_d: i32,
    /// VMX profile.
    pub profile: Profile,
    /// Whether UYVY content changes every frame.
    pub animate: bool,
    /// Audio tone settings.
    pub audio: AudioToneConfig,
}

impl Default for SendSessionConfig {
    fn default() -> Self {
        Self {
            name: "Test Pattern".into(),
            width: 1920,
            height: 1080,
            fps_n: 30,
            fps_d: 1,
            profile: Profile::OmtSq,
            animate: true,
            audio: AudioToneConfig::default(),
        }
    }
}

/// Live send statistics snapshot.
#[derive(Debug, Clone, Default)]
pub struct SendStats {
    /// Approximate video FPS over the last window.
    pub video_fps: f32,
    /// Average encode time in milliseconds.
    pub encode_ms: f32,
    /// Frames submitted.
    pub frames: i64,
    /// Frames dropped by the sender queue.
    pub dropped: i64,
    /// True when encode cannot sustain target FPS.
    pub behind: bool,
    /// Listening TCP port.
    pub port: u16,
    /// Accepted peer TCP connections (≈ 2 per receiver client).
    pub connections: u32,
    /// Approximate receiver clients (`connections / 2`, rounded up).
    pub clients: u32,
    /// Peers subscribed to video.
    pub video_subscribers: u32,
    /// Peers subscribed to audio.
    pub audio_subscribers: u32,
    /// Cumulative bytes sent.
    pub bytes_sent: i64,
}

type FrameProvider = Arc<dyn Fn(u64) -> Vec<u8> + Send + Sync>;

/// Background OMT sender owning a Tokio video task + OS audio thread.
pub struct SendSession {
    running: Arc<AtomicBool>,
    stats: Arc<Mutex<SendStats>>,
    audio_join: Option<thread::JoinHandle<()>>,
    video_join: Option<tokio::task::JoinHandle<()>>,
}

impl SendSession {
    /// Start sending frames produced by `provider` (returns UYVY bytes per frame index).
    pub fn start(config: SendSessionConfig, provider: FrameProvider) -> Result<Self, OmtError> {
        let sender = Arc::new(Mutex::new(Sender::create_with_config(
            &config.name,
            FrameType::VIDEO | FrameType::AUDIO | FrameType::METADATA,
            SenderConfig {
                send_buffer: NETWORK_SEND_BUFFER,
                recv_buffer: NETWORK_SEND_RECEIVE_BUFFER,
                send_queue_depth: NETWORK_ASYNC_COUNT,
            },
        )?));
        {
            let mut s = sender.lock();
            s.set_sender_info(SenderInfo::new(
                "OMT Tools",
                "MikanseiLaboratory",
                env!("CARGO_PKG_VERSION"),
            ));
        }
        let port = sender.lock().port();
        {
            let name = config.name.clone();
            // DNS-SD register is blocking; keep advertisement alive via leak (same as before).
            runtime::handle()
                .block_on(runtime::spawn_blocking(move || {
                    let mut discovery = Discovery::new()?;
                    discovery.register(&name, port)?;
                    std::mem::forget(discovery);
                    Ok::<(), OmtError>(())
                }))
                .map_err(|e| OmtError::Discovery(e.to_string()))??;
        }

        let running = Arc::new(AtomicBool::new(true));
        let stats = Arc::new(Mutex::new(SendStats {
            port,
            ..Default::default()
        }));
        let epoch = Instant::now();

        let audio_running = Arc::clone(&running);
        let audio_sender = Arc::clone(&sender);
        let audio_cfg = config.audio.clone();
        let audio_join = thread::Builder::new()
            .name("omt-send-audio".into())
            .spawn(move || audio_loop(audio_sender, audio_cfg, audio_running, epoch))?;

        let video_running = Arc::clone(&running);
        let video_sender = Arc::clone(&sender);
        let video_stats = Arc::clone(&stats);
        let video_cfg = config.clone();
        let video_join = runtime::spawn(async move {
            video_loop(
                video_sender,
                video_cfg,
                provider,
                video_running,
                video_stats,
                epoch,
            )
            .await;
        });

        Ok(Self {
            running,
            stats,
            audio_join: Some(audio_join),
            video_join: Some(video_join),
        })
    }

    /// Snapshot of send statistics.
    pub fn stats(&self) -> SendStats {
        self.stats.lock().clone()
    }

    /// Stop tasks / threads.
    pub fn stop(&mut self) {
        self.running.store(false, Ordering::Relaxed);
        if let Some(join) = self.video_join.take() {
            join.abort();
        }
        if let Some(join) = self.audio_join.take() {
            let _ = join.join();
        }
    }
}

impl Drop for SendSession {
    fn drop(&mut self) {
        self.stop();
    }
}

fn audio_loop(
    sender: Arc<Mutex<Sender>>,
    cfg: AudioToneConfig,
    running: Arc<AtomicBool>,
    epoch: Instant,
) {
    let mut audio_buf = Vec::new();
    let mut phase = 0.0f64;
    let samples = cfg.samples.max(1) as i64;
    let rate = cfg.sample_rate.max(1) as i64;
    let mut samples_sent = 0i64;

    while running.load(Ordering::Relaxed) {
        let want = sender.lock().audio_subscribed();
        if !want {
            thread::sleep(Duration::from_millis(10));
            continue;
        }
        let due = Duration::from_secs_f64(samples_sent as f64 / rate as f64);
        let now = epoch.elapsed();
        if due > now {
            thread::sleep(due - now);
        } else if now > due + Duration::from_millis(50) {
            // Snap to the shared wall epoch (same basis as video_loop) so A/V PTS
            // stay on one timeline after a catch-up.
            samples_sent = (now.as_secs_f64() * rate as f64).round() as i64;
            samples_sent -= samples_sent % samples;
        }
        audio_buf.clear();
        if cfg.tone_hz > 0.0 {
            append_sine_planar(
                &mut audio_buf,
                cfg.channels,
                samples as i32,
                cfg.sample_rate,
                cfg.tone_hz,
                cfg.level_dbfs,
                &mut phase,
            );
        } else {
            // Mute: planar silence.
            let ch = cfg.channels.max(1) as usize;
            let n = samples.max(0) as usize;
            audio_buf.resize(ch * n * 4, 0);
        }
        let timestamp = samples_sent.saturating_mul(TICKS_PER_SECOND) / rate;
        samples_sent += samples;
        let frame = MediaFrame {
            frame_type: FrameType::AUDIO,
            timestamp,
            codec: Codec::Fpa1 as i32,
            sample_rate: cfg.sample_rate,
            channels: cfg.channels,
            samples_per_channel: samples as i32,
            active_channels: 0,
            data: std::mem::take(&mut audio_buf),
            ..Default::default()
        };
        let _ = sender.lock().send_audio(frame);
    }
}

async fn video_loop(
    sender: Arc<Mutex<Sender>>,
    cfg: SendSessionConfig,
    provider: FrameProvider,
    running: Arc<AtomicBool>,
    stats: Arc<Mutex<SendStats>>,
    epoch: Instant,
) {
    let Ok(mut vmx) = VmxCodec::new(VmxConfig {
        width: cfg.width,
        height: cfg.height,
        profile: cfg.profile,
        color_space: vmx::ColorSpace::Undefined,
    }) else {
        return;
    };

    let stride = (cfg.width as usize) * 2;
    let mut vmx_buf = vec![0u8; 8 << 20];
    let mut cached: Option<Vec<u8>> = None;
    let mut frame_idx = 0u64;
    let mut last_stats = Instant::now();
    let mut encode_us_acc = 0u64;
    let mut video_sent = 0u64;
    let video_interval =
        (cfg.fps_d as i64).saturating_mul(TICKS_PER_SECOND) / cfg.fps_n.max(1) as i64;
    let target_fps = cfg.fps_n as f64 / cfg.fps_d.max(1) as f64;

    if !cfg.animate {
        let uyvy = provider(0);
        if uyvy.len() == stride * cfg.height as usize
            && vmx.encode_uyvy(&uyvy, stride).is_ok()
            && let Ok(n) = vmx.save_to(&mut vmx_buf)
        {
            cached = Some(vmx_buf[..n].to_vec());
        }
    }

    while running.load(Ordering::Relaxed) {
        let video_ok = tokio::task::block_in_place(|| {
            let mut s = sender.lock();
            let _ = s.poll_accept();
            let _ = s.poll_peer_metadata();
            s.video_subscribed()
        });
        if !video_ok {
            tokio::time::sleep(Duration::from_millis(5)).await;
            continue;
        }

        let target = Duration::from_secs_f64(frame_idx as f64 / target_fps);
        let now = epoch.elapsed();
        let behind = now > target + Duration::from_secs_f64(2.0 / target_fps);
        if target > now {
            tokio::time::sleep(target - now).await;
        } else if behind {
            frame_idx = (now.as_secs_f64() * target_fps).floor() as u64;
        }

        let t0 = Instant::now();
        let payload = if let Some(cached) = cached.as_ref() {
            cached.clone()
        } else {
            let idx = frame_idx;
            let encode_result = tokio::task::block_in_place(|| {
                let uyvy = provider(idx);
                if uyvy.len() != stride * cfg.height as usize {
                    return None;
                }
                if vmx.encode_uyvy(&uyvy, stride).is_err() {
                    return None;
                }
                match vmx.save_to(&mut vmx_buf) {
                    Ok(n) => Some(vmx_buf[..n].to_vec()),
                    Err(_) => None,
                }
            });
            match encode_result {
                Some(p) => p,
                None => {
                    tokio::time::sleep(Duration::from_millis(5)).await;
                    continue;
                }
            }
        };
        encode_us_acc += t0.elapsed().as_micros() as u64;

        let timestamp = (frame_idx as i64).saturating_mul(video_interval);
        let frame = MediaFrame {
            frame_type: FrameType::VIDEO,
            timestamp,
            codec: Codec::Vmx1 as i32,
            width: cfg.width,
            height: cfg.height,
            frame_rate_n: cfg.fps_n,
            frame_rate_d: cfg.fps_d,
            aspect_ratio: cfg.width as f32 / cfg.height.max(1) as f32,
            color_space: ColorSpace::Undefined,
            data: payload,
            ..Default::default()
        };
        if tokio::task::block_in_place(|| sender.lock().send_video(frame)).is_ok() {
            video_sent += 1;
        }
        frame_idx += 1;

        if last_stats.elapsed() >= Duration::from_secs(1) {
            let (st, port, connections, video_subs, audio_subs) =
                tokio::task::block_in_place(|| {
                    let s = sender.lock();
                    (
                        s.statistics(),
                        s.port(),
                        s.connection_count() as u32,
                        s.video_subscriber_count() as u32,
                        s.audio_subscriber_count() as u32,
                    )
                });
            let avg_ms = if video_sent > 0 {
                (encode_us_acc as f64 / video_sent as f64) / 1000.0
            } else {
                0.0
            };
            let mut snap = stats.lock();
            snap.video_fps = video_sent as f32;
            snap.encode_ms = avg_ms as f32;
            snap.frames = st.frames;
            snap.dropped = st.frames_dropped;
            snap.behind = behind || snap.video_fps + 0.5 < target_fps as f32;
            snap.port = port;
            snap.connections = connections;
            snap.clients = connections.div_ceil(2);
            snap.video_subscribers = video_subs;
            snap.audio_subscribers = audio_subs;
            snap.bytes_sent = st.bytes_sent;
            encode_us_acc = 0;
            video_sent = 0;
            last_stats = Instant::now();
        }

        tokio::task::yield_now().await;
    }
}

fn append_sine_planar(
    dst: &mut Vec<u8>,
    channels: i32,
    samples: i32,
    sample_rate: i32,
    tone_hz: f32,
    level_dbfs: f32,
    phase: &mut f64,
) {
    let ch = channels.max(1) as usize;
    let n = samples.max(0) as usize;
    let rate = sample_rate.max(1) as f64;
    let freq = tone_hz as f64;
    // Peak amplitude from dBFS (1.0 == 0 dBFS). Clamp to avoid NaN / runaway gain.
    let amplitude = 10f32.powf(level_dbfs.clamp(-120.0, 0.0) / 20.0);
    dst.reserve(ch * n * 4);
    let start = *phase;
    for _c in 0..ch {
        let mut p = start;
        for _s in 0..n {
            let sample = (TAU as f64 * freq * p / rate).sin() as f32 * amplitude;
            dst.extend_from_slice(&sample.to_le_bytes());
            p += 1.0;
            if p >= rate {
                p -= rate;
            }
        }
    }
    *phase = start + n as f64;
    if *phase >= rate {
        *phase %= rate;
    }
}
