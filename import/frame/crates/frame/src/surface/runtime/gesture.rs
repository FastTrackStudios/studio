//! Pointer/keyboard gestures → value edits, one state machine per control
//! kind. Pure: events in, [`Output`]s out, time passed explicitly — so the
//! interaction contract (the `surface` spec) is unit-testable and identical
//! on every host (desktop, plugin window, iPad, browser).

use crate::surface::proto::{ButtonMode, ControlKind, EditPhase, Orientation};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Modifiers {
    /// Fine adjustment (Shift).
    pub fine: bool,
    /// Alternate action (Cmd/Ctrl-click resets, like double-click).
    pub alt: bool,
    /// The Alt / Option key (drag and wheel modes, chord clicks).
    pub option: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Key {
    Up,
    Down,
    Left,
    Right,
    PageUp,
    PageDown,
    Home,
    End,
    Space,
    Enter,
    Escape,
    Tab,
    Backspace,
    Delete,
    /// A character key (with Cmd/Ctrl: a shortcut, e.g. select all).
    Char(char),
}

/// Input to a control. Coordinates are in surface pixels; `t` is ms.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Input {
    Down {
        x: f64,
        y: f64,
        t: f64,
        mods: Modifiers,
    },
    Move {
        x: f64,
        y: f64,
        t: f64,
        mods: Modifiers,
    },
    Up {
        x: f64,
        y: f64,
        t: f64,
    },
    /// Pointer capture lost (window blur, touch cancel): ends the gesture.
    Cancel {
        t: f64,
    },
    /// Wheel notches; positive = up/away = increase.
    Wheel {
        notches: f64,
        mods: Modifiers,
    },
    DoubleClick,
    Key {
        key: Key,
        mods: Modifiers,
    },
    /// Time passing with no input (drives footswitch holds).
    Tick {
        t: f64,
    },
}

/// What a control asks the runtime to do.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Output {
    /// Set the control's position (0..1).
    Set { position: f64, phase: EditPhase },
    /// Set an XY pad's two positions.
    SetXy { x: f64, y: f64, phase: EditPhase },
    /// Return to the default position.
    Reset,
    /// Fire the bound action.
    Fire,
    /// Footswitch: short press released.
    Tap,
    /// Footswitch: held past the threshold (fires once, while still down).
    Hold,
    /// Footswitch: released after a hold.
    HoldRelease,
    /// Handle: turn the wheel param by `delta` (position units).
    Nudge { delta: f64 },
}

/// Wheel step of a handle's wheel param (an EQ band's Q), per notch.
pub const HANDLE_WHEEL_STEP: f64 = 0.02;

/// Drag distance for a full 0→1 knob sweep (the spec's sensitivity constant).
pub const KNOB_FULL_SWEEP_PX: f64 = 200.0;
/// Fine-modifier scale for drags and wheel.
pub const FINE: f64 = 0.1;
pub const WHEEL_STEP: f64 = 0.01;
pub const KEY_STEP: f64 = 0.01;
pub const PAGE_STEP: f64 = 0.1;

/// The control's geometry, needed by position-based gestures (faders, XY).
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Bounds {
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
}

#[derive(Debug, Clone, Copy, PartialEq)]
enum Drag {
    Idle,
    /// Relative drag from `(x, y)` where the position was `start`.
    Dragging {
        last_x: f64,
        last_y: f64,
        position: f64,
    },
    /// Footswitch pressed at `t0`; `held` once the hold fired.
    Pressed {
        t0: f64,
        held: bool,
    },
    /// Button pressed (for toggle-on-release / momentary).
    Button,
    /// Handle drag: unit point `(u, v)` after the last event at `(x, y)`.
    Point {
        last_x: f64,
        last_y: f64,
        u: f64,
        v: f64,
    },
}

/// One control's interaction state.
#[derive(Debug, Clone, PartialEq)]
pub struct Gesture {
    kind: ControlKind,
    drag: Drag,
}

