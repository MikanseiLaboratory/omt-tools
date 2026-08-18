//! libomtnet-compatible file logging and tracing setup.

use std::fmt::{self, Write as FmtWrite};
use std::fs::{self, File, OpenOptions};
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};

use tracing::Event;
use tracing::field::{Field, Visit};
use tracing_subscriber::EnvFilter;
use tracing_subscriber::layer::{Context, Layer};
use tracing_subscriber::prelude::*;

/// Environment variable overriding the OMT storage directory.
pub const OMT_STORAGE_PATH: &str = "OMT_STORAGE_PATH";

static LOG_FILE: OnceLock<Mutex<Option<File>>> = OnceLock::new();

/// Directory that contains `settings.xml` and `logs/`.
pub fn storage_dir() -> PathBuf {
    storage_dir_from(
        std::env::var(OMT_STORAGE_PATH)
            .ok()
            .filter(|s| !s.is_empty()),
        std::env::var_os("ProgramData").map(PathBuf::from),
        std::env::var_os("HOME")
            .or_else(|| std::env::var_os("USERPROFILE"))
            .map(PathBuf::from),
    )
}

fn storage_dir_from(
    storage_override: Option<String>,
    program_data: Option<PathBuf>,
    home: Option<PathBuf>,
) -> PathBuf {
    if let Some(p) = storage_override {
        return PathBuf::from(p);
    }
    default_storage_dir(program_data, home)
}

fn default_storage_dir(program_data: Option<PathBuf>, home: Option<PathBuf>) -> PathBuf {
    #[cfg(windows)]
    {
        let _ = home;
        program_data
            .unwrap_or_else(|| PathBuf::from(r"C:\ProgramData"))
            .join("OMT")
    }
    #[cfg(not(windows))]
    {
        let _ = program_data;
        home.unwrap_or_else(|| PathBuf::from(".")).join(".OMT")
    }
}

/// `{storage}/logs`.
pub fn logs_dir() -> PathBuf {
    storage_dir().join("logs")
}

/// `{exe_name}{pid}.log` under [`logs_dir`], matching libomtnet `OMTLogging`.
pub fn default_log_path() -> PathBuf {
    log_path_in(&logs_dir())
}

fn log_path_in(dir: &Path) -> PathBuf {
    dir.join(format!("{}.log", process_name_and_id()))
}

fn process_name_and_id() -> String {
    let pid = std::process::id();
    match std::env::current_exe() {
        Ok(exe) => match exe.file_name() {
            Some(name) => format!("{}{pid}", name.to_string_lossy()),
            None => pid.to_string(),
        },
        Err(_) => pid.to_string(),
    }
}

fn open_append(path: &Path) -> io::Result<File> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    OpenOptions::new().create(true).append(true).open(path)
}

fn ensure_file() -> &'static Mutex<Option<File>> {
    LOG_FILE.get_or_init(|| match open_append(&default_log_path()) {
        Ok(mut file) => {
            let _ = writeln!(file, "{},[OMTLogging],Log Started", timestamp());
            let _ = file.flush();
            Mutex::new(Some(file))
        }
        Err(_) => Mutex::new(None),
    })
}

/// Append a libomtnet-style line: `{datetime},[{source}],{message}`.
fn write_line(message: &str, source: &str) {
    let line = format!("{},[{}],{message}", timestamp(), source);
    let Ok(mut guard) = ensure_file().lock() else {
        return;
    };
    if let Some(file) = guard.as_mut() {
        let _ = writeln!(file, "{line}");
        let _ = file.flush();
    }
}

/// Initialize tracing to stderr (`RUST_LOG`) and the official OMT log file.
///
/// Safe to call more than once; later calls are ignored if a subscriber is
/// already installed.
pub fn init_tracing() {
    let _ = ensure_file();
    let env_filter = EnvFilter::from_default_env();
    let _ = tracing_subscriber::registry()
        .with(env_filter)
        .with(tracing_subscriber::fmt::layer())
        .with(OmtFileLayer)
        .try_init();
}

struct OmtFileLayer;

impl<S> Layer<S> for OmtFileLayer
where
    S: tracing::Subscriber,
{
    fn on_event(&self, event: &Event<'_>, _ctx: Context<'_, S>) {
        let mut message = String::new();
        event.record(&mut MessageVisitor(&mut message));
        if message.is_empty() {
            message.push_str(event.metadata().name());
        }
        write_line(&message, event.metadata().target());
    }
}

