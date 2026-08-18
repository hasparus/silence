use silence_core::{strip, CommentKinds, Error, LineMode, Lines, Options, PreserveConfig};
use silence_langs::Lang;

type TestResult = Result<(), Box<dyn std::error::Error>>;

fn strip_default(src: &str, lang: Lang) -> Result<String, Error> {
    Ok(strip(src, lang, &Options::default())?.output)
}

/// The pair is what protects the pair. A sentinel with no partner in the
/// file is indistinguishable from prose, so it is treated as prose.
#[test]
fn only_a_sentinel_with_its_partner_survives() -> TestResult {
    let src = "let a = 1; // codegen-start\nlet b = 2; // slop\nlet c = 3; // codegen-end\n";
    let out = strip_default(src, Lang::Rust)?;
    assert!(out.contains("codegen-start"), "{out}");
    assert!(out.contains("codegen-end"), "{out}");
    assert!(!out.contains("slop"), "{out}");

    let lone = strip_default("let a = 1; // codegen-end\n", Lang::Rust)?;
    assert_eq!(lone, "let a = 1;\n");
    Ok(())
}

/// C and Java put a banner fence on its own line behind a `*` gutter, so
/// the sentinel is not the first token of the comment.
#[test]
fn a_gutter_does_not_hide_the_sentinel() -> TestResult {
    let src = "/*\n * codegen-start abc\n */\nlet x = 1; // slop\n/*\n * codegen-end abc\n */\n";
    let out = strip_default(src, Lang::Rust)?;
    assert!(out.contains("codegen-start abc"), "{out}");
    assert!(out.contains("codegen-end abc"), "{out}");
    assert!(!out.contains("slop"), "{out}");
    Ok(())
}

/// A pair fences the region between its halves. Halves in the wrong order
/// fence nothing, so they are a coincidence of wording, not a marker.
#[test]
fn a_close_before_its_open_is_not_a_pair() -> TestResult {
    let src = "let a = 1; // zeta-end came first\nlet b = 2; // zeta-start came second\n";
    assert_eq!(strip_default(src, Lang::Rust)?, "let a = 1;\nlet b = 2;\n");
    Ok(())
}

/// front-end, cold-start and friends have a sentinel's exact shape. Nothing
/// but the missing partner separates them from a real marker.
#[test]
fn hyphenated_compounds_are_still_prose() -> TestResult {
    let src = "let a = 1; // front-end only\nlet b = 2; // cold-start path\nlet c = 3; // dead-end\nlet d = 4; // Back-End\n";
    let out = strip_default(src, Lang::Rust)?;
    assert_eq!(out, "let a = 1;\nlet b = 2;\nlet c = 3;\nlet d = 4;\n");
    Ok(())
}

#[test]
fn markers_go_away_with_directive_detection_off() -> TestResult {
    let src = "let a = 1; // codegen-start\nlet b = 2; // codegen-end\n";
    let opts = Options {
        preserve: PreserveConfig::empty(),
        ..Default::default()
    };
    assert_eq!(
        strip(src, Lang::Rust, &opts)?.output,
        "let a = 1;\nlet b = 2;\n"
    );
    Ok(())
}

#[test]
fn jsx_machine_markers_survive_while_prose_goes() -> TestResult {
    let src = "<div>\n      {/* impeccable-variants-start cd383158 */}\n      {/* the card */}\n      <Card />\n      {/* impeccable-variants-end cd383158 */}\n    </div>\n";
    let out = strip_default(src, Lang::Tsx)?;
    assert!(out.contains("impeccable-variants-start cd383158"));
    assert!(out.contains("impeccable-variants-end cd383158"));
    assert!(!out.contains("the card"));
    Ok(())
}

