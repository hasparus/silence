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

#[test]
fn strips_subdirs_by_default_without_recursive_flag() -> TestResult {
    let dir = tmp("walk-default-recursive")?;
    std::fs::create_dir_all(dir.join("sub/deep"))?;
    std::fs::write(dir.join("top.ts"), "const a = 1; // remove\n")?;
    std::fs::write(dir.join("sub/deep/nested.ts"), "const b = 2; // remove\n")?;

    let out = run_silence(&dir, &["strip", "."], &[])?;
    assert!(out.status.success(), "{}", String::from_utf8_lossy(&out.stderr));

    assert_eq!(std::fs::read_to_string(dir.join("top.ts"))?, "const a = 1;\n");
    assert_eq!(
        std::fs::read_to_string(dir.join("sub/deep/nested.ts"))?,
        "const b = 2;\n",
        "nested file must be stripped without -r"
    );
    Ok(())
}
