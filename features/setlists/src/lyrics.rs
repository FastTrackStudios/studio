//! A song's lyrics as text, for its page: from its chart's timed lyric
//! lanes (`[lyrics lrc]`, keyflow ≥ 0.4), else from its LRC.

/// The lyrics of one section of the song.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Block {
    /// What the chart calls the section (`Verse 2`, `Chorus`).
    pub section: String,
    pub lines: Vec<String>,
}

/// The lyrics in `chart`'s `[lyrics lrc]` lanes, one block per section
/// that has one, in order.
///
/// A lane is the stamped words `@<bar>[:<beat>] word`: `Si- @1:2.5 lent`
/// is one word over two places (`Silent`), `_` holds the word before.
/// A line ends where the singing rests for a bar or more (the next stamp
/// is two bars on or more), or where the source line does.
#[must_use]
pub fn from_chart(chart: &str) -> Vec<Block> {
    let mut blocks = Vec::new();
    let mut names = SectionNames::default();
    let mut section = String::new();
    let mut in_lane = false;
    for line in chart.lines() {
        let trimmed = line.trim();
        if trimmed.eq_ignore_ascii_case("[lyrics lrc]") || trimmed.starts_with("[lyrics") {
            in_lane = true;
            continue;
        }
        if let Some(name) = names.header(trimmed) {
            section = name;
            in_lane = false;
            continue;
        }
        if trimmed.is_empty() {
            in_lane = false;
            continue;
        }
        if in_lane {
            let lines = lane_lines(trimmed);
            if lines.is_empty() {
                continue;
            }
            match blocks.last_mut() {
                Some(Block {
                    section: s,
                    lines: l,
                }) if *s == section => l.extend(lines),
                _ => blocks.push(Block {
                    section: section.clone(),
                    lines,
                }),
            }
        }
    }
    blocks
}

/// The sung lines of an LRC file (its `[mm:ss.xx]` stamps and `[ar:…]`
/// tags dropped).
#[must_use]
pub fn from_lrc(lrc: &str) -> Vec<String> {
    lrc.lines()
        .filter_map(|line| {
            let mut rest = line.trim();
            let mut stamped = false;
            while let Some(after) = rest.strip_prefix('[') {
                let (tag, tail) = after.split_once(']')?;
                if tag.chars().next().is_some_and(|c| c.is_ascii_digit()) {
                    stamped = true;
                } else {
                    return None;
                }
                rest = tail.trim();
            }
            (stamped && !rest.is_empty()).then(|| rest.to_owned())
        })
        .collect()
}

/// Names sections as a reader would: `VS` → `Verse 1`, `Verse 2`; a
/// custom `[Tag (note)]` → `Tag`.
#[derive(Default)]
struct SectionNames {
    verses: u32,
    last: String,
}

impl SectionNames {
    fn header(&mut self, line: &str) -> Option<String> {
        if let Some(inner) = line.strip_prefix('[') {
            let inner = inner.split(']').next().unwrap_or_default();
            if inner.starts_with("lyrics") {
                return None;
            }
            let name = inner.split(" (").next().unwrap_or(inner).trim();
            self.last = String::new();
            return Some(name.to_owned());
        }
        let mut words = line.split_whitespace();
        let code = words.next()?;
        // A header's next word is its bar count, or nothing (a recall).
        if !words
            .next()
            .is_none_or(|w| w.chars().all(|c| c.is_ascii_digit()))
        {
            return None;
        }
        let name = match code {
            "VS" => {
                if self.last != "VS" {
                    self.verses = self.verses.saturating_add(1);
                }
                format!("Verse {}", self.verses)
            }
            "CH" => "Chorus".to_owned(),
            "PRE" => "Pre-Chorus".to_owned(),
            "BR" => "Bridge".to_owned(),
            "IN" => "Intro".to_owned(),
            "OUT" => "Outro".to_owned(),
            "INT" => "Interlude".to_owned(),
            "INST" => "Instrumental".to_owned(),
            "SOLO" => "Solo".to_owned(),
            _ => return None,
        };
        code.clone_into(&mut self.last);
        Some(name)
    }
}

/// One source line of a lane as sung lines.
fn lane_lines(lane: &str) -> Vec<String> {
    let mut lines: Vec<String> = Vec::new();
    let mut words: Vec<String> = Vec::new();
    let mut joining = false;
    let mut last_bar: Option<u32> = None;
    for token in lane.split_whitespace() {
        if let Some(stamp) = token.strip_prefix('@') {
            let bar = stamp.split(':').next().and_then(|b| b.parse::<u32>().ok());
            if let (Some(bar), Some(last)) = (bar, last_bar) {
                if bar > last.saturating_add(1) && !joining && !words.is_empty() {
                    lines.push(words.join(" "));
                    words.clear();
                }
            }
            if bar.is_some() {
                last_bar = bar;
            }
            continue;
        }
        if token.chars().all(|c| c == '_') {
            continue;
        }
        let (syllable, continues) = token
            .strip_suffix('-')
            .map_or((token, false), |s| (s, true));
        match words.last_mut() {
            Some(word) if joining => word.push_str(syllable),
            _ => words.push(syllable.to_owned()),
        }
        joining = continues;
    }
    if !words.is_empty() {
        lines.push(words.join(" "));
    }
    lines
}

#[cfg(test)]
mod tests {
    use super::*;

    const CHART: &str = "Amazing Grace - Traditional\n#G 72bpm 3/4\n\nCount 2\n\nVS 8\nG G C G\nG G D D\n[lyrics lrc]\n@1 A- @1:2 ma- @1:3 zing @2 grace @3 how @3:3 sweet @4 the @4:3 sound _\n@6 That @6:2 saved @7 a @7:3 wretch\n\nINST 4\nG C D G\n\nVS 8\nG G C G\nG G D D\n[lyrics lrc]\n@1 'Twas @1:3 grace @3 that @3:3 taught @6 my @6:3 heart\n";

    #[test]
    fn a_lane_reads_as_lines_of_whole_words() {
        let blocks = from_chart(CHART);
        assert_eq!(
            blocks,
            vec![
                Block {
                    section: "Verse 1".to_owned(),
                    lines: vec![
                        "Amazing grace how sweet the sound".to_owned(),
                        "That saved a wretch".to_owned()
                    ],
                },
                Block {
                    section: "Verse 2".to_owned(),
                    lines: vec![
                        "'Twas grace".to_owned(),
                        "that taught".to_owned(),
                        "my heart".to_owned()
                    ],
                },
            ]
        );
    }

    #[test]
    fn lrc_lines_lose_their_stamps_and_tags() {
        let lrc =
            "[ar:Someone]\n[00:01.20] Silent night\n[00:05.00]\n[00:07.10][00:30.00] Holy night\n";
        assert_eq!(from_lrc(lrc), vec!["Silent night", "Holy night"]);
    }
}
