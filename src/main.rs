mod core;
mod engine;
mod ibus;

use log::info;

#[tokio::main]
async fn main() {
    env_logger::init();
    info!("JaIM - Japanese AI-powered Input Method");
    info!("Starting JaIM engine...");

    // TODO: Initialize core components
    // TODO: Start IBus engine service
}
