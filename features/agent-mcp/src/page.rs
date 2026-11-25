//! Pages instead of a cut: an answer bigger than the budget is kept
//! here, and the first page of it answered with where the next starts
//! (`more {result, offset}`).

use std::collections::VecDeque;
use std::sync::{Mutex, PoisonError};

use serde_json::{Value, json};

/// How many big answers are kept for `more`.
const KEPT: usize = 16;

/// The answers kept for paging.
#[derive(Debug, Default)]
pub struct Pages {
    kept: Mutex<(u64, VecDeque<(String, Value)>)>,
}

/// What a page is cut from: the list in the answer it pages through.
#[derive(Clone, Debug, PartialEq, Eq)]
enum Pageable {
    /// The answer is a list.
    Whole,
    /// The answer's field `name` is (its longest list).
    Field(String),
    /// No list to page: the text, by characters.
    Text,
}

fn pageable(v: &Value) -> Pageable {
    match v {
        Value::Array(_) => Pageable::Whole,
        Value::Object(o) => o
            .iter()
            .filter(|(_, x)| x.as_array().is_some_and(|a| !a.is_empty()))
            .max_by_key(|(_, x)| x.to_string().len())
            .map_or(Pageable::Text, |(k, _)| Pageable::Field(k.clone())),
        _ => Pageable::Text,
    }
}

impl Pages {
    fn keep(&self, v: Value) -> String {
        let mut kept = self.kept.lock().unwrap_or_else(PoisonError::into_inner);
        kept.0 = kept.0.saturating_add(1);
        let id = format!("r{}", kept.0);
        kept.1.push_back((id.clone(), v));
        while kept.1.len() > KEPT {
            kept.1.pop_front();
        }
        drop(kept);
        id
    }

    fn get(&self, id: &str) -> Option<Value> {
        let kept = self.kept.lock().unwrap_or_else(PoisonError::into_inner);
        kept.1.iter().find(|(k, _)| k == id).map(|(_, v)| v.clone())
    }

    /// `v` as it is when it fits in `budget` bytes of JSON; else its first
    /// page, kept for `more`.
    pub fn fit(&self, v: Value, budget: usize) -> Value {
        if v.to_string().len() <= budget {
            return v;
        }
        let page = cut(&v, 0, None, budget);
        let id = self.keep(v);
        with_result(page, &id)
    }

    /// The page of kept answer `id` from `offset` (`limit` items at most).
    ///
    /// # Errors
    ///
    /// No such answer is kept (any more).
    pub fn more(
        &self,
        id: &str,
        offset: usize,
        limit: Option<usize>,
        budget: usize,
    ) -> Result<Value, String> {
        let v = self.get(id).ok_or_else(|| {
            format!("no result {id} is kept (the last {KEPT} big answers are); call the tool again")
        })?;
        Ok(with_result(cut(&v, offset, limit, budget), id))
    }
}

fn with_result(mut page: Value, id: &str) -> Value {
    if let Some(p) = page.get_mut("page").and_then(Value::as_object_mut) {
        p.insert("result".into(), json!(id));
        if p.get("next").is_some_and(|n| !n.is_null()) {
            p.insert(
                "more".into(),
                json!(format!(
                    "more {{\"result\": \"{id}\", \"offset\": {}}}",
                    p["next"]
                )),
            );
        }
    }
    page
}

/// The items of `list` from `offset` that fit in `budget` (one at least),
/// `limit` at most; and where the next page starts.
fn take(
    list: &[Value],
    offset: usize,
    limit: Option<usize>,
    budget: usize,
) -> (Vec<Value>, Option<usize>) {
    let mut out = Vec::new();
    let mut used = 0usize;
    for item in list.iter().skip(offset) {
        let size = item.to_string().len().saturating_add(1);
        if !out.is_empty()
            && (used.saturating_add(size) > budget || limit.is_some_and(|l| out.len() >= l))
        {
            break;
        }
        used = used.saturating_add(size);
        out.push(item.clone());
    }
    let end = offset.saturating_add(out.len());
    (out, (end < list.len()).then_some(end))
}

fn page_note(offset: usize, count: usize, total: usize, next: Option<usize>, of: &str) -> Value {
    json!({ "of": of, "offset": offset, "count": count, "total": total, "next": next })
}

