//! The keyboard, through the FTS keybind profile.
//!
//! The profile is the one REAPER runs on (`reaper-input`'s
//! `fasttrackstudio` — `transport.styx`, `editing.styx`,
//! `navigation.styx`, …), read by studio's `keybinds` into an
//! `architect_input::InputProcessor`, so a key means the same thing in this window
//! as it does in REAPER with the extension loaded. The bindings name
//! REAPER actions by number; [`Action`] is the subset this window can
//! do, and [`action_of`] is the table from one to the other. A bound
//! action the window cannot do yet comes back as [`Action::Unbound`]
//! with its id, so it is logged rather than silently dropped.
//!
//! Where the profile is found: `FTS_INPUT_PROFILE` (a profile
//! directory), else the copy `keybinds` compiles in (so an
//! installed app, started from anywhere, has the same keys), else a
//! built-in copy of the bindings this window needs.
//!
//! A sequence's first key (`z`, "Zoom") leaves a pending prefix, and
//! [`Keys::which_key`] lists what can follow it, with the profile's own
//! labels, for the which-key popup ([`crate::which_key`]).

use std::collections::HashMap;
use std::path::PathBuf;

use architect_input::{
    InputCommand, InputContext, InputEvent, InputProcessor, KeyChord, KeyCode, KeyTrie,
    KeymapConfig, LeafAction, ModeId, Modifiers, WhichKeyEntry, WhichKeyLabels, WhichKeyNext,
};

use crate::which_key::{Entry, WhichKey};
pub use crate::window_queue::{Action, StretchEdit, Visibility};

/// The arrangement's action for a verb every timeline shares
/// (`timeline::actions`), where it has one: the view's zooms and history,
/// the cursor's steps, snapping. The grid's division and the razor verbs
/// are the Editor's; here they stay unbound.
fn shared(id: &str) -> Option<Action> {
    use crate::zoom::Command as Z;
    use timeline::actions::{cursor, grid, view};
    let step = |vertical, inward| Action::View(Z::Step { vertical, inward });
    Some(match id {
        _ if id == view::ZOOM_IN.id => step(false, true),
        _ if id == view::ZOOM_OUT.id => step(false, false),
        _ if id == view::ROWS_IN.id => step(true, true),
        _ if id == view::ROWS_OUT.id => step(true, false),
        _ if id == view::BACK.id => Action::View(Z::Back),
        _ if id == view::FORWARD.id => Action::View(Z::Forward),
        _ if id == view::FIT.id => Action::View(Z::Project),
        _ if id == cursor::LEFT_MEASURE.id => Action::CursorBar(-1),
        _ if id == cursor::RIGHT_MEASURE.id => Action::CursorBar(1),
        // A beat is the arrangement's step: its grid is not set by key.
        _ if id == cursor::LEFT_GRID.id => Action::CursorBeat(-1),
        _ if id == cursor::RIGHT_GRID.id => Action::CursorBeat(1),
        _ if id == grid::TOGGLE.id => Action::ToggleSnap,
        _ => return None,
    })
}

