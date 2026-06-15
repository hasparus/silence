use serde::Deserialize;
use std::path::PathBuf;

#[derive(Debug, Deserialize, Default)]
#[serde(default)]
struct HookStdin {
    args: Option<HookArgs>,
    tool_input: Option<HookArgs>,
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

pub fn paths_from_stdin(input: &str) -> Result<Vec<PathBuf>, String> {
    if input.trim().is_empty() {
        return Ok(Vec::new());
    }
    let event: HookStdin = serde_json::from_str(input).map_err(|e| e.to_string())?;
    Ok(event.into_paths())
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
        let paths = paths_from_stdin(
            r#"{"hook_event_name":"PostToolUse","tool_input":{"file_path":"/tmp/a.rs"}}"#,
        )?;
        assert_eq!(paths, vec![PathBuf::from("/tmp/a.rs")]);
        Ok(())
    }

    #[test]
    fn opencode_args_patch_text() -> TestResult {
        let paths =
            paths_from_stdin(r#"{"args":{"patchText":"*** Update File: src/a.rs\n+// x\n"}}"#)?;
        assert_eq!(paths, vec![PathBuf::from("src/a.rs")]);
        Ok(())
    }

    #[test]
    fn codex_tool_input_patch_in_input_field() -> TestResult {
        let paths = paths_from_stdin(
            r#"{"tool_input":{"input":"*** Update File: b.rs\n*** End Patch\n"}}"#,
        )?;
        assert_eq!(paths, vec![PathBuf::from("b.rs")]);
        Ok(())
    }

    #[test]
    fn empty_stdin_is_ok() -> TestResult {
        assert!(paths_from_stdin("  ")?.is_empty());
        Ok(())
    }

    #[test]
    fn invalid_json_fails() {
        assert!(paths_from_stdin("{").is_err());
    }
}
