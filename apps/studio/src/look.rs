//! How the pages show things: a key's colour round the circle of fifths,
//! a colour from any name, durations, the library's filter ranges and a
//! record's or track's status. No content: only how content looks.

/// `m:ss`, or `h:mm:ss` past an hour.
pub fn dur(s: f64) -> String {
    let s = s.round() as i64;
    let (h, m, x) = (s / 3600, s % 3600 / 60, s % 60);
    if h > 0 {
        format!("{h}:{m:02}:{x:02}")
    } else {
        format!("{m}:{x:02}")
    }
}

pub const FIFTHS: [&str; 12] = [
    "C", "G", "D", "A", "E", "B", "F#", "Db", "Ab", "Eb", "Bb", "F",
];

fn alias(k: &str) -> &str {
    match k {
        "Gb" => "F#",
        "C#" => "Db",
        "G#" => "Ab",
        "D#" => "Eb",
        "A#" => "Bb",
        k => k,
    }
}

/// A key's hue round the circle of fifths.
pub fn hue(k: &str) -> Option<u32> {
    let k = k.split('→').next().unwrap_or(k);
    FIFTHS
        .iter()
        .position(|f| *f == alias(k))
        .map(|i| (i as u32 * 30 + 20) % 360)
}

fn semi(k: &str) -> i32 {
    match k {
        "C" => 0,
        "C#" | "Db" => 1,
        "D" => 2,
        "D#" | "Eb" => 3,
        "E" => 4,
        "F" => 5,
        "F#" | "Gb" => 6,
        "G" => 7,
        "G#" | "Ab" => 8,
        "A" => 9,
        "A#" | "Bb" => 10,
        "B" => 11,
        _ => 0,
    }
}

/// Semitones from one key to another, the short way (−6 to +5).
pub fn steps(from: &str, to: &str) -> i32 {
    let from = from.split('→').next().unwrap_or(from);
    (semi(to) - semi(from) + 18).rem_euclid(12) - 6
}

/// A hue from any text, the same every time.
pub fn str_hue(t: &str) -> u32 {
    t.chars().fold(7u32, |h, c| (h * 31 + c as u32) % 360)
}

/// A key's name as the circle of fifths spells it (`Gb` is `F#`), if it is one.
pub fn norm_key(k: &str) -> Option<&'static str> {
    let k = k.split('→').next().unwrap_or(k);
    FIFTHS.iter().copied().find(|f| *f == alias(k))
}

/// The first letter in capitals.
pub fn cap(t: &str) -> String {
    let mut c = t.chars();
    c.next()
        .map(|f| f.to_uppercase().collect::<String>() + c.as_str())
        .unwrap_or_default()
}

/// The arrangement a song plays in when nothing else is chosen: as
/// recorded (its session's main branch).
pub const AS_RECORDED: &str = "Normal";

/// The library's tempo ranges: name, from, to (bpm).
pub const TEMPI: [(&str, u32, u32); 4] = [
    ("Slow", 0, 80),
    ("Mid", 80, 110),
    ("Up", 110, 140),
    ("Fast", 140, 999),
];

/// What a song can be ready with, as the library's filters name it.
pub const READY: [(char, &str); 4] = [
    ('c', "Chart"),
    ('l', "Lyrics"),
    ('s', "Stems"),
    ('m', "Tempo map"),
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keys_are_spelled_and_coloured_round_the_fifths() {
        assert_eq!(norm_key("Gb"), Some("F#"));
        assert_eq!(norm_key("H"), None);
        assert_eq!(hue("C"), Some(20));
        assert_eq!(steps("G", "A"), 2);
        assert_eq!(steps("C", "Bb"), -2);
        assert_eq!(dur(3725.0), "1:02:05");
        assert_eq!(dur(65.0), "1:05");
        assert_eq!(cap("band"), "Band");
    }
}
