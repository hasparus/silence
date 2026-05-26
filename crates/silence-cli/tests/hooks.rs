mod common;

use std::io::Write;
use std::process::Command;

use common::{git, run_silence, silence_bin, tmp, TestResult};

#[test]
fn install_hook_merges_into_existing_claude_settings_json() -> TestResult {
    let home = tmp("install-merge-home")?;
    std::fs::create_dir_all(home.join(".claude"))?;
    let settings_path = home.join(".claude/settings.json");
    let pre_existing = r#"{
  "permissions": { "allow": ["Bash(ls *)"] },
  "hooks": {
    "PostToolUse": [
      { "matcher": "Bash", "hooks": [ { "type": "command", "command": "echo other" } ] }
    ]
  }
}"#;
    std::fs::write(&settings_path, pre_existing)?;

    let cwd = tmp("install-merge-cwd")?;
    let out = run_silence(&cwd, &["hooks", "install"], &[("HOME", &home)])?;
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );

    let merged: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(&settings_path)?)?;

    assert_eq!(
        merged["permissions"]["allow"][0].as_str(),
        Some("Bash(ls *)")
    );

    let entries = merged["hooks"]["PostToolUse"]
        .as_array()
        .ok_or("PostToolUse must be an array")?;
    assert_eq!(entries.len(), 2, "user's existing PostToolUse must survive");
    let mut commands: Vec<String> = Vec::new();
    for entry in entries {
        let hooks = entry["hooks"]
            .as_array()
            .ok_or("entry.hooks must be an array")?;
        for h in hooks {
            let command_text = h["command"]
                .as_str()
                .ok_or("hook.command must be a string")?;
            commands.push(command_text.to_string());
        }
    }
    assert!(commands.iter().any(|c| c == "echo other"));
    assert!(commands
        .iter()
        .any(|c| c.contains("silence") && c.contains(" hook")));

    let again = run_silence(&cwd, &["hooks", "install"], &[("HOME", &home)])?;
    assert!(again.status.success());
    let after: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(&settings_path)?)?;
    assert_eq!(
        after["hooks"]["PostToolUse"]
            .as_array()
            .ok_or("PostToolUse must be an array")?
            .len(),
        2
    );

    let un = run_silence(&cwd, &["hooks", "uninstall"], &[("HOME", &home)])?;
    assert!(un.status.success());
    let after_un: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(&settings_path)?)?;
    let remaining = after_un["hooks"]["PostToolUse"]
        .as_array()
        .ok_or("PostToolUse must be an array")?;
    assert_eq!(remaining.len(), 1);
    assert_eq!(
        remaining[0]["hooks"][0]["command"].as_str(),
        Some("echo other")
    );
    assert_eq!(
        after_un["permissions"]["allow"][0].as_str(),
        Some("Bash(ls *)")
    );
    Ok(())
}

#[test]
fn hook_preserves_doc_comments_alongside_stripping_regular_ones() -> TestResult {
    let repo = tmp("hook-doc-comments")?;
    git(&repo, &["init", "-q"])?;
    git(&repo, &["config", "user.email", "t@t"])?;
    git(&repo, &["config", "user.name", "t"])?;
    git(&repo, &["config", "commit.gpgsign", "false"])?;

    std::fs::write(repo.join("a.rs"), "fn baseline() {}\n")?;
    git(&repo, &["add", "."])?;
    git(&repo, &["commit", "-q", "-m", "baseline", "--no-gpg-sign"])?;

    let new_contents = "fn baseline() {}\n\
/// outer doc — must survive\n\
/// second line of outer doc\n\
pub fn documented() {}\n\
//! inner doc — must survive\n\
/** block doc — must survive */\n\
pub fn other() {}\n\
// regular line comment — must be stripped\n\
let x = 5; // trailing regular — must be stripped\n";
    std::fs::write(repo.join("a.rs"), new_contents)?;

    let out = run_silence(&repo, &["hook", "a.rs"], &[])?;
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );

    let after = std::fs::read_to_string(repo.join("a.rs"))?;
    assert!(
        after.contains("/// outer doc — must survive"),
        "outer doc comment was stripped:\n{after}"
    );
    assert!(
        after.contains("/// second line of outer doc"),
        "second line of outer doc was stripped:\n{after}"
    );
    assert!(
        after.contains("//! inner doc — must survive"),
        "inner doc comment was stripped:\n{after}"
    );
    assert!(
        after.contains("/** block doc — must survive */"),
        "block doc comment was stripped:\n{after}"
    );
    assert!(
        !after.contains("regular line comment"),
        "regular line comment survived:\n{after}"
    );
    assert!(
        !after.contains("trailing regular"),
        "trailing regular comment survived:\n{after}"
    );
    Ok(())
}

