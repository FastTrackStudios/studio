//! Blending keyed SVG paths.
//!
//! Paths with the same commands in the same order (only the numbers
//! differ) blend number by number; otherwise the blend switches from one
//! to the next at the midpoint.

/// The path at `v` along `shapes` (0 = the first, 1 = the second, …).
pub(crate) fn blend(shapes: &[String], v: f64) -> Option<String> {
    let last = shapes.len().checked_sub(1)?;
    if last == 0 || v <= 0.0 {
        return shapes.first().cloned();
    }
    if v >= last as f64 {
        return shapes.last().cloned();
    }
    let i = (v.floor() as usize).min(last - 1);
    let f = v - i as f64;
    let (a, b) = (split(&shapes[i]), split(&shapes[i + 1]));
    if a.0 != b.0 || a.1.len() != b.1.len() {
        return Some(if f < 0.5 {
            shapes[i].clone()
        } else {
            shapes[i + 1].clone()
        });
    }
    let mut out = String::new();
    for (k, word) in a.0.iter().enumerate() {
        out.push_str(word);
        if let (Some(x), Some(y)) = (a.1.get(k), b.1.get(k)) {
            let n = x + (y - x) * f;
            out.push_str(&format!("{n:.3}"));
        }
    }
    Some(out)
}

/// A path as its text between numbers (normalised: runs of separators
/// become one space) and the numbers themselves: `words[k]` precedes
/// `numbers[k]`.
fn split(d: &str) -> (Vec<String>, Vec<f64>) {
    let b = d.as_bytes();
    let mut words = Vec::new();
    let mut nums = Vec::new();
    let mut word = String::new();
    let mut i = 0;
    while i < b.len() {
        let c = b[i];
        let starts_num = c.is_ascii_digit()
            || c == b'.'
            || ((c == b'-' || c == b'+')
                && b.get(i + 1)
                    .is_some_and(|n| n.is_ascii_digit() || *n == b'.'));
        if starts_num {
            let s = i;
            i += 1;
            let mut dot = c == b'.';
            while i < b.len() {
                let d = b[i];
                if d.is_ascii_digit() {
                    i += 1;
                } else if d == b'.' && !dot {
                    dot = true;
                    i += 1;
                } else if (d == b'e' || d == b'E')
                    && b.get(i + 1)
                        .is_some_and(|n| n.is_ascii_digit() || *n == b'-' || *n == b'+')
                {
                    i += 2;
                } else {
                    break;
                }
            }
            nums.push(d[s..i].parse::<f64>().unwrap_or(0.0));
            words.push(normal(&word));
            word.clear();
        } else {
            word.push(c as char);
            i += 1;
        }
    }
    words.push(normal(&word));
    (words, nums)
}

fn normal(w: &str) -> String {
    let t: String = w.chars().map(|c| if c == ',' { ' ' } else { c }).collect();
    let parts: Vec<&str> = t.split_whitespace().collect();
    if parts.is_empty() {
        if w.is_empty() {
            String::new()
        } else {
            " ".into()
        }
    } else {
        let mut s = parts.join(" ");
        if w.starts_with([' ', ',']) {
            s.insert(0, ' ');
        }
        if w.ends_with([' ', ',']) {
            s.push(' ');
        }
        s
    }
}
