use silence_core::{CommentKinds, LineMode, PreserveConfig};
use std::collections::HashMap;
use std::io::Read;
use std::path::{Path, PathBuf};

use crate::git;
use crate::hook_input;
use crate::strip::{lang_for, strip_file, LineRanges, StripOpts, StripOutcome, WriteMode};

enum HookSkip {
    NotInGitChanges(PathBuf),
    UnsupportedLang(PathBuf),
    StripFailed(PathBuf, String),
    OutsideRepo(PathBuf),
}

pub fn hook_targets(explicit: &[PathBuf]) -> Vec<PathBuf> {
    let mut targets = if explicit.is_empty() {
        match hook_targets_from_stdin() {
            Ok(paths) => paths,
            Err(e) => {
                eprintln!("silence: skip hook stdin: {e}");
                Vec::new()
            }
        }
    } else {
        explicit.to_vec()
    };
    targets.sort();
    targets.dedup();
    let repo_root = git::changes(git::Scope::All)
        .ok()
        .and_then(|c| c.root.canonicalize().ok());
    let mut kept = Vec::with_capacity(targets.len());
    for path in targets {
        if !path.is_file() {
            eprintln!("silence: skip {}: not a file", path.display());
            continue;
        }
        if let Some(root) = &repo_root {
            match path.canonicalize() {
                Ok(canon) if canon.starts_with(root) => {}
                _ => {
                    log_skip(HookSkip::OutsideRepo(path));
                    continue;
                }
            }
        }
        if lang_for(&path).is_none() {
            log_skip(HookSkip::UnsupportedLang(path));
            continue;
        }
        kept.push(path);
    }
    kept
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

    for path in targets {
        let Some(ranges) = hook_line_ranges(&path, git_ranges.as_ref()) else {
            log_skip(HookSkip::NotInGitChanges(path));
            continue;
        };
        let file_opts = StripOpts {
            line_ranges: ranges,
            ..opts.clone()
        };
        match strip_file(&path, &file_opts) {
            StripOutcome::Hook
            | StripOutcome::Unchanged
            | StripOutcome::Checked { .. }
            | StripOutcome::Wrote { .. } => {}
            StripOutcome::Failed { msg } => log_skip(HookSkip::StripFailed(path, msg)),
            StripOutcome::NoLang => log_skip(HookSkip::UnsupportedLang(path)),
        }
    }
}

fn log_skip(skip: HookSkip) {
    match skip {
        HookSkip::NotInGitChanges(path) => {
            eprintln!(
                "silence: skip {}: not in uncommitted changes",
                path.display()
            );
        }
        HookSkip::UnsupportedLang(path) => {
            eprintln!("silence: skip {}: unsupported language", path.display());
        }
        HookSkip::StripFailed(path, msg) => {
            eprintln!("silence: skip {}: {msg}", path.display());
        }
        HookSkip::OutsideRepo(path) => {
            eprintln!("silence: skip {}: outside repository root", path.display());
        }
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

fn hook_targets_from_stdin() -> Result<Vec<PathBuf>, String> {
    let mut input = String::new();
    std::io::stdin()
        .read_to_string(&mut input)
        .map_err(|e| e.to_string())?;
    hook_input::paths_from_stdin(&input)
}
