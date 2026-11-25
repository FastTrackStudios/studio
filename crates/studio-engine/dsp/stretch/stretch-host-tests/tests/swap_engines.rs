use std::f32::consts::TAU;
use std::path::PathBuf;
use stretch_abi::SliceSource;
use stretch_abi::host::EngineRegistry;

/// The cdylibs land in the profile directory the test binary sits under
/// (`<target>/<profile>/deps/<test>`), whatever the target dir and profile.
fn plugin_path(stem: &str) -> PathBuf {
    let exe = std::env::current_exe().expect("test binary path");
    let target_dir = exe
        .parent()
        .and_then(std::path::Path::parent)
        .expect("test binary under <target>/<profile>/deps");
    let filename = if cfg!(target_os = "macos") {
        format!("lib{stem}.dylib")
    } else if cfg!(target_os = "windows") {
        format!("{stem}.dll")
    } else {
        format!("lib{stem}.so")
    };
    target_dir.join(filename)
}

fn sine_wave(sr: f32, hz: f32, frames: usize) -> Vec<f32> {
    (0..frames)
        .map(|i| 0.5 * (TAU * hz * (i as f32) / sr).sin())
        .collect()
}

fn rms(samples: &[f32]) -> f32 {
    if samples.is_empty() {
        return 0.0;
    }
    let sum: f32 = samples.iter().map(|s| s * s).sum();
    (sum / samples.len() as f32).sqrt()
}

#[test]
#[ignore = "needs the stretch-pv and stretch-wsola cdylibs, which cargo test does not build: `cargo build -p stretch-pv -p stretch-wsola` with the same profile first, then run with --ignored"]
fn dlopen_and_hot_swap_pv_and_wsola() {
    let pv_so = plugin_path("stretch_pv");
    let wsola_so = plugin_path("stretch_wsola");
    assert!(
        pv_so.exists(),
        "build workspace first so {} exists",
        pv_so.display()
    );
    assert!(
        wsola_so.exists(),
        "build workspace first so {} exists",
        wsola_so.display()
    );

    let mut registry = EngineRegistry::new();
    unsafe {
        registry.register(&pv_so).expect("load stretch_pv");
        registry.register(&wsola_so).expect("load stretch_wsola");
    }

    let ids: Vec<&str> = registry.list().iter().map(|m| m.id.as_str()).collect();
    assert!(ids.contains(&"fts.stretch.pv"));
    assert!(ids.contains(&"fts.stretch.wsola"));

    let sr = 48_000.0;
    let signal = sine_wave(sr as f32, 440.0, 48_000);
    let mut source = SliceSource {
        left: &signal,
        right: &signal,
    };

    // Hot-swap across every registered engine on the exact same stretch+pitch job.
    for id in ["fts.stretch.pv", "fts.stretch.wsola"] {
        let plugin = registry.get(id).expect("engine registered");
        let mut voice = plugin.create_voice(sr).expect("create voice");
        assert_eq!(voice.engine_id(), id);

        voice.seek(4800.0, 0.8, 3.0, &mut source);
        let mut out_l = vec![0.0_f32; 8192];
        let mut out_r = vec![0.0_f32; 8192];

        // Render in realistic 512-frame audio blocks.
        for chunk in 0..(8192 / 512) {
            let start = chunk * 512;
            let end = start + 512;
            voice.render(
                &mut out_l[start..end],
                &mut out_r[start..end],
                0.8,
                3.0,
                &mut source,
            );
        }

        let level_l = rms(&out_l[1024..]);
        let level_r = rms(&out_r[1024..]);
        assert!(
            level_l > 0.15 && level_l < 0.7,
            "{id}: unexpected L rms {level_l}"
        );
        assert!(
            (level_l - level_r).abs() < 0.02,
            "{id}: stereo imbalance L={level_l} R={level_r}"
        );
        assert!(
            out_l.iter().all(|s| s.is_finite()),
            "{id}: non-finite sample produced"
        );
    }
}
