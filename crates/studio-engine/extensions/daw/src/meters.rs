//! The engine's meters as values: what a mixer pane draws its meters from
//! with no frame crossing.
//!
//! The engine's own meter frames (its `daw` facade's `meter_events`, the
//! frames Studio's windows draw from over their link) are written into this
//! extension's shared value table (`architect_scene::engine::shared`, the
//! file the host maps), one slot a part a track
//! ([`studio_engine::engine::meter_value`]: each channel's peak and hold,
//! and the louder peak), as the fraction of the meter they light. Only the
//! frames of the set's current song count. A pane of an extension that uses
//! a service this one provides reads the table (`studio.mixer`, which uses
//! `fts.session`), and the host moves the meters' parts by it at the
//! display's rate. A value is written only when it changed, so a still
//! meter lets the host rest.

use std::time::Duration;

use architect_scene::anim::Name;
use architect_scene::engine::{EngineWriter, Slot, shared};
use studio_engine::engine::{
    METER_PARTS, METER_TRACKS, TRACK_PARTS, VALUE_TRACKS, meter_fraction, meter_value, track_value,
};

/// Publishes the engine's meters, on a thread of its own, for as long as
/// the process runs; and, in the same table, the tracks' mute and solo
/// (`crate::buttons`).
pub fn publish() {
    let _ = std::thread::Builder::new()
        .name("studio-meters".to_owned())
        .spawn(|| {
            if let Err(e) = run() {
                tracing::warn!(error = %e, "daw: the meters are not published as values");
            }
        });
}

/// The project the set has current.
pub(crate) fn current_project() -> Option<String> {
    studio_engine::studio_engine::set::peek(|view| {
        let set = view.setlist.as_ref()?;
        set.songs.get(set.at).map(|s| s.project.clone())
    })
}

/// The table's slots and what each last held.
struct Table {
    writer: EngineWriter,
    slots: Vec<Vec<Slot>>,
    last: Vec<[f64; 5]>,
}

impl Table {
    /// `levels` (by project track index) written where they changed.
    fn write(&mut self, levels: &[daw_proto::TrackLevels]) {
        for (i, (slot, was)) in self.slots.iter().zip(self.last.iter_mut()).enumerate() {
            let now = levels.get(i).map_or([0.0; 5], |l| {
                [
                    meter_fraction(l.peak_left),
                    meter_fraction(l.peak_right),
                    meter_fraction(l.hold_left),
                    meter_fraction(l.hold_right),
                    meter_fraction(l.peak_left.max(l.peak_right)),
                ]
            });
            for ((s, v), w) in slot.iter().zip(now).zip(was.iter_mut()) {
                if v.to_bits() != w.to_bits() {
                    self.writer.write(*s, v);
                    *w = v;
                }
            }
        }
    }
}

fn run() -> Result<(), String> {
    let names: Vec<Name> = (0..METER_TRACKS)
        .flat_map(|i| METER_PARTS.iter().map(move |p| meter_value(i, p)))
        .chain((0..VALUE_TRACKS).flat_map(|i| TRACK_PARTS.iter().map(move |p| track_value(i, p))))
        .map(|n| Name::new(&n).map_err(|e| e.to_string()))
        .collect::<Result<_, _>>()?;
    let path = shared::path_of("daw").map_err(|e| e.to_string())?;
    let (table, writer) = shared::create(&path, names).map_err(|e| e.to_string())?;
    crate::buttons::publish(writer.clone());
    let slots: Vec<Vec<Slot>> = (0..METER_TRACKS)
        .map(|i| {
            METER_PARTS
                .iter()
                .filter_map(|p| Name::new(&meter_value(i, p)).ok())
                .filter_map(|n| table.slot(&n))
                .collect()
        })
        .collect();
    let mut table = Table {
        writer,
        slots,
        last: vec![[f64::NAN; 5]; METER_TRACKS],
    };
    table.write(&[]);
    tracing::info!(values.path = %path.display(), values.tracks = METER_TRACKS, "daw: the meters are published as values");
    let runtime = studio_engine::open::runtime().ok_or("no runtime to follow the meters on")?;
    // Subscribed again if the stream ends (the engine reopened, a set
    // replaced).
    loop {
        runtime.block_on(async {
            let Some(daw) = daw::rpc::Daw::try_get() else {
                return;
            };
            let mut stream = daw.meter_events();
            tracing::info!("daw: following the engine's meter frames");
            while let Ok(Some(frame)) = stream.recv().await {
                let frame = frame.get();
                if current_project().is_some_and(|p| p == frame.project_guid) {
                    table.write(&frame.tracks);
                }
            }
        });
        std::thread::sleep(Duration::from_millis(250));
    }
}
