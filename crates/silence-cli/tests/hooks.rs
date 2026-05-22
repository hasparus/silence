use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

fn silence_bin() -> &'static Path {
    Path::new(env!("CARGO_BIN_EXE_silence"))
}

fn tmp(name: &str) -> PathBuf {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join(name);
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn run(dir: &Path, args: &[&str], extra_env: &[(&str, &Path)]) -> Output {
    let mut cmd = Command::new(silence_bin());
    cmd.args(args).current_dir(dir);
    for (k, v) in extra_env {
        cmd.env(k, v);
    }
    cmd.output().expect("the silence binary should run")
}

fn git(dir: &Path, args: &[&str]) {
    let out = Command::new("git")
        .args(args)
        .current_dir(dir)
        .output()
        .expect("git must be installed");
    assert!(
        out.status.success(),
        "git {args:?} failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
}

#[test]
fn install_hook_merges_into_existing_claude_settings_json() {
    let home = tmp("install-merge-home");
    std::fs::create_dir_all(home.join(".claude")).unwrap();
    let settings_path = home.join(".claude/settings.json");
    let pre_existing = r#"{
  "permissions": { "allow": ["Bash(ls *)"] },
  "hooks": {
    "PostToolUse": [
      { "matcher": "Bash", "hooks": [ { "type": "command", "command": "echo other" } ] }
    ]
  }
}"#;
    std::fs::write(&settings_path, pre_existing).unwrap();

    let cwd = tmp("install-merge-cwd");
    let out = run(&cwd, &["--install-hook"], &[("HOME", &home)]);
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );

    let merged: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(&settings_path).unwrap()).unwrap();

    assert_eq!(
        merged["permissions"]["allow"][0].as_str(),
        Some("Bash(ls *)")
    );

    let entries = merged["hooks"]["PostToolUse"].as_array().unwrap();
    assert_eq!(entries.len(), 2, "user's existing PostToolUse must survive");
    let commands: Vec<&str> = entries
        .iter()
        .flat_map(|e| e["hooks"].as_array().unwrap())
        .map(|h| h["command"].as_str().unwrap())
        .collect();
    assert!(commands.contains(&"echo other"));
    assert!(commands
        .iter()
        .any(|c| c.contains("silence") && c.contains("--hook")));

    let again = run(&cwd, &["--install-hook"], &[("HOME", &home)]);
    assert!(again.status.success());
    let after: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(&settings_path).unwrap()).unwrap();
    assert_eq!(after["hooks"]["PostToolUse"].as_array().unwrap().len(), 2);

    let un = run(&cwd, &["--uninstall-hook"], &[("HOME", &home)]);
    assert!(un.status.success());
    let after_un: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(&settings_path).unwrap()).unwrap();
    let remaining = after_un["hooks"]["PostToolUse"].as_array().unwrap();
    assert_eq!(remaining.len(), 1);
    assert_eq!(
        remaining[0]["hooks"][0]["command"].as_str(),
        Some("echo other")
    );
    assert_eq!(
        after_un["permissions"]["allow"][0].as_str(),
        Some("Bash(ls *)")
    );
}

#[test]
fn hook_only_strips_inside_the_uncommitted_change() {
    let repo = tmp("hook-uncommitted");
    git(&repo, &["init", "-q"]);
    git(&repo, &["config", "user.email", "t@t"]);
    git(&repo, &["config", "user.name", "t"]);
    git(&repo, &["config", "commit.gpgsign", "false"]);

    std::fs::write(
        repo.join("a.rs"),
        "fn a() {}\n// committed comment, must survive\n",
    )
    .unwrap();
    git(&repo, &["add", "."]);
    git(&repo, &["commit", "-q", "-m", "baseline", "--no-gpg-sign"]);

    std::fs::write(
        repo.join("a.rs"),
        "fn a() {}\n// committed comment, must survive\nfn b() {} // agent slop\n",
    )
    .unwrap();

    let out = run(&repo, &["--hook", "a.rs"], &[]);
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );

    assert_eq!(
        std::fs::read_to_string(repo.join("a.rs")).unwrap(),
        "fn a() {}\n// committed comment, must survive\nfn b() {}\n",
        "hook must touch only the new (uncommitted) line"
    );
}

#[test]
fn hook_reads_file_path_from_stdin_json() {
    let repo = tmp("hook-stdin-json");
    git(&repo, &["init", "-q"]);
    git(&repo, &["config", "user.email", "t@t"]);
    git(&repo, &["config", "user.name", "t"]);
    git(&repo, &["config", "commit.gpgsign", "false"]);
    std::fs::write(repo.join("a.rs"), "fn a() {}\n").unwrap();
    git(&repo, &["add", "."]);
    git(&repo, &["commit", "-q", "-m", "baseline", "--no-gpg-sign"]);

    std::fs::write(repo.join("a.rs"), "fn a() {}\nfn b() {} // slop\n").unwrap();

    let path = repo.join("a.rs");
    let payload = format!(
        "{{\"tool_name\":\"Edit\",\"tool_input\":{{\"file_path\":{}}}}}",
        serde_json::to_string(&path.to_string_lossy().to_string()).unwrap()
    );
    let mut child = Command::new(silence_bin())
        .args(["--hook"])
        .current_dir(&repo)
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .unwrap();
    child
        .stdin
        .as_mut()
        .unwrap()
        .write_all(payload.as_bytes())
        .unwrap();
    let out = child.wait_with_output().unwrap();
    assert!(out.status.success());
    assert_eq!(
        std::fs::read_to_string(repo.join("a.rs")).unwrap(),
        "fn a() {}\nfn b() {}\n"
    );
}

