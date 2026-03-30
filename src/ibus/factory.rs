/// IBus Factory D-Bus interface and service startup.
///
/// Implements org.freedesktop.IBus.Factory which creates engine instances
/// on demand from the IBus daemon.
use log::info;
use std::path::PathBuf;
use zbus::{connection::Builder, interface, object_server::SignalEmitter, Connection};

use super::config::JaimConfig;
use super::engine_impl::JaimEngine;

/// IBus Factory — creates engine instances on request from IBus daemon.
pub struct JaimFactory {
    config: JaimConfig,
}

impl JaimFactory {
    pub fn new() -> Self {
        Self {
            config: JaimConfig::load(),
        }
    }
}

#[interface(name = "org.freedesktop.IBus.Factory")]
impl JaimFactory {
    /// Called by IBus daemon to create a new engine instance.
    async fn create_engine(
        &self,
        #[zbus(signal_emitter)] _emitter: SignalEmitter<'_>,
        #[zbus(connection)] connection: &Connection,
        engine_name: &str,
    ) -> zbus::fdo::Result<zbus::zvariant::OwnedObjectPath> {
        info!("JaIM Factory: CreateEngine({})", engine_name);

        let path = format!("/org/freedesktop/IBus/Engine/{}", engine_name);
        let engine = JaimEngine::new(&self.config);

        connection
            .object_server()
            .at(path.as_str(), engine)
            .await
            .map_err(|e| zbus::fdo::Error::Failed(e.to_string()))?;

        Ok(path
            .try_into()
            .map_err(|e: zbus::zvariant::Error| zbus::fdo::Error::Failed(e.to_string()))?)
    }
}

/// Resolve the IBus D-Bus address.
///
/// IBus runs its own private bus separate from the session bus.
/// The address is found via:
/// 1. `IBUS_ADDRESS` environment variable (set by IBus when launching engines)
/// 2. Bus file in `~/.config/ibus/bus/`
fn get_ibus_address() -> Option<String> {
    // Check environment variable first
    if let Ok(addr) = std::env::var("IBUS_ADDRESS") {
        if !addr.is_empty() {
            return Some(addr);
        }
    }

    // Fall back to reading the IBus bus file (use the most recently modified one)
    let config_dir = std::env::var("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|_| {
            let home = std::env::var("HOME").unwrap_or_default();
            PathBuf::from(home).join(".config")
        });
    let bus_dir = config_dir.join("ibus").join("bus");

    let mut newest: Option<(std::time::SystemTime, String)> = None;
    let entries = std::fs::read_dir(&bus_dir).ok()?;
    for entry in entries.flatten() {
        let mtime = entry.metadata().ok()?.modified().ok()?;
        if let Ok(contents) = std::fs::read_to_string(entry.path()) {
            for line in contents.lines() {
                if let Some(addr) = line.strip_prefix("IBUS_ADDRESS=") {
                    if !addr.is_empty() {
                        if newest.as_ref().is_none_or(|(t, _)| mtime > *t) {
                            newest = Some((mtime, addr.to_string()));
                        }
                    }
                }
            }
        }
    }

    newest.map(|(_, addr)| addr)
}

/// Start the IBus service: register Factory on the IBus private bus.
pub async fn start_ibus_service() -> zbus::Result<Connection> {
    info!("JaIM: Starting IBus service...");

    let connection = if let Some(addr) = get_ibus_address() {
        info!("JaIM: Connecting to IBus bus at {}", addr);
        Builder::address(addr.as_str())?
            .name("org.freedesktop.IBus.JaIM")?
            .serve_at("/org/freedesktop/IBus/Factory", JaimFactory::new())?
            .build()
            .await?
    } else {
        info!("JaIM: IBus address not found, falling back to session bus");
        Builder::session()?
            .name("org.freedesktop.IBus.JaIM")?
            .serve_at("/org/freedesktop/IBus/Factory", JaimFactory::new())?
            .build()
            .await?
    };

    info!("JaIM: IBus service registered successfully");
    info!("JaIM: Waiting for IBus daemon requests...");

    Ok(connection)
}
