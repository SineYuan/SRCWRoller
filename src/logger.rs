use log::{Level, LevelFilter, Metadata, Record};

pub use log::{debug, error, info, warn};

struct SimpleLogger;

impl log::Log for SimpleLogger {
    fn enabled(&self, metadata: &Metadata) -> bool {
        let target = metadata.target();
        if target.starts_with("radb") {
            return false;
        }
        metadata.level() <= Level::Debug
    }

    fn log(&self, record: &Record) {
        if self.enabled(record.metadata()) {
            let level_str = match record.level() {
                Level::Error => "\x1b[31m[ERROR]\x1b[0m",
                Level::Warn => "\x1b[33m[WARN]\x1b[0m",
                Level::Info => "\x1b[32m[INFO]\x1b[0m",
                Level::Debug => "\x1b[36m[DEBUG]\x1b[0m",
                Level::Trace => "\x1b[90m[TRACE]\x1b[0m",
            };
            
            let timestamp = chrono::Local::now().format("%H:%M:%S");
            
            if record.level() == Level::Error {
                eprintln!("{} {} {}", timestamp, level_str, record.args());
            } else {
                println!("{} {} {}", timestamp, level_str, record.args());
            }
        }
    }

    fn flush(&self) {}
}

static LOGGER: SimpleLogger = SimpleLogger;

pub fn init(debug: bool) {
    let level = if debug {
        LevelFilter::Debug
    } else {
        LevelFilter::Info
    };
    
    log::set_logger(&LOGGER)
        .map(|()| log::set_max_level(level))
        .expect("初始化日志系统失败");
}

#[macro_export]
macro_rules! log_success {
    ($($arg:tt)*) => {
        println!("\x1b[32m[SUCCESS]\x1b[0m {}", format!($($arg)*))
    };
}

#[macro_export]
macro_rules! log_section {
    ($($arg:tt)*) => {
        println!("{}", "=".repeat(50));
        println!("{}", format!($($arg)*));
        println!("{}", "=".repeat(50));
    };
}

#[macro_export]
macro_rules! log_step {
    ($step:expr, $($arg:tt)*) => {
        println!("\n--- Step {}: {} ---", $step, format!($($arg)*))
    };
}

#[macro_export]
macro_rules! log_retry {
    ($current:expr, $max:expr) => {
        println!("\n{} 第 {} 次尝试 {}", "=".repeat(30), $current, "=".repeat(30))
    };
    ($current:expr, $max:expr, $($arg:tt)*) => {
        println!("\n{} 第 {} 次尝试 {} {}", "=".repeat(30), $current, "=".repeat(30), format!($($arg)*))
    };
}
