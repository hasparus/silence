mod config;
mod git;
mod hooks;

use anyhow::Result;
use clap::Parser;
use config::LoadedConfig;
use ignore::WalkBuilder;
use rayon::prelude::*;
use silence_core::{
    strip, CommentKinds, LineMode, Options, PreserveConfig, DEFAULT_PRESERVE_PATTERNS,
};
use silence_langs::Lang;
use std::collections::HashMap;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};

#[derive(Parser, Debug)]
#[command(
    name = "silence",
    version,
    about = "Remove or check for comments in source code, using tree-sitter."
)]
#[allow(clippy::struct_excessive_bools)]
struct Cli {
    #[arg(conflicts_with_all = ["staged", "unstaged", "changes"])]
    paths: Vec<PathBuf>,

    #[arg(long)]
    check: bool,

    #[arg(short, long)]
    recursive: bool,

    #[arg(long)]
    preserve_lines: bool,

    /// Don't apply the built-in preserve list (TODO/FIXME/lint directives).
    #[arg(long)]
    no_default_preserve: bool,

    #[arg(long)]
    inline: bool,
    #[arg(long)]
    block: bool,

    #[arg(long)]
    backup: bool,

    #[arg(long, conflicts_with_all = ["unstaged", "changes"])]
    staged: bool,
    #[arg(long, conflicts_with_all = ["staged", "changes"])]
    unstaged: bool,
    #[arg(long, visible_alias = "changes-only", conflicts_with_all = ["staged", "unstaged"])]
    changes: bool,

    #[arg(long)]
    hook: bool,

    #[arg(long)]
    install_hook: bool,
    #[arg(long)]
    uninstall_hook: bool,
    #[arg(long = "hook-status", visible_alias = "list-hooks")]
    hook_status: bool,
    #[arg(long)]
    project: bool,

    #[arg(long)]
    config: bool,
    #[arg(long)]
    create_config: bool,
    #[arg(long)]
    llm: bool,

    #[arg(long)]
    threads: Option<usize>,

    #[arg(long)]
    verbose: bool,
}

fn main() {
    if let Err(e) = run() {
        eprintln!("error: {e:#}");
        std::process::exit(2);
    }
}

#[allow(clippy::too_many_lines)]
fn run() -> Result<()> {
    let cli = Cli::parse();

    if cli.llm {
        print!("{LLM_GUIDE}");
        return Ok(());
    }
    if cli.create_config {
        return create_config();
    }
    let hook_scope = if cli.project {
        hooks::Scope::Project
    } else {
        hooks::Scope::User
    };
    if cli.install_hook {
        return hooks::install(hook_scope);
    }
    if cli.uninstall_hook {
        return hooks::uninstall(hook_scope);
    }
    if cli.hook_status {
        return hooks::status(hook_scope);
    }

    if let Some(n) = cli.threads {
        rayon::ThreadPoolBuilder::new()
            .num_threads(n)
            .build_global()
            .ok();
    }

    let cwd = std::env::current_dir()?;
    let loaded = LoadedConfig::discover(&cwd);
    let preserve = loaded.preserve(cli.no_default_preserve);
    for bad in preserve.invalid_patterns() {
        eprintln!("warning: ignoring invalid preserve pattern: {bad}");
    }

    if cli.config {
        print_config(&loaded, cli.no_default_preserve);
        return Ok(());
    }

    if cli.hook {
        return run_hook(&cli.paths, &preserve);
    }

    if cli.verbose {
        match &loaded.path {
            Some(p) => eprintln!("config: {}", p.display()),
            None => eprintln!("config: built-in defaults"),
        }
    }

    let settings = Settings {
        line_mode: if cli.preserve_lines {
            LineMode::PreserveLines
        } else {
            LineMode::Collapse
        },
        preserve,
        kinds: kinds_from_flags(cli.inline, cli.block),
        check: cli.check,
        backup: cli.backup,
        verbose: cli.verbose,
    };

    let git_scope = if cli.staged {
        Some(git::Scope::Staged)
    } else if cli.unstaged {
        Some(git::Scope::Unstaged)
    } else if cli.changes {
        Some(git::Scope::All)
    } else {
        None
    };

    let jobs: Vec<(PathBuf, Vec<(usize, usize)>)> = if let Some(scope) = git_scope {
        let ch = git::changes(scope)?;
        ch.files
            .into_iter()
            .map(|(rel, ranges)| (ch.root.join(rel), ranges))
            .filter(|(p, _)| lang_for(p).is_some())
            .collect()
    } else {
        if cli.paths.is_empty() {
            anyhow::bail!(
                "at least one path is required unless using --staged/--unstaged/--changes"
            );
        }
        let mut out = Vec::new();
        for p in &cli.paths {
            for f in collect_paths(p, cli.recursive)? {
                out.push((f, Vec::new()));
            }
        }
        out.sort_by(|a, b| a.0.cmp(&b.0));
        out.dedup_by(|a, b| a.0 == b.0);
        out
    };

    if jobs.is_empty() {
        eprintln!("no supported files to process");
        return Ok(());
    }

    let removed_total = AtomicUsize::new(0);
    let files_with_comments = AtomicUsize::new(0);
    let settings_ref = &settings;

    let results: Vec<JobResult> = jobs
        .par_iter()
        .map(|(path, ranges)| process_one(path, ranges, settings_ref))
        .collect();

    let mut error_count = 0usize;
    for r in &results {
        match r {
            JobResult::Ok { removed } if *removed > 0 => {
                removed_total.fetch_add(*removed, Ordering::Relaxed);
                files_with_comments.fetch_add(1, Ordering::Relaxed);
            }
            JobResult::Err { path, msg } => {
                error_count += 1;
                eprintln!("  skip {}: {msg}", path.display());
            }
            JobResult::Ok { .. } => {}
        }
    }

    let total = removed_total.load(Ordering::Relaxed);
    let nfiles = files_with_comments.load(Ordering::Relaxed);

    if cli.check {
        if total > 0 {
            eprintln!("{total} comment(s) in {nfiles} file(s) would be removed");
        } else if cli.verbose {
            eprintln!("no removable comments found");
        }
    } else if cli.verbose {
        eprintln!("removed {total} comment(s) across {nfiles} file(s)");
    }

    if error_count > 0 {
        eprintln!("{error_count} file(s) could not be processed");
        std::process::exit(2);
    }
    if cli.check && total > 0 {
        std::process::exit(1);
    }

    Ok(())
}

