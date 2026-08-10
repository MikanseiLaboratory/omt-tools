//! System audio playback for received OMT PCM (cpal).

use std::collections::VecDeque;
use std::sync::atomic::{AtomicI32, Ordering};
use std::sync::mpsc::{self, Sender};
use std::sync::Arc;
use std::thread;

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{SampleFormat, StreamConfig};
use parking_lot::Mutex;
use tracing::warn;

const RING_CAP_SAMPLES: usize = 48_000 * 2 * 2; // ~2s stereo @ 48 kHz

/// Peak levels for VU display (linear 0..1, per channel).
#[derive(Debug, Clone, Copy, Default)]
pub struct AudioLevels {
    /// Left (or mono) peak.
    pub peak_l: f32,
    /// Right peak (0 when mono).
    pub peak_r: f32,
    /// Audio packets pushed into the player.
    pub frames: u64,
    /// Declared sample rate of the last packet.
    pub sample_rate: i32,
    /// Declared channel count of the last packet.
    pub channels: i32,
}

/// A selectable system audio output device.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AudioOutputDevice {
    /// Device name (stable enough for selection within a session).
    pub name: String,
    /// Whether this is the current host default output.
    pub is_default: bool,
}

/// List available cpal output devices (best-effort).
pub fn list_output_devices() -> Vec<AudioOutputDevice> {
    let host = cpal::default_host();
    let default_name = host
        .default_output_device()
        .and_then(|d| d.name().ok());
    let Ok(devices) = host.output_devices() else {
        return Vec::new();
    };
    let mut out = Vec::new();
    for device in devices {
        let Ok(name) = device.name() else {
            continue;
        };
        let is_default = default_name.as_ref() == Some(&name);
        out.push(AudioOutputDevice { name, is_default });
    }
    out.sort_by(|a, b| match (b.is_default, a.is_default) {
        (true, false) => std::cmp::Ordering::Greater,
        (false, true) => std::cmp::Ordering::Less,
        _ => a.name.cmp(&b.name),
    });
    out
}

struct DeviceGeometry {
    channels: usize,
    rate: u32,
}

struct Shared {
    /// Interleaved f32 ring (device channel count).
    ring: Mutex<VecDeque<f32>>,
    levels: Mutex<AudioLevels>,
    /// Playback boost in dB (applied when pushing).
    boost_db: AtomicI32,
    geometry: Mutex<DeviceGeometry>,
    /// `None` = system default output.
    device_name: Mutex<Option<String>>,
}

enum AudioCmd {
    SetDevice(Option<String>),
    Shutdown,
}

/// Plays decoded planar float PCM on a selectable output device.
///
/// The cpal stream is owned by a dedicated OS thread (`cpal::Stream` is not
/// `Send`), while PCM / levels are shared via [`Arc`].
pub struct AudioOutput {
    shared: Arc<Shared>,
    cmd_tx: Mutex<Option<Sender<AudioCmd>>>,
}

impl Default for AudioOutput {
    fn default() -> Self {
        Self::new()
    }
}

impl AudioOutput {
    /// Open the default output device (falls back to a silent stub on failure).
    pub fn new() -> Self {
        let shared = Arc::new(Shared {
            ring: Mutex::new(VecDeque::with_capacity(RING_CAP_SAMPLES.min(8192))),
            levels: Mutex::new(AudioLevels::default()),
            boost_db: AtomicI32::new(0),
            geometry: Mutex::new(DeviceGeometry {
                channels: 2,
                rate: 48_000,
            }),
            device_name: Mutex::new(None),
        });

        let (cmd_tx, cmd_rx) = mpsc::channel();
        let shared_thread = Arc::clone(&shared);
        if let Err(e) = thread::Builder::new()
            .name("omt-audio-out".into())
            .spawn(move || {
                if let Err(e) = run_output_thread(shared_thread, cmd_rx) {
                    warn!("audio output thread ended: {e}");
                }
            })
        {
            warn!("failed to spawn audio output thread: {e}");
            return Self {
                shared,
                cmd_tx: Mutex::new(None),
            };
        }

        Self {
            shared,
            cmd_tx: Mutex::new(Some(cmd_tx)),
        }
    }

