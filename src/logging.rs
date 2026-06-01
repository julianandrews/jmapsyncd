use crate::config::LogLevel;
use log::LevelFilter;

pub fn init(level: Option<LogLevel>) {
    let mut builder = env_logger::Builder::from_default_env();

    if let Some(level) = level {
        builder.filter_level(LevelFilter::from(level));
    }

    builder.init();
}
