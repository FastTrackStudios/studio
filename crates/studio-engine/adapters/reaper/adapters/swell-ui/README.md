# swell-ui

Typed `Window`/`Hwnd` wrappers around REAPER's SWELL (the Win32 API on
macOS and Linux), originally Benjamin Klum's from helgoboss's ReaLearn.

It lived in FastTrackStudios/reaper-lib until October 2026 and moved here
beside `reaper-dioxus` and `reaper-embed`, so studio takes nothing from
reaper-lib. It keeps reaper-lib's licence, MIT OR Apache-2.0. The macOS
code (`src/macos.rs`) sends its few AppKit messages through objc2 0.6.
