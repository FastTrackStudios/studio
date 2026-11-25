# The expression editor's own harness (crates/studio-connect/views/midi/
# expression-editor, package `editor`): `just editor <recipe>`. The stress
# and benchmark runs, and what they measure: scripts/ui-stress/GUIDE.md.

set working-directory := ".."
set shell := ["bash", "scripts/dev-shell.sh", "-c"]
set script-interpreter := ["bash", "scripts/dev-shell.sh"]

# The editor in a window you keep open, with rsx! hot reload (restart it after an rsx! edit that adds or removes a node: Blitz reports `invalid key`)
serve *ARGS:
    dx serve -p editor --features standalone,ui --example serve --platform desktop --renderer native {{ARGS}}

# One window on a file (no hot reload, no chooser)
window SOURCE="phrase" *ARGS:
    cargo run -p editor --features standalone,ui --example editor -- {{SOURCE}} {{ARGS}}

# The workstation on a real song, served (SONG: set-in-stone or unbreakable; FRESH=true stages a throwaway copy)
[script]
practice SONG="set-in-stone" FRESH="false":
    staging=(--cached); [[ "{{FRESH}}" == true ]] && staging=()
    project=$(cargo run -p editor --features standalone --example practice -- "${staging[@]}" "{{SONG}}")
    mkdir -p target
    EXPRESSION_EDITOR_ARGS="'$project' --drums --size 1600x900" \
        dx serve -p editor --features standalone --example workstation \
        --platform desktop --renderer native 2>&1 | tee target/ee-practice.log

# The same workstation in a WebView (dioxus-desktop): real CSS and devtools. PROBE=1 records render events for the dioxus MCP
[script]
webview SONG="set-in-stone" FRESH="false" PROBE="0":
    staging=(--cached); [[ "{{FRESH}}" == true ]] && staging=()
    project=$(cargo run -p editor --features standalone --example practice -- "${staging[@]}" "{{SONG}}")
    mkdir -p target
    FTS_DIOXUS_PROBE="{{PROBE}}" \
    RUST_LOG="${RUST_LOG:-warn,expression_editor_ui::frame_meter=info}" \
    EXPRESSION_EDITOR_ARGS="'$project' --drums --size 1600x900" \
        dx serve -p editor --example webview --platform desktop --features standalone,webview 2>&1 | tee target/ee-webview.log

# Stage the practice projects without a window; prints their paths
[script]
practice-prepare SONG="both" FRESH="false":
    staging=(--cached); [[ "{{FRESH}}" == true ]] && staging=()
    cargo run -p editor --features standalone --example practice -- "${staging[@]}" "{{SONG}}"

# The headless full-workstation stress run, on fresh copies (the run of record)
[script]
stress SONG="set-in-stone" FRAMES="120" PROFILE="dev" ENFORCE="false":
    cargo build -p editor --features standalone --example stress --example practice --profile "{{PROFILE}}"
    out="${CARGO_TARGET_DIR:-target}/$([[ "{{PROFILE}}" == dev ]] && echo debug || echo "{{PROFILE}}")/examples"
    project=$("$out/practice" "{{SONG}}")
    report=$(mktemp -d -t fts-ui-stress-XXXXXX)
    options=(); [[ "{{ENFORCE}}" == true ]] && options+=(--enforce)
    printf 'Practice project: %s\nReport directory: %s\n' "$project" "$report"
    RUST_BACKTRACE=1 FTS_STRESS_FRAMES="{{FRAMES}}" FTS_STRESS_PROFILE="{{PROFILE}}" \
        "$out/stress" "$project" --drums --size 1600x900 --out "$report" 2>&1 | tee "$report/run.log"
    python3 scripts/ui-stress/run.py "$report" "${options[@]}"

# The fast loop: the cached staging, optionally one PHASE (a signal, not a result: confirm with `stress`)
[script]
bench SONG="set-in-stone" FRAMES="120" PHASE="all_panels" PROFILE="release":
    cargo build -p editor --features standalone --example stress --example practice --profile "{{PROFILE}}"
    out="${CARGO_TARGET_DIR:-target}/$([[ "{{PROFILE}}" == dev ]] && echo debug || echo "{{PROFILE}}")/examples"
    project=$("$out/practice" --cached "{{SONG}}")
    report=$(mktemp -d -t fts-ui-bench-XXXXXX)
    printf 'Practice project: %s\nReport directory: %s\n' "$project" "$report"
    RUST_BACKTRACE=1 FTS_STRESS_FRAMES="{{FRAMES}}" FTS_STRESS_PROFILE="{{PROFILE}}" FTS_STRESS_PHASE="{{PHASE}}" \
        "$out/stress" "$project" --drums --size 1600x900 --out "$report" 2>&1 | tee "$report/run.log"
    python3 scripts/ui-stress/run.py "$report"

# Two benchmark runs side by side, with their load averages
bench-compare BASELINE CANDIDATE:
    python3 scripts/ui-stress/compare.py {{BASELINE}} {{CANDIDATE}}

# The frame rate the WebView presented at (its `ui.fps` lines), worst frame included
[script]
fps LINES="20":
    log=target/ee-webview.log
    [[ -f "$log" ]] || { echo "no $log: run just editor webview first" >&2; exit 1; }
    sed -E 's/\x1b\[[0-9;]*[a-zA-Z]//g' "$log" \
      | grep -o 'ui\.fps=[0-9.]*  *ui\.worst_frame_ms=[0-9.]*' \
      | sed -E 's/ui\.fps=([0-9.]*)  *ui\.worst_frame_ms=([0-9.]*)/\1 \2/' \
      | awk '{ printf "%6.1f fps   worst %6.1f ms\n", $1, $2 }' | tail -n "{{LINES}}"

# What the last practice or webview window logged, repeated warnings collapsed (WHICH: webview or practice)
[script]
log WHICH="webview" LINES="40":
    log="target/ee-{{WHICH}}.log"
    [[ -f "$log" ]] || { echo "no $log: run just editor {{WHICH}} first" >&2; exit 1; }
    echo "REPEATED (count, message):"
    sed -E 's/\x1b\[[0-9;]*[a-zA-Z]//g; s/^[0-9]{2}:[0-9]{2}:[0-9]{2}//; s/^\s*\[[a-z]+\]\s*//' "$log" \
        | grep -aE "WARN|ERROR|panic" | sort | uniq -c | sort -rn | head -15 || echo "  (none)"
    echo; echo "LAST {{LINES}} LINES:"; tail -n "{{LINES}}" "$log"

# The WebView on a private X display a script can drive and capture (scripts/ui-stress/GUIDE.md)
vdisplay CMD="run" NAME="run":
    scripts/ui-stress/webview-display.sh "{{CMD}}" "{{NAME}}"

# Stop the private display and anything left on it
vdisplay-stop:
    pkill -9 -x webview || true
    scripts/ui-stress/webview-display.sh stop
