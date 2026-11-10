//! Opening a production in its DAW: the platform's open command on its
//! file, which starts whatever the system has registered for it (Live for
//! an `.als`, Pro Tools for a `.ptx`, Logic for a `.logicx` bundle).

use std::path::Path;
use std::process::{Command, Stdio};

/// The command that opens `file`: its program and arguments.
#[must_use]
pub fn command(file: &Path) -> (String, Vec<String>) {
    let file = file.display().to_string();
    if cfg!(target_os = "macos") {
        ("open".to_owned(), vec![file])
    } else if cfg!(target_os = "windows") {
        // `start`'s first quoted argument is the window's title.
        (
            "cmd".to_owned(),
            vec!["/C".to_owned(), "start".to_owned(), String::new(), file],
        )
    } else {
        ("xdg-open".to_owned(), vec![file])
    }
}

/// Start the platform's open command on `file` and leave it running.
///
/// # Errors
///
/// The file is not there, or the command did not start.
pub fn open(file: &Path) -> Result<(), String> {
    if !file.exists() {
        return Err(format!("{}: not on this machine", file.display()));
    }
    let (program, args) = command(file);
    Command::new(&program)
        .args(&args)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .map(drop)
        .map_err(|e| format!("{program}: {e}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_open_command_ends_with_the_file() {
        let (program, args) = command(Path::new("/music/Album Project/Album.als"));
        assert_ne!(program, "");
        assert_eq!(
            args.last().map(String::as_str),
            Some("/music/Album Project/Album.als")
        );
        assert!(open(Path::new("/nowhere/at/all.als")).is_err());
    }
}
