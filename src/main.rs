use log::info;
use simplelog;
use std::fs::OpenOptions;

mod client;
mod client_handler;
mod message;
mod server;

fn main() {
    simplelog::WriteLogger::init(
        simplelog::LevelFilter::Info,
        simplelog::Config::default(),
        OpenOptions::new()
            .create(true)
            .append(true)
            .open("/home/farzeen/relay.log")
            .unwrap(),
    )
    .unwrap();
    info!("Starting daemon...");
    let status = server::start();
    info!("Dameon started: {}", status.is_ok());

    info!("Starting client ..");
    client::client();
}
