//! Live reload: replace the design under a running surface.

use std::collections::HashSet;
use std::mem::take;

use crate::document::proto::NodeId;

use super::{Runtime, Surface};

impl Runtime {
    /// Swap in a new design (an edited document) and keep what's running:
    /// parameter values (incl. optimistic edits and edits still
    /// coalescing), the latest stream frames, meter ballistics, local
    /// positions, and a gesture in flight — for every control whose node
    /// is still there with the same kind (node ids are stable across
    /// edits). Controls that are gone drop their state. Motions keep
    /// theirs for every node (and motion slot) still there.
    ///
    /// The host lays the new design out and passes its boxes again
    /// ([`Runtime::set_boxes`]).
    pub fn swap_surface(&mut self, surface: Surface) {
        let mut next = Runtime::new(surface, self.catalog.clone());
        let same =
            |id: &NodeId| match (self.surface.controls.get(id), next.surface.controls.get(id)) {
                (Some(a), Some(b)) => a.kind == b.kind,
                _ => false,
            };
        let kept: HashSet<NodeId> = next
            .surface
            .controls
            .keys()
            .copied()
            .filter(|id| same(id))
            .collect();
        next.values = take(&mut self.values);
        next.streams = take(&mut self.streams);
        next.coalescer = take(&mut self.coalescer);
        next.meter_scale = self.meter_scale.clone();
        next.meters = take(&mut self.meters)
            .into_iter()
            .filter(|((id, _, _), _)| kept.contains(id))
            .collect();
        next.local_positions = take(&mut self.local_positions)
            .into_iter()
            .filter(|(id, _)| kept.contains(id))
            .collect();
        for (id, g) in take(&mut self.gestures) {
            if kept.contains(&id) {
                next.gestures.insert(id, g);
            }
        }
        next.captured = self.captured.filter(|id| kept.contains(id));
        next.hovered = self.hovered.filter(|id| kept.contains(id));
        // Motions keep their state (a ringing spring, a turning reel) for
        // every node still there, and the clock runs on.
        if let (Ok(motion), Ok(prev)) = (next.motion.get_mut(), self.motion.get_mut()) {
            std::mem::swap(motion, prev);
            motion.swap(&next.surface.scene);
        }
        next.texts = take(&mut self.texts);
        next.text_measure = self.text_measure.take();
        next.curves = std::mem::take(&mut self.curves);
        // Selection, focus, an edit in progress: kept for what's still there
        // (instances keep their ids across rebuilds).
        let mut ui = take(&mut self.ui);
        ui.retain(|id| kept.contains(id));
        next.ui = ui;
        *self = next;
    }
}
