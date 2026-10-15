//! The verbs both timelines have, under one id each: what a keymap binds
//! once and both surfaces answer, the arrangement and the Editor alike.
//!
//! The surfaces keep their own ids too (REAPER's numbers on the
//! arrangement, `FTS_EDITOR_*` on the Editor, both in users' keymaps):
//! these are aliases each accepts for what it can do. A surface that has
//! no such verb (the arrangement's grid is not set from a key) leaves the
//! id unbound rather than guessing.
//!
//! One module per group, as the Editor's are: the macro's constants sit
//! beside the trait, and `grid::TOGGLE` and `razor::CLEAR` must not meet.

/// The grid: its division, its feel, whether it snaps and follows the
/// zoom.
pub mod grid {
    #[architect::actions(namespace = "TIMELINE_GRID")]
    pub trait TimelineGridActions {
        #[action(
            description = "Grid: whole measures",
            category = "Timeline",
            group = "Grid"
        )]
        fn measure(&self);
        #[action(
            description = "Grid: half notes",
            category = "Timeline",
            group = "Grid"
        )]
        fn half(&self);
        #[action(
            description = "Grid: quarter notes",
            category = "Timeline",
            group = "Grid"
        )]
        fn quarter(&self);
        #[action(
            description = "Grid: eighth notes",
            category = "Timeline",
            group = "Grid"
        )]
        fn eighth(&self);
        #[action(
            description = "Grid: sixteenth notes",
            category = "Timeline",
            group = "Grid"
        )]
        fn sixteenth(&self);
        #[action(description = "Grid: finer", category = "Timeline", group = "Grid")]
        fn finer(&self);
        #[action(description = "Grid: coarser", category = "Timeline", group = "Grid")]
        fn coarser(&self);
        #[action(
            description = "Snap to the grid on or off",
            category = "Timeline",
            group = "Grid"
        )]
        fn toggle(&self);
        #[action(description = "Grid: dotted", category = "Timeline", group = "Grid")]
        fn dotted(&self);
        #[action(description = "Grid: triplets", category = "Timeline", group = "Grid")]
        fn triplet(&self);
        #[action(
            description = "Grid: follow the zoom",
            category = "Timeline",
            group = "Grid"
        )]
        fn adaptive(&self);
    }
}

/// The edit cursor, stepped.
pub mod cursor {
    #[architect::actions(namespace = "TIMELINE_CURSOR")]
    pub trait TimelineCursorActions {
        #[action(
            description = "Cursor back a measure",
            category = "Timeline",
            group = "Cursor"
        )]
        fn left_measure(&self);
        #[action(
            description = "Cursor on a measure",
            category = "Timeline",
            group = "Cursor"
        )]
        fn right_measure(&self);
        #[action(
            description = "Cursor back a grid step",
            category = "Timeline",
            group = "Cursor"
        )]
        fn left_grid(&self);
        #[action(
            description = "Cursor on a grid step",
            category = "Timeline",
            group = "Cursor"
        )]
        fn right_grid(&self);
    }
}

/// Where the view is.
pub mod view {
    #[architect::actions(namespace = "TIMELINE_VIEW")]
    pub trait TimelineViewActions {
        #[action(description = "Zoom time in", category = "Timeline", group = "View")]
        fn zoom_in(&self);
        #[action(description = "Zoom time out", category = "Timeline", group = "View")]
        fn zoom_out(&self);
        #[action(
            description = "Zoom the rows in",
            category = "Timeline",
            group = "View"
        )]
        fn rows_in(&self);
        #[action(
            description = "Zoom the rows out",
            category = "Timeline",
            group = "View"
        )]
        fn rows_out(&self);
        #[action(
            description = "Back to the previous zoom",
            category = "Timeline",
            group = "View"
        )]
        fn back(&self);
        #[action(
            description = "Forward to the next zoom",
            category = "Timeline",
            group = "View"
        )]
        fn forward(&self);
        #[action(
            description = "Fit the whole of it",
            category = "Timeline",
            group = "View"
        )]
        fn fit(&self);
        #[action(description = "Reset the view", category = "Timeline", group = "View")]
        fn reset(&self);
        #[action(
            description = "Contextual zoom about the cursor",
            category = "Timeline",
            group = "View"
        )]
        fn contextual(&self);
    }
}

/// The razor areas.
pub mod razor {
    #[architect::actions(namespace = "TIMELINE_RAZOR")]
    pub trait TimelineRazorActions {
        #[action(
            description = "Clear the razor areas",
            category = "Timeline",
            group = "Razor"
        )]
        fn clear(&self);
        #[action(
            description = "Delete what the razor areas hold",
            category = "Timeline",
            group = "Razor"
        )]
        fn delete_contents(&self);
        #[action(
            description = "Duplicate what the razor areas hold",
            category = "Timeline",
            group = "Razor"
        )]
        fn duplicate(&self);
        #[action(
            description = "Split at the razor areas' edges",
            category = "Timeline",
            group = "Razor"
        )]
        fn split(&self);
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn the_ids_are_namespaced_and_distinct() {
        let ids = [
            super::grid::TOGGLE.id,
            super::razor::CLEAR.id,
            super::view::RESET.id,
            super::cursor::LEFT_GRID.id,
        ];
        assert_eq!(ids[0], "TIMELINE_GRID_TOGGLE");
        assert_eq!(ids[1], "TIMELINE_RAZOR_CLEAR");
        let unique: std::collections::HashSet<_> = ids.iter().collect();
        assert_eq!(unique.len(), ids.len());
    }
}
