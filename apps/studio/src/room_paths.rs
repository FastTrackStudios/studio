//! Where what a room has loaded is, for a member that reads it by path
//! from a folder rather than off its own disk: a browser, whose Studio
//! folder is a path in a Drive location or a folder at a link
//! (`crate::room_web`).
//!
//! The room's file is `<studio>/Rooms/<room>.room`, and what the leader
//! loads is kept relative to `<studio>` (`room::Loaded`), so the same
//! setlist means the same file on every device. A setlist's lines are
//! relative to the setlist's own folder (`../Songs/<song>`).

/// `rel` under the folder `base` (a path in a Drive location, a path on
/// the page's own site, or a whole URL), with `.` and `..` taken out. A
/// `..` past the top stays at the top.
pub fn under(base: &str, rel: &str) -> String {
    // A URL's scheme and host are not path: only what follows is.
    let (head, path) = match base.find("://") {
        Some(i) => {
            let after = i.saturating_add(3);
            let at = base
                .get(after..)
                .and_then(|rest| rest.find('/'))
                .map_or(base.len(), |j| after.saturating_add(j));
            (base.get(..at).unwrap_or(base), base.get(at..).unwrap_or(""))
        }
        None => ("", base),
    };
    let rooted = path.starts_with('/') || !head.is_empty();
    let mut parts: Vec<&str> = Vec::new();
    for part in path.split('/').chain(rel.split('/')) {
        match part {
            "" | "." => {}
            ".." => {
                parts.pop();
            }
            part => parts.push(part),
        }
    }
    let joined = parts.join("/");
    if rooted {
        format!("{head}/{joined}")
    } else {
        joined
    }
}

/// The folder `path` (a file's) is in.
pub fn folder_of(path: &str) -> String {
    let path = path.trim_end_matches('/');
    path.rsplit_once('/')
        .map_or_else(String::new, |(dir, _)| dir.to_owned())
}

/// The last part of `path`: a song's or a setlist's name.
pub fn name_of(path: &str) -> String {
    let path = path.trim_end_matches('/');
    path.rsplit('/').next().unwrap_or(path).to_owned()
}

/// The folder a song is in, from the path a room loaded it by: the path
/// itself for a song's folder, its folder for the project or session file
/// in one (`<song>/<song>.RPP`, `<song>/<song>.session`).
pub fn song_folder(path: &str) -> String {
    let path = path.trim_end_matches('/');
    let name = name_of(path).to_ascii_lowercase();
    let is_file = [".rpp", ".session"].iter().any(|ext| name.ends_with(ext));
    if is_file {
        folder_of(path)
    } else {
        path.to_owned()
    }
}

/// The Studio folder a room's file is in (`<studio>/Rooms/<room>.room`).
pub fn studio_of(room_file: &str) -> Option<String> {
    let rooms = folder_of(room_file);
    (name_of(&rooms) == "Rooms").then(|| folder_of(&rooms))
}

/// Where room `id`'s file is in the Studio folder `studio`.
pub fn room_file(studio: &str, id: &str) -> String {
    under(studio, &format!("Rooms/{id}.room"))
}

/// Where room `id`'s lobby (what a guest joins by, `room::Lobby`) is in
/// the Studio folder `studio`.
pub fn lobby_file(studio: &str, id: &str) -> String {
    under(studio, &format!("Rooms/Lobby/{id}.lobby"))
}

/// The song folder a setlist's line names by path, the setlist being at
/// `setlist` (its path): the line's path (read as `crate::setfile::song`
/// reads it, the arrangement it may name left out), relative to the
/// setlist's folder. `None` for a line that names a song by its session's
/// id (found in Drive's indexes, `crate::drive_sets`, not by path) or by
/// its title alone.
pub fn song_of_line(setlist: &str, line: &str) -> Option<String> {
    match crate::setfile::song(line.trim()) {
        crate::setfile::Song::Path(path) if !path.trim().is_empty() => {
            Some(song_folder(&under(&folder_of(setlist), path.trim())))
        }
        _ => None,
    }
}

/// Whether a path a room loaded is outside the Studio folder (kept as the
/// leader's device has it): a member reading by path cannot open it.
pub fn outside_studio(loaded: &str) -> bool {
    loaded.starts_with('/') || loaded.contains(":\\") || loaded.contains(":/")
}

#[cfg(test)]
mod tests;
