//! Adds a bank that lights one channel of one fixture at a time, to find
//! out what each channel of a fixture whose chart is unknown really does.
//!
//! ```sh
//! cargo run -p ignition-daslight --example channel_test -- \
//!     "in.dvc" "out.dvc" "Mover 1" "MOVER CHANNELS"
//! ```
//!
//! Buttons: `ALL 0`, then `CHn 255` and `CHn 128` for every channel (the
//! rest held at 0), then `ALL 255`.

use std::path::Path;

use ignition_daslight::profile::channels;
use ignition_daslight::{Error, Scene, Showfile};

fn main() -> Result<(), Error> {
    let args: Vec<String> = std::env::args().collect();
    let (Some(input), Some(output), Some(fixture), Some(bank)) = (args.get(1), args.get(2), args.get(3), args.get(4))
    else {
        return Err(Error::Patch("usage: channel_test <in.dvc> <out.dvc> <fixture name> <bank name>".to_owned()));
    };
    let mut show = Showfile::open(Path::new(input))?;
    let patch = show.patch()?;
    let f = patch
        .fixtures
        .iter()
        .find(|f| &f.name == fixture)
        .ok_or_else(|| Error::Patch(format!("no fixture named {fixture}")))?;
    let profile = patch.profile(&f.profile).ok_or_else(|| Error::Patch(format!("{fixture} has no profile")))?;
    let count = channels(profile).len();
    let mut scenes = Vec::new();
    let mut add = |name: String, lit: Option<(usize, u8)>, all: u8| {
        let mut values = vec![Some(all); count];
        if let Some((i, v)) = lit
            && let Some(slot) = values.get_mut(i)
        {
            *slot = Some(v);
        }
        let mut s = Scene::new(&name, "#ff40a0ff");
        s.fixtures.push((f.uid.clone(), values));
        scenes.push(s);
    };
    add("ALL 0".to_owned(), None, 0);
    for i in 0..count {
        let n = i.saturating_add(1);
        add(format!("CH{n} 255"), Some((i, 255)), 0);
        add(format!("CH{n} 128"), Some((i, 128)), 0);
    }
    add("ALL 255".to_owned(), None, 255);
    show.add_bank(bank, "#ff40a0ff", &scenes, &patch)?;
    show.save(Path::new(output))?;
    println!("{bank}: {} buttons on {fixture} ({count} channels) -> {output}", scenes.len());
    Ok(())
}