#[test]
fn hook_only_strips_inside_the_uncommitted_change() -> TestResult {
    let repo = tmp("hook-uncommitted")?;
    git(&repo, &["init", "-q"])?;
    git(&repo, &["config", "user.email", "t@t"])?;
    git(&repo, &["config", "user.name", "t"])?;
    git(&repo, &["config", "commit.gpgsign", "false"])?;

    std::fs::write(
        repo.join("a.rs"),
        "fn a() {}\n// committed comment, must survive\n",
    )?;
    git(&repo, &["add", "."])?;
    git(&repo, &["commit", "-q", "-m", "baseline", "--no-gpg-sign"])?;

    std::fs::write(
        repo.join("a.rs"),
        "fn a() {}\n// committed comment, must survive\nfn b() {} // agent slop\n",
    )?;

    let out = run_silence(&repo, &["hook", "a.rs"], &[])?;
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );

    assert_eq!(
        std::fs::read_to_string(repo.join("a.rs"))?,
        "fn a() {}\n// committed comment, must survive\nfn b() {}\n",
        "hook must touch only the new (uncommitted) line"
    );
    Ok(())
}

#[test]
fn hook_reads_file_path_from_stdin_json() -> TestResult {
    let repo = tmp("hook-stdin-json")?;
    git(&repo, &["init", "-q"])?;
    git(&repo, &["config", "user.email", "t@t"])?;
    git(&repo, &["config", "user.name", "t"])?;
    git(&repo, &["config", "commit.gpgsign", "false"])?;
    std::fs::write(repo.join("a.rs"), "fn a() {}\n")?;
    git(&repo, &["add", "."])?;
    git(&repo, &["commit", "-q", "-m", "baseline", "--no-gpg-sign"])?;

    std::fs::write(repo.join("a.rs"), "fn a() {}\nfn b() {} // slop\n")?;

    let path = repo.join("a.rs");
    let path_str = serde_json::to_string(&path.to_string_lossy().to_string())?;
    let payload = format!(
        "{{\"hook_event_name\":\"PostToolUse\",\"tool_name\":\"Edit\",\"tool_input\":{{\"file_path\":{path_str}}}}}"
    );
    let mut child = Command::new(silence_bin())
        .args(["hook"])
        .current_dir(&repo)
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()?;
    child
        .stdin
        .as_mut()
        .ok_or("child stdin must be open")?
        .write_all(payload.as_bytes())?;
    let out = child.wait_with_output()?;
    assert!(out.status.success());
    assert_eq!(
        std::fs::read_to_string(repo.join("a.rs"))?,
        "fn a() {}\nfn b() {}\n"
    );
    Ok(())
}

#[test]
fn install_hook_uses_claude_write_edit_and_multiedit_matcher() -> TestResult {
    let home = tmp("claude-matcher-home")?;
    let cwd = tmp("claude-matcher-cwd")?;

    let out = run_silence(
        &cwd,
        &["hooks", "install", "--to", "claude"],
        &[("HOME", &home)],
    )?;
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );

    let settings: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(home.join(".claude/settings.json"))?)?;
    assert_eq!(
        settings["hooks"]["PostToolUse"][0]["matcher"].as_str(),
        Some("Write|Edit|MultiEdit")
    );
    Ok(())
}

