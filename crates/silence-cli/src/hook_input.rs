use serde::Deserialize;
use std::path::PathBuf;

use silence_core::Lines;

/// `tool_response` stays untyped here on purpose: it is a tool's own output, the
/// most harness-variable part of the payload. Parsing it strictly would let one
/// unexpected shape fail the whole event and strip nothing at all.
#[derive(Debug, Deserialize, Default)]
#[serde(default)]
struct HookStdin {
    args: Option<HookArgs>,
    tool_input: Option<HookArgs>,
    tool_response: Option<serde_json::Value>,
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

/// Claude Code reports what a `Write`/`Edit` actually changed in the tool result:
/// `structuredPatch` carries the hunks, `type` distinguishes a new file from a
/// rewrite. Other harnesses send neither, so the job carries no lines and the
/// caller falls back to git.
#[derive(Debug, Deserialize, Default)]
#[serde(default)]
struct ToolResponse {
    #[serde(alias = "filePath")]
    file_path: Option<String>,
    #[serde(rename = "type")]
    kind: Option<String>,
    #[serde(rename = "structuredPatch")]
    structured_patch: Option<Vec<Hunk>>,
}

#[derive(Debug, Deserialize, Default)]
#[serde(default)]
struct Hunk {
    #[serde(rename = "newStart")]
    new_start: usize,
    lines: Vec<String>,
}

#[derive(Debug)]
pub struct HookJob {
    pub path: PathBuf,
    /// What the harness said the write touched. `None` means it said nothing,
    /// so the caller falls back to git.
    pub lines: Option<Lines>,
}

pub fn jobs_from_stdin(input: &str) -> Result<Vec<HookJob>, String> {
    if input.trim().is_empty() {
        return Ok(Vec::new());
    }
    let event: HookStdin = serde_json::from_str(input).map_err(|e| e.to_string())?;
    Ok(event.into_jobs())
}

impl HookStdin {
    fn into_jobs(self) -> Vec<HookJob> {
        let mut paths = Vec::new();
        if let Some(args) = self.args.or(self.tool_input) {
            args.push_paths(&mut paths);
        }

        let patched = self
            .tool_response
            .and_then(|raw| serde_json::from_value::<ToolResponse>(raw).ok())
            .and_then(ToolResponse::into_patched);
        if let Some((path, _)) = &patched {
            paths.push(path.clone());
        }

        paths
            .into_iter()
            .map(|path| {
                let lines = match &patched {
                    Some((target, lines)) if *target == path => Some(lines.clone()),
                    _ => None,
                };
                HookJob { path, lines }
            })
            .collect()
    }
}

impl ToolResponse {
    fn into_patched(self) -> Option<(PathBuf, Lines)> {
        let hunks = self.structured_patch?;
        let lines = if self.kind.as_deref() == Some("create") {
            Lines::All
        } else {
            Lines::Ranges(added_ranges(&hunks))
        };
        Some((PathBuf::from(self.file_path?), lines))
    }
}

/// Line numbers refer to the post-write file, so `-` lines do not advance the
/// counter and `+` lines are what the agent put there. jsdiff also emits
/// `\ No newline at end of file` as a hunk line; it is a marker, not content,
/// and counting it would shift every range after it.
fn added_ranges(hunks: &[Hunk]) -> Vec<(usize, usize)> {
    let mut out: Vec<(usize, usize)> = Vec::new();
    for hunk in hunks {
        let mut line = hunk.new_start;
        for raw in &hunk.lines {
            match raw.chars().next() {
                Some('-' | '\\') => {}
                Some('+') => {
                    match out.last_mut() {
                        Some(last) if last.1 + 1 == line => last.1 = line,
                        _ => out.push((line, line)),
                    }
                    line += 1;
                }
                _ => line += 1,
            }
        }
    }
    out
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
    use std::path::Path;

    type TestResult = Result<(), Box<dyn std::error::Error>>;

    fn paths_from_stdin(input: &str) -> Result<Vec<PathBuf>, String> {
        Ok(jobs_from_stdin(input)?
            .into_iter()
            .map(|j| j.path)
            .collect())
    }

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

    #[test]
    fn no_tool_response_is_unknown() -> TestResult {
        let jobs = jobs_from_stdin(r#"{"tool_input":{"file_path":"/tmp/a.rs"}}"#)?;
        assert_eq!(jobs[0].lines, None);
        Ok(())
    }

    #[test]
    fn edit_patch_yields_added_lines_only() -> TestResult {
        let jobs = jobs_from_stdin(
            r#"{"tool_input":{"file_path":"/tmp/a.py"},
                "tool_response":{"filePath":"/tmp/a.py","structuredPatch":[
                  {"newStart":10,"newLines":4,"lines":[
                    " def a():","-    return 1","+    # new","+    return 2"]}]}}"#,
        )?;
        assert_eq!(jobs[0].lines, Some(Lines::Ranges(vec![(11, 12)])));
        Ok(())
    }

    #[test]
    fn deleted_lines_do_not_advance_the_counter() -> TestResult {
        let jobs = jobs_from_stdin(
            r#"{"tool_response":{"filePath":"a.rs","structuredPatch":[
                 {"newStart":1,"newLines":2,"lines":["-a","-b","+c"," d","+e"]}]}}"#,
        )?;
        assert_eq!(jobs[0].lines, Some(Lines::Ranges(vec![(1, 1), (3, 3)])));
        Ok(())
    }

    #[test]
    fn no_newline_marker_does_not_shift_ranges() -> TestResult {
        let jobs = jobs_from_stdin(
            r#"{"tool_response":{"filePath":"a.rs","structuredPatch":[
                 {"newStart":1,"newLines":2,"lines":[
                   "-old","\\ No newline at end of file","+agent"," human"]}]}}"#,
        )?;
        assert_eq!(jobs[0].lines, Some(Lines::Ranges(vec![(1, 1)])));
        Ok(())
    }

    #[test]
    fn write_create_covers_the_whole_file() -> TestResult {
        let jobs = jobs_from_stdin(
            r#"{"tool_response":{"type":"create","filePath":"/tmp/n.rs","structuredPatch":[]}}"#,
        )?;
        assert_eq!(jobs[0].lines, Some(Lines::All));
        Ok(())
    }

    #[test]
    fn write_update_that_changed_nothing_is_empty() -> TestResult {
        let jobs = jobs_from_stdin(
            r#"{"tool_response":{"type":"update","filePath":"/tmp/n.rs","structuredPatch":[]}}"#,
        )?;
        assert_eq!(jobs[0].lines, Some(Lines::Ranges(Vec::new())));
        Ok(())
    }

    #[test]
    fn patch_scope_does_not_leak_to_other_paths() -> TestResult {
        let jobs = jobs_from_stdin(
            r#"{"tool_input":{"input":"*** Update File: b.rs\n"},
                "tool_response":{"filePath":"a.rs","structuredPatch":[
                  {"newStart":1,"newLines":1,"lines":["+x"]}]}}"#,
        )?;
        let b = jobs
            .iter()
            .find(|j| j.path == Path::new("b.rs"))
            .ok_or("b.rs missing from jobs")?;
        assert_eq!(b.lines, None);
        Ok(())
    }
}