impl Gesture {
    #[must_use]
    pub fn new(kind: ControlKind) -> Self {
        Self {
            kind,
            drag: Drag::Idle,
        }
    }

    #[must_use]
    pub fn is_active(&self) -> bool {
        !matches!(self.drag, Drag::Idle)
    }

    /// Feed one input. `position` is the control's current position (0..1),
    /// `bounds` its on-screen box.
    pub fn handle(&mut self, input: Input, position: f64, bounds: Bounds) -> Vec<Output> {
        match self.kind.clone() {
            ControlKind::Knob { .. } => self.knob(input, position),
            ControlKind::Fader { orientation } => self.fader(input, position, bounds, orientation),
            ControlKind::Button { mode } => self.button(input, position, bounds, mode),
            ControlKind::Footswitch { hold_ms } => self.footswitch(input, f64::from(hold_ms)),
            ControlKind::XyPad => self.xy(input, bounds),
            ControlKind::Selector => self.selector(input, position),
            // Handles need their point and area: see `point`.
            ControlKind::Meter { .. }
            | ControlKind::Gauge { .. }
            | ControlKind::Readout
            | ControlKind::Plot { .. }
            | ControlKind::Curve { .. }
            | ControlKind::Handle { .. }
            | ControlKind::Repeat { .. }
            | ControlKind::Canvas { .. }
            | ControlKind::Popover { .. }
            | ControlKind::Menu { .. }
            | ControlKind::TextField { .. } => Vec::new(),
        }
    }

    /// A handle: drag moves its unit point `(u, v)` (`v` up) by the
    /// pointer's travel over `area` (its parent) — relative, so grabbing it
    /// off-centre doesn't jump — Shift for fine; the wheel nudges its wheel
    /// param; double-click (or Alt-press) resets; arrows step it.
    pub fn point(&mut self, input: Input, (u, v): (f64, f64), area: Bounds) -> Vec<Output> {
        let (w, h) = (area.width.max(1.0), area.height.max(1.0));
        let key_step = |mods: Modifiers| if mods.fine { KEY_STEP * FINE } else { KEY_STEP };
        let at = |u: f64, v: f64, phase| Output::SetXy {
            x: u.clamp(0.0, 1.0),
            y: v.clamp(0.0, 1.0),
            phase,
        };
        match (input, self.drag) {
            (Input::Down { mods, .. }, _) if mods.alt => vec![Output::Reset],
            (Input::Down { x, y, .. }, _) => {
                self.drag = Drag::Point {
                    last_x: x,
                    last_y: y,
                    u,
                    v,
                };
                vec![at(u, v, EditPhase::Begin)]
            }
            (
                Input::Move { x, y, mods, .. },
                Drag::Point {
                    last_x,
                    last_y,
                    u,
                    v,
                },
            ) => {
                let s = if mods.fine { FINE } else { 1.0 };
                let (nu, nv) = (
                    (u + (x - last_x) / w * s).clamp(0.0, 1.0),
                    (v - (y - last_y) / h * s).clamp(0.0, 1.0),
                );
                self.drag = Drag::Point {
                    last_x: x,
                    last_y: y,
                    u: nu,
                    v: nv,
                };
                vec![at(nu, nv, EditPhase::Change)]
            }
            (Input::Up { .. } | Input::Cancel { .. }, Drag::Point { u, v, .. }) => {
                self.drag = Drag::Idle;
                vec![at(u, v, EditPhase::End)]
            }
            (Input::Wheel { notches, mods }, _) => {
                vec![Output::Nudge {
                    delta: notches * HANDLE_WHEEL_STEP * if mods.fine { FINE } else { 1.0 },
                }]
            }
            (Input::DoubleClick, _) => {
                self.drag = Drag::Idle;
                vec![Output::Reset]
            }
            (Input::Key { key, mods }, Drag::Idle) => match key {
                Key::Left => vec![at(u - key_step(mods), v, EditPhase::End)],
                Key::Right => vec![at(u + key_step(mods), v, EditPhase::End)],
                Key::Up => vec![at(u, v + key_step(mods), EditPhase::End)],
                Key::Down => vec![at(u, v - key_step(mods), EditPhase::End)],
                _ => Vec::new(),
            },
            _ => Vec::new(),
        }
    }