/// REAPER's action ids, as the profile binds them, to what this window
/// does about them.
#[must_use]
pub fn action_of(id: &str) -> Action {
    match id {
        "40044" => Action::PlayStop,
        "40073" => Action::PlayPause,
        "40042" => Action::GoToStart,
        "40757" | "40012" => Action::SplitAtCursor,
        "FTS_QUANTIZE_DRUMS" => Action::QuantizeDrums,
        "FTS_REVERT_DRUMS" => Action::RevertDrums,
        "FTS_LEVEL_DRUMS" => Action::LevelDrums,
        "FTS_SHOW_LANES" => Action::ShowLanes,
        "FTS_COMP_NEXT" => Action::CompCycle(1),
        "FTS_COMP_PREV" => Action::CompCycle(-1),
        "FTS_COMP_BEST" => Action::CompBest,
        "FTS_COMP_KEEP" => Action::CompKeep,
        "FTS_TAKE_UPRANK" => Action::RankTake {
            by: 1,
            mouse: false,
        },
        "FTS_TAKE_DOWNRANK" => Action::RankTake {
            by: -1,
            mouse: false,
        },
        "FTS_TAKE_UPRANK_MOUSE" => Action::RankTake { by: 1, mouse: true },
        "FTS_TAKE_DOWNRANK_MOUSE" => Action::RankTake {
            by: -1,
            mouse: true,
        },
        "FTS_QUANTIZE_PANEL" => Action::QuantizePanel,
        "FTS_GAIN_STAGE" => Action::GainStage { clear: false },
        "FTS_GAIN_STAGE_CLEAR" => Action::GainStage { clear: true },
        comp if comp.starts_with("FTS_COMP_TAKE_") => comp
            .trim_start_matches("FTS_COMP_TAKE_")
            .parse()
            .map_or_else(|_| Action::Unbound(comp.to_owned()), Action::CompTake),
        "40006" | "40697" => Action::DeleteSelectedItems,
        "40035" | "40182" => Action::SelectAllItems,
        "40057" | "40698" => Action::CopyItems,
        "40058" | "42398" => Action::PasteItems,
        "40020" => Action::ClearSelection,
        "1157" => Action::ToggleSnap,
        "40838" => Action::CursorBar(-1),
        "40837" => Action::CursorBar(1),
        "40646" => Action::CursorBeat(-1),
        "40647" => Action::CursorBeat(1),
        "40285" => Action::TrackStep {
            by: 1,
            extend: false,
        },
        "40286" => Action::TrackStep {
            by: -1,
            extend: false,
        },
        "40421" => Action::TrackStep {
            by: 1,
            extend: true,
        },
        "40420" => Action::TrackStep {
            by: -1,
            extend: true,
        },
        "40172" => Action::Marker(-1),
        "40173" => Action::Marker(1),
        "1013" => Action::ToggleRecord,
        "40078" => Action::ToggleMixer,
        "40026" => Action::Save,
        "40029" => Action::Undo,
        "40030" => Action::Redo,
        "FTS_EXPRESSION_EDITOR_OPEN" => Action::ToggleEditor,
        "FTS_TOGGLE_FX" => Action::ToggleFx,
        "FTS_BROWSER" => Action::ToggleBrowser,
        // Shift+D: the window's docks. REAPER's own "Toggle docker
        // visibility" (40279, Alt+D) is REAPER's alone.
        "FTS_TOGGLE_DOCKS" => Action::ToggleDocks,
        "FTS_STEP_NEXT" => Action::BarStep(1),
        "FTS_POI_NEXT" => Action::PoiStep(1),
        "FTS_POI_PREV" => Action::PoiStep(-1),
        "FTS_POI_TOGGLE" => Action::PoiToggle,
        "_FTS_SESSION_MODE_ORGANIZE" => Action::Mode("Organize"),
        "_FTS_SESSION_MODE_WRITE" => Action::Mode("Write"),
        "_FTS_SESSION_MODE_PRODUCE" => Action::Mode("Produce"),
        "_FTS_SESSION_MODE_RECORD" => Action::Mode("Record"),
        "_FTS_SESSION_MODE_EDIT" => Action::Mode("Edit"),
        "_FTS_SESSION_MODE_MIX" => Action::Mode("Mix"),
        "_FTS_SESSION_MODE_MASTER" => Action::Mode("Master"),
        "_FTS_SESSION_MODE_LIVE" => Action::Mode("Live"),
        "_FTS_SESSION_MODE_VIDEO" => Action::Mode("Video"),
        "_FTS_SESSION_MODE_SCORING" => Action::Mode("Scoring"),
        "FTS_STEP_PREV" => Action::BarStep(-1),
        "FTS_FX_FULL" => Action::FxFull,
        "FTS_SCENE_OVERVIEW" => Action::ToggleScene("overview"),
        "FTS_SCENE_OVERVIEW_TOP" => Action::ToggleScene("overview-top"),
        "FTS_VISIBILITY_MANAGER_SHOW_ALL" => Action::Visibility(Visibility::ShowAll),
        "FTS_VISIBILITY_MANAGER_HIDE_ALL" => Action::Visibility(Visibility::HideAll),
        other => {
            if let Some(shared) = shared(other) {
                return shared;
            }
            if let Some(group) = other.strip_prefix("FTS_VISIBILITY_MANAGER_TOGGLE_") {
                return Action::Visibility(Visibility::Toggle(group.to_owned()));
            }
            crate::zoom::Command::of(other)
                .map_or_else(|| Action::Unbound(other.to_owned()), Action::View)
        }
    }
}

