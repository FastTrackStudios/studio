//! The production (non-test) theme host reloads without blocking a window.

use std::path::PathBuf;
use std::time::{Duration, Instant};

#[test]
fn file_reload_publishes_on_the_background_worker() {
    let themes = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("themes");
    let temporary =
        std::env::temp_dir().join(format!("studio-theme-worker-{}", std::process::id()));
    std::fs::create_dir_all(&temporary).unwrap();
    for entry in std::fs::read_dir(themes.join("workbench")).unwrap() {
        let path = entry.unwrap().path();
        std::fs::copy(&path, temporary.join(path.file_name().unwrap())).unwrap();
    }
    session_ui::skin::load(vec![themes.join("console"), temporary.clone()], None).unwrap();
    let before = session_ui::skin::version();
    let tokens = temporary.join("tokens.frame");
    let original = std::fs::read_to_string(&tokens).unwrap();
    assert!(original.contains("#4fc3a1"));
    std::fs::write(&tokens, original.replace("#4fc3a1", "#ff00ff")).unwrap();
    let deadline = Instant::now() + Duration::from_secs(10);
    while session_ui::skin::version() == before && Instant::now() < deadline {
        session_ui::skin::poll();
        std::thread::sleep(Duration::from_millis(20));
    }
    assert!(
        session_ui::skin::version() > before,
        "reload was not published"
    );
    assert_eq!(
        session_ui::skin::loaded()
            .unwrap()
            .token("accent")
            .and_then(|v| v.color())
            .map(|c| c.to_rgba8()),
        Some([255, 0, 255, 255]),
    );
    std::fs::remove_dir_all(&temporary).unwrap();
}
