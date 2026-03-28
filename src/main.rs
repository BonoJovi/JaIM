mod core;
mod engine;
mod ibus;

use log::info;

#[tokio::main]
async fn main() {
    env_logger::init();
    info!("JaIM - Japanese AI-powered Input Method");
    info!("Starting JaIM engine...");

    match ibus::start_ibus_service().await {
        Ok(connection) => {
            info!("JaIM: IBus service started successfully");
            // Keep the connection alive
            loop {
                connection.monitor_activity().await;
            }
        }
        Err(e) => {
            eprintln!("JaIM: Failed to start IBus service: {}", e);
            std::process::exit(1);
        }
    }
}