struct Settings {
    line_mode: LineMode,
    preserve: PreserveConfig,
    kinds: CommentKinds,
    check: bool,
    backup: bool,
    verbose: bool,
}

enum JobResult {
    Ok { removed: usize },
    Err { path: PathBuf, msg: String },
}

fn kinds_from_flags(inline: bool, block: bool) -> CommentKinds {
    match (inline, block) {
        (true, false) => CommentKinds {
            line: true,
            block: false,
        },
        (false, true) => CommentKinds {
            line: false,
            block: true,
        },
        _ => CommentKinds::default(),
    }
}

fn process_one(path: &Path, ranges: &[(usize, usize)], s: &Settings) -> JobResult {
    let Some(lang) = lang_for(path) else {
        return JobResult::Ok { removed: 0 };
    };
    let source = match std::fs::read_to_string(path) {
        Ok(s) => s,
        Err(e) => {
            return JobResult::Err {
                path: path.to_path_buf(),
                msg: e.to_string(),
            }
        }
    };

    let opts = Options {
        line_mode: s.line_mode,
        preserve: s.preserve.clone(),
        line_ranges: ranges.to_vec(),
        kinds: s.kinds,
    };

    match strip(&source, lang, &opts) {
        Ok(outcome) => {
            if outcome.removed > 0 {
                if s.check {
                    println!("{}: {} comment(s)", path.display(), outcome.removed);
                } else if outcome.output != source {
                    if s.backup {
                        if let Err(e) = std::fs::copy(path, backup_path(path)) {
                            return JobResult::Err {
                                path: path.to_path_buf(),
                                msg: format!("backup failed: {e}"),
                            };
                        }
                    }
                    if let Err(e) = std::fs::write(path, &outcome.output) {
                        return JobResult::Err {
                            path: path.to_path_buf(),
                            msg: e.to_string(),
                        };
                    }
                    if s.verbose {
                        eprintln!("  {} (-{} comments)", path.display(), outcome.removed);
                    }
                }
            }
            JobResult::Ok {
                removed: outcome.removed,
            }
        }
        Err(e) => JobResult::Err {
            path: path.to_path_buf(),
            msg: e.to_string(),
        },
    }
}

fn backup_path(path: &Path) -> PathBuf {
    let mut s = path.as_os_str().to_os_string();
    s.push(".bak");
    PathBuf::from(s)
}

fn lang_for(path: &Path) -> Option<Lang> {
    let ext = path.extension()?.to_str()?;
    Lang::from_extension(ext)
}

fn collect_paths(root: &Path, recursive: bool) -> Result<Vec<PathBuf>> {
    if root.is_file() {
        return Ok(vec![root.to_path_buf()]);
    }
    let mut builder = WalkBuilder::new(root);
    builder.standard_filters(true);
    builder.add_custom_ignore_filename(".silenceignore");
    if let Some(home) = std::env::var_os("HOME") {
        let global = PathBuf::from(home).join(".config/.silenceignore");
        if global.is_file() {
            let _ = builder.add_ignore(global);
        }
    }
    if !recursive {
        builder.max_depth(Some(1));
    }
    let mut out = Vec::new();
    for entry in builder.build() {
        let entry = entry?;
        let p = entry.path();
        if p.is_file() && lang_for(p).is_some() {
            out.push(p.to_path_buf());
        }
    }
    Ok(out)
}

