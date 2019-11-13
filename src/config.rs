use env_logger::Builder;
use log::LevelFilter;
use structopt::StructOpt;
use url::Url;

/// Command line arguments given to the process.
#[derive(StructOpt)]
pub struct Arguments {
    /// The remote end to connect to.
    pub url: Url,
    /// How verbosely to log.
    #[structopt(short, long, parse(from_occurrences))]
    verbose: u8,
}

impl Arguments {
    /// Determine what logging level to use, given an input number. Higher levels mean more
    /// logging.
    fn log_level(level: u8) -> LevelFilter {
        match level {
            0 => LevelFilter::Off,
            1 => LevelFilter::Error,
            2 => LevelFilter::Warn,
            3 => LevelFilter::Info,
            4 => LevelFilter::Debug,
            _ => LevelFilter::Trace,
        }
    }

    /// Read the configuration arguments, and return them. Will also set up the application,
    /// configuring logging.
    pub fn new() -> Self {
        let args = Arguments::from_args();

        let mut logger = Builder::new();
        logger.filter_level(Self::log_level(args.verbose));
        logger.filter_module("nt", Self::log_level(args.verbose + 1));
        logger.init();

        args
    }
}