/// The keyboard: the processor and where its map came from.
pub struct Keys {
    processor: InputProcessor,
    context: InputContext,
    pub source: String,
    /// The which-key labels the keymap does not keep.
    labels: WhichKeyLabels,
}

impl Keys {
    /// Load the FTS profile, or fall back to the built-in bindings.
    #[must_use]
    pub fn load() -> Self {
        let (profile, source) = profile_dir()
            .and_then(|dir| {
                keybinds::load_profile_dir(&dir).map(|p| (p, dir.display().to_string()))
            })
            .or_else(|| {
                keybinds::embedded::fasttrackstudio()
                    .map(|p| (p, "fasttrackstudio (embedded)".to_owned()))
            })
            .or_else(|| {
                KeymapConfig::from_json_str(BUILTIN).ok().map(|keymap| {
                    let profile = keybinds::Profile {
                        keymap,
                        labels: HashMap::new(),
                    };
                    (profile, "built-in".to_owned())
                })
            })
            .unwrap_or_else(|| (keybinds::Profile::default(), "none".to_owned()));
        let processor =
            InputProcessor::from_config(profile.keymap).unwrap_or_else(|_| InputProcessor::new());
        Self {
            processor,
            context: InputContext::new(),
            source,
            labels: profile.labels,
        }
    }

    /// A key went down. The actions it asked for, in order — empty
    /// when the key is not bound, so the window can keep its own.
    pub fn press(&mut self, key: KeyCode, modifiers: Modifiers) -> Vec<Action> {
        let event = InputEvent::Key(architect_input::KeyEvent { key, modifiers });
        self.processor
            .process(event, &self.context)
            .iter()
            .filter_map(InputCommand::action)
            .map(|id| action_of(id))
            .collect()
    }

    /// A key came back up. Only from a real key-up: the processor keeps
    /// held keys so auto-repeat cannot walk a tree over and over, and a
    /// held prefix is sticky (hold `z`, tap `t` then `v`: both fire).
    /// `true` when the popup should go.
    pub fn release(&mut self, key: KeyCode, modifiers: Modifiers) -> bool {
        self.processor
            .notify_key_release(&KeyChord::new(key, modifiers))
    }

    /// Drop a half-typed sequence: Escape, or a held `z` used as the zoom
    /// tool rather than as a prefix.
    pub fn cancel(&mut self) {
        // The public way to drop one. It hands back the abandoned first
        // key as unhandled, which nothing here wants.
        let _ = self.processor.timeout_expired();
    }

    /// Whether a sequence is half-typed.
    #[must_use]
    pub fn is_pending(&self) -> bool {
        self.processor.pending_display().is_some()
    }

    /// What can follow the keys typed so far, for the which-key popup.
    /// `None` when nothing is pending.
    #[must_use]
    pub fn which_key(&self) -> Option<WhichKey> {
        let popup = self.processor.which_key(&self.context, &self.labels)?;
        let mut entries: Vec<Entry> = popup.entries.iter().map(entry_of).collect();
        entries.sort_by(|a, b| a.key.cmp(&b.key));
        Some(WhichKey {
            title: popup.title.unwrap_or_default(),
            typed: popup
                .typed
                .iter()
                .map(chord_label)
                .collect::<Vec<_>>()
                .join(" "),
            entries,
        })
    }
}

/// One popup row: a group is always available; a leaf is when this
/// window does what it is bound to, and reads as its label, else its id.
fn entry_of(entry: &WhichKeyEntry) -> Entry {
    let (label, available) = match &entry.next {
        WhichKeyNext::Group => (entry.label.clone().unwrap_or_default(), true),
        WhichKeyNext::Leaf(LeafAction::Action(id)) => (
            entry.label.clone().unwrap_or_else(|| id.clone()),
            !matches!(action_of(id), Action::Unbound(_)),
        ),
        WhichKeyNext::Leaf(_) => (entry.label.clone().unwrap_or_default(), false),
    };
    Entry {
        key: chord_label(&entry.chord),
        label,
        group: entry.is_group(),
        available,
    }
}

impl Keys {
    /// Every binding of the normal mode that asks for one action: the
    /// keys pressed, in order, and what they ask for. In no set order.
    #[must_use]
    pub fn bindings(&self) -> Vec<(Vec<KeyChord>, Action)> {
        let mut found = Vec::new();
        if let Some(KeyTrie::Node(root)) = self.processor.keymaps().get(&ModeId::new("normal")) {
            collect_bindings(root, &mut Vec::new(), &mut found);
        }
        found
    }
}

