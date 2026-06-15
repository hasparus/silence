use anyhow::{Context, Result};
use clap::ValueEnum;
use serde_json::{json, Value};
use std::path::{Path, PathBuf};

use silence_grammars::paths::{display_home_relative, home_dir};

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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum JsonAgent {
    Claude,
    Codex,
}

enum AgentKind {
    Json {
        name: &'static str,
        path: fn(Scope) -> PathBuf,
        matcher: &'static str,
        note: Option<&'static str>,
        agent: JsonAgent,
    },
    File {
        name: &'static str,
        path: fn(Scope) -> PathBuf,
        content: fn(&str) -> String,
    },
}

pub fn install(scope: Scope, agents: &[Agent]) {
    let bin = silence_bin();
    let command = silence_command(&bin);
    run(
        scope,
        Op::Install { command: &command },
        "installing",
        agents,
    );
}

pub fn uninstall(scope: Scope, agents: &[Agent]) {
    run(scope, Op::Uninstall, "removing", agents);
}

pub fn status(scope: Scope, agents: &[Agent]) {
    run(scope, Op::Status, "status of", agents);
}

fn run(scope: Scope, op: Op, verb: &str, agents: &[Agent]) {
    println!(
        "{verb} silence post-edit hooks ({} scope)",
        scope_label(scope)
    );
    let selected = selected_agents(agents);
    let mut reports = Vec::with_capacity(selected.len());
    for agent in selected {
        reports.push(apply_agent(agent, scope, op));
    }
    let mut any_note = false;
    for r in &reports {
        println!(
            "  {:<13} {:<18} {}",
            r.agent,
            r.state,
            display_home_relative(&r.path)
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
}

fn agent_kind(agent: Agent) -> AgentKind {
    match agent {
        Agent::Claude => AgentKind::Json {
            name: "Claude Code",
            path: claude_path,
            matcher: "Write|Edit|MultiEdit",
            note: None,
            agent: JsonAgent::Claude,
        },
        Agent::Codex => AgentKind::Json {
            name: "Codex",
            path: codex_path,
            matcher: "apply_patch",
            note: Some("run /hooks in codex to trust"),
            agent: JsonAgent::Codex,
        },
        Agent::Opencode => AgentKind::File {
            name: "Opencode",
            path: opencode_path,
            content: opencode_plugin,
        },
        Agent::Pi => AgentKind::File {
            name: "Pi",
            path: pi_path,
            content: pi_extension,
        },
    }
}

fn apply_agent(agent: Agent, scope: Scope, op: Op) -> Report {
    match agent_kind(agent) {
        AgentKind::Json {
            name,
            path,
            matcher,
            note,
            agent,
        } => json_hook(name, path(scope), matcher, agent, op, note),
        AgentKind::File {
            name,
            path,
            content,
        } => {
            let bin = silence_bin();
            file_hook(name, path(scope), &content(&bin), op)
        }
    }
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

fn json_hook(
    agent_name: &'static str,
    path: PathBuf,
    matcher: &str,
    agent: JsonAgent,
    op: Op,
    note: Option<&'static str>,
) -> Report {
    let is_codex = agent == JsonAgent::Codex;
    let result: Result<&'static str> = (|| match op {
        Op::Install { command } => {
            let mut root = read_json(&path)?;
            if is_codex {
                remove_legacy_codex_silence_entry(&mut root)?;
            }
            let arr = post_tool_use(&mut root)?;
            if is_codex && update_codex_silence_entry(arr, matcher, command) {
                write_json(&path, &root)?;
                return Ok("updated");
            }
            if arr.iter().any(is_silence_entry) {
                return Ok("already installed");
            }
            let entry = match agent {
                JsonAgent::Codex => json!({
                    "matcher": matcher,
                    "hooks": [{
                        "type": "command",
                        "command": command,
                        "statusMessage": "Trimming comments",
                    }],
                }),
                JsonAgent::Claude => json!({
                    "matcher": matcher,
                    "hooks": [{ "type": "command", "command": command }],
                }),
            };
            arr.push(entry);
            write_json(&path, &root)?;
            Ok("installed")
        }
        Op::Uninstall => {
            if !path.exists() {
                return Ok("not set");
            }
            let mut root = read_json(&path)?;
            let mut changed = if is_codex {
                remove_legacy_codex_silence_entry(&mut root)?
            } else {
                false
            };
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
    report(agent_name, path, result, note)
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
            if !is_silence_hook_command(existing) {
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

fn is_silence_hook_command(command: &str) -> bool {
    let parts: Vec<&str> = command.split_whitespace().collect();
    parts.last() == Some(&"hook") && parts.iter().any(|p| p.contains("silence"))
}

fn is_silence_entry(entry: &Value) -> bool {
    entry
        .get("hooks")
        .and_then(Value::as_array)
        .is_some_and(|hooks| {
            hooks.iter().any(|h| {
                h.get("command")
                    .and_then(Value::as_str)
                    .is_some_and(is_silence_hook_command)
            })
        })
}

fn file_hook(agent: &'static str, path: PathBuf, content: &str, op: Op) -> Report {
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
        Scope::User => home_dir().join(".claude/settings.json"),
        Scope::Project => PathBuf::from(".claude/settings.json"),
    }
}

fn codex_path(scope: Scope) -> PathBuf {
    match scope {
        Scope::User => home_dir().join(".codex/hooks.json"),
        Scope::Project => PathBuf::from(".codex/hooks.json"),
    }
}

fn opencode_path(scope: Scope) -> PathBuf {
    match scope {
        Scope::User => home_dir().join(".config/opencode/plugins/silence.ts"),
        Scope::Project => PathBuf::from(".opencode/plugin/silence.ts"),
    }
}

fn pi_path(scope: Scope) -> PathBuf {
    match scope {
        Scope::User => home_dir().join(".pi/agent/extensions/silence.ts"),
        Scope::Project => PathBuf::from(".pi/extensions/silence.ts"),
    }
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
    format!("{} hook", shell_quote(bin))
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

/// Opencode plugin. Its `tool.execute.after` handler returns `void`, so the
/// only way to feed the model is to mutate the passed `output` in place;
/// mutations to `output.output` reach the model for built-in write/edit/patch
/// tools (not MCP tools). See sst/opencode#3384. The asset ships comment-free
/// because silence's own hook strips its comments — hence this note lives here.
fn opencode_plugin(bin: &str) -> String {
    include_str!("../assets/opencode-plugin.ts").replace("__BIN__", &js_string(bin))
}

/// Pi extension. Its `tool_result` handler returns a partial patch that pi
/// merges into the LLM-visible result, so we return `{ content: [...] }` with
/// the note appended. Asset is comment-free for the same reason as above.
fn pi_extension(bin: &str) -> String {
    include_str!("../assets/pi-extension.ts").replace("__BIN__", &js_string(bin))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn silence_command_is_shell_safe() {
        assert_eq!(
            silence_command("/usr/local/bin/silence"),
            "/usr/local/bin/silence hook"
        );
        assert_eq!(
            silence_command("/tmp/Codex Apps/silence"),
            "'/tmp/Codex Apps/silence' hook"
        );
        assert_eq!(
            silence_command("/tmp/$postId's/silence"),
            "'/tmp/$postId'\\''s/silence' hook"
        );
    }
}
