//! What a [`ThemeHost`] costs on a real theme: a cold load, a poll that
//! finds nothing, an edit (a whole recompile), and how long a reader of
//! `current()` waits while another thread compiles. p50 / p99 / max.
//!
//! ```sh
//! cargo run --release -p frame --features theme --example theme_host_bench -- \
//!     <schema file> <theme path>...
//! ```
//!
//! The schema file is a host's fields, one per line: `field name`,
//! `live name` or `layout name`.

use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use frame::theme::host::ThemeHost;
use frame::theme::runtime::Schema;

fn report(what: &str, mut times: Vec<Duration>) {
    times.sort();
    let at = |q: f64| times[((times.len() - 1) as f64 * q).round() as usize];
    println!(
        "{what:<34} n={:<6} p50 {:>10.1} µs   p99 {:>10.1} µs   max {:>10.1} µs",
        times.len(),
        at(0.5).as_secs_f64() * 1e6,
        at(0.99).as_secs_f64() * 1e6,
        times.last().unwrap().as_secs_f64() * 1e6,
    );
}

fn main() {
    let mut args = std::env::args().skip(1);
    let schema_file = args.next().expect("a schema file");
    let paths: Vec<PathBuf> = args.map(PathBuf::from).collect();
    let mut schema = Schema::new();
    for line in std::fs::read_to_string(schema_file).unwrap().lines() {
        schema = match line.split_once(' ') {
            Some(("field", n)) => schema.field(n),
            Some(("live", n)) => schema.live(n),
            Some(("layout", n)) => schema.layout(n),
            _ => schema,
        };
    }
    let host = Arc::new(ThemeHost::new(schema));
    if let Err(errors) = host.load(paths.clone(), None) {
        for e in errors {
            eprintln!("{e}");
        }
        std::process::exit(1);
    }
    let info = host.inspect();
    let bytes: u64 = info.files.iter().map(|f| f.bytes).sum();
    println!(
        "theme: {} files, {bytes} bytes, {} roles, {} tokens",
        info.files.len(),
        info.roles.len(),
        info.tokens.len()
    );

    let n = 200;
    report(
        "load (read, parse, layer, compile)",
        (0..n)
            .map(|_| {
                let t = Instant::now();
                host.load(paths.clone(), None).unwrap();
                t.elapsed()
            })
            .collect(),
    );
    report(
        "poll, nothing changed (stat)",
        (0..n * 10)
            .map(|_| {
                let t = Instant::now();
                assert!(host.poll().is_none());
                t.elapsed()
            })
            .collect(),
    );
    let file = PathBuf::from(&info.files.last().unwrap().path);
    let markup = std::fs::read_to_string(&file).unwrap();
    report(
        "edit (one file's draft, recompile)",
        (0..n)
            .map(|_| {
                let t = Instant::now();
                host.edit(&file, markup.clone()).unwrap();
                t.elapsed()
            })
            .collect(),
    );

    // A drawing thread reading `current()` while another recompiles.
    let stop = Arc::new(AtomicBool::new(false));
    let compiler = {
        let (host, stop, file, markup) = (host.clone(), stop.clone(), file.clone(), markup);
        std::thread::spawn(move || {
            let mut compiles = 0u32;
            while !stop.load(Ordering::Relaxed) {
                host.edit(&file, markup.clone()).unwrap();
                compiles += 1;
            }
            compiles
        })
    };
    let mut reads = Vec::with_capacity(200_000);
    let start = Instant::now();
    while start.elapsed() < Duration::from_secs(2) {
        let t = Instant::now();
        std::hint::black_box(host.current());
        reads.push(t.elapsed());
    }
    stop.store(true, Ordering::Relaxed);
    let compiles = compiler.join().unwrap();
    println!("(compiles on the other thread meanwhile: {compiles})");
    report("current() during compiles", reads);
}
