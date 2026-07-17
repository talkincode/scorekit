//! Read-only semantic diff between two scene files.
//!
//! Compares musical meaning, not text: two YAMLs that reorder keys or change
//! comments diff as identical. Output is line-oriented porcelain
//! (`<op> <path> <old> -> <new>`) so agents and CI can parse it without a
//! YAML parser; `--json` callers get the same records as a JSON array.

use crate::schema::Scene;
use serde_json::{Value, json};

/// One semantic change between two scenes.
pub struct Change {
    /// `~` modified, `+` added, `-` removed.
    pub op: char,
    /// Dotted path into the scene, e.g. `tracks[1].intensity`.
    pub path: String,
    pub old: Option<String>,
    pub new: Option<String>,
}

impl Change {
    fn modified(path: impl Into<String>, old: impl ToString, new: impl ToString) -> Self {
        Change {
            op: '~',
            path: path.into(),
            old: Some(old.to_string()),
            new: Some(new.to_string()),
        }
    }

    fn added(path: impl Into<String>, new: impl ToString) -> Self {
        Change {
            op: '+',
            path: path.into(),
            old: None,
            new: Some(new.to_string()),
        }
    }

    fn removed(path: impl Into<String>, old: impl ToString) -> Self {
        Change {
            op: '-',
            path: path.into(),
            old: Some(old.to_string()),
            new: None,
        }
    }

    pub fn porcelain(&self) -> String {
        match (self.op, &self.old, &self.new) {
            ('~', Some(o), Some(n)) => format!("~ {} {} -> {}", self.path, o, n),
            ('+', _, Some(n)) => format!("+ {} {}", self.path, n),
            ('-', Some(o), _) => format!("- {} {}", self.path, o),
            _ => unreachable!("invalid change record"),
        }
    }

    pub fn to_json(&self) -> Value {
        json!({
            "op": self.op.to_string(),
            "path": self.path,
            "old": self.old,
            "new": self.new,
        })
    }
}

fn scalar(v: &Value) -> String {
    match v {
        Value::String(s) => s.clone(),
        // Scene floats are f32s authored by humans; casting back to f32
        // strips the f64 widening noise (0.55, not 0.550000011920929).
        Value::Number(n) if n.is_f64() => {
            format!("{}", n.as_f64().expect("checked is_f64") as f32)
        }
        other => other.to_string(),
    }
}

/// Recursively diff two JSON values, emitting one change per leaf.
fn walk(path: &str, a: &Value, b: &Value, out: &mut Vec<Change>) {
    match (a, b) {
        (Value::Object(ma), Value::Object(mb)) => {
            for (k, va) in ma {
                let p = if path.is_empty() {
                    k.clone()
                } else {
                    format!("{path}.{k}")
                };
                match mb.get(k) {
                    Some(vb) => walk(&p, va, vb, out),
                    None => out.push(Change::removed(p, scalar(va))),
                }
            }
            for (k, vb) in mb {
                if !ma.contains_key(k) {
                    let p = if path.is_empty() {
                        k.clone()
                    } else {
                        format!("{path}.{k}")
                    };
                    out.push(Change::added(p, scalar(vb)));
                }
            }
        }
        (Value::Array(va), Value::Array(vb)) => {
            for (i, (ea, eb)) in va.iter().zip(vb.iter()).enumerate() {
                walk(&format!("{path}[{i}]"), ea, eb, out);
            }
            for (i, eb) in vb.iter().enumerate().skip(va.len()) {
                out.push(Change::added(format!("{path}[{i}]"), scalar(eb)));
            }
            for (i, ea) in va.iter().enumerate().skip(vb.len()) {
                out.push(Change::removed(format!("{path}[{i}]"), scalar(ea)));
            }
        }
        _ if a == b => {}
        _ => out.push(Change::modified(path, scalar(a), scalar(b))),
    }
}

/// Null fields carry no musical meaning; drop them so `title: ~` and an
/// absent title diff as equal.
fn strip_nulls(v: Value) -> Value {
    match v {
        Value::Object(m) => Value::Object(
            m.into_iter()
                .filter(|(_, v)| !v.is_null())
                .map(|(k, v)| (k, strip_nulls(v)))
                .collect(),
        ),
        Value::Array(a) => Value::Array(a.into_iter().map(strip_nulls).collect()),
        other => other,
    }
}

/// Semantic diff of two validated scenes. Empty result = musically identical.
pub fn scenes(a: &Scene, b: &Scene) -> Vec<Change> {
    let va = strip_nulls(serde_json::to_value(a).expect("scene serializes"));
    let vb = strip_nulls(serde_json::to_value(b).expect("scene serializes"));
    let mut out = Vec::new();
    walk("", &va, &vb, &mut out);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scene(yaml: &str) -> Scene {
        let s: Scene = serde_yaml_ng::from_str(yaml).unwrap();
        s.validate().unwrap();
        s
    }

    #[test]
    fn identical_scenes_diff_empty() {
        let a =
            scene("tempo: 100\nbars: 2\ntracks:\n  - instrument: piano\n    pattern: sustain\n");
        let b = scene("bars: 2\ntempo: 100\ntracks:\n  - {instrument: piano, pattern: sustain}\n");
        assert!(scenes(&a, &b).is_empty());
    }

    #[test]
    fn tempo_and_track_changes_are_reported() {
        let a =
            scene("tempo: 100\nbars: 2\ntracks:\n  - instrument: piano\n    pattern: sustain\n");
        let b = scene(
            "tempo: 120\nbars: 2\ntracks:\n  - instrument: piano\n    pattern: sustain\n    intensity: 0.9\n  - instrument: drums\n    pattern: drums\n",
        );
        let d = scenes(&a, &b);
        let lines: Vec<String> = d.iter().map(Change::porcelain).collect();
        assert!(
            lines.contains(&"~ tempo 100 -> 120".to_owned()),
            "{lines:?}"
        );
        assert!(
            lines.contains(&"~ tracks[0].intensity 0.6 -> 0.9".to_owned()),
            "{lines:?}"
        );
        assert!(
            lines.iter().any(|l| l.starts_with("+ tracks[1]")),
            "{lines:?}"
        );
    }
}