fn cut(v: &Value, offset: usize, limit: Option<usize>, budget: usize) -> Value {
    // What the page's own frame costs, roughly: the rest of the object.
    let budget = budget.saturating_sub(400).max(1000);
    match pageable(v) {
        Pageable::Whole => {
            let list = v.as_array().map_or(&[][..], Vec::as_slice);
            let (items, next) = take(list, offset, limit, budget);
            json!({ "items": items, "page": page_note(offset, items.len(), list.len(), next, "items") })
        }
        Pageable::Field(field) => {
            let list = v
                .get(&field)
                .and_then(Value::as_array)
                .map_or(&[][..], Vec::as_slice);
            let mut rest = v.clone();
            if let Some(o) = rest.as_object_mut() {
                o.remove(&field);
            }
            let rest_len = rest.to_string().len();
            let (items, next) = take(
                list,
                offset,
                limit,
                budget.saturating_sub(rest_len).max(1000),
            );
            let note = page_note(offset, items.len(), list.len(), next, &field);
            if let Some(o) = rest.as_object_mut() {
                o.insert(field, Value::Array(items));
                o.insert("page".into(), note);
            }
            rest
        }
        Pageable::Text => {
            let text = v.to_string();
            let chars: Vec<char> = text.chars().collect();
            let end = offset.saturating_add(budget).min(chars.len());
            let chunk: String = chars
                .get(offset.min(end)..end)
                .unwrap_or_default()
                .iter()
                .collect();
            let next = (end < chars.len()).then_some(end);
            json!({ "text": chunk, "page": page_note(offset, end.saturating_sub(offset), chars.len(), next, "text (characters of the JSON)") })
        }
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::indexing_slicing)]
mod tests {
    use super::*;

    fn big() -> Value {
        json!({ "track": "Drums", "tracks": (0..200).map(|i| json!({"path": format!("Drums/T{i}"), "pad": "x".repeat(100)})).collect::<Vec<_>>() })
    }

    #[test]
    fn a_small_answer_is_answered_as_it_is() {
        let p = Pages::default();
        assert_eq!(p.fit(json!({"a": 1}), 1000), json!({"a": 1}));
    }

    #[test]
    fn a_big_answer_pages_through_its_longest_list() {
        let p = Pages::default();
        let first = p.fit(big(), 5000);
        assert_eq!(first["track"], json!("Drums"));
        let n = first["tracks"].as_array().unwrap().len();
        assert!(n > 10 && n < 200, "{n}");
        assert!(first.to_string().len() <= 5000);
        assert_eq!(first["page"]["total"], json!(200));
        let id = first["page"]["result"].as_str().unwrap().to_owned();
        let mut seen = n;
        let mut next = first["page"]["next"].as_u64();
        while let Some(at) = next {
            let page = p
                .more(&id, usize::try_from(at).unwrap(), None, 5000)
                .unwrap();
            assert_eq!(page["tracks"][0]["path"], json!(format!("Drums/T{seen}")));
            seen += page["tracks"].as_array().unwrap().len();
            next = page["page"]["next"].as_u64();
        }
        assert_eq!(seen, 200);
    }

    #[test]
    fn a_limit_caps_a_page() {
        let p = Pages::default();
        let first = p.fit(big(), 5000);
        let id = first["page"]["result"].as_str().unwrap();
        let page = p.more(id, 3, Some(2), 50_000).unwrap();
        assert_eq!(page["tracks"].as_array().unwrap().len(), 2);
        assert_eq!(page["page"]["next"], json!(5));
    }

    #[test]
    fn a_list_answer_pages_as_items() {
        let p = Pages::default();
        let v = Value::Array(
            (0..500)
                .map(|i| json!(format!("line {i} {}", "y".repeat(50))))
                .collect(),
        );
        let first = p.fit(v, 4000);
        assert_eq!(
            first["items"][0],
            json!(format!("line 0 {}", "y".repeat(50)))
        );
        assert_eq!(first["page"]["of"], json!("items"));
    }

    #[test]
    fn text_with_no_list_pages_by_characters() {
        let p = Pages::default();
        let first = p.fit(json!({"blob": "z".repeat(10_000)}), 3000);
        assert!(first["text"].as_str().unwrap().len() <= 3000);
        assert!(first["page"]["next"].as_u64().is_some());
    }

    #[test]
    fn an_answer_not_kept_says_so() {
        assert!(Pages::default().more("r9", 0, None, 1000).is_err());
    }
}