#[allow(clippy::unnecessary_wraps)]
fn run_hook(explicit: &[PathBuf], preserve: &PreserveConfig) -> Result<()> {
    let mut targets: Vec<PathBuf> = if explicit.is_empty() {
        let mut input = String::new();
        let mut from_stdin = Vec::new();
        if std::io::stdin().read_to_string(&mut input).is_ok() && !input.trim().is_empty() {
            if let Ok(v) = serde_json::from_str::<serde_json::Value>(&input) {
                collect_json_paths(&v, &mut from_stdin);
            }
        }
        from_stdin
    } else {
        explicit.to_vec()
    };

    targets.sort();
    targets.dedup();
    targets.retain(|p| p.is_file() && lang_for(p).is_some());
    if targets.is_empty() {
        return Ok(());
    }

    let git = hook_git_ranges();

    for path in &targets {
        let ranges = match &git {
            Some(map) => {
                let canon = path.canonicalize().unwrap_or_else(|_| path.clone());
                match map.get(&canon) {
                    Some(r) => r.clone(),
                    None => continue,
                }
            }
            None => Vec::new(),
        };
        let Ok(source) = std::fs::read_to_string(path) else {
            continue;
        };
        let Some(lang) = lang_for(path) else {
            continue;
        };
        let opts = Options {
            line_mode: LineMode::Collapse,
            preserve: preserve.clone(),
            line_ranges: ranges,
            kinds: CommentKinds::default(),
        };
        if let Ok(outcome) = strip(&source, lang, &opts) {
            if outcome.removed > 0
                && outcome.output != source
                && std::fs::write(path, &outcome.output).is_ok()
            {
                eprintln!(
                    "silence: stripped {} comment(s) from {}",
                    outcome.removed,
                    path.display()
                );
            }
        }
    }
    Ok(())
}

fn hook_git_ranges() -> Option<HashMap<PathBuf, Vec<(usize, usize)>>> {
    let ch = git::changes(git::Scope::All).ok()?;
    let mut map = HashMap::new();
    for (rel, ranges) in ch.files {
        let abs = ch.root.join(rel);
        let key = abs.canonicalize().unwrap_or(abs);
        map.insert(key, ranges);
    }
    Some(map)
}

fn collect_json_paths(v: &serde_json::Value, out: &mut Vec<PathBuf>) {
    use serde_json::Value;
    match v {
        Value::Object(map) => {
            for (k, val) in map {
                if let Value::String(s) = val {
                    match k.as_str() {
                        "file_path" | "filePath" | "path" | "filename" | "file" => {
                            out.push(PathBuf::from(s));
                        }
                        "patch" | "diff" | "input" => collect_patch_paths(s, out),
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

fn print_config(loaded: &LoadedConfig, no_default_preserve: bool) {
    match &loaded.path {
        Some(p) => println!("config file: {}", p.display()),
        None => println!("config file: none (built-in defaults)"),
    }
    println!("user preserve patterns:");
    if loaded.user_patterns().is_empty() {
        println!("  (none)");
    } else {
        for p in loaded.user_patterns() {
            println!("  {p}");
        }
    }
    let defaults_on = loaded.uses_defaults(no_default_preserve);
    println!(
        "built-in preserve list: {}",
        if defaults_on { "active" } else { "disabled" }
    );
    if defaults_on {
        for p in DEFAULT_PRESERVE_PATTERNS {
            println!("  {p}");
        }
        println!("directive detection: active (@tag, namespace:value, <xml/>)");
    }
}

fn create_config() -> Result<()> {
    let path = std::env::current_dir()?.join(".silence.toml");
    if path.exists() {
        anyhow::bail!("{} already exists", path.display());
    }
    std::fs::write(&path, CONFIG_TEMPLATE)?;
    println!("wrote {}", path.display());
    Ok(())
}

const CONFIG_TEMPLATE: &str = r#"# silence configuration — https://github.com/hasparus/silence
#
# Comments matching any of these substrings (or globs) are kept. Merged with
# the built-in list (TODO, FIXME, HACK, lint directives, ...) by default.
preserve = ["TODO", "FIXME", "*IMPORTANT*"]

# Set to false to drop the built-in preserve list and directive detection,
# keeping only the patterns above.
# use_default_preserve = true
"#;

const LLM_GUIDE: &str = "\
silence — remove slop comments from source code (tree-sitter based).

USAGE
  silence <path>              strip comments from a file or directory
  silence <path> -r           recurse into subdirectories
  silence <path> --check      report only; exit 1 if comments would be removed
  silence --staged            strip comments inside staged git hunks
  silence --changes           strip comments inside all uncommitted changes

KEEP RULES
  Ordinary comments are removed. Kept by default: TODO/FIXME/HACK/XXX/SAFETY,
  common lint directives, and directive-shaped comments (@ts-ignore,
  //go:embed, /// <reference/>). Add more in .silence.toml.

LANGUAGES
  Rust, TypeScript, JavaScript, JSX/TSX, Python, Go.
  Respects .gitignore and .silenceignore.

AGENT GUIDANCE
  Do not write comments that restate the code or narrate the change. To clean
  a file, run `silence <file>` rather than deleting comments by hand.
";