/// [`Keys::bindings`] under `node`, reached by `path`.
fn collect_bindings(
    node: &architect_input::TrieNode,
    path: &mut Vec<KeyChord>,
    found: &mut Vec<(Vec<KeyChord>, Action)>,
) {
    for (chord, child) in &node.children {
        path.push(chord.clone());
        match child {
            KeyTrie::Leaf(architect_input::LeafAction::Action(id)) => {
                found.push((path.clone(), action_of(id.as_str())));
            }
            KeyTrie::Node(next) => collect_bindings(next, path, found),
            KeyTrie::Leaf(_) => {}
        }
        path.pop();
    }
}

/// How a chord reads in the popup: `t`, `Shift+t`, `Ctrl+Up`.
fn chord_label(chord: &KeyChord) -> String {
    let mut s = String::new();
    for (held, name) in [
        (chord.modifiers.ctrl, "Ctrl+"),
        (chord.modifiers.alt, "Alt+"),
        (chord.modifiers.shift, "Shift+"),
        (chord.modifiers.meta, "Cmd+"),
    ] {
        if held {
            s.push_str(name);
        }
    }
    s.push_str(&match &chord.key {
        KeyCode::Character(c) => c.clone(),
        KeyCode::ArrowUp => "Up".into(),
        KeyCode::ArrowDown => "Down".into(),
        KeyCode::ArrowLeft => "Left".into(),
        KeyCode::ArrowRight => "Right".into(),
        KeyCode::F(n) => format!("F{n}"),
        other => format!("{other:?}"),
    });
    s
}

/// A profile directory named by the environment, for trying out a
/// profile being edited without rebuilding.
fn profile_dir() -> Option<PathBuf> {
    let dir = PathBuf::from(std::env::var_os("FTS_INPUT_PROFILE")?);
    dir.join("profile.styx").exists().then_some(dir)
}

/// A winit key as the processor's, with the shift folded out of the
/// character: the profile writes `Shift+e`, and the keyboard gives an
/// `E` — the chord is the letter and the modifier, not the capital.
#[must_use]
pub fn key_code(named: Option<&str>, text: Option<&str>) -> Option<KeyCode> {
    match named {
        Some("Enter") => return Some(KeyCode::Enter),
        Some("Escape") => return Some(KeyCode::Escape),
        Some("Tab") => return Some(KeyCode::Tab),
        Some("Backspace") => return Some(KeyCode::Backspace),
        Some("Delete") => return Some(KeyCode::Delete),
        Some("ArrowUp") => return Some(KeyCode::ArrowUp),
        Some("ArrowDown") => return Some(KeyCode::ArrowDown),
        Some("ArrowLeft") => return Some(KeyCode::ArrowLeft),
        Some("ArrowRight") => return Some(KeyCode::ArrowRight),
        Some(f) if f.len() <= 3 && f.starts_with('F') => {
            return f
                .get(1..)
                .and_then(|n| n.parse::<u8>().ok())
                .map(KeyCode::F);
        }
        _ => {}
    }
    let text = text?;
    let mut chars = text.chars();
    let first = chars.next()?;
    if chars.next().is_some() {
        return None;
    }
    Some(KeyCode::Character(first.to_lowercase().collect()))
}

