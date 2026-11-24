//! The flight recorder, and the Logs view's side of it.
//!
//! A `tracing` layer ([`RingLayer`]) keeps the last lines in a bounded
//! ring, and one panic hook per process ([`install_panic_hook`]) logs the
//! panic through `tracing` (so it is in the ring) and leaves two files in
//! the app's data folder (`<data dir>/<app>/`):
//!
//! - `crash.log` ([`crash_log`]): each panic, its backtrace and what was
//!   logged just before it, appended;
//! - `flight-log.txt` ([`flight_log`]): the ring itself, written on a
//!   panic and whenever the app calls [`persist`], so a run the system
//!   kills without a panic still leaves its last lines; [`restore_previous`]
//!   puts them on the Logs view at the next launch.
//!
//! A [`snapshot`] the Logs view can call in the browser too, where there is
//! no recorder and it is empty.
//!
//! Adapted from Architect's flight recorder (MIT OR Apache-2.0).

#[cfg(not(target_arch = "wasm32"))]
pub use native::*;

#[cfg(not(target_arch = "wasm32"))]
mod native {
    use std::collections::VecDeque;
    use std::fmt::Write as _;
    use std::io::Write as _;
    use std::path::{Path, PathBuf};
    use std::sync::{Mutex, OnceLock};
    use std::time::{Instant, SystemTime, UNIX_EPOCH};

    /// How many lines the ring keeps; the oldest drop first.
    pub const CAPACITY: usize = 600;
    /// The crash log's file name inside the app dir.
    pub const CRASH_LOG: &str = "crash.log";
    /// The flight log's file name inside the app dir.
    pub const FLIGHT_LOG: &str = "flight-log.txt";
    /// Where [`restore_previous`] moves the flight log it read.
    pub const FLIGHT_LOG_PREVIOUS: &str = "flight-log-previous.txt";
    /// Past this size `crash.log` starts over: the newest crashes are read.
    const CRASH_LOG_KEEP: u64 = 512 * 1024;
    /// Lines of the ring copied under each crash in `crash.log`.
    const CRASH_TAIL: usize = 40;
    /// Lines of the previous flight log [`restore_previous`] puts back.
    const RESTORE_TAIL: usize = 200;

    static RING: Mutex<VecDeque<String>> = Mutex::new(VecDeque::new());
    /// The app dir [`install_panic_hook`] was given; set once.
    static APP_DIR: OnceLock<PathBuf> = OnceLock::new();

    /// Append a line to the ring, dropping the oldest past [`CAPACITY`].
    pub fn push(line: String) {
        if let Ok(mut ring) = RING.lock() {
            while ring.len() >= CAPACITY {
                ring.pop_front();
            }
            ring.push_back(line);
        }
    }

    /// The ring's lines, oldest first.
    #[must_use]
    pub fn snapshot() -> Vec<String> {
        RING.lock()
            .map(|ring| ring.iter().cloned().collect())
            .unwrap_or_default()
    }

    /// The last `n` lines of the ring, oldest first.
    fn tail(n: usize) -> Vec<String> {
        let lines = snapshot();
        let skip = lines.len().saturating_sub(n);
        lines.into_iter().skip(skip).collect()
    }

