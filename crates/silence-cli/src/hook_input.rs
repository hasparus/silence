use serde::Deserialize;
use std::path::PathBuf;

#[derive(Debug, Deserialize, Default)]
#[serde(default)]
struct HookStdin {
    args: Option<HookArgs>,
    tool_input: Option<HookArgs>,
    hook_event_name: Option<String>,
}

/// Parsed agent hook event: the files it touched plus, for Claude Code, the
/// event name (e.g. `PostToolUse`) we echo back when feeding context to the model.
pub struct HookEvent {
    pub paths: Vec<PathBuf>,
    pub claude_event: Option<String>,
}

impl HookEvent {
    /// An event sourced from explicit paths (Codex/Opencode/Pi, CLI), with no
    /// Claude `additionalContext` channel.
    pub fn from_paths(paths: Vec<PathBuf>) -> Self {
        Self {
            paths,
            claude_event: None,
        }
    }
}

#[derive(Debug, Deserialize, Default)]
#[serde(default)]
struct HookArgs {
    #[serde(alias = "filePath", alias = "path", alias = "filename", alias = "file")]
    file_path: Option<String>,
    #[serde(alias = "patchText")]
    patch: Option<String>,
    diff: Option<String>,
    input: Option<String>,
}

pub fn event_from_stdin(input: &str) -> Result<HookEvent, String> {
    if input.trim().is_empty() {
        return Ok(HookEvent {
            paths: Vec::new(),
            claude_event: None,
        });
    }
    let event: HookStdin = serde_json::from_str(input).map_err(|e| e.to_string())?;
    let claude_event = event
        .hook_event_name
        .clone()
        .filter(|name| !name.is_empty());
    Ok(HookEvent {
        paths: event.into_paths(),
        claude_event,
    })
}

impl HookStdin {
    fn into_paths(self) -> Vec<PathBuf> {
        let mut out = Vec::new();
        if let Some(args) = self.args.or(self.tool_input) {
            args.push_paths(&mut out);
        }
        out.sort();
        out.dedup();
        out
    }
}

impl HookArgs {
    fn push_paths(&self, out: &mut Vec<PathBuf>) {
        if let Some(path) = &self.file_path {
            out.push(PathBuf::from(path));
        }
        if let Some(patch) = self.patch_text() {
            paths_from_patch(patch, out);
        }
    }

    fn patch_text(&self) -> Option<&str> {
        self.patch
            .as_deref()
            .or(self.diff.as_deref())
            .or(self.input.as_deref())
    }
}

fn paths_from_patch(patch: &str, out: &mut Vec<PathBuf>) {
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

#[cfg(test)]
mod tests {
    use super::*;

    type TestResult = Result<(), Box<dyn std::error::Error>>;

    #[test]
    fn claude_tool_input_file_path() -> TestResult {
        let event = event_from_stdin(
            r#"{"hook_event_name":"PostToolUse","tool_input":{"file_path":"/tmp/a.rs"}}"#,
        )?;
        assert_eq!(event.paths, vec![PathBuf::from("/tmp/a.rs")]);
        assert_eq!(event.claude_event.as_deref(), Some("PostToolUse"));
        Ok(())
    }

    #[test]
    fn opencode_args_patch_text() -> TestResult {
        let event =
            event_from_stdin(r#"{"args":{"patchText":"*** Update File: src/a.rs\n+// x\n"}}"#)?;
        assert_eq!(event.paths, vec![PathBuf::from("src/a.rs")]);
        assert_eq!(event.claude_event, None);
        Ok(())
    }

    #[test]
    fn codex_tool_input_patch_in_input_field() -> TestResult {
        let event = event_from_stdin(
            r#"{"tool_input":{"input":"*** Update File: b.rs\n*** End Patch\n"}}"#,
        )?;
        assert_eq!(event.paths, vec![PathBuf::from("b.rs")]);
        assert_eq!(event.claude_event, None);
        Ok(())
    }

    #[test]
    fn empty_stdin_is_ok() -> TestResult {
        let event = event_from_stdin("  ")?;
        assert!(event.paths.is_empty());
        assert_eq!(event.claude_event, None);
        Ok(())
    }

    #[test]
    fn invalid_json_fails() {
        assert!(event_from_stdin("{").is_err());
    }
}
