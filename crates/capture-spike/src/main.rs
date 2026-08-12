//! Screen capture spike — validates OS backends before productizing Screen Capture.

mod capture;

use capture::CaptureProbe;
use clap::Parser;

#[derive(Debug, Parser)]
#[command(
    name = "omt-screen-capture-spike",
    about = "Probe OS screen-capture backends for OMT Tools"
)]
struct Args {
    /// Run a short capture smoke test when the backend is available.
    #[arg(long, default_value_t = false)]
    smoke: bool,
}

fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .init();

    let args = Args::parse();
    let probe = CaptureProbe::detect()?;
    println!("backend={}", probe.backend.id());
    println!("available={}", probe.available);
    println!("notes={}", probe.notes);
    for req in &probe.requirements {
        println!("requirement={req}");
    }

    if args.smoke {
        match probe.backend.smoke_capture() {
            Ok(info) => {
                println!(
                    "smoke=ok frames={} size={}x{} bgra_bytes={}",
                    info.frames, info.width, info.height, info.bgra_len
                );
                if info.frames > 0 {
                    // BGRA can be passed to Sender uncompressed. UYVY size is shown for comparison.
                    let rgb_w = info.width.max(2);
                    let rgb_h = info.height.max(1);
                    let mut dummy_rgb = vec![16u8; (rgb_w * rgb_h * 3) as usize];
                    for px in dummy_rgb.chunks_exact_mut(3) {
                        px[0] = 32;
                        px[1] = 64;
                        px[2] = 96;
                    }
                    let uyvy = omt_media::uyvy_from_rgb_frame(&dummy_rgb, rgb_w, rgb_h);
                    println!("uyvy_bytes={}", uyvy.len());
                }
            }
            Err(e) => {
                println!("smoke=error err={e}");
                std::process::exit(2);
            }
        }
    }

    if !probe.available {
        std::process::exit(3);
    }
    Ok(())
}