#[test]
fn install_hook_uses_codex_hooks_wrapper_shape() -> TestResult {
    let home = tmp("codex-shape-home")?;
    let cwd = tmp("codex-shape-cwd")?;

    let out = run_silence(&cwd, &["hooks", "install"], &[("HOME", &home)])?;
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );

    let codex_hooks: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(home.join(".codex/hooks.json"))?)?;

    assert!(
        codex_hooks.get("hooks").is_some(),
        "codex hooks.json must wrap hooks under a `hooks` key, got: {codex_hooks}"
    );
    assert!(
        codex_hooks.get("PostToolUse").is_none(),
        "codex hooks.json must not keep legacy root PostToolUse, got: {codex_hooks}"
    );
    let entries = codex_hooks["hooks"]["PostToolUse"]
        .as_array()
        .ok_or("PostToolUse must be an array")?;
    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0]["matcher"].as_str(), Some("apply_patch"));
    let command = entries[0]["hooks"][0]["command"]
        .as_str()
        .ok_or("hook.command must be a string")?;
    assert!(command.contains("silence") && command.contains(" hook"));
    assert_eq!(
        entries[0]["hooks"][0]["statusMessage"].as_str(),
        Some("Trimming comments")
    );
    Ok(())
}

#[test]
fn install_hook_to_codex_only_does_not_touch_other_agents() -> TestResult {
    let home = tmp("codex-only-home")?;
    let cwd = tmp("codex-only-cwd")?;

    let out = run_silence(
        &cwd,
        &["hooks", "install", "--to", "codex"],
        &[("HOME", &home)],
    )?;
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );

    assert!(home.join(".codex/hooks.json").exists());
    assert!(!home.join(".claude/settings.json").exists());
    assert!(!home.join(".config/opencode/plugins/silence.ts").exists());
    assert!(!home.join(".pi/agent/extensions/silence.ts").exists());

    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(stdout.contains("Codex"));
    assert!(!stdout.contains("Claude Code"));
    assert!(!stdout.contains("Opencode"));
    assert!(!stdout.contains("Pi"));
    Ok(())
}

#[test]
fn install_hook_accepts_multiple_to_flags() -> TestResult {
    let home = tmp("multi-to-home")?;
    let cwd = tmp("multi-to-cwd")?;

    let out = run_silence(
        &cwd,
        &["hooks", "install", "--to", "codex", "--to", "claude"],
        &[("HOME", &home)],
    )?;
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );

    assert!(home.join(".codex/hooks.json").exists());
    assert!(home.join(".claude/settings.json").exists());
    assert!(!home.join(".config/opencode/plugins/silence.ts").exists());
    assert!(!home.join(".pi/agent/extensions/silence.ts").exists());
    Ok(())
}

#[test]
fn install_hook_migrates_legacy_codex_root_shape() -> TestResult {
    let home = tmp("codex-legacy-home")?;
    let cwd = tmp("codex-legacy-cwd")?;
    std::fs::create_dir_all(home.join(".codex"))?;
    std::fs::write(
        home.join(".codex/hooks.json"),
        r#"{
  "PostToolUse": [
    {
      "matcher": "apply_patch",
      "hooks": [
        { "type": "command", "command": "/old/silence hook" }
      ]
    }
  ]
}"#,
    )?;

    let out = run_silence(
        &cwd,
        &["hooks", "install", "--to", "codex"],
        &[("HOME", &home)],
    )?;
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );

    let codex_hooks: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(home.join(".codex/hooks.json"))?)?;
    assert!(codex_hooks.get("PostToolUse").is_none());
    let entries = codex_hooks["hooks"]["PostToolUse"]
        .as_array()
        .ok_or("PostToolUse must be an array")?;
    assert_eq!(entries.len(), 1);
    let command = entries[0]["hooks"][0]["command"]
        .as_str()
        .ok_or("hook.command must be a string")?;
    assert!(command.contains("silence") && command.contains(" hook"));
    Ok(())
}

