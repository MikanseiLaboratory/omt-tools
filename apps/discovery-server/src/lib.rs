//! Shared Discovery Server controller used by the CLI and GUI.

use std::net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr};

use openmediatransport::{
    DISCOVERY_SERVER_DEFAULT_PORT, DiscoveryServerEvent, DiscoveryServerHandle,
    DiscoveryServerSnapshot, OmtAddress, default_bind_addr,
};

/// Maximum retained GUI/CLI event lines.
const MAX_EVENTS: usize = 200;

/// Bind + port settings for the in-process discovery server.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ServerSettings {
    /// Bind host (`::`, `0.0.0.0`, or a unicast address).
    pub bind: String,
    /// TCP port (default 6399).
    pub port: u16,
}

impl Default for ServerSettings {
    fn default() -> Self {
        Self {
            bind: "::".into(),
            port: DISCOVERY_SERVER_DEFAULT_PORT,
        }
    }
}

/// Runtime wrapper around [`DiscoveryServerHandle`].
pub struct ServerController {
    handle: DiscoveryServerHandle,
    settings: ServerSettings,
    events: Vec<String>,
}

impl ServerController {
    /// Create a stopped controller with the given listen settings.
    pub fn new(settings: ServerSettings) -> Self {
        Self {
            handle: DiscoveryServerHandle::with_bind(parse_bind(&settings.bind, settings.port)),
            settings,
            events: Vec::new(),
        }
    }

    /// Current bind/port text fields.
    pub fn settings(&self) -> &ServerSettings {
        &self.settings
    }

    /// Replace bind/port. Fails while the server is running.
    pub fn set_settings(&mut self, settings: ServerSettings) -> Result<(), String> {
        if self.is_running() {
            return Err("stop the server before changing bind or port".into());
        }
        let bind = parse_bind(&settings.bind, settings.port);
        self.handle.set_bind(bind).map_err(|e| e.to_string())?;
        self.settings = settings;
        Ok(())
    }

    /// Bind and start the accept loop.
    pub fn start(&mut self) -> Result<(), String> {
        let bind = parse_bind(&self.settings.bind, self.settings.port);
        self.handle.set_bind(bind).map_err(|e| e.to_string())?;
        self.handle.start().map_err(|e| e.to_string())?;
        self.push_event(format!("starting on {}", self.handle.bind()));
        Ok(())
    }

    /// Stop and join the accept loop.
    pub fn stop(&mut self) -> Result<(), String> {
        self.handle.join().map_err(|e| e.to_string())?;
        self.drain_handle_events();
        Ok(())
    }

    /// True while the accept loop is running.
    pub fn is_running(&self) -> bool {
        self.handle.is_running()
    }

    /// Bound address after start (OS-assigned if port was 0).
    pub fn bind_addr(&self) -> SocketAddr {
        self.handle.bind()
    }

    /// Connected peers and registered sources.
    pub fn snapshot(&self) -> DiscoveryServerSnapshot {
        self.handle.snapshot()
    }

    /// Drain handle events into the local log.
    pub fn poll(&mut self) {
        self.drain_handle_events();
    }

    /// Recent human-readable event lines (newest last).
    pub fn events(&self) -> &[String] {
        &self.events
    }

    /// Drop retained event lines.
    pub fn clear_events(&mut self) {
        self.events.clear();
    }

    fn drain_handle_events(&mut self) {
        for event in self.handle.drain_events() {
            self.push_event(format_event(&event));
        }
    }

    fn push_event(&mut self, line: String) {
        self.events.push(line);
        let extra = self.events.len().saturating_sub(MAX_EVENTS);
        if extra > 0 {
            self.events.drain(0..extra);
        }
    }
}

/// Parse a bind host plus port into a [`SocketAddr`].
pub fn parse_bind(bind: &str, port: u16) -> SocketAddr {
    let bind = bind.trim();
    if bind.is_empty() || bind == "::" {
        return default_bind_addr(port);
    }
    if bind == "0.0.0.0" {
        return SocketAddr::from((Ipv4Addr::UNSPECIFIED, port));
    }
    if let Ok(addr) = bind.parse::<SocketAddr>() {
        return addr;
    }
    if let Ok(ip) = bind.parse::<IpAddr>() {
        return SocketAddr::new(ip, port);
    }
    if let Ok(v6) = bind.parse::<Ipv6Addr>() {
        return SocketAddr::from((v6, port));
    }
    default_bind_addr(port)
}

/// A selectable listen address (all-interfaces or a NIC unicast IP).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BindChoice {
    /// Value written into the bind field (`::`, `0.0.0.0`, or a unicast IP).
    pub bind: String,
    /// Interface name when this is a NIC address.
    pub iface: Option<String>,
}

