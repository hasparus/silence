use anyhow::{Context, Result};
use serde_json::{json, Value};
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Copy)]
pub enum Scope {
    User,
    Project,
}

#[derive(Clone, Copy)]
enum Op<'a> {
    Install { command: &'a str },
    Uninstall,
    Status,
}

struct Report {
    agent: &'static str,
    state: &'static str,
    path: PathBuf,
    note: Option<&'static str>,
}

pub fn install(scope: Scope) -> Result<()> {
    let bin = silence_bin();
    let command = silence_command(&bin);
    run(scope, &bin, Op::Install { command: &command }, "installing")
}

pub fn uninstall(scope: Scope) -> Result<()> {
    let bin = silence_bin();
    run(scope, &bin, Op::Uninstall, "removing")
}

pub fn status(scope: Scope) -> Result<()> {
    let bin = silence_bin();
    run(scope, &bin, Op::Status, "status of")
}

#[allow(clippy::unnecessary_wraps)]
fn run(scope: Scope, bin: &str, op: Op, verb: &str) -> Result<()> {
    println!(
        "{verb} silence post-edit hooks ({} scope)",
        scope_label(scope)
    );
    let reports = [
        json_agent(
            "Claude Code",
            claude_path(scope),
            "Write|Edit",
            JsonShape::WrappedInHooks,
            op,
            None,
        ),
        json_agent(
            "Codex",
            codex_path(scope),
            "apply_patch",
            JsonShape::PostToolUseAtRoot,
            op,
            Some("run /hooks in codex to trust"),
        ),
        file_agent("Opencode", opencode_path(scope), &opencode_plugin(bin), op),
        file_agent("Pi", pi_path(scope), &pi_extension(bin), op),
    ];
    let mut any_note = false;
    for r in &reports {
        println!(
            "  {:<13} {:<18} {}",
            r.agent,
            r.state,
            display_path(&r.path)
        );
        if r.note.is_some() && r.state == "installed" {
            any_note = true;
        }
    }
    if matches!(op, Op::Install { .. }) && any_note {
        println!();
        for r in &reports {
            if let (Some(note), "installed") = (r.note, r.state) {
                println!("  note: {} — {}", r.agent, note);
            }
        }
    }
    Ok(())
}

#[derive(Clone, Copy)]
enum JsonShape {
    WrappedInHooks,
    PostToolUseAtRoot,
}

fn json_agent(
    agent: &'static str,
    path: PathBuf,
    matcher: &str,
    shape: JsonShape,
    op: Op,
    note: Option<&'static str>,
) -> Report {
    let result: Result<&'static str> = (|| match op {
        Op::Install { command } => {
            let mut root = read_json(&path)?;
            let arr = post_tool_use(&mut root, shape)?;
            if arr.iter().any(is_silence_entry) {
                return Ok("already installed");
            }
            arr.push(json!({
                "matcher": matcher,
                "hooks": [{ "type": "command", "command": command }],
            }));
            write_json(&path, &root)?;
            Ok("installed")
        }
        Op::Uninstall => {
            if !path.exists() {
                return Ok("not set");
            }
            let mut root = read_json(&path)?;
            let arr = post_tool_use(&mut root, shape)?;
            let before = arr.len();
            arr.retain(|e| !is_silence_entry(e));
            if arr.len() == before {
                return Ok("not set");
            }
            write_json(&path, &root)?;
            Ok("removed")
        }
        Op::Status => {
            if !path.exists() {
                return Ok("not set");
            }
            let mut root = read_json(&path)?;
            let active = post_tool_use(&mut root, shape)?
                .iter()
                .any(is_silence_entry);
            Ok(if active { "active" } else { "not set" })
        }
    })();
    report(agent, path, result, note)
}

fn read_json(path: &Path) -> Result<Value> {
    if !path.exists() {
        return Ok(json!({}));
    }
    let text = std::fs::read_to_string(path)?;
    if text.trim().is_empty() {
        return Ok(json!({}));
    }
    serde_json::from_str(&text).with_context(|| format!("{} is not valid JSON", path.display()))
}

fn write_json(path: &Path, root: &Value) -> Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let mut text = serde_json::to_string_pretty(root)?;
    text.push('\n');
    std::fs::write(path, text)?;
    Ok(())
}

fn post_tool_use(root: &mut Value, shape: JsonShape) -> Result<&mut Vec<Value>> {
    let obj = root
        .as_object_mut()
        .context("config root is not a JSON object")?;
    let host = match shape {
        JsonShape::PostToolUseAtRoot => obj,
        JsonShape::WrappedInHooks => {
            let hooks = obj.entry("hooks").or_insert_with(|| json!({}));
            hooks
                .as_object_mut()
                .context("\"hooks\" is not a JSON object")?
        }
    };
    let post = host.entry("PostToolUse").or_insert_with(|| json!([]));
    post.as_array_mut()
        .context("\"PostToolUse\" is not a JSON array")
}

