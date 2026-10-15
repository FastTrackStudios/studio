//! The takes of a real song's kit ranked for a stretch (`COMP_CANDIDATES`),
//! on a copy of it.
//!
//! ```sh
//! FTS_PERF_SESSION=/path/to/a/copy/of/Unbreakable.session FTS_COMP_SPAN=100,104 [FTS_COMP_DETECT=1] \
//!   cargo nextest run --cargo-profile release-fast -p session-daw --features signal \
//!   --test comp_real --run-ignored all
//! ```

use std::path::PathBuf;

#[test]
#[ignore = "reads a real session; set FTS_PERF_SESSION"]
fn a_real_songs_takes_are_ranked() {
    let Some(path) = std::env::var_os("FTS_PERF_SESSION").map(PathBuf::from) else {
        return;
    };
    let (from, to) = std::env::var("FTS_COMP_SPAN")
        .ok()
        .and_then(|s| {
            let (a, b) = s.split_once(',')?;
            Some((a.parse::<f64>().ok()?, b.parse::<f64>().ok()?))
        })
        .unwrap_or((100.0, 104.0));
    session_daw::open::open_silent(&path).expect("the session opens");
    // FTS_COMP_DETECT: the hits found on every take over the stretch first
    // (HITS_DETECT with all_takes), so every take can be judged.
    if std::env::var_os("FTS_COMP_DETECT").is_some() {
        session_daw::actions::list()
            .run_json(
                "HITS_DETECT",
                serde_json::json!({ "from": from, "to": to, "all_takes": true })
                    .to_string()
                    .as_bytes(),
            )
            .expect("detected");
    }
    let said = session_daw::actions::list()
        .run_json(
            "COMP_CANDIDATES",
            serde_json::json!({ "from": from, "to": to })
                .to_string()
                .as_bytes(),
        )
        .expect("ranked");
    let ranked: serde_json::Value = serde_json::from_slice(&said).expect("json");
    let takes = ranked[0]["takes"].as_array().cloned().unwrap_or_default();
    assert!(takes.len() >= 2, "a kit recorded more than once: {ranked}");
    assert!(
        takes.iter().filter(|t| t["current"] == true).count() <= 1,
        "one take plays there at most: {ranked}"
    );
    let judged: Vec<f64> = takes
        .iter()
        .filter(|t| t["undetected"] == false && t["empty"] == false)
        .filter_map(|t| t["score"].as_f64())
        .collect();
    assert!(
        judged.windows(2).all(|w| w[0] >= w[1]),
        "the judged best first: {ranked}"
    );
    if std::env::var_os("FTS_COMP_DETECT").is_some() {
        assert!(
            judged.len() >= 2,
            "every take judged once detected: {ranked}"
        );
        assert!(std::env::var_os("FTS_COMP_SHOW").is_none(), "{ranked:#}");
    }
}
