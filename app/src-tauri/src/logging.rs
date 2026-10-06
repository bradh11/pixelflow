//! A small logger to the terminal: warnings always, and timings (debug) in development builds
//! or when `PIXELFLOW_LOG=debug` is set (`PIXELFLOW_LOG=warn` quiets a development build).

use log::{Level, LevelFilter, Log, Metadata, Record};

struct Terminal;

impl Log for Terminal {
    fn enabled(&self, metadata: &Metadata) -> bool {
        metadata.level() <= log::max_level() && metadata.target().starts_with("pixelflow")
    }

    fn log(&self, record: &Record) {
        if self.enabled(record.metadata()) {
            let level = match record.level() {
                Level::Error => "error",
                Level::Warn => "warn",
                Level::Info => "info",
                Level::Debug => "debug",
                Level::Trace => "trace",
            };
            eprintln!("[pixelflow {level}] {}", record.args());
        }
    }

    fn flush(&self) {}
}

/// The level `PIXELFLOW_LOG` asks for, else debug in development builds and warnings otherwise.
fn level(setting: Option<&str>) -> LevelFilter {
    match setting.map(str::to_ascii_lowercase).as_deref() {
        Some("off") => LevelFilter::Off,
        Some("error") => LevelFilter::Error,
        Some("warn") => LevelFilter::Warn,
        Some("info") => LevelFilter::Info,
        Some("debug") => LevelFilter::Debug,
        Some("trace") => LevelFilter::Trace,
        _ if cfg!(debug_assertions) => LevelFilter::Debug,
        _ => LevelFilter::Warn,
    }
}

/// Starts logging (once; later calls do nothing).
pub(crate) fn init() {
    static TERMINAL: Terminal = Terminal;
    if log::set_logger(&TERMINAL).is_ok() {
        log::set_max_level(level(std::env::var("PIXELFLOW_LOG").ok().as_deref()));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_level_follows_the_setting() {
        assert_eq!(level(Some("DEBUG")), LevelFilter::Debug);
        assert_eq!(level(Some("warn")), LevelFilter::Warn);
        assert_eq!(level(Some("off")), LevelFilter::Off);
        let default = if cfg!(debug_assertions) {
            LevelFilter::Debug
        } else {
            LevelFilter::Warn
        };
        assert_eq!(level(None), default);
        assert_eq!(level(Some("nonsense")), default);
    }
}