#[test]
fn install_hook_uses_codex_root_shape_not_claude_shape() {
    let home = tmp("codex-shape-home");
    let cwd = tmp("codex-shape-cwd");

    let out = run(&cwd, &["--install-hook"], &[("HOME", &home)]);
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );

    let codex_hooks: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(home.join(".codex/hooks.json")).unwrap())
            .unwrap();

    assert!(
        codex_hooks.get("PostToolUse").is_some(),
        "codex hooks.json must have PostToolUse at the root, got: {codex_hooks}"
    );
    assert!(
        codex_hooks.get("hooks").is_none(),
        "codex hooks.json must NOT wrap in a `hooks` key (that's Claude's shape)"
    );
    let entries = codex_hooks["PostToolUse"].as_array().unwrap();
    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0]["matcher"].as_str(), Some("apply_patch"));
    let command = entries[0]["hooks"][0]["command"].as_str().unwrap();
    assert!(command.contains("silence") && command.contains("--hook"));
}

#[test]
fn install_hook_opencode_uses_singular_project_and_plural_global() {
    let home = tmp("opencode-paths-home");
    let cwd = tmp("opencode-paths-cwd");

    let user = run(&cwd, &["--install-hook"], &[("HOME", &home)]);
    assert!(user.status.success());
    assert!(
        home.join(".config/opencode/plugins/silence.js").exists(),
        "user-scope opencode plugin must land in .config/opencode/plugins/ (PLURAL)"
    );
    assert!(
        !home.join(".config/opencode/plugin/silence.js").exists(),
        "must not also write to the singular variant"
    );

    let scoped = run(&cwd, &["--install-hook", "--project"], &[("HOME", &home)]);
    assert!(scoped.status.success());
    assert!(
        cwd.join(".opencode/plugin/silence.js").exists(),
        "project-scope opencode plugin must land in .opencode/plugin/ (SINGULAR)"
    );
}

#[test]
fn generated_plugin_and_extension_have_verified_field_names() {
    let home = tmp("plugin-content-home");
    let cwd = tmp("plugin-content-cwd");
    let out = run(&cwd, &["--install-hook"], &[("HOME", &home)]);
    assert!(out.status.success());

    let opencode_plugin =
        std::fs::read_to_string(home.join(".config/opencode/plugins/silence.js")).unwrap();
    assert!(
        opencode_plugin.contains("\"tool.execute.after\""),
        "opencode plugin must hook tool.execute.after"
    );
    assert!(
        opencode_plugin.contains("args.filePath"),
        "opencode write/edit tools use `filePath` — that's what the plugin must read"
    );
    assert!(opencode_plugin.contains("--hook"));

    let pi_extension =
        std::fs::read_to_string(home.join(".pi/agent/extensions/silence.ts")).unwrap();
    assert!(
        pi_extension.contains("\"tool_result\""),
        "pi extension must subscribe to the tool_result event"
    );
    assert!(
        pi_extension.contains("event.input && event.input.path"),
        "pi's edit/write tools use `path` (not file_path)"
    );
    assert!(pi_extension.contains("--hook"));
}

#[test]
fn hook_extracts_path_from_codex_apply_patch_payload() {
    let repo = tmp("hook-codex-apply-patch");
    git(&repo, &["init", "-q"]);
    git(&repo, &["config", "user.email", "t@t"]);
    git(&repo, &["config", "user.name", "t"]);
    git(&repo, &["config", "commit.gpgsign", "false"]);
    std::fs::write(repo.join("a.rs"), "fn a() {}\n").unwrap();
    git(&repo, &["add", "."]);
    git(&repo, &["commit", "-q", "-m", "baseline", "--no-gpg-sign"]);

    std::fs::write(repo.join("a.rs"), "fn a() {}\nfn b() {} // slop\n").unwrap();

    let payload = r#"{
      "hook_event_name": "PostToolUse",
      "tool_name": "apply_patch",
      "tool_input": {
        "input": "*** Begin Patch\n*** Update File: a.rs\n@@\n fn a() {}\n+fn b() {} // slop\n*** End Patch\n"
      }
    }"#;

    let mut child = Command::new(silence_bin())
        .args(["--hook"])
        .current_dir(&repo)
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .unwrap();
    child
        .stdin
        .as_mut()
        .unwrap()
        .write_all(payload.as_bytes())
        .unwrap();
    let out = child.wait_with_output().unwrap();
    assert!(out.status.success());
    assert_eq!(
        std::fs::read_to_string(repo.join("a.rs")).unwrap(),
        "fn a() {}\nfn b() {}\n",
        "hook must find the file path inside the patch text and strip the slop"
    );
}