    fn stepped(position: f64, delta: f64) -> Vec<Output> {
        vec![Output::Set {
            position: (position + delta).clamp(0.0, 1.0),
            phase: EditPhase::End,
        }]
    }

    fn keys(position: f64, key: Key, mods: Modifiers) -> Vec<Output> {
        let step = if mods.fine { KEY_STEP * FINE } else { KEY_STEP };
        match key {
            Key::Up | Key::Right => Self::stepped(position, step),
            Key::Down | Key::Left => Self::stepped(position, -step),
            Key::PageUp => Self::stepped(position, PAGE_STEP),
            Key::PageDown => Self::stepped(position, -PAGE_STEP),
            Key::Home => vec![Output::Set {
                position: 0.0,
                phase: EditPhase::End,
            }],
            Key::End => vec![Output::Set {
                position: 1.0,
                phase: EditPhase::End,
            }],
            Key::Space
            | Key::Enter
            | Key::Escape
            | Key::Tab
            | Key::Backspace
            | Key::Delete
            | Key::Char(_) => Vec::new(),
        }
    }

    /// Relative vertical drag; the drag survives leaving the control's bounds
    /// (capture) and ends only on release/cancel.
    fn knob(&mut self, input: Input, position: f64) -> Vec<Output> {
        match (input, self.drag) {
            (Input::Down { mods, .. }, _) if mods.alt => vec![Output::Reset],
            (Input::Down { x, y, .. }, _) => {
                self.drag = Drag::Dragging {
                    last_x: x,
                    last_y: y,
                    position,
                };
                vec![Output::Set {
                    position,
                    phase: EditPhase::Begin,
                }]
            }
            (
                Input::Move { x, y, mods, .. },
                Drag::Dragging {
                    last_y,
                    position: p,
                    ..
                },
            ) => {
                // Up (smaller y) increases. Accumulate from the last event so
                // toggling fine mid-drag doesn't jump.
                let scale = if mods.fine { FINE } else { 1.0 };
                let next = (p + (last_y - y) / KNOB_FULL_SWEEP_PX * scale).clamp(0.0, 1.0);
                self.drag = Drag::Dragging {
                    last_x: x,
                    last_y: y,
                    position: next,
                };
                vec![Output::Set {
                    position: next,
                    phase: EditPhase::Change,
                }]
            }
            (Input::Up { .. } | Input::Cancel { .. }, Drag::Dragging { position: p, .. }) => {
                self.drag = Drag::Idle;
                vec![Output::Set {
                    position: p,
                    phase: EditPhase::End,
                }]
            }
            (Input::Wheel { notches, mods }, Drag::Idle) => Self::stepped(
                position,
                notches
                    * if mods.fine {
                        WHEEL_STEP * FINE
                    } else {
                        WHEEL_STEP
                    },
            ),
            (Input::DoubleClick, _) => {
                self.drag = Drag::Idle;
                vec![Output::Reset]
            }
            (Input::Key { key, mods }, Drag::Idle) => Self::keys(position, key, mods),
            _ => Vec::new(),
        }
    }