/// Bind targets: dual-stack any, IPv4 any, then each NIC address.
pub fn bind_choices() -> Vec<BindChoice> {
    let mut choices = vec![
        BindChoice {
            bind: "::".into(),
            iface: None,
        },
        BindChoice {
            bind: "0.0.0.0".into(),
            iface: None,
        },
    ];
    let mut seen = std::collections::BTreeSet::from(["::".to_string(), "0.0.0.0".to_string()]);
    if let Ok(ifaces) = if_addrs::get_if_addrs() {
        for iface in ifaces {
            let ip = iface.ip();
            if ip.is_unspecified() {
                continue;
            }
            let bind = ip.to_string();
            if !seen.insert(bind.clone()) {
                continue;
            }
            choices.push(BindChoice {
                bind,
                iface: Some(iface.name),
            });
        }
    }
    choices
}

fn format_event(event: &DiscoveryServerEvent) -> String {
    match event {
        DiscoveryServerEvent::Started { bind } => format!("started on {bind}"),
        DiscoveryServerEvent::ClientConnected { peer } => format!("connected {peer}"),
        DiscoveryServerEvent::ClientDisconnected { peer } => format!("disconnected {peer}"),
        DiscoveryServerEvent::SourceRegistered { address, peer } => {
            format!("added {} from {peer}", format_source(address))
        }
        DiscoveryServerEvent::SourceRemoved { address, peer } => {
            format!("removed {} from {peer}", format_source(address))
        }
        DiscoveryServerEvent::Error { message } => format!("error {message}"),
        DiscoveryServerEvent::Stopped => "stopped".into(),
    }
}

fn format_source(address: &OmtAddress) -> String {
    format!("{}:{}", address.instance_name(), address.port)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::thread;
    use std::time::Duration;

    use openmediatransport::{DiscoveryClient, OmtAddress};

    fn wait_until(timeout: Duration, mut pred: impl FnMut() -> bool) -> bool {
        let deadline = std::time::Instant::now() + timeout;
        while std::time::Instant::now() < deadline {
            if pred() {
                return true;
            }
            thread::sleep(Duration::from_millis(20));
        }
        pred()
    }

    #[test]
    fn parse_bind_defaults_to_ipv6_any() {
        let addr = parse_bind("::", 6399);
        assert!(addr.is_ipv6());
        assert!(addr.ip().is_unspecified());
        assert_eq!(addr.port(), 6399);
        assert_eq!(
            parse_bind("127.0.0.1", 6400),
            SocketAddr::from(([127, 0, 0, 1], 6400))
        );
    }

    #[test]
    fn bind_choices_include_any_and_local_addrs() {
        let choices = bind_choices();
        assert!(choices.iter().any(|c| c.bind == "::" && c.iface.is_none()));
        assert!(
            choices
                .iter()
                .any(|c| c.bind == "0.0.0.0" && c.iface.is_none())
        );
    }

    #[test]
    fn controller_start_register_disconnect_stop() {
        let mut server = ServerController::new(ServerSettings {
            bind: "127.0.0.1".into(),
            port: 0,
        });
        server.start().unwrap();
        assert!(wait_until(Duration::from_secs(2), || server.is_running()));
        let port = server.bind_addr().port();
        assert_ne!(port, 0);

        let mut client = DiscoveryClient::new("127.0.0.1");
        client.port = port;
        client.connect().unwrap();
        let mut addr = OmtAddress::from_full_name("CTRLHOST (Cam1)", 6411);
        addr.addresses = vec!["10.0.0.9".into()];
        client.register(&addr).unwrap();

        assert!(wait_until(Duration::from_secs(5), || {
            server.poll();
            server
                .snapshot()
                .sources
                .iter()
                .any(|s| s.instance_name() == "CTRLHOST (Cam1)")
        }));
        assert_eq!(server.snapshot().peer_count(), 1);

        drop(client);
        assert!(wait_until(Duration::from_secs(3), || {
            server.poll();
            server.snapshot().peer_count() == 0 && server.snapshot().sources.is_empty()
        }));

        server.stop().unwrap();
        assert!(!server.is_running());
        server.poll();
        assert!(server.events().iter().any(|e| e.contains("started")));
        assert!(server.events().iter().any(|e| e.contains("added")));
        assert!(server.events().iter().any(|e| e.contains("stopped")));
    }

    #[test]
    fn controller_start_fails_when_port_busy() {
        let occupied = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let port = occupied.local_addr().unwrap().port();
        let mut server = ServerController::new(ServerSettings {
            bind: "127.0.0.1".into(),
            port,
        });
        let err = server.start().unwrap_err();
        drop(occupied);
        assert!(
            err.to_ascii_lowercase().contains("addr")
                || err.contains("10048")
                || err.contains("in use")
                || err.contains("os error"),
            "unexpected start error: {err}"
        );
    }
}