/// The bindings this window needs, in the profile's own ids, for a
/// checkout without the profile beside it. Kept in step with
/// `fasttrackstudio/*.styx` by hand; the profile wins when it is there.
const BUILTIN: &str = r#"{
  "modes": { "normal": { "display_name": "NORMAL" } },
  "keymap": {
    "normal": {
      "Space": "40044",
      "Shift+Space": "40073",
      "Enter": "40042",
      "0": "40042",
      "s": "40757",
      "d": "40006",
      "Shift+d": "FTS_TOGGLE_DOCKS",
      "e": "FTS_EXPRESSION_EDITOR_OPEN",
      "b": "FTS_BROWSER",
      "[": "FTS_STEP_PREV",
      "]": "FTS_STEP_NEXT",
      "Shift+{": "FTS_POI_PREV",
      "Shift+}": "FTS_POI_NEXT",
      "Shift+|": "FTS_POI_TOGGLE",
      "q": "FTS_QUANTIZE_DRUMS",
      "Shift+q": "FTS_QUANTIZE_PANEL",
      "Alt+q": "FTS_REVERT_DRUMS",
      "Alt+Shift+q": "FTS_LEVEL_DRUMS",
      "Alt+l": "FTS_SHOW_LANES",
      "t": "FTS_COMP_NEXT",
      "Shift+t": "FTS_COMP_PREV",
      "Ctrl+t": "FTS_COMP_BEST",
      "Ctrl+Shift+t": "FTS_COMP_KEEP",
      "Delete": "40697",
      "Backspace": "40697",
      "Escape": "40020",
      "Ctrl+a": "40035",
      "ArrowLeft": "40838",
      "ArrowRight": "40837",
      "h": "40838",
      "l": "40837",
      "Ctrl+h": "40646",
      "Ctrl+l": "40647",
      "ArrowUp": "FTS_TAKE_UPRANK",
      "ArrowDown": "FTS_TAKE_DOWNRANK",
      "j": "40285",
      "k": "40286",
      "Shift+j": "40421",
      "Shift+k": "40420",
      ",": "40172",
      ".": "40173",
      "r": "1013"
    }
  }
}"#;

/// A rename open in the arrangement has the keyboard. Each of these flags
/// holds the one process-wide switch, `utils::keyboard`, so the dock's
/// arrange key, Signal's keys and any other global hotkey stand aside too,
/// and a field anywhere else in Studio silences the window's keys here.
static TYPING: utils::keyboard::Flag = utils::keyboard::Flag::new();

/// The chart editor has the keyboard: set when it is clicked into,
/// cleared by a click anywhere else.
static EDITING: utils::keyboard::Flag = utils::keyboard::Flag::new();

/// An overlay over the window has the keyboard: Studio's command palette,
/// open-by-name or shortcuts sheet, typed into while a song is open.
static OVERLAY: utils::keyboard::Flag = utils::keyboard::Flag::new();

/// An overlay over the window takes the keyboard (`true`) or gives it back.
/// While it has it, the window's keys and the arrangement's stand aside.
pub fn set_overlay(on: bool) {
    OVERLAY.set(on);
}

/// Whether an overlay over the window has the keyboard ([`set_overlay`]).
#[must_use]
pub fn overlay() -> bool {
    OVERLAY.get()
}

/// See [`TYPING`]. Set by the arrangement every frame it is drawn (a rename
/// open or not) — so a field that closes without saying so frees the
/// keyboard a frame later rather than never.
pub fn set_typing(on: bool) {
    TYPING.set(on);
}

/// The chart editor has the keyboard (`true`) or has lost it.
pub fn set_editing(on: bool) {
    EDITING.set(on);
}

/// Whether text is being typed anywhere in the process: a rename, the
/// chart editor, an overlay, any field holding `utils::keyboard`, or the
/// user's keys-off switch. The window's own keys stand aside while it is.
#[must_use]
pub fn typing() -> bool {
    utils::keyboard::typing()
}

/// Whether the window acts on the transport's keys itself
/// ([`use_window_transport_keys`]) — when it does, a panel must not act
/// on the ones that toggle (play/stop, record), or one press is two.
static WINDOW_TRANSPORT: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

#[must_use]
pub fn window_has_transport() -> bool {
    WINDOW_TRANSPORT.load(std::sync::atomic::Ordering::Relaxed)
}