fn is_silence_entry(entry: &Value) -> bool {
    entry
        .get("hooks")
        .and_then(Value::as_array)
        .is_some_and(|hooks| {
            hooks.iter().any(|h| {
                h.get("command")
                    .and_then(Value::as_str)
                    .is_some_and(|c| c.contains("--hook") && c.contains("silence"))
            })
        })
}

fn file_agent(agent: &'static str, path: PathBuf, content: &str, op: Op) -> Report {
    let result: Result<&'static str> = (|| match op {
        Op::Install { .. } => {
            let existed = path.exists();
            if let Some(parent) = path.parent() {
                std::fs::create_dir_all(parent)?;
            }
            std::fs::write(&path, content)?;
            Ok(if existed { "updated" } else { "installed" })
        }
        Op::Uninstall => {
            if !path.exists() {
                return Ok("not set");
            }
            std::fs::remove_file(&path)?;
            Ok("removed")
        }
        Op::Status => Ok(if path.exists() { "active" } else { "not set" }),
    })();
    report(agent, path, result, None)
}

fn report(
    agent: &'static str,
    path: PathBuf,
    result: Result<&'static str>,
    note: Option<&'static str>,
) -> Report {
    let state = match result {
        Ok(s) => s,
        Err(e) => {
            eprintln!("  {agent}: {e:#}");
            "error"
        }
    };
    Report {
        agent,
        state,
        path,
        note,
    }
}

fn claude_path(scope: Scope) -> PathBuf {
    match scope {
        Scope::User => home().join(".claude/settings.json"),
        Scope::Project => PathBuf::from(".claude/settings.json"),
    }
}

fn codex_path(scope: Scope) -> PathBuf {
    match scope {
        Scope::User => home().join(".codex/hooks.json"),
        Scope::Project => PathBuf::from(".codex/hooks.json"),
    }
}

fn opencode_path(scope: Scope) -> PathBuf {
    match scope {
        Scope::User => home().join(".config/opencode/plugins/silence.js"),
        Scope::Project => PathBuf::from(".opencode/plugin/silence.js"),
    }
}

fn pi_path(scope: Scope) -> PathBuf {
    match scope {
        Scope::User => home().join(".pi/agent/extensions/silence.ts"),
        Scope::Project => PathBuf::from(".pi/extensions/silence.ts"),
    }
}

fn home() -> PathBuf {
    std::env::var_os("HOME")
        .or_else(|| std::env::var_os("USERPROFILE"))
        .map_or_else(|| PathBuf::from("."), PathBuf::from)
}

fn display_path(path: &Path) -> String {
    let home = home();
    path.strip_prefix(&home).map_or_else(
        |_| path.display().to_string(),
        |rest| format!("~/{}", rest.display()),
    )
}

fn scope_label(scope: Scope) -> &'static str {
    match scope {
        Scope::User => "user",
        Scope::Project => "project",
    }
}

fn silence_bin() -> String {
    std::env::current_exe()
        .ok()
        .and_then(|p| p.to_str().map(String::from))
        .unwrap_or_else(|| "silence".into())
}

fn silence_command(bin: &str) -> String {
    if bin.contains(char::is_whitespace) {
        format!("\"{bin}\" --hook")
    } else {
        format!("{bin} --hook")
    }
}

fn js_string(s: &str) -> String {
    serde_json::to_string(s).unwrap_or_else(|_| format!("{s:?}"))
}

fn opencode_plugin(bin: &str) -> String {
    format!(
        r#"// silence post-edit hook — installed by `silence --install-hook`.
// Strips agent-written comments. Delete this file (or run
// `silence --uninstall-hook`) to remove it.
const BIN = {bin};

export const SilencePlugin = async ({{ $ }}) => ({{
  "tool.execute.after": async (input, output) => {{
    const tool = input && input.tool;
    if (tool !== "write" && tool !== "edit") return;
    const args = (output && output.args) || (input && input.args) || {{}};
    const file = args.filePath;
    if (file) await $`${{BIN}} --hook ${{file}}`.quiet().nothrow();
  }},
}});
"#,
        bin = js_string(bin)
    )
}

fn pi_extension(bin: &str) -> String {
    format!(
        r#"// silence post-edit hook — installed by `silence --install-hook`.
// Strips agent-written comments. Delete this file (or run
// `silence --uninstall-hook`) to remove it.
import {{ execFile }} from "node:child_process";

const BIN = {bin};

export default function (pi: any) {{
  pi.on("tool_result", (event: any) => {{
    if (!event || event.isError) return;
    const tool = event.toolName;
    if (tool !== "edit" && tool !== "write") return;
    const file = event.input && event.input.path;
    if (file) execFile(BIN, ["--hook", String(file)], () => {{}});
  }});
}}
"#,
        bin = js_string(bin)
    )
}
