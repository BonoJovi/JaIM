/// IBus Factory D-Bus interface and service startup.
///
/// Implements org.freedesktop.IBus.Factory which creates engine instances
/// on demand from the IBus daemon.
use log::info;
use zbus::{connection::Builder, interface, object_server::SignalEmitter, Connection};

use super::engine_impl::JaimEngine;

/// IBus Factory — creates engine instances on request from IBus daemon.
pub struct JaimFactory;

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
        let engine = JaimEngine::new();

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

/// Start the IBus service: register Factory on the session bus.
pub async fn start_ibus_service() -> zbus::Result<Connection> {
    info!("JaIM: Starting IBus service...");

    let connection = Builder::session()?
        .name("org.freedesktop.IBus.JaIM")?
        .serve_at("/org/freedesktop/IBus/Factory", JaimFactory)?
        .build()
        .await?;

    info!("JaIM: IBus service registered on session bus");
    info!("JaIM: Waiting for IBus daemon requests...");

    Ok(connection)
}
