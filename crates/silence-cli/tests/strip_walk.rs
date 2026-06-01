mod common;

use common::{run_silence, tmp, TestResult};

#[test]
fn recursive_strip_respects_ignore_without_git_repo() -> TestResult {
    for (name, tmp_name, ignore_filename) in [
        ("gitignore", "walk-gitignore-no-repo", ".gitignore"),
        (
            "silenceignore",
            "walk-silenceignore-no-repo",
            ".silenceignore",
        ),
    ] {
        let dir = tmp(tmp_name)?;
        std::fs::write(dir.join(ignore_filename), "ignored.ts\n")?;
        std::fs::write(dir.join("ignored.ts"), "const ignored = 1; // keep\n")?;
        std::fs::write(dir.join("kept.ts"), "const kept = 1; // remove\n")?;

        let out = run_silence(&dir, &["strip", ".", "--recursive"], &[])?;

        assert!(
            out.status.success(),
            "{name}: {}",
            String::from_utf8_lossy(&out.stderr)
        );
        assert_eq!(
            std::fs::read_to_string(dir.join("ignored.ts"))?,
            "const ignored = 1; // keep\n",
            "{name}: ignored.ts"
        );
        assert_eq!(
            std::fs::read_to_string(dir.join("kept.ts"))?,
            "const kept = 1;\n",
            "{name}: kept.ts"
        );
    }
    Ok(())
}