    /// Relative drag along the fader's axis, scaled to its track length.
    fn fader(
        &mut self,
        input: Input,
        position: f64,
        bounds: Bounds,
        o: Orientation,
    ) -> Vec<Output> {
        let track = match o {
            Orientation::Vertical => bounds.height,
            Orientation::Horizontal => bounds.width,
        }
        .max(1.0);
        match (input, self.drag) {
            (Input::Down { mods, .. }, _) if mods.alt => vec![Output::Reset],
            (Input::Down { x, y, .. }, _) => {
                self.drag = Drag::Dragging {
                    last_x: x,
                    last_y: y,
                    position,
                };
                vec![Output::Set {
                    position,
                    phase: EditPhase::Begin,
                }]
            }
            (
                Input::Move { x, y, mods, .. },
                Drag::Dragging {
                    last_x,
                    last_y,
                    position: p,
                },
            ) => {
                let delta = match o {
                    Orientation::Vertical => last_y - y,
                    Orientation::Horizontal => x - last_x,
                };
                let scale = if mods.fine { FINE } else { 1.0 };
                let next = (p + delta / track * scale).clamp(0.0, 1.0);
                self.drag = Drag::Dragging {
                    last_x: x,
                    last_y: y,
                    position: next,
                };
                vec![Output::Set {
                    position: next,
                    phase: EditPhase::Change,
                }]
            }
            (Input::Up { .. } | Input::Cancel { .. }, Drag::Dragging { position: p, .. }) => {
                self.drag = Drag::Idle;
                vec![Output::Set {
                    position: p,
                    phase: EditPhase::End,
                }]
            }
            (Input::Wheel { notches, mods }, Drag::Idle) => Self::stepped(
                position,
                notches
                    * if mods.fine {
                        WHEEL_STEP * FINE
                    } else {
                        WHEEL_STEP
                    },
            ),
            (Input::DoubleClick, _) => {
                self.drag = Drag::Idle;
                vec![Output::Reset]
            }
            (Input::Key { key, mods }, Drag::Idle) => Self::keys(position, key, mods),
            _ => Vec::new(),
        }
    }

    fn button(
        &mut self,
        input: Input,
        position: f64,
        bounds: Bounds,
        mode: ButtonMode,
    ) -> Vec<Output> {
        let inside = |x: f64, y: f64| {
            x >= bounds.x
                && y >= bounds.y
                && x <= bounds.x + bounds.width
                && y <= bounds.y + bounds.height
        };
        match (mode, input, self.drag) {
            (ButtonMode::Trigger | ButtonMode::Cycle, Input::Down { .. }, _) => vec![Output::Fire],
            (
                ButtonMode::Cycle,
                Input::Key {
                    key: Key::Space | Key::Enter,
                    ..
                },
                _,
            ) => vec![Output::Fire],
            (
                ButtonMode::Select { index, count },
                Input::Down { .. }
                | Input::Key {
                    key: Key::Space | Key::Enter,
                    ..
                },
                _,
            ) => {
                vec![Output::Set {
                    position: ButtonMode::select_position(index, count),
                    phase: EditPhase::End,
                }]
            }
            (ButtonMode::Momentary, Input::Down { .. }, _) => {
                self.drag = Drag::Button;
                vec![Output::Set {
                    position: 1.0,
                    phase: EditPhase::End,
                }]
            }
            (ButtonMode::Momentary, Input::Up { .. } | Input::Cancel { .. }, Drag::Button) => {
                self.drag = Drag::Idle;
                vec![Output::Set {
                    position: 0.0,
                    phase: EditPhase::End,
                }]
            }
            (ButtonMode::Toggle, Input::Down { .. }, _) => {
                self.drag = Drag::Button;
                Vec::new()
            }
            // Toggle flips on release *inside* — dragging off cancels.
            (ButtonMode::Toggle, Input::Up { x, y, .. }, Drag::Button) => {
                self.drag = Drag::Idle;
                if inside(x, y) {
                    vec![Output::Set {
                        position: if position >= 0.5 { 0.0 } else { 1.0 },
                        phase: EditPhase::End,
                    }]
                } else {
                    Vec::new()
                }
            }
            (ButtonMode::Toggle, Input::Cancel { .. }, _) => {
                self.drag = Drag::Idle;
                Vec::new()
            }
            (
                ButtonMode::Toggle,
                Input::Key {
                    key: Key::Space | Key::Enter,
                    ..
                },
                Drag::Idle,
            ) => {
                vec![Output::Set {
                    position: if position >= 0.5 { 0.0 } else { 1.0 },
                    phase: EditPhase::End,
                }]
            }
            (
                ButtonMode::Trigger,
                Input::Key {
                    key: Key::Space | Key::Enter,
                    ..
                },
                _,
            ) => vec![Output::Fire],
            _ => Vec::new(),
        }
    }