struct MessageVisitor<'a>(&'a mut String);

impl Visit for MessageVisitor<'_> {
    fn record_debug(&mut self, field: &Field, value: &dyn fmt::Debug) {
        if !self.0.is_empty() {
            self.0.push(' ');
        }
        if field.name() == "message" {
            let _ = write!(self.0, "{value:?}");
        } else {
            let _ = write!(self.0, "{}={value:?}", field.name());
        }
    }

    fn record_str(&mut self, field: &Field, value: &str) {
        if !self.0.is_empty() {
            self.0.push(' ');
        }
        if field.name() == "message" {
            self.0.push_str(value);
        } else {
            self.0.push_str(field.name());
            self.0.push('=');
            self.0.push_str(value);
        }
    }
}

fn timestamp() -> String {
    #[cfg(windows)]
    {
        #[repr(C)]
        struct SystemTime {
            w_year: u16,
            w_month: u16,
            w_day_of_week: u16,
            w_day: u16,
            w_hour: u16,
            w_minute: u16,
            w_second: u16,
            w_milliseconds: u16,
        }
        unsafe extern "system" {
            fn GetLocalTime(lp_system_time: *mut SystemTime);
        }
        let mut st = SystemTime {
            w_year: 0,
            w_month: 0,
            w_day_of_week: 0,
            w_day: 0,
            w_hour: 0,
            w_minute: 0,
            w_second: 0,
            w_milliseconds: 0,
        };
        unsafe { GetLocalTime(&mut st) };
        format!(
            "{:04}-{:02}-{:02} {:02}:{:02}:{:02}",
            st.w_year, st.w_month, st.w_day, st.w_hour, st.w_minute, st.w_second
        )
    }
    #[cfg(unix)]
    {
        #[repr(C)]
        struct Tm {
            tm_sec: i32,
            tm_min: i32,
            tm_hour: i32,
            tm_mday: i32,
            tm_mon: i32,
            tm_year: i32,
            tm_wday: i32,
            tm_yday: i32,
            tm_isdst: i32,
            _pad: [u8; 32],
        }
        unsafe extern "C" {
            fn time(tloc: *mut i64) -> i64;
            fn localtime_r(timep: *const i64, result: *mut Tm) -> *mut Tm;
        }
        unsafe {
            let mut t: i64 = 0;
            time(&mut t);
            let mut tm = Tm {
                tm_sec: 0,
                tm_min: 0,
                tm_hour: 0,
                tm_mday: 0,
                tm_mon: 0,
                tm_year: 0,
                tm_wday: 0,
                tm_yday: 0,
                tm_isdst: 0,
                _pad: [0; 32],
            };
            if localtime_r(&t, &mut tm).is_null() {
                return format!("{t}");
            }
            format!(
                "{:04}-{:02}-{:02} {:02}:{:02}:{:02}",
                tm.tm_year + 1900,
                tm.tm_mon + 1,
                tm.tm_mday,
                tm.tm_hour,
                tm.tm_min,
                tm.tm_sec
            )
        }
    }
    #[cfg(not(any(windows, unix)))]
    {
        String::from("unknown-time")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn storage_override_wins() {
        let dir = storage_dir_from(
            Some(String::from("/tmp/omt-store")),
            Some(PathBuf::from(r"C:\ProgramData")),
            Some(PathBuf::from("/home/user")),
        );
        assert_eq!(dir, PathBuf::from("/tmp/omt-store"));
    }

    #[test]
    fn default_storage_dir_is_platform_specific() {
        let dir = default_storage_dir(
            Some(PathBuf::from(r"C:\ProgramData")),
            Some(PathBuf::from("/home/user")),
        );
        #[cfg(windows)]
        {
            assert_eq!(dir, PathBuf::from(r"C:\ProgramData\OMT"));
        }
        #[cfg(not(windows))]
        {
            assert_eq!(dir, PathBuf::from("/home/user/.OMT"));
        }
    }

    #[test]
    fn log_filename_is_module_name_plus_pid() {
        let path = log_path_in(Path::new("/tmp/logs"));
        let name = path.file_name().unwrap().to_string_lossy();
        assert!(
            name.ends_with(&format!("{}.log", std::process::id())),
            "expected libomtnet ModuleName+pid.log, got {name}"
        );
    }
}
