use anyhow::{Context, Result};
use clap::ValueEnum;
use serde_json::{json, Value};
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Copy)]
pub enum Scope {
    User,
    Project,
}

#[derive(Debug, Clone, Copy, Eq, PartialEq, ValueEnum)]
pub enum Agent {
    #[value(alias = "claude-code")]
    Claude,
    Codex,
    Opencode,
    Pi,
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

pub fn install(scope: Scope, agents: &[Agent]) -> Result<()> {
    let bin = silence_bin();
    let command = silence_command(&bin);
    run(
        scope,
        &bin,
        Op::Install { command: &command },
        "installing",
        agents,
    )
}

pub fn uninstall(scope: Scope, agents: &[Agent]) -> Result<()> {
    let bin = silence_bin();
    run(scope, &bin, Op::Uninstall, "removing", agents)
}

pub fn status(scope: Scope, agents: &[Agent]) -> Result<()> {
    let bin = silence_bin();
    run(scope, &bin, Op::Status, "status of", agents)
}

#[allow(clippy::unnecessary_wraps)]
fn run(scope: Scope, bin: &str, op: Op, verb: &str, agents: &[Agent]) -> Result<()> {
    println!(
        "{verb} silence post-edit hooks ({} scope)",
        scope_label(scope)
    );
    let selected = selected_agents(agents);
    let mut reports = Vec::with_capacity(selected.len());
    for agent in selected {
        match agent {
            Agent::Claude => reports.push(json_agent(
                "Claude Code",
                claude_path(scope),
                "Write|Edit",
                op,
                None,
            )),
            Agent::Codex => reports.push(codex_agent(
                codex_path(scope),
                "apply_patch",
                op,
                Some("run /hooks in codex to trust"),
            )),
            Agent::Opencode => reports.push(file_agent(
                "Opencode",
                opencode_path(scope),
                &opencode_plugin(bin),
                op,
            )),
            Agent::Pi => reports.push(file_agent("Pi", pi_path(scope), &pi_extension(bin), op)),
        }
    }
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

fn selected_agents(agents: &[Agent]) -> Vec<Agent> {
    if agents.is_empty() {
        vec![Agent::Claude, Agent::Codex, Agent::Opencode, Agent::Pi]
    } else {
        let mut selected = Vec::with_capacity(agents.len());
        for agent in agents {
            if !selected.contains(agent) {
                selected.push(*agent);
            }
        }
        selected
    }
}

fn json_agent(
    agent: &'static str,
    path: PathBuf,
    matcher: &str,
    op: Op,
    note: Option<&'static str>,
) -> Report {
    let result: Result<&'static str> = (|| match op {
        Op::Install { command } => {
            let mut root = read_json(&path)?;
            let arr = post_tool_use(&mut root)?;
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
            let arr = post_tool_use(&mut root)?;
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
            let active = post_tool_use(&mut root)?.iter().any(is_silence_entry);
            Ok(if active { "active" } else { "not set" })
        }
    })();
    report(agent, path, result, note)
}

fn codex_agent(path: PathBuf, matcher: &str, op: Op, note: Option<&'static str>) -> Report {
    let result: Result<&'static str> = (|| match op {
        Op::Install { command } => {
            let mut root = read_json(&path)?;
            remove_legacy_codex_silence_entry(&mut root)?;
            let arr = post_tool_use(&mut root)?;
            if update_codex_silence_entry(arr, matcher, command) {
                write_json(&path, &root)?;
                return Ok("updated");
            }
            if arr.iter().any(is_silence_entry) {
                return Ok("already installed");
            }
            arr.push(json!({
                "matcher": matcher,
                "hooks": [{
                    "type": "command",
                    "command": command,
                    "statusMessage": "Trimming comments",
                }],
            }));
            write_json(&path, &root)?;
            Ok("installed")
        }
        Op::Uninstall => {
            if !path.exists() {
                return Ok("not set");
            }
            let mut root = read_json(&path)?;
            let mut changed = remove_legacy_codex_silence_entry(&mut root)?;
            let arr = post_tool_use(&mut root)?;
            let before = arr.len();
            arr.retain(|e| !is_silence_entry(e));
            changed |= arr.len() != before;
            if !changed {
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
            let active = post_tool_use(&mut root)?.iter().any(is_silence_entry);
            Ok(if active { "active" } else { "not set" })
        }
    })();
    report("Codex", path, result, note)
}

fn update_codex_silence_entry(arr: &mut [Value], matcher: &str, command: &str) -> bool {
    let Some(entry) = arr.iter_mut().find(|entry| is_silence_entry(entry)) else {
        return false;
    };
    let mut changed = false;
    if entry.get("matcher").and_then(Value::as_str) != Some(matcher) {
        entry["matcher"] = json!(matcher);
        changed = true;
    }
    if let Some(hooks) = entry.get_mut("hooks").and_then(Value::as_array_mut) {
        for hook in hooks {
            let Some(existing) = hook.get("command").and_then(Value::as_str) else {
                continue;
            };
            if !(existing.contains("--hook") && existing.contains("silence")) {
                continue;
            }
            let command_changed = existing != command;
            if hook.get("type").and_then(Value::as_str) != Some("command") {
                hook["type"] = json!("command");
                changed = true;
            }
            if command_changed {
                hook["command"] = json!(command);
                changed = true;
            }
            if hook.get("statusMessage").and_then(Value::as_str) != Some("Trimming comments") {
                hook["statusMessage"] = json!("Trimming comments");
                changed = true;
            }
        }
    }
    changed
}

fn remove_legacy_codex_silence_entry(root: &mut Value) -> Result<bool> {
    let obj = root
        .as_object_mut()
        .context("config root is not a JSON object")?;
    let Some(post) = obj.get_mut("PostToolUse") else {
        return Ok(false);
    };
    let arr = post
        .as_array_mut()
        .context("\"PostToolUse\" is not a JSON array")?;
    let before = arr.len();
    arr.retain(|e| !is_silence_entry(e));
    let changed = arr.len() != before;
    if arr.is_empty() {
        obj.remove("PostToolUse");
    }
    Ok(changed)
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

fn post_tool_use(root: &mut Value) -> Result<&mut Vec<Value>> {
    let obj = root
        .as_object_mut()
        .context("config root is not a JSON object")?;
    let hooks = obj.entry("hooks").or_insert_with(|| json!({}));
    let host = hooks
        .as_object_mut()
        .context("\"hooks\" is not a JSON object")?;
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
    format!("{} --hook", shell_quote(bin))
}

fn shell_quote(s: &str) -> String {
    if s.is_empty() {
        return "''".into();
    }
    if s.bytes()
        .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'/' | b'.' | b'_' | b'-' | b':'))
    {
        return s.into();
    }
    format!("'{}'", s.replace('\'', "'\\''"))
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn silence_command_is_shell_safe() {
        assert_eq!(
            silence_command("/usr/local/bin/silence"),
            "/usr/local/bin/silence --hook"
        );
        assert_eq!(
            silence_command("/tmp/Codex Apps/silence"),
            "'/tmp/Codex Apps/silence' --hook"
        );
        assert_eq!(
            silence_command("/tmp/$postId's/silence"),
            "'/tmp/$postId'\\''s/silence' --hook"
        );
    }
}