    /// Currently selected device name (`None` = system default).
    pub fn selected_device_name(&self) -> Option<String> {
        self.shared.device_name.lock().clone()
    }

    /// Switch output device. `None` selects the system default.
    pub fn set_output_device(&self, name: Option<String>) {
        *self.shared.device_name.lock() = name.clone();
        self.clear();
        if let Some(tx) = self.cmd_tx.lock().as_ref() {
            let _ = tx.send(AudioCmd::SetDevice(name));
        }
    }

    /// Snapshot of recent true-peak levels (linear 0..1).
    pub fn levels(&self) -> AudioLevels {
        *self.shared.levels.lock()
    }

    /// Set playback boost in dB (typical 0 / 6 / 10 / 20).
    pub fn set_boost_db(&self, db: i32) {
        self.shared.boost_db.store(db, Ordering::Relaxed);
    }

    /// Clear the ring buffer (on disconnect / source change).
    pub fn clear(&self) {
        self.shared.ring.lock().clear();
        *self.shared.levels.lock() = AudioLevels::default();
    }

    /// Push planar f32 PCM (`ch0[samples]…chN[samples]` tightly packed as LE bytes).
    pub fn push_planar_f32(&self, data: &[u8], channels: i32, samples: i32, sample_rate: i32) {
        let ch = channels.max(1) as usize;
        let n = samples.max(0) as usize;
        let expected = ch * n * 4;
        if n == 0 || data.len() < expected {
            return;
        }

        let gain = db_to_gain(self.shared.boost_db.load(Ordering::Relaxed));
        let mut peak_l = 0.0f32;
        let mut peak_r = 0.0f32;

        let mut planes: Vec<Vec<f32>> = Vec::with_capacity(ch);
        for c in 0..ch {
            let base = c * n * 4;
            let mut plane = Vec::with_capacity(n);
            for s in 0..n {
                let o = base + s * 4;
                let sample = f32::from_le_bytes(data[o..o + 4].try_into().unwrap()) * gain;
                let a = sample.abs();
                if c == 0 {
                    peak_l = peak_l.max(a);
                } else if c == 1 {
                    peak_r = peak_r.max(a);
                }
                plane.push(sample.clamp(-1.0, 1.0));
            }
            planes.push(plane);
        }
        if ch == 1 {
            peak_r = peak_l;
        }

        {
            let mut levels = self.shared.levels.lock();
            // Hold true packet peak for metering (matches external tools like vMix).
            levels.peak_l = peak_l;
            levels.peak_r = peak_r;
            levels.frames = levels.frames.saturating_add(1);
            levels.sample_rate = sample_rate;
            levels.channels = channels;
        }

        let (out_ch, dst_rate) = {
            let geo = self.shared.geometry.lock();
            (geo.channels.max(1), geo.rate.max(1))
        };
        let src_rate = sample_rate.max(1) as u32;
        let out_n = if src_rate == dst_rate {
            n
        } else {
            ((n as u64 * dst_rate as u64) / src_rate as u64).max(1) as usize
        };

        let mut ring = self.shared.ring.lock();
        for i in 0..out_n {
            let src_i = if out_n == n {
                i
            } else {
                ((i as u64 * n as u64) / out_n as u64) as usize
            };
            for c in 0..out_ch {
                let sample = if c < ch {
                    planes[c][src_i.min(n - 1)]
                } else if ch == 1 {
                    planes[0][src_i.min(n - 1)]
                } else {
                    0.0
                };
                if ring.len() >= RING_CAP_SAMPLES {
                    ring.pop_front();
                }
                ring.push_back(sample);
            }
        }
    }
}

impl Drop for AudioOutput {
    fn drop(&mut self) {
        if let Some(tx) = self.cmd_tx.lock().take() {
            let _ = tx.send(AudioCmd::Shutdown);
        }
    }
}

fn db_to_gain(db: i32) -> f32 {
    10f32.powf(db as f32 / 20.0)
}

