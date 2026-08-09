//! Discovery browser wrapper.

use std::time::Duration;

use openmediatransport::{Discovery, OmtAddress, OmtError};

/// A discovered OMT source suitable for UI lists.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DiscoveredSource {
    /// Display name (`HOSTNAME (Source)`).
    pub name: String,
    /// Connectable `omt://` URL.
    pub url: String,
    /// Advertised TCP port.
    pub port: u16,
}

impl From<&OmtAddress> for DiscoveredSource {
    fn from(addr: &OmtAddress) -> Self {
        Self {
            name: addr.instance_name(),
            url: addr.to_url(),
            port: addr.port,
        }
    }
}

/// Thin wrapper around [`Discovery`] for periodic LAN browsing.
#[derive(Debug, Default)]
pub struct SourceBrowser {
    discovery: Option<Discovery>,
    sources: Vec<DiscoveredSource>,
}

impl SourceBrowser {
    /// Create an empty browser.
    pub fn new() -> Self {
        Self::default()
    }

    /// Refresh the source list, waiting up to `wait` for mDNS answers.
    pub fn refresh(&mut self, wait: Duration) -> Result<&[DiscoveredSource], OmtError> {
        if self.discovery.is_none() {
            self.discovery = Some(Discovery::new()?);
        }
        let discovery = self.discovery.as_mut().expect("discovery initialized");
        discovery.refresh_for(wait)?;
        self.sources = discovery.sources().iter().map(DiscoveredSource::from).collect();
        Ok(&self.sources)
    }

    /// Last known sources.
    pub fn sources(&self) -> &[DiscoveredSource] {
        &self.sources
    }
}
