//! What the render worker leaves behind when a plugin takes it down.
//!
//! The worker installs a handler for the fatal signals (SEGV, BUS, ILL,
//! FPE, ABRT). It writes a small report to a file the parent named before
//! the worker started, then lets the signal kill the process as it would
//! have. The report says:
//!
//! - the signal;
//! - `plugin`: what the render was processing on the crashing thread, when
//!   the render code said so ([`Processing`]);
//! - `object`: the shared object of every frame on the crashing thread's
//!   stack, faulting instruction first (`dladdr`). A CLAP bundle in that
//!   list is the plugin that crashed, whatever the render code said.
//!
//! Writing in a signal handler is limited to async-signal-safe calls:
//! `open`, `write`, `close`. `dladdr` and glibc's `backtrace` are not
//! listed as safe; they are what every crash reporter calls, and
//! `backtrace` is called once at install so that it is already loaded.
//! The parent turns the report into an answer with [`Report::parse`] and
//! [`Report::plugin`], which are plain functions with tests.

use std::cell::Cell;
use std::path::Path;

/// The longest plugin name a thread keeps for its report.
const NAME: usize = 160;

thread_local! {
    /// What this thread processes now: its name's bytes and length.
    static CURRENT: Cell<([u8; NAME], usize)> = const { Cell::new(([0; NAME], 0)) };
}

/// While it lives, this thread is processing the plugin it names: a crash
/// on this thread is reported as that plugin's. Cheap enough to hold
/// around each plugin's `process` (a copy of at most 160 bytes into a
/// thread-local, no allocation).
pub struct Processing {
    previous: ([u8; NAME], usize),
}

impl Processing {
    #[must_use]
    pub fn enter(name: &str) -> Self {
        let mut buf = [0u8; NAME];
        let bytes = name.as_bytes();
        let n = bytes.len().min(NAME);
        buf[..n].copy_from_slice(&bytes[..n]);
        let previous = CURRENT.with(|c| c.replace((buf, n)));
        Self { previous }
    }
}

impl Drop for Processing {
    fn drop(&mut self) {
        CURRENT.with(|c| c.set(self.previous));
    }
}

/// Where the handler writes: a NUL-terminated path, set once at install.
static mut REPORT_PATH: [u8; 4096] = [0; 4096];

/// Installs the handler; its report goes to `report`.
///
/// # Errors
/// The path is too long, or a handler could not be installed.
pub fn install(report: &Path) -> Result<(), String> {
    use std::os::unix::ffi::OsStrExt as _;
    let bytes = report.as_os_str().as_bytes();
    if bytes.len() >= 4095 {
        return Err("the crash report's path is too long".to_owned());
    }
    // SAFETY: written once, before any handler can read it.
    unsafe {
        let dst = (&raw mut REPORT_PATH).cast::<u8>();
        std::ptr::copy_nonoverlapping(bytes.as_ptr(), dst, bytes.len());
        dst.add(bytes.len()).write(0);
    }
    warm_backtrace();
    for sig in [
        libc::SIGSEGV,
        libc::SIGBUS,
        libc::SIGILL,
        libc::SIGFPE,
        libc::SIGABRT,
    ] {
        // SAFETY: a zeroed sigaction with our handler; the handler only
        // makes the calls the module docs list.
        let ok = unsafe {
            let mut action: libc::sigaction = std::mem::zeroed();
            action.sa_sigaction = on_fatal as *const () as usize;
            action.sa_flags = libc::SA_SIGINFO | libc::SA_ONSTACK | libc::SA_RESETHAND;
            libc::sigemptyset(&mut action.sa_mask);
            libc::sigaction(sig, &action, std::ptr::null_mut()) == 0
        };
        if !ok {
            return Err(format!("signal {sig}: no handler"));
        }
    }
    Ok(())
}

/// The frames of this thread's stack, at most `out.len()`.
#[cfg(all(target_os = "linux", target_env = "gnu"))]
fn frames(out: &mut [*mut libc::c_void]) -> usize {
    unsafe extern "C" {
        fn backtrace(buffer: *mut *mut libc::c_void, size: libc::c_int) -> libc::c_int;
    }
    // SAFETY: `out` is a valid buffer of its length.
    let n = unsafe { backtrace(out.as_mut_ptr(), out.len() as libc::c_int) };
    usize::try_from(n).unwrap_or(0)
}

#[cfg(not(all(target_os = "linux", target_env = "gnu")))]
fn frames(_out: &mut [*mut libc::c_void]) -> usize {
    0
}

fn warm_backtrace() {
    let mut buf = [std::ptr::null_mut(); 4];
    let _ = frames(&mut buf);
}

/// The faulting instruction, from the signal's context.
///
/// # Safety
/// `context` is the `ucontext_t` the kernel handed the handler.
unsafe fn fault_pc(context: *mut libc::c_void) -> usize {
    if context.is_null() {
        return 0;
    }
    #[cfg(all(target_os = "linux", target_arch = "x86_64"))]
    {
        let uc = context.cast::<libc::ucontext_t>();
        // SAFETY: as the function's contract says.
        unsafe { (*uc).uc_mcontext.gregs[libc::REG_RIP as usize] as usize }
    }
    #[cfg(all(target_os = "linux", target_arch = "aarch64"))]
    {
        let uc = context.cast::<libc::ucontext_t>();
        // SAFETY: as the function's contract says.
        unsafe { (*uc).uc_mcontext.pc as usize }
    }
    #[cfg(not(all(
        target_os = "linux",
        any(target_arch = "x86_64", target_arch = "aarch64")
    )))]
    {
        0
    }
}

/// A fixed buffer the handler writes its report into.
struct Out {
    buf: [u8; 8192],
    len: usize,
}