    fn app_dir() -> Option<&'static Path> {
        APP_DIR.get().map(PathBuf::as_path)
    }

    /// `<app dir>/crash.log`, once [`install_panic_hook`] ran.
    #[must_use]
    pub fn crash_log() -> Option<PathBuf> {
        app_dir().map(|dir| dir.join(CRASH_LOG))
    }

    /// `<app dir>/flight-log.txt`, once [`install_panic_hook`] ran.
    #[must_use]
    pub fn flight_log() -> Option<PathBuf> {
        app_dir().map(|dir| dir.join(FLIGHT_LOG))
    }

    /// Install the flight recorder's panic hook for `app` (`Session`,
    /// `Signal`), chained onto the hook before it: the panic is logged
    /// with `tracing::error!` (into the ring, through a [`RingLayer`]),
    /// appended to `<data dir>/<app>/crash.log` with a backtrace and the
    /// lines before it, the ring written to `flight-log.txt`, and then the
    /// previous hook runs.
    ///
    /// Call it once per process, after the subscriber with its
    /// [`RingLayer`]; a second call installs nothing. `false` when it was
    /// already installed or this platform has no data folder.
    #[allow(clippy::must_use_candidate)]
    pub fn install_panic_hook(app: &str) -> bool {
        let Some(dir) = dirs::data_dir() else {
            return false;
        };
        if APP_DIR.set(dir.join(app)).is_err() {
            return false;
        }
        let previous = std::panic::take_hook();
        std::panic::set_hook(Box::new(move |info| {
            let thread = std::thread::current();
            let thread = thread.name().unwrap_or("<unnamed>");
            let what = info.to_string();
            tracing::error!(panic.thread = thread, panic.what = %what, "panic");
            record_crash(thread, &what);
            persist();
            previous(info);
        }));
        true
    }

    /// Append one crash to `crash.log`: when, where, the panic, a backtrace
    /// and the lines logged just before it.
    fn record_crash(thread: &str, what: &str) {
        let Some(path) = crash_log() else { return };
        if let Some(dir) = path.parent() {
            let _ = std::fs::create_dir_all(dir);
        }
        if std::fs::metadata(&path).is_ok_and(|meta| meta.len() > CRASH_LOG_KEEP) {
            let _ = std::fs::remove_file(&path);
        }
        let Ok(mut file) = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&path)
        else {
            return;
        };
        let when = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_or(0, |since| since.as_secs());
        let backtrace = std::backtrace::Backtrace::force_capture();
        let before = tail(CRASH_TAIL).join("\n");
        let _ = writeln!(
            file,
            "=== crash at unix {when} on thread {thread} ===\n{what}\n\n{backtrace}\n--- logged before it ---\n{before}\n",
        );
    }

    /// Write the ring to `flight-log.txt`, so a run that ends without
    /// warning still leaves its last lines. Nothing before
    /// [`install_panic_hook`].
    pub fn persist() {
        let Some(path) = flight_log() else { return };
        if let Some(dir) = path.parent() {
            let _ = std::fs::create_dir_all(dir);
        }
        let _ = std::fs::write(path, snapshot().join("\n"));
    }

    /// At launch: put the previous run's last lines back into the ring,
    /// under a marker, and move the flight log aside to
    /// `flight-log-previous.txt`. How many lines came back.
    #[allow(clippy::must_use_candidate)]
    pub fn restore_previous() -> usize {
        let Some(path) = flight_log() else { return 0 };
        let Ok(text) = std::fs::read_to_string(&path) else {
            return 0;
        };
        let lines: Vec<&str> = text.lines().collect();
        let skip = lines.len().saturating_sub(RESTORE_TAIL);
        let restored = lines.len().saturating_sub(skip);
        push("──── the previous run's last lines ────".to_owned());
        for line in lines.into_iter().skip(skip) {
            push(format!("prev| {line}"));
        }
        push("──── this run ────".to_owned());
        let _ = std::fs::rename(&path, path.with_file_name(FLIGHT_LOG_PREVIOUS));
        restored
    }

    /// The capture layer: every event becomes one ring line,
    /// `[seconds since the layer was made] LEVEL target: message fields`.
    pub struct RingLayer {
        start: Instant,
    }

    impl RingLayer {
        #[must_use]
        pub fn new() -> Self {
            Self {
                start: Instant::now(),
            }
        }
    }

    impl Default for RingLayer {
        fn default() -> Self {
            Self::new()
        }
    }

    /// Collects an event's message and fields into one line.
    struct LineVisitor<'a>(&'a mut String);

    impl tracing::field::Visit for LineVisitor<'_> {
        fn record_debug(&mut self, field: &tracing::field::Field, value: &dyn std::fmt::Debug) {
            if field.name() == "message" {
                let _ = write!(self.0, "{value:?} ");
            } else {
                let _ = write!(self.0, "{}={value:?} ", field.name());
            }
        }
    }

    impl<S: tracing::Subscriber> tracing_subscriber::Layer<S> for RingLayer {
        fn on_event(
            &self,
            event: &tracing::Event<'_>,
            _ctx: tracing_subscriber::layer::Context<'_, S>,
        ) {
            let mut message = String::new();
            event.record(&mut LineVisitor(&mut message));
            let meta = event.metadata();
            push(format!(
                "[{:9.3}] {:5} {}: {}",
                self.start.elapsed().as_secs_f64(),
                meta.level().as_str(),
                meta.target(),
                message.trim_end(),
            ));
        }
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        #[test]
        fn ring_keeps_the_newest_capacity_lines() {
            let total = CAPACITY.saturating_add(25);
            for i in 0..total {
                push(format!("ring-test {i}"));
            }
            let lines = snapshot();
            assert_eq!(lines.len(), CAPACITY);
            assert_eq!(lines.first().map(String::as_str), Some("ring-test 25"));
            assert_eq!(
                lines.last().cloned(),
                Some(format!("ring-test {}", total.saturating_sub(1)))
            );
            assert_eq!(tail(3).len(), 3);
        }
    }
}

/// In the browser nothing records: the Logs view shows nothing.
#[cfg(target_arch = "wasm32")]
#[must_use]
pub fn snapshot() -> Vec<String> {
    Vec::new()
}
