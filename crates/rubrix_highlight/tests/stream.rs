use std::fs;
use std::path::{Path, PathBuf};

use rubrix_highlight::{Highlights, Kind, Lang, Stream, highlight};

const CHUNK_SIZES: [usize; 4] = [1, 7, 64, 4096];

fn golden_texts() -> Vec<(Lang, PathBuf, String)> {
    let root =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/golden");
    let mut texts = Vec::new();
    for dir in sorted_entries(&root) {
        let name = dir.file_name().unwrap().to_str().unwrap();
        let lang = Lang::from_name(name).unwrap();
        for source in sorted_entries(&dir) {
            if source.extension().is_some_and(|ext| ext == "golden") {
                continue;
            }
            let text = fs::read_to_string(&source).unwrap();
            texts.push((lang, source, text));
        }
    }
    assert!(!texts.is_empty(), "no golden sources under {root:?}");
    texts
}

fn sorted_entries(dir: &Path) -> Vec<PathBuf> {
    let mut entries: Vec<PathBuf> = fs::read_dir(dir)
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .collect();
    entries.sort();
    entries
}

fn chunks(text: &str, size: usize) -> impl Iterator<Item = &str> {
    let mut start = 0;
    std::iter::from_fn(move || {
        if start == text.len() {
            return None;
        }
        let end =
            text.ceil_char_boundary((start + size).min(text.len()));
        let chunk = &text[start..end];
        start = end;
        Some(chunk)
    })
}

fn stream(lang: Lang, text: &str, size: usize) -> Highlights {
    let mut stream = Stream::new(lang);
    for chunk in chunks(text, size) {
        stream.push(chunk);
    }
    stream.finish()
}

#[track_caller]
fn assert_streams_like_one_shot(lang: Lang, text: &str, what: &str) {
    let want = highlight(lang, text);
    for size in CHUNK_SIZES {
        let got = stream(lang, text, size);
        got.check(text).unwrap_or_else(|err| {
            panic!("{what} at chunk size {size}: {err}")
        });
        assert!(
            got == want,
            "{what} at chunk size {size} differs from one-shot"
        );
    }
}

#[test]
fn golden_files_stream_like_one_shot() {
    for (lang, source, text) in golden_texts() {
        assert_streams_like_one_shot(
            lang,
            &text,
            &source.display().to_string(),
        );
    }
}

#[test]
fn crlf_pair_split_across_chunks() {
    let text = "let a = 1;\r\nlet b = \"x\";\r\n";
    let mut stream = Stream::new(Lang::Rust);
    assert_eq!(stream.push("let a = 1;\r"), 0..0);
    assert_eq!(stream.push("\nlet b = \"x\";\r"), 0..1);
    assert_eq!(stream.push("\n"), 1..2);
    let got = stream.finish();
    got.check(text).unwrap();
    assert!(got == highlight(Lang::Rust, text));
    assert_eq!(got.line_count(), 3);
}

#[test]
fn crlf_text_streams_like_one_shot() {
    for (lang, source, text) in golden_texts() {
        let crlf = text.replace("\r\n", "\n").replace('\n', "\r\n");
        assert_streams_like_one_shot(
            lang,
            &crlf,
            &format!("{} as CRLF", source.display()),
        );
    }
}

#[test]
fn partial_shows_an_open_string_until_it_closes() {
    let mut stream = Stream::new(Lang::Rust);
    assert_eq!(stream.push("let s = \"ab"), 0..0);
    let open = stream.partial();
    assert_eq!(open.last().unwrap().kind, Kind::String);
    assert_eq!(open.last().unwrap().end, 11);

    assert_eq!(stream.push("c\"; x"), 0..0);
    let closed = stream.partial();
    assert_ne!(closed, open);
    assert_eq!(closed.last().unwrap().end, 16);
    assert!(closed.len() > open.len());

    assert_eq!(stream.push("\n"), 0..1);
    assert_eq!(stream.runs(0), closed);
    assert_eq!(stream.partial(), []);
}

#[test]
fn partial_carries_state_from_committed_lines() {
    let mut stream = Stream::new(Lang::Rust);
    stream.push("/* open\nstill");
    assert_eq!(stream.partial().len(), 1);
    assert_eq!(stream.partial()[0].kind, Kind::Comment);
}

#[test]
fn partial_leaves_out_a_pending_cr() {
    let mut stream = Stream::new(Lang::Rust);
    stream.push("let x\r");
    assert_eq!(stream.partial().last().unwrap().end, 5);
}

#[test]
fn push_returns_the_lines_it_completes() {
    let mut stream = Stream::new(Lang::Plain);
    assert_eq!(stream.push(""), 0..0);
    assert_eq!(stream.push("a"), 0..0);
    assert_eq!(stream.push("b\nc\n\nd"), 0..3);
    assert_eq!(stream.push("e\n"), 3..4);
    assert_eq!(stream.push("\n"), 4..5);
    assert_eq!(stream.finish().line_count(), 6);
}

#[test]
fn empty_and_unterminated_texts() {
    assert_eq!(Stream::new(Lang::Rust).finish().line_count(), 1);
    for text in ["", "\n", "fn main() {}", "fn main() {}\n"] {
        assert_streams_like_one_shot(Lang::Rust, text, text);
    }
}

#[test]
fn multi_byte_chars_stream_whole() {
    let text = "let café = \"naïve 🎉\";\n// 日本語\n";
    assert_streams_like_one_shot(Lang::Rust, text, "unicode");
    assert_eq!(
        chunks("aé🎉", 1).collect::<Vec<_>>(),
        ["a", "é", "🎉"]
    );
    assert_eq!(chunks("aé🎉", 2).collect::<Vec<_>>(), ["aé", "🎉"]);
}
