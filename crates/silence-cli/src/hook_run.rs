use serde_json::Value;
use silence_core::{CommentKinds, LineMode, PreserveConfig};
use std::collections::HashMap;
use std::io::Read;
use std::path::{Path, PathBuf};

use crate::git;
use crate::strip::{lang_for, strip_file, LineRanges, StripOpts, WriteMode};

pub fn hook_targets(explicit: &[PathBuf]) -> Vec<PathBuf> {
    let mut targets = if explicit.is_empty() {
        hook_targets_from_stdin()
    } else {
        explicit.to_vec()
    };
    targets.sort();
    targets.dedup();
    targets.retain(|p| p.is_file() && lang_for(p).is_some());
    targets
}

pub fn run_hook(explicit: &[PathBuf], preserve: &PreserveConfig) {
    let targets = hook_targets(explicit);
    if targets.is_empty() {
        return;
    }

    let git_ranges = hook_git_ranges();
    let opts = StripOpts {
        line_mode: LineMode::Collapse,
        preserve: preserve.clone(),
        line_ranges: Vec::new(),
        kinds: CommentKinds::default(),
        write: WriteMode::Hook,
    };

    for path in &targets {
        let Some(ranges) = hook_line_ranges(path, git_ranges.as_ref()) else {
            continue;
        };
        let file_opts = StripOpts {
            line_ranges: ranges,
            ..opts.clone()
        };
        let _ = strip_file(path, &file_opts);
    }
}

fn hook_line_ranges(path: &Path, git: Option<&HashMap<PathBuf, LineRanges>>) -> Option<LineRanges> {
    let Some(map) = git else {
        return Some(Vec::new());
    };
    let canon = path.canonicalize().unwrap_or_else(|_| path.to_path_buf());
    map.get(&canon).cloned()
}

fn hook_git_ranges() -> Option<HashMap<PathBuf, LineRanges>> {
    let ch = git::changes(git::Scope::All).ok()?;
    let mut map = HashMap::new();
    for (rel, ranges) in ch.files {
        let abs = ch.root.join(rel);
        let key = abs.canonicalize().unwrap_or(abs);
        map.insert(key, ranges);
    }
    Some(map)
}

fn hook_targets_from_stdin() -> Vec<PathBuf> {
    let mut input = String::new();
    if std::io::stdin().read_to_string(&mut input).is_err() || input.trim().is_empty() {
        return Vec::new();
    }
    let Ok(v) = serde_json::from_str::<Value>(&input) else {
        return Vec::new();
    };
    let mut out = Vec::new();
    collect_json_paths(&v, &mut out);
    out
}

fn collect_json_paths(v: &Value, out: &mut Vec<PathBuf>) {
    match v {
        Value::Object(map) => {
            for (k, val) in map {
                if let Value::String(s) = val {
                    match k.as_str() {
                        "file_path" | "filePath" | "path" | "filename" | "file" => {
                            out.push(PathBuf::from(s));
                        }
                        "patch" | "patchText" | "diff" | "input" => collect_patch_paths(s, out),
                        _ => {}
                    }
                }
                collect_json_paths(val, out);
            }
        }
        Value::Array(items) => {
            for it in items {
                collect_json_paths(it, out);
            }
        }
        _ => {}
    }
}

fn collect_patch_paths(patch: &str, out: &mut Vec<PathBuf>) {
    for raw in patch.lines() {
        let line = raw.trim_start();
        for prefix in ["*** Update File: ", "*** Add File: ", "*** Move to: "] {
            if let Some(rest) = line.strip_prefix(prefix) {
                out.push(PathBuf::from(rest.trim()));
            }
        }
        if let Some(rest) = raw.strip_prefix("+++ b/") {
            out.push(PathBuf::from(rest.trim()));
        }
    }
}
