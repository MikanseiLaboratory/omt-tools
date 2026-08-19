//! OMT Discovery Server CLI — matches the official console app.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread;
use std::time::Duration;

use clap::Parser;
use omt_discovery_server::{ServerController, ServerSettings};
use openmediatransport::DISCOVERY_SERVER_DEFAULT_PORT;
use suite_core::init_tracing;

#[derive(Debug, Parser)]
#[command(
    name = "omt-discovery-server",
    about = "OMT Discovery Server (TCP relay, default port 6399)"
)]
struct Args {
    /// Listen port.
    #[arg(short, long, default_value_t = DISCOVERY_SERVER_DEFAULT_PORT)]
    port: u16,
    /// Bind address (`::` = dual-stack any, matching the official app).
    #[arg(short, long, default_value = "::")]
    bind: String,
}

fn main() -> anyhow::Result<()> {
    init_tracing();
    let args = Args::parse();

    println!("OMTDiscoveryServer");
    println!("Command Line: omt-discovery-server --port <port> [--bind <addr>]");

    let mut server = ServerController::new(ServerSettings {
        bind: args.bind,
        port: args.port,
    });
    server.start().map_err(anyhow::Error::msg)?;
    println!(
        "Server running on {}, press CTRL+C to exit...",
        server.bind_addr()
    );

    let running = Arc::new(AtomicBool::new(true));
    let flag = Arc::clone(&running);
    ctrlc::set_handler(move || {
        flag.store(false, Ordering::SeqCst);
    })?;

    let mut printed = 0usize;
    while running.load(Ordering::SeqCst) {
        server.poll();
        let events = server.events();
        if events.len() > printed {
            for line in &events[printed..] {
                println!("{line}");
            }
            printed = events.len();
        }
        thread::sleep(Duration::from_millis(100));
    }

    server.stop().map_err(anyhow::Error::msg)?;
    println!("stopped");
    Ok(())
}
