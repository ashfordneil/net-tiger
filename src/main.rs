mod config;
mod executor;
mod reactor;

use self::config::Arguments;

fn main() {
    let args = Arguments::new();

    log::debug!("Starting up");
    log::info!("Connecting to {:?}", args.url);

    log::error!("We can't connect yet");
}