fn resolve_device(name: &Option<String>) -> Result<cpal::Device, String> {
    let host = cpal::default_host();
    if let Some(want) = name {
        let devices = host.output_devices().map_err(|e| e.to_string())?;
        for device in devices {
            if device.name().ok().as_ref() == Some(want) {
                return Ok(device);
            }
        }
        warn!("audio device `{want}` not found; falling back to default");
    }
    host.default_output_device()
        .ok_or_else(|| "no default output device".to_string())
}

fn run_output_thread(
    shared: Arc<Shared>,
    cmd_rx: mpsc::Receiver<AudioCmd>,
) -> Result<(), String> {
    let mut selected = shared.device_name.lock().clone();
    loop {
        let device = match resolve_device(&selected) {
            Ok(d) => d,
            Err(e) => {
                warn!("audio output unavailable: {e}");
                // Wait for a device change / shutdown instead of exiting permanently.
                match cmd_rx.recv() {
                    Ok(AudioCmd::SetDevice(name)) => {
                        selected = name;
                        *shared.device_name.lock() = selected.clone();
                        continue;
                    }
                    Ok(AudioCmd::Shutdown) | Err(_) => return Ok(()),
                }
            }
        };

        let supported = match device.default_output_config() {
            Ok(c) => c,
            Err(e) => {
                warn!("audio output config failed: {e}");
                match cmd_rx.recv() {
                    Ok(AudioCmd::SetDevice(name)) => {
                        selected = name;
                        *shared.device_name.lock() = selected.clone();
                        continue;
                    }
                    Ok(AudioCmd::Shutdown) | Err(_) => return Ok(()),
                }
            }
        };
        let sample_format = supported.sample_format();
        let config: StreamConfig = supported.into();
        {
            let mut geo = shared.geometry.lock();
            geo.channels = config.channels as usize;
            geo.rate = config.sample_rate.0;
        }
        shared.ring.lock().clear();

        let shared_cb = Arc::clone(&shared);
        let stream = match sample_format {
            SampleFormat::F32 => build_stream::<f32>(&device, &config, shared_cb),
            SampleFormat::I16 => build_stream::<i16>(&device, &config, shared_cb),
            SampleFormat::U16 => build_stream::<u16>(&device, &config, shared_cb),
            other => {
                warn!("unsupported sample format: {other:?}");
                match cmd_rx.recv() {
                    Ok(AudioCmd::SetDevice(name)) => {
                        selected = name;
                        *shared.device_name.lock() = selected.clone();
                        continue;
                    }
                    Ok(AudioCmd::Shutdown) | Err(_) => return Ok(()),
                }
            }
        };
        let stream = match stream {
            Ok(s) => s,
            Err(e) => {
                warn!("audio stream build failed: {e}");
                match cmd_rx.recv() {
                    Ok(AudioCmd::SetDevice(name)) => {
                        selected = name;
                        *shared.device_name.lock() = selected.clone();
                        continue;
                    }
                    Ok(AudioCmd::Shutdown) | Err(_) => return Ok(()),
                }
            }
        };
        if let Err(e) = stream.play() {
            warn!("audio stream play failed: {e}");
        }

        match cmd_rx.recv() {
            Ok(AudioCmd::SetDevice(name)) => {
                drop(stream);
                selected = name;
                *shared.device_name.lock() = selected.clone();
            }
            Ok(AudioCmd::Shutdown) | Err(_) => {
                drop(stream);
                return Ok(());
            }
        }
    }
}

fn build_stream<T>(
    device: &cpal::Device,
    config: &StreamConfig,
    shared: Arc<Shared>,
) -> Result<cpal::Stream, String>
where
    T: cpal::Sample + cpal::SizedSample + cpal::FromSample<f32>,
{
    device
        .build_output_stream(
            config,
            move |data: &mut [T], _| {
                let mut ring = shared.ring.lock();
                for sample in data.iter_mut() {
                    let v = ring.pop_front().unwrap_or(0.0);
                    *sample = T::from_sample(v);
                }
            },
            |err| warn!("audio stream error: {err}"),
            None,
        )
        .map_err(|e| e.to_string())
}