    /// Tap vs hold: a press shorter than `hold_ms` is a tap on release; a
    /// press reaching `hold_ms` fires `Hold` immediately (from a move or a
    /// tick), then `HoldRelease` on release. Hardware footswitches feed the
    /// same machine, so on-screen and on-floor behave identically.
    fn footswitch(&mut self, input: Input, hold_ms: f64) -> Vec<Output> {
        let crossed = |t: f64, t0: f64, held: bool| !held && t - t0 >= hold_ms;
        match (input, self.drag) {
            (Input::Down { t, .. }, _) => {
                self.drag = Drag::Pressed { t0: t, held: false };
                Vec::new()
            }
            (Input::Move { t, .. } | Input::Tick { t }, Drag::Pressed { t0, held })
                if crossed(t, t0, held) =>
            {
                self.drag = Drag::Pressed { t0, held: true };
                vec![Output::Hold]
            }
            (Input::Up { t, .. }, Drag::Pressed { t0, held }) => {
                self.drag = Drag::Idle;
                if held {
                    vec![Output::HoldRelease]
                } else if t - t0 >= hold_ms {
                    vec![Output::Hold, Output::HoldRelease]
                } else {
                    vec![Output::Tap]
                }
            }
            (Input::Cancel { .. }, Drag::Pressed { held, .. }) => {
                self.drag = Drag::Idle;
                if held {
                    vec![Output::HoldRelease]
                } else {
                    Vec::new()
                }
            }
            (
                Input::Key {
                    key: Key::Space | Key::Enter,
                    ..
                },
                Drag::Idle,
            ) => vec![Output::Tap],
            _ => Vec::new(),
        }
    }

    /// Absolute: the point under the pointer is the value (y up).
    fn xy(&mut self, input: Input, b: Bounds) -> Vec<Output> {
        let at = |x: f64, y: f64| {
            let nx = if b.width > 0.0 {
                ((x - b.x) / b.width).clamp(0.0, 1.0)
            } else {
                0.0
            };
            let ny = if b.height > 0.0 {
                (1.0 - (y - b.y) / b.height).clamp(0.0, 1.0)
            } else {
                0.0
            };
            (nx, ny)
        };
        match (input, self.drag) {
            (Input::Down { x, y, .. }, _) => {
                self.drag = Drag::Dragging {
                    last_x: x,
                    last_y: y,
                    position: 0.0,
                };
                let (nx, ny) = at(x, y);
                vec![Output::SetXy {
                    x: nx,
                    y: ny,
                    phase: EditPhase::Begin,
                }]
            }
            (Input::Move { x, y, .. }, Drag::Dragging { .. }) => {
                self.drag = Drag::Dragging {
                    last_x: x,
                    last_y: y,
                    position: 0.0,
                };
                let (nx, ny) = at(x, y);
                vec![Output::SetXy {
                    x: nx,
                    y: ny,
                    phase: EditPhase::Change,
                }]
            }
            (Input::Up { .. } | Input::Cancel { .. }, Drag::Dragging { last_x, last_y, .. }) => {
                self.drag = Drag::Idle;
                let (nx, ny) = at(last_x, last_y);
                vec![Output::SetXy {
                    x: nx,
                    y: ny,
                    phase: EditPhase::End,
                }]
            }
            (Input::DoubleClick, _) => vec![Output::Reset],
            _ => Vec::new(),
        }
    }

    /// Selectors step through options with wheel/keys; clicks are resolved
    /// by the runtime (which option was hit) and arrive as `Set`s directly.
    fn selector(&mut self, input: Input, position: f64) -> Vec<Output> {
        match input {
            Input::Wheel { notches, .. } => Self::stepped(position, notches * PAGE_STEP),
            Input::Key { key, mods } => Self::keys(position, key, mods),
            _ => Vec::new(),
        }
    }
}
