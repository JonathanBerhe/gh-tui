//! Snapshot tests for the markdown renderer. One test per fixture file.
//! Review with `cargo insta review` after updating semantics.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::fs;

use gh_render::render_markdown;

/// Lossy plain-text view of the rendered output: one line per logical line,
/// styles dropped. Easier to read in snapshot diffs than the full Debug
/// output of `Vec<Line>`. The styling logic itself is exercised via the
/// renderer's unit tests in `markdown.rs`.
fn flatten(input: &str) -> String {
    let lines = render_markdown(input);
    lines
        .into_iter()
        .map(|l| {
            l.spans
                .into_iter()
                .map(|s| s.content.into_owned())
                .collect::<String>()
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn fixture(name: &str) -> String {
    let path = format!("tests/fixtures/{name}.md");
    fs::read_to_string(&path).unwrap_or_else(|e| panic!("read {path}: {e}"))
}

macro_rules! md_snapshot {
    ($name:ident) => {
        #[test]
        fn $name() {
            let input = fixture(stringify!($name));
            insta::assert_snapshot!(flatten(&input));
        }
    };
}

md_snapshot!(paragraph);
md_snapshot!(headings);
md_snapshot!(lists);
md_snapshot!(nested_list);
md_snapshot!(code_block);
md_snapshot!(inline_code);
md_snapshot!(bold_italic);
md_snapshot!(link);
md_snapshot!(image);
md_snapshot!(mermaid);
md_snapshot!(blockquote);
md_snapshot!(hr);