#[test]
fn install_hook_updates_existing_codex_entry_with_status_message() -> TestResult {
    let home = tmp("codex-update-home")?;
    let cwd = tmp("codex-update-cwd")?;
    std::fs::create_dir_all(home.join(".codex"))?;
    std::fs::write(
        home.join(".codex/hooks.json"),
        r#"{
  "hooks": {
    "PostToolUse": [
      {
        "matcher": "apply_patch",
        "hooks": [
          { "type": "command", "command": "/old/silence hook" }
        ]
      }
    ]
  }
}"#,
    )?;

    let out = run_silence(
        &cwd,
        &["hooks", "install", "--to", "codex"],
        &[("HOME", &home)],
    )?;
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );

    let codex_hooks: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(home.join(".codex/hooks.json"))?)?;
    let hook = &codex_hooks["hooks"]["PostToolUse"][0]["hooks"][0];
    assert_eq!(hook["statusMessage"].as_str(), Some("Trimming comments"));
    assert_ne!(hook["command"].as_str(), Some("/old/silence hook"));
    assert!(String::from_utf8_lossy(&out.stdout).contains("updated"));
    Ok(())
}

#[test]
fn install_hook_opencode_uses_singular_project_and_plural_global() -> TestResult {
    let home = tmp("opencode-paths-home")?;
    let cwd = tmp("opencode-paths-cwd")?;

    let user = run_silence(&cwd, &["hooks", "install"], &[("HOME", &home)])?;
    assert!(user.status.success());
    assert!(
        home.join(".config/opencode/plugins/silence.ts").exists(),
        "user-scope opencode plugin must land in .config/opencode/plugins/ (PLURAL)"
    );
    assert!(
        !home.join(".config/opencode/plugin/silence.ts").exists(),
        "must not also write to the singular variant"
    );

    let scoped = run_silence(&cwd, &["hooks", "install", "--project"], &[("HOME", &home)])?;
    assert!(scoped.status.success());
    assert!(
        cwd.join(".opencode/plugin/silence.ts").exists(),
        "project-scope opencode plugin must land in .opencode/plugin/ (SINGULAR)"
    );
    Ok(())
}

#[test]
fn generated_plugin_and_extension_have_verified_field_names() -> TestResult {
    let home = tmp("plugin-content-home")?;
    let cwd = tmp("plugin-content-cwd")?;
    let out = run_silence(&cwd, &["hooks", "install"], &[("HOME", &home)])?;
    assert!(out.status.success());

    let opencode_plugin =
        std::fs::read_to_string(home.join(".config/opencode/plugins/silence.ts"))?;
    assert!(
        opencode_plugin.contains("\"tool.execute.after\""),
        "opencode plugin must hook tool.execute.after"
    );
    assert!(
        opencode_plugin.contains("@opencode-ai/plugin"),
        "opencode plugin must use official plugin types"
    );
    assert!(
        opencode_plugin.contains("isFileTool") && opencode_plugin.contains("hasFilePath"),
        "opencode write/edit tools use `filePath` on input.args"
    );
    assert!(
        opencode_plugin.contains("apply_patch") && opencode_plugin.contains("hasPatchText"),
        "opencode apply_patch uses `patchText` — plugin must forward it to hook stdin"
    );
    assert!(
        opencode_plugin.contains("execFile(BIN"),
        "opencode plugin should execute without shell expansion"
    );
    assert!(opencode_plugin.contains("[\"hook\""));

    let pi_extension = std::fs::read_to_string(home.join(".pi/agent/extensions/silence.ts"))?;
    assert!(
        pi_extension.contains("\"tool_result\""),
        "pi extension must subscribe to the tool_result event"
    );
    assert!(
        pi_extension.contains("isEditToolResult") && pi_extension.contains("isWriteToolResult"),
        "pi extension must narrow edit/write tool results"
    );
    assert!(
        pi_extension.contains("event.input.path"),
        "pi's edit/write tools use `path` (not file_path)"
    );
    assert!(pi_extension.contains("[\"hook\""));
    Ok(())
}

