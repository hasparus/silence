mod common;

use common::{run_silence, tmp, TestResult};

#[test]
fn recursive_strip_respects_gitignore_without_git_repo() -> TestResult {
    let dir = tmp("walk-gitignore-no-repo")?;
    std::fs::write(dir.join(".gitignore"), "ignored.ts\n")?;
    std::fs::write(dir.join("ignored.ts"), "const ignored = 1; // keep\n")?;
    std::fs::write(dir.join("kept.ts"), "const kept = 1; // remove\n")?;

    let out = run_silence(&dir, &["strip", ".", "--recursive"], &[])?;

    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert_eq!(
        std::fs::read_to_string(dir.join("ignored.ts"))?,
        "const ignored = 1; // keep\n"
    );
    assert_eq!(
        std::fs::read_to_string(dir.join("kept.ts"))?,
        "const kept = 1;\n"
    );
    Ok(())
}

#[test]
fn recursive_strip_respects_silenceignore_without_git_repo() -> TestResult {
    let dir = tmp("walk-silenceignore-no-repo")?;
    std::fs::write(dir.join(".silenceignore"), "ignored.ts\n")?;
    std::fs::write(dir.join("ignored.ts"), "const ignored = 1; // keep\n")?;
    std::fs::write(dir.join("kept.ts"), "const kept = 1; // remove\n")?;

    let out = run_silence(&dir, &["strip", ".", "--recursive"], &[])?;

    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert_eq!(
        std::fs::read_to_string(dir.join("ignored.ts"))?,
        "const ignored = 1; // keep\n"
    );
    assert_eq!(
        std::fs::read_to_string(dir.join("kept.ts"))?,
        "const kept = 1;\n"
    );
    Ok(())
}