/// The transport's keys, for the WINDOW rather than any one panel.
///
/// Space plays and stops whatever has the focus — the arrangement, the
/// mixer, the progress bar just clicked, nothing at all. A panel that keeps
/// the keymap to itself hears keys only while it has the focus, which is
/// how a click on the progress bar took the space bar away. Read through
/// the same FTS keymap as everything else, so the bindings are the
/// profile's; only play/stop and go-to-start are acted on here, and the
/// arrangement leaves those to the window (see `ArrangeEditor`'s actions)
/// so a key is not acted on twice.
#[cfg(feature = "native")]
pub fn use_window_transport_keys() {
    use std::cell::{Cell, RefCell};
    use std::rc::Rc;
    use winit::event::{ElementState, WindowEvent};
    // Space is `Key::Character(" ")` in winit 0.31, not a named key.
    use winit::keyboard::Key;

    let keys = dioxus::prelude::use_hook(|| {
        WINDOW_TRANSPORT.store(true, std::sync::atomic::Ordering::Relaxed);
        Rc::new(RefCell::new(Keys::load()))
    });
    // The window's panes, when the shell has them: `x` and `f` press them
    // here, whatever has the focus — the mixer alone has no arrangement
    // to hand its keys to.
    let mixer = dioxus::prelude::try_use_context::<crate::shell::MixerToggle>();
    let fx = dioxus::prelude::try_use_context::<crate::shell::FxToggle>();
    let editor = dioxus::prelude::try_use_context::<crate::shell::EditorPane>();
    let pins = dioxus::prelude::try_use_context::<crate::shell::Pins>();
    let held = dioxus::prelude::use_hook(|| Rc::new(Cell::new(Modifiers::NONE)));
    dioxus_native::use_window_event(move |event, _| match event {
        WindowEvent::ModifiersChanged(modifiers) => {
            let state = modifiers.state();
            held.set(Modifiers {
                ctrl: state.control_key(),
                alt: state.alt_key(),
                shift: state.shift_key(),
                meta: state.meta_key(),
            });
        }
        // The window letting go of the keyboard lets go of the keys.
        WindowEvent::Focused(false) => crate::tool::set_zoom_held(false),
        WindowEvent::KeyboardInput { event, .. } => {
            // The zoom key, by the physical key (with Shift the character
            // is "Z"), for both surfaces — see `tool::zoom_held`.
            if event.physical_key
                == winit::keyboard::PhysicalKey::Code(winit::keyboard::KeyCode::KeyZ)
            {
                crate::tool::set_zoom_held(event.state.is_pressed());
            }
            let (named, text): (Option<String>, Option<String>) = match &event.logical_key {
                Key::Named(named) => (Some(format!("{named:?}")), None),
                Key::Character(c) => (None, Some(c.to_string())),
                _ => (None, None),
            };
            let Some(code) = key_code(named.as_deref(), text.as_deref()) else {
                return;
            };
            if event.state == ElementState::Released {
                keys.borrow_mut().release(code, held.get());
                return;
            }
            if event.repeat || typing() {
                return;
            }
            // The Editor half way through one of its own sequences (`g e`,
            // `a x`): the key is its, not a pane's. This hook runs before
            // the document hands the key on, so the Editor still shows the
            // sequence pending. Only the panes stand aside — the transport
            // never does, or a sequence left pending would take Space.
            let editors = editor::paint::keys::is_pending();
            for action in keys.borrow_mut().press(code, held.get()) {
                if editors
                    && matches!(
                        action,
                        Action::ToggleMixer
                            | Action::ToggleFx
                            | Action::FxFull
                            | Action::ToggleEditor
                    )
                {
                    continue;
                }
                use crate::engine::{Move, transport};
                // In the dock the panes are its panels: the key toggles
                // the panel, as the dock bar's button does.
                // Not the docks' chord: that is the keymap's, in
                // `window::through_list`.
                if action != Action::ToggleDocks
                    && crate::window::dock_request(&action).is_some_and(crate::actions::if_docked)
                {
                    continue;
                }
                match action {
                    Action::PlayStop | Action::PlayPause => transport(Move::PlayStop, 0.0),
                    // Idempotent, so the arrangement also acting on it (it
                    // moves its edit cursor home too) does no harm. Record
                    // is not on a key yet — see `ArrangeEditor`.
                    Action::GoToStart => transport(Move::Home, 0.0),
                    Action::ToggleMixer => {
                        if let Some(mixer) = mixer {
                            mixer.press();
                        }
                    }
                    Action::ToggleEditor => {
                        if let Some(editor) = editor {
                            editor.press();
                        }
                    }
                    Action::ToggleFx => {
                        if let Some(fx) = fx {
                            fx.press();
                        }
                    }
                    Action::FxFull => {
                        if let Some(fx) = fx {
                            fx.press_full();
                        }
                    }
                    Action::ToggleBrowser => {
                        if let Some(pins) = pins {
                            use dioxus::prelude::{ReadableExt as _, WritableExt as _};
                            let mut browser = pins.browser;
                            let open = *browser.peek();
                            browser.set(!open);
                        }
                    }
                    _ => {}
                }
            }
        }
        _ => {}
    });
}