#[test]
fn comment_marker_inside_string_is_not_removed() -> TestResult {
    let src = r#"let url = "http://example.com"; // strip me"#;
    let out = strip_default(src, Lang::Rust)?;
    assert_eq!(out, r#"let url = "http://example.com";"#);
    Ok(())
}

#[test]
fn hash_inside_python_string_is_safe() -> TestResult {
    let src = r#"pattern = "a#b"  # real comment"#;
    let out = strip_default(src, Lang::Python)?;
    assert_eq!(out, r#"pattern = "a#b""#);
    Ok(())
}

#[test]
fn unbalanced_quote_does_not_desync() -> TestResult {
    let src = "let x = 5; // it's fine\nlet y = 6;";
    let out = strip_default(src, Lang::Rust)?;
    assert_eq!(out, "let x = 5;\nlet y = 6;");
    Ok(())
}

#[test]
fn multiline_block_comment_fully_removed() -> TestResult {
    let src = "fn a() {}\n/* line one\n   line two\n   line three */\nfn b() {}\n";
    let out = strip_default(src, Lang::Rust)?;
    assert_eq!(out, "fn a() {}\nfn b() {}\n");
    Ok(())
}

#[test]
fn string_with_trailing_backslash_then_comment() -> TestResult {
    let src = r#"let p = "foo\\"; // comment"#;
    let out = strip_default(src, Lang::Rust)?;
    assert_eq!(out, r#"let p = "foo\\";"#);
    Ok(())
}

#[test]
fn trailing_comment_trims_whitespace() -> TestResult {
    let src = "let x = 5;    // foo\n";
    assert_eq!(strip_default(src, Lang::Rust)?, "let x = 5;\n");
    Ok(())
}

#[test]
fn comment_only_line_is_deleted_in_collapse_mode() -> TestResult {
    let src = "fn a() {}\n// gone\nfn b() {}\n";
    assert_eq!(strip_default(src, Lang::Rust)?, "fn a() {}\nfn b() {}\n");
    Ok(())
}

#[test]
fn comment_only_line_blanked_in_preserve_mode() -> TestResult {
    let src = "fn a() {}\n// gone\nfn b() {}\n";
    let opts = Options {
        line_mode: LineMode::PreserveLines,
        ..Default::default()
    };
    let out = strip(src, Lang::Rust, &opts)?.output;
    assert_eq!(out, "fn a() {}\n\nfn b() {}\n");
    Ok(())
}

#[test]
fn preserve_pattern_keeps_todo() -> TestResult {
    let src = "// TODO: keep me\n// remove me\nfn a() {}\n";
    let opts = Options {
        preserve: PreserveConfig::with_patterns(vec!["TODO".into()]),
        ..Default::default()
    };
    let outcome = strip(src, Lang::Rust, &opts)?;
    assert_eq!(outcome.output, "// TODO: keep me\nfn a() {}\n");
    assert_eq!(outcome.removed, 1);
    assert_eq!(outcome.preserved, 1);
    Ok(())
}

#[test]
fn default_preserves_doc_comments() -> TestResult {
    let src = "/// Public docs.\nfunction a() {}\n/** JSDoc. */\nconst x = 1; // strip\n";
    let out = strip_default(src, Lang::JavaScript)?;
    assert_eq!(
        out,
        "/// Public docs.\nfunction a() {}\n/** JSDoc. */\nconst x = 1;\n"
    );
    Ok(())
}

#[test]
fn line_ranges_limit_scope() -> TestResult {
    let src = "// keep\nlet x = 1;\nlet y = 2; // go\n";
    let opts = Options {
        lines: Lines::Ranges(vec![(3, 3)]),
        ..Default::default()
    };
    let out = strip(src, Lang::Rust, &opts)?.output;
    assert_eq!(out, "// keep\nlet x = 1;\nlet y = 2;\n");
    Ok(())
}

#[test]
fn inline_filter_keeps_block_comments() -> TestResult {
    let src = "// line\n/* block */\nfn a() {}\n";
    let opts = Options {
        kinds: CommentKinds {
            line: true,
            block: false,
        },
        ..Default::default()
    };
    assert_eq!(
        strip(src, Lang::Rust, &opts)?.output,
        "/* block */\nfn a() {}\n"
    );
    Ok(())
}

#[test]
fn block_filter_keeps_line_comments() -> TestResult {
    let src = "// line\n/* block */\nfn a() {}\n";
    let opts = Options {
        kinds: CommentKinds {
            line: false,
            block: true,
        },
        ..Default::default()
    };
    assert_eq!(
        strip(src, Lang::Rust, &opts)?.output,
        "// line\nfn a() {}\n"
    );
    Ok(())
}

#[test]
fn go_and_python_and_ts_smoke() -> TestResult {
    assert_eq!(
        strip_default("package main\n// x\nfunc main() {}\n", Lang::Go)?,
        "package main\nfunc main() {}\n"
    );
    assert_eq!(strip_default("x = 1  # c\n", Lang::Python)?, "x = 1\n");
    assert_eq!(
        strip_default("const x = 1; // c\n", Lang::TypeScript)?,
        "const x = 1;\n"
    );
    Ok(())
}

#[test]
fn optional_pack_strip_smoke() -> TestResult {
    const CASES: &[(Lang, &str, &str)] = &[
        (
            Lang::Java,
            "class Main {\n  // x\n  void f() {}\n}\n",
            "class Main {\n  void f() {}\n}\n",
        ),
        (
            Lang::Kotlin,
            "fun main() {\n  // x\n  println(\"hi\")\n}\n",
            "fun main() {\n  println(\"hi\")\n}\n",
        ),
        (
            Lang::CSharp,
            "class Main {\n  // x\n  void F() {}\n}\n",
            "class Main {\n  void F() {}\n}\n",
        ),
        (
            Lang::Swift,
            "func main() {\n  // x\n  print(\"hi\")\n}\n",
            "func main() {\n  print(\"hi\")\n}\n",
        ),
        (
            Lang::Css,
            ".a {\n  /* x */\n  color: red;\n}\n",
            ".a {\n  color: red;\n}\n",
        ),
        (
            Lang::Json,
            "{\n  // x\n  \"a\": 1\n}\n",
            "{\n  \"a\": 1\n}\n",
        ),
        (Lang::Yaml, "a: 1\n# x\nb: 2\n", "a: 1\nb: 2\n"),
    ];
    for &(lang, src, expected) in CASES {
        assert_eq!(strip_default(src, lang)?, expected);
    }
    Ok(())
}

#[test]
fn css_inline_block_comment_trims_to_declaration() -> TestResult {
    let src = ".a { color: red; /* strip me */ }\n";
    assert_eq!(strip_default(src, Lang::Css)?, ".a { color: red; }\n");
    Ok(())
}

#[test]
fn css_url_with_comment_marker_is_safe() -> TestResult {
    let src = ".a {\n  background: url(\"http://example.com/*x*/\");\n  /* gone */\n}\n";
    assert_eq!(
        strip_default(src, Lang::Css)?,
        ".a {\n  background: url(\"http://example.com/*x*/\");\n}\n"
    );
    Ok(())
}

#[test]
fn css_multiline_block_comment_fully_removed() -> TestResult {
    let src = ".a {}\n/* line one\n   line two */\n.b {}\n";
    assert_eq!(strip_default(src, Lang::Css)?, ".a {}\n.b {}\n");
    Ok(())
}

#[test]
fn python_shebang_is_preserved() -> TestResult {
    let src = "#!/usr/bin/env python3\n# remove me\nx = 1\n";
    assert_eq!(
        strip_default(src, Lang::Python)?,
        "#!/usr/bin/env python3\nx = 1\n"
    );
    Ok(())
}

#[test]
fn multiple_block_comments_on_one_line_collapse() -> TestResult {
    let src = "fn a() {}\n/* one */ /* two */\nfn b() {}\n";
    assert_eq!(strip_default(src, Lang::Rust)?, "fn a() {}\nfn b() {}\n");
    Ok(())
}

#[test]
fn inline_block_comment_keeps_token_separator() -> TestResult {
    let src = "let/* hi */x = 5;\n";
    assert_eq!(strip_default(src, Lang::Rust)?, "let x = 5;\n");
    Ok(())
}

#[test]
fn crlf_trailing_comment_keeps_carriage_return() -> TestResult {
    let src = "let x = 5; // c\r\nlet y = 6;\r\n";
    assert_eq!(
        strip_default(src, Lang::Rust)?,
        "let x = 5;\r\nlet y = 6;\r\n"
    );
    Ok(())
}

#[test]
fn astro_strips_frontmatter_and_template_comments() -> TestResult {
    let src = "---\nconst x = 1; // strip\n// gone\n---\n<div>hi</div>\n<!-- strip -->\n";
    assert_eq!(
        strip_default(src, Lang::Astro)?,
        "---\nconst x = 1;\n---\n<div>hi</div>\n"
    );
    Ok(())
}

#[test]
fn astro_template_only_strips_html_comments() -> TestResult {
    let src = "<div>hi</div>\n<!-- gone -->\n";
    assert_eq!(strip_default(src, Lang::Astro)?, "<div>hi</div>\n");
    Ok(())
}

#[test]
fn astro_line_ranges_use_file_rows() -> TestResult {
    let src = "---\nlet a = 1; // keep\nlet b = 2; // go\n---\n";
    let opts = Options {
        lines: Lines::Ranges(vec![(3, 3)]),
        ..Default::default()
    };
    assert_eq!(
        strip(src, Lang::Astro, &opts)?.output,
        "---\nlet a = 1; // keep\nlet b = 2;\n---\n"
    );
    Ok(())
}

/// A `{}` left where a comment was is not valid-looking markup; the braces
/// belong to the comment.
#[test]
fn jsx_comment_takes_its_braces_with_it() -> TestResult {
    let src = "const a = <div>{/* note */}</div>;\n";
    assert_eq!(
        strip_default(src, Lang::JavaScript)?,
        "const a = <div></div>;\n"
    );
    Ok(())
}

#[test]
fn a_jsx_comment_on_its_own_line_leaves_no_braces() -> TestResult {
    let src = "<div>\n  {/* the card */}\n  <Card />\n</div>\n";
    assert_eq!(
        strip_default(src, Lang::Tsx)?,
        "<div>\n  <Card />\n</div>\n"
    );
    Ok(())
}

/// The braces are wider than the comment, so scoping must still read the
/// comment's own rows: a line range covering only the `{` is not a range
/// covering the comment under it.
#[test]
fn jsx_braces_do_not_pull_a_comment_into_scope() -> TestResult {
    let src = "<div>\n  {\n    /* hand-written, do not delete */\n  }\n  <Card />\n</div>\n";
    let opts = Options {
        lines: Lines::Ranges(vec![(2, 2)]),
        ..Default::default()
    };
    assert_eq!(strip(src, Lang::Tsx, &opts)?.output, src);
    Ok(())
}

/// Braces holding two comments are emptied by removing both, so they go
/// the same way one comment's braces do.
#[test]
fn braces_go_when_the_last_comment_under_them_goes() -> TestResult {
    let inline = "const x = <div>{/* a */ /* b */}</div>;\n";
    assert_eq!(
        strip_default(inline, Lang::Tsx)?,
        "const x = <div></div>;\n"
    );

    let block = "<div>\n  {\n    /* a */\n    /* b */\n  }\n  <Card />\n</div>\n";
    assert_eq!(
        strip_default(block, Lang::Tsx)?,
        "<div>\n  <Card />\n</div>\n"
    );
    Ok(())
}

/// JSX trims a space at the edge of a line but keeps one beside an
/// expression container, so at a line edge the braces are the only reason
/// the rendered space survives. They stay, empty, exactly as before.
#[test]
fn braces_holding_a_rendered_space_stay_empty() -> TestResult {
    let trailing =
        "const A = (\n  <p>\n    Signed in as {/* name */}\n    <b>{n}</b>.\n  </p>\n);\n";
    assert_eq!(
        strip_default(trailing, Lang::Tsx)?,
        "const A = (\n  <p>\n    Signed in as {}\n    <b>{n}</b>.\n  </p>\n);\n"
    );

    let leading = "const B = (\n  <p>\n    <b>{f}</b>\n    {/* note */} and counting\n  </p>\n);\n";
    assert_eq!(
        strip_default(leading, Lang::Tsx)?,
        "const B = (\n  <p>\n    <b>{f}</b>\n    {} and counting\n  </p>\n);\n"
    );
    Ok(())
}

/// Between two lines of prose the newlines sit at the edge of their own runs
/// and render as nothing; joined into one run they become a space. The braces
/// are what keeps them apart.
#[test]
fn braces_between_two_lines_of_prose_stay() -> TestResult {
    let src = "const T = (\n  <p>\n    a\n    {/* c */}\n    b\n  </p>\n);\n";
    assert_eq!(
        strip_default(src, Lang::Tsx)?,
        "const T = (\n  <p>\n    a\n    {}\n    b\n  </p>\n);\n"
    );
    Ok(())
}

/// Entities are read per run of text, so joining two runs can spell one that
/// was never written: `&amp` beside `;` is not `&amp;`.
#[test]
fn braces_that_would_spell_an_entity_stay() -> TestResult {
    let src = "const U = <p>&amp{/* c */};</p>;\n";
    assert_eq!(
        strip_default(src, Lang::Tsx)?,
        "const U = <p>&amp{};</p>;\n"
    );
    Ok(())
}

/// A space on one side and markup on the other reads the same either way, so
/// the braces go and the space the author wrote stays.
#[test]
fn braces_go_where_the_text_reads_the_same_without_them() -> TestResult {
    let src = "const C = () => <p>\n  Signed in as {/* display name */}<b>{name}</b>.\n</p>;\n";
    assert_eq!(
        strip_default(src, Lang::Tsx)?,
        "const C = () => <p>\n  Signed in as <b>{name}</b>.\n</p>;\n"
    );
    Ok(())
}

/// A space between two wrappers is a text node, not padding between
/// comments, so it cannot be swallowed by merging them.
#[test]
fn a_space_between_two_wrappers_is_content() -> TestResult {
    let src = "const y = <p>A{/* a */} {/* b */}B</p>;\n";
    assert_eq!(strip_default(src, Lang::Tsx)?, "const y = <p>A B</p>;\n");
    Ok(())
}

/// Two wrappers sharing a line are not alone on it: the space between
/// them renders, so neither set of braces can go.
#[test]
fn two_wrappers_on_one_line_keep_their_braces() -> TestResult {
    let src = "<Row>\n  {/* left */ /* icon */} {/* right */}\n</Row>\n";
    assert_eq!(strip_default(src, Lang::Tsx)?, "<Row>\n  {} {}\n</Row>\n");
    Ok(())
}

/// The braces already separated their neighbours, so putting a space where
/// they were changes what the markup renders.
#[test]
fn removing_braces_does_not_space_out_what_they_separated() -> TestResult {
    let src = "export const T = () => <p>Hello{/* greeting */}World</p>;\n";
    assert_eq!(
        strip_default(src, Lang::Tsx)?,
        "export const T = () => <p>HelloWorld</p>;\n"
    );
    Ok(())
}

/// An attribute's value is not packaging around a comment: `bar=` alone
/// does not parse.
#[test]
fn an_attribute_keeps_its_braces() -> TestResult {
    let src = "const a = <Foo bar={/* c */} />;\n";
    assert_eq!(
        strip_default(src, Lang::Tsx)?,
        "const a = <Foo bar={} />;\n"
    );
    Ok(())
}

/// Two comments under one wrapper name the same span to remove, and a
/// duplicate span used to swallow whatever removal came after it.
#[test]
fn a_shared_wrapper_does_not_swallow_the_next_removal() -> TestResult {
    let src = "const x = <div>{/* a */ /* b */}{/* c */}</div>;\n";
    let out = strip(src, Lang::Tsx, &Options::default())?;
    assert_eq!(out.output, "const x = <div></div>;\n");
    assert_eq!(out.removed, 3);
    Ok(())
}

/// One survivor is reason enough to keep the braces: they still have to
/// hold it.
#[test]
fn braces_stay_when_a_sibling_comment_survives() -> TestResult {
    let src = "const x = <div>{/* slop */ /* codegen-start a */}{/* codegen-end a */}</div>;\n";
    assert_eq!(
        strip_default(src, Lang::Tsx)?,
        "const x = <div>{ /* codegen-start a */}{/* codegen-end a */}</div>;\n"
    );
    Ok(())
}

/// The braces are only the comment's when the comment is all they hold.
#[test]
fn braces_holding_more_than_a_comment_stay() -> TestResult {
    let src = "const a = <div>{/* note */ value}</div>;\n";
    assert_eq!(
        strip_default(src, Lang::Tsx)?,
        "const a = <div>{ value}</div>;\n"
    );
    Ok(())
}

/// A preserved marker keeps its braces, or the JSX around it stops parsing.
#[test]
fn a_preserved_jsx_marker_keeps_its_braces() -> TestResult {
    let src =
        "<div>\n  {/* codegen-start abc */}\n  {/* slop */}\n  {/* codegen-end abc */}\n</div>\n";
    let out = strip_default(src, Lang::Tsx)?;
    assert_eq!(
        out,
        "<div>\n  {/* codegen-start abc */}\n  {/* codegen-end abc */}\n</div>\n"
    );
    Ok(())
}