#[test]
fn hook_extracts_path_from_opencode_patch_text_payload() -> TestResult {
    let repo = tmp("hook-opencode-patch-text")?;
    git(&repo, &["init", "-q"])?;
    git(&repo, &["config", "user.email", "t@t"])?;
    git(&repo, &["config", "user.name", "t"])?;
    git(&repo, &["config", "commit.gpgsign", "false"])?;
    std::fs::write(repo.join("a.rs"), "fn a() {}\n")?;
    git(&repo, &["add", "."])?;
    git(&repo, &["commit", "-q", "-m", "baseline", "--no-gpg-sign"])?;

    std::fs::write(repo.join("a.rs"), "fn a() {}\nfn b() {} // slop\n")?;

    let payload = r#"{
      "tool": "apply_patch",
      "args": {
        "patchText": "*** Begin Patch\n*** Update File: a.rs\n@@\n fn a() {}\n+fn b() {} // slop\n*** End Patch\n"
      }
    }"#;

    let mut child = Command::new(silence_bin())
        .args(["hook"])
        .current_dir(&repo)
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()?;
    child
        .stdin
        .as_mut()
        .ok_or("child stdin must be open")?
        .write_all(payload.as_bytes())?;
    let out = child.wait_with_output()?;
    assert!(out.status.success());
    assert_eq!(
        std::fs::read_to_string(repo.join("a.rs"))?,
        "fn a() {}\nfn b() {}\n"
    );
    Ok(())
}

#[test]
fn hook_extracts_path_from_codex_apply_patch_payload() -> TestResult {
    let repo = tmp("hook-codex-apply-patch")?;
    git(&repo, &["init", "-q"])?;
    git(&repo, &["config", "user.email", "t@t"])?;
    git(&repo, &["config", "user.name", "t"])?;
    git(&repo, &["config", "commit.gpgsign", "false"])?;
    std::fs::write(repo.join("a.rs"), "fn a() {}\n")?;
    git(&repo, &["add", "."])?;
    git(&repo, &["commit", "-q", "-m", "baseline", "--no-gpg-sign"])?;

    std::fs::write(repo.join("a.rs"), "fn a() {}\nfn b() {} // slop\n")?;

    let payload = r#"{
      "hook_event_name": "PostToolUse",
      "tool_name": "apply_patch",
      "tool_input": {
        "input": "*** Begin Patch\n*** Update File: a.rs\n@@\n fn a() {}\n+fn b() {} // slop\n*** End Patch\n"
      }
    }"#;

    let mut child = Command::new(silence_bin())
        .args(["hook"])
        .current_dir(&repo)
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()?;
    child
        .stdin
        .as_mut()
        .ok_or("child stdin must be open")?
        .write_all(payload.as_bytes())?;
    let out = child.wait_with_output()?;
    assert!(out.status.success());
    assert_eq!(
        std::fs::read_to_string(repo.join("a.rs"))?,
        "fn a() {}\nfn b() {}\n",
        "hook must find the file path inside the patch text and strip the slop"
    );
    Ok(())
}

#[test]
fn hook_in_mixed_staged_file_does_not_strip_committed_comments() -> TestResult {
    let repo = tmp("hook-mixed-staged")?;
    git(&repo, &["init", "-q"])?;
    git(&repo, &["config", "user.email", "t@t"])?;
    git(&repo, &["config", "user.name", "t"])?;
    git(&repo, &["config", "commit.gpgsign", "false"])?;
    std::fs::write(
        repo.join("a.rs"),
        "fn a() {}\n// committed comment, must survive\n",
    )?;
    git(&repo, &["add", "."])?;
    git(&repo, &["commit", "-q", "-m", "baseline", "--no-gpg-sign"])?;

    std::fs::write(
        repo.join("a.rs"),
        "fn a() {}\n// committed comment, must survive\nfn staged() {} // staged\n",
    )?;
    git(&repo, &["add", "a.rs"])?;
    std::fs::write(
        repo.join("a.rs"),
        "fn a() {}\n// committed comment, must survive\nfn staged() {} // staged\nfn working() {} // working\n",
    )?;

    let out = run_silence(&repo, &["hook", "a.rs"], &[])?;
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );

    assert_eq!(
        std::fs::read_to_string(repo.join("a.rs"))?,
        "fn a() {}\n// committed comment, must survive\nfn staged() {}\nfn working() {}\n"
    );
    Ok(())
}
