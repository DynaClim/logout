//! A simple logger for the [`log`](https://crates.io/crates/log) facade. One
//! log message is written per line. Each line also includes the time it was
//! logged, the logging level and the ID of the thread.
//!
//! # Log format
//!
//! Each log message has the following format:
//!
//! ```text
//! [<time>] (<thread-name> <thread-id>) <level> <message>\n
//! ```
//!
//! Where:
//! `<time>` is the current time with utc-offset (if available). Available format RFC2822 and RFC3339.
//! `<thread-name>` and `<thread-id>` are thread identifiers defined by `std::thread`.
//! `<level>` is the log level as defined by `log::LogLevel`.
//! `<message>` is the log message.
//!
//! # Errors
//!
//! Best effort is made to handle errors. Write failures result in falling back to `stderr`.
//!
//! # Performance
//!
//! The logger relies on a global `Mutex` to serialize access to the user
//! supplied sink.

use log::{LevelFilter, Log, Metadata, Record, SetLoggerError};
use std::fs::{File, OpenOptions};
use std::io::{Stderr, Write};
use std::path::Path;
use std::sync::Mutex;
use std::thread;
use time::{
    OffsetDateTime,
    format_description::well_known::{Rfc2822, Rfc3339},
};

/// Configure the [`log`](https://crates.io/crates/log) facade.
///
/// # Examples
///
/// Log to stderr.
/// ```rust
/// use logout::new_log;
///
/// # fn main() -> Result<()> {
///     new_log().enable()?;
/// # }
/// ```
///
/// Log to a file, specifying minimum log level and time format.
/// ```rust
/// use log::LevelFilter;
/// use logout::{new_log, TimeFormat};
/// use std::io;
///
/// # fn main() -> Result<()> {
///     new_log()
///       .to_file(&log_path)?
///       .max_log_level(LevelFilter::Info)
///       .time_format(TimeFormat::Rfc2822)
///       .enable()?;
/// # }
/// ```
///
/// Log to a custom sink.
/// ```rust
/// use log::LevelFilter;
/// use logout::new_log;
/// use std::io;
///
/// # fn main() -> Result<()> {
///     new_log()
///       .sink(std::io:stderr())?
///       .max_log_level(LevelFilter::Info)
///       .enable()?;
/// # }
/// ```
#[must_use]
pub fn new_log() -> Logger<Stderr> {
    Logger::new(std::io::stderr())
}

#[derive(Copy, Clone, Debug)]
pub enum TimeFormat {
    Rfc2822,
    Rfc3339,
}

#[derive(Debug)]
pub struct Logger<T: Write + Send + 'static> {
    sink: Mutex<T>,
    time_format: TimeFormat,
    level: LevelFilter,
}

impl<T: Write + Send + 'static> Logger<T> {
    fn new(sink: T) -> Self {
        Self {
            sink: Mutex::new(sink),
            time_format: TimeFormat::Rfc2822,
            level: LevelFilter::Info,
        }
    }

    pub fn to_file(&self, path: impl AsRef<Path>) -> Result<Logger<File>, std::io::Error> {
        let sink = OpenOptions::new().append(true).create(true).open(path)?;
        Ok(Logger {
            sink: Mutex::new(sink),
            time_format: self.time_format,
            level: self.level,
        })
    }

    pub fn sink<U: Write + Send + 'static>(&self, sink: U) -> Logger<U> {
        Logger {
            sink: Mutex::new(sink),
            time_format: self.time_format,
            level: self.level,
        }
    }

    pub fn time_format(self, time_format: TimeFormat) -> Self {
        Self {
            sink: self.sink,
            time_format,
            level: self.level,
        }
    }

    pub fn max_log_level(self, level: LevelFilter) -> Self {
        Self {
            sink: self.sink,
            time_format: self.time_format,
            level,
        }
    }

    pub fn enable(self) -> Result<(), SetLoggerError> {
        log::set_max_level(self.level);
        // Will fail if `set_logger` or `set_boxed_logger` has already been called.
        log::set_boxed_logger(Box::new(self))
    }

    fn log(&self, record: &Record) {
        let now = match OffsetDateTime::now_local() {
            Ok(now_local) => now_local,
            Err(_) => OffsetDateTime::now_utc(),
        };

        let now = match self.time_format {
            TimeFormat::Rfc2822 => now.format(&Rfc2822),
            TimeFormat::Rfc3339 => now.format(&Rfc3339),
        };

        let msg = format!(
            "[{}] ({} {:?}) [{}] {}",
            now.unwrap_or("time error".to_string()),
            thread::current().name().unwrap_or("<unnamed>"),
            thread::current().id(),
            record.level(),
            record.args()
        );

        match self.sink.lock() {
            Ok(mut sink) => {
                if let Err(e) = writeln!(sink, "{msg}") {
                    // Fallback write to stderr.
                    eprintln!("error writing to sink, falling back to stderr: {e}");
                    eprintln!("{msg}");
                }
            }
            Err(_) => {
                // Fallback write to stderr.
                eprintln!("{msg}");
            }
        };
    }
}

impl<T: Write + Send + 'static> Log for Logger<T> {
    fn enabled(&self, metadata: &Metadata) -> bool {
        metadata.level() <= self.level
    }

    fn log(&self, record: &Record) {
        if self.enabled(record.metadata()) {
            self.log(record);
        }
    }

    fn flush(&self) {}
}