impl Out {
    fn push(&mut self, bytes: &[u8]) {
        let n = bytes.len().min(self.buf.len() - self.len);
        self.buf[self.len..self.len + n].copy_from_slice(&bytes[..n]);
        self.len += n;
    }

    fn number(&mut self, mut n: u64) {
        let mut digits = [0u8; 20];
        let mut i = digits.len();
        loop {
            i -= 1;
            digits[i] = b'0' + (n % 10) as u8;
            n /= 10;
            if n == 0 {
                break;
            }
        }
        self.push(&digits[i..]);
    }

    /// The shared object `addr` is in, as a line `object=<path>`.
    fn object(&mut self, addr: usize) {
        if addr == 0 {
            return;
        }
        // SAFETY: dladdr fills `info` or returns 0; the name it gives is
        // a NUL-terminated string the loader owns.
        unsafe {
            let mut info: libc::Dl_info = std::mem::zeroed();
            if libc::dladdr(addr as *const libc::c_void, &mut info) == 0 || info.dli_fname.is_null()
            {
                return;
            }
            let name = std::ffi::CStr::from_ptr(info.dli_fname).to_bytes();
            self.push(b"object=");
            self.push(name);
            self.push(b"\n");
        }
    }
}

extern "C" fn on_fatal(sig: libc::c_int, _info: *mut libc::siginfo_t, context: *mut libc::c_void) {
    let mut out = Out {
        buf: [0; 8192],
        len: 0,
    };
    out.push(b"signal=");
    out.number(u64::try_from(sig).unwrap_or(0));
    out.push(b"\n");
    let (name, n) = CURRENT.with(Cell::get);
    if n > 0 {
        out.push(b"plugin=");
        out.push(&name[..n]);
        out.push(b"\n");
    }
    // SAFETY: the kernel's context for this signal.
    out.object(unsafe { fault_pc(context) });
    let mut stack = [std::ptr::null_mut(); 48];
    let n = frames(&mut stack);
    for frame in &stack[..n] {
        out.object(*frame as usize);
    }
    // SAFETY: REPORT_PATH is NUL-terminated (install); open/write/close are
    // async-signal-safe.
    unsafe {
        let path = (&raw const REPORT_PATH).cast::<libc::c_char>();
        let fd = libc::open(path, libc::O_WRONLY | libc::O_CREAT | libc::O_TRUNC, 0o600);
        if fd >= 0 {
            let _ = libc::write(fd, out.buf.as_ptr().cast(), out.len);
            libc::close(fd);
        }
    }
    // SA_RESETHAND put the default action back: returning re-runs the
    // fault (or `abort` raises again), and the process dies of the signal.
}

/// A crash report, read back by the parent.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Report {
    pub signal: Option<i32>,
    /// What the render code said the thread was processing.
    pub processing: Option<String>,
    /// The shared objects on the crashing thread's stack, innermost first.
    pub objects: Vec<String>,
}

impl Report {
    /// The report's text.
    #[must_use]
    pub fn parse(text: &str) -> Self {
        let mut out = Self::default();
        for line in text.lines() {
            if let Some(v) = line.strip_prefix("signal=") {
                out.signal = v.trim().parse().ok();
            } else if let Some(v) = line.strip_prefix("plugin=") {
                out.processing = Some(v.to_owned());
            } else if let Some(v) = line.strip_prefix("object=")
                && !out.objects.iter().any(|o| o == v)
            {
                out.objects.push(v.to_owned());
            }
        }
        out
    }

    /// The plugin that crashed, as best known: the innermost plugin
    /// bundle on the stack (`.clap`, `.vst3`, `.so` under one), named by
    /// its bundle; else what the render said it was processing.
    #[must_use]
    pub fn plugin(&self) -> Option<Plugin> {
        if let Some(file) = self.objects.iter().find(|o| is_plugin_object(o)) {
            return Some(Plugin {
                name: self.processing.clone().unwrap_or_else(|| bundle_name(file)),
                file: Some(file.clone()),
            });
        }
        self.processing
            .clone()
            .map(|name| Plugin { name, file: None })
    }

    /// The signal's name (`SIGSEGV`).
    #[must_use]
    pub fn signal_name(&self) -> Option<&'static str> {
        Some(match self.signal? {
            libc::SIGSEGV => "SIGSEGV",
            libc::SIGBUS => "SIGBUS",
            libc::SIGILL => "SIGILL",
            libc::SIGFPE => "SIGFPE",
            libc::SIGABRT => "SIGABRT",
            libc::SIGKILL => "SIGKILL",
            _ => return None,
        })
    }
}

/// A plugin a crash is laid at.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Plugin {
    pub name: String,
    /// The shared object that faulted, when the stack named one.
    pub file: Option<String>,
}

/// Whether `object` is a plugin's binary rather than the engine's, the
/// system's or a toolchain library.
#[must_use]
pub fn is_plugin_object(object: &str) -> bool {
    let lower = object.to_ascii_lowercase();
    lower.ends_with(".clap")
        || lower.contains(".clap/")
        || lower.contains(".vst3/")
        || lower.ends_with(".vst3")
        || lower.contains("/.clap/")
        || lower.contains("/clap/")
}

/// A bundle's name from a binary's path: `Signal` for
/// `/home/u/.clap/Signal.clap` or `…/Foo.vst3/Contents/x86_64-linux/Foo.so`.
#[must_use]
pub fn bundle_name(object: &str) -> String {
    let path = Path::new(object);
    for part in path.ancestors() {
        let Some(name) = part.file_name().and_then(|n| n.to_str()) else {
            continue;
        };
        for ext in [".clap", ".vst3"] {
            if let Some(stem) = name.strip_suffix(ext) {
                return stem.to_owned();
            }
        }
    }
    path.file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or(object)
        .to_owned()
}
