use std::fs;
use std::path::{Path, PathBuf};

use rubrix_highlight::{Lang, check_line, highlight, lex_line};

fn repo() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn files(dir: &Path) -> Vec<PathBuf> {
    let mut found = Vec::new();
    let mut dirs = vec![dir.to_path_buf()];
    while let Some(dir) = dirs.pop() {
        for entry in fs::read_dir(&dir).unwrap() {
            let path = entry.unwrap().path();
            if path.is_dir() {
                if !path.ends_with("target") {
                    dirs.push(path);
                }
            } else {
                found.push(path);
            }
        }
    }
    found.sort();
    found
}

fn detect(path: &Path, text: &str) -> Lang {
    let first_line = text.lines().next().unwrap_or_default();
    Lang::detect(&path.to_string_lossy(), first_line)
}

fn check_file(path: &Path, lang: Lang) {
    let text = fs::read_to_string(path).unwrap();
    highlight(lang, &text)
        .check(&text)
        .unwrap_or_else(|err| panic!("{}: {err}", path.display()));
}

#[test]
fn fynix_sources() {
    let sources: Vec<PathBuf> = files(&repo().join("vendor/fynix"))
        .into_iter()
        .filter(|path| {
            path.extension().is_some_and(|ext| ext == "rs")
        })
        .collect();
    assert!(sources.len() >= 72, "found {} sources", sources.len());
    for path in &sources {
        let text = fs::read_to_string(path).unwrap();
        assert_eq!(
            detect(path, &text),
            Lang::Rust,
            "{}",
            path.display()
        );
        check_file(path, Lang::Rust);
    }
}

#[test]
fn bench_fixtures() {
    let fixtures: Vec<PathBuf> =
        files(&repo().join("bench/fixtures"))
            .into_iter()
            .filter(|path| {
                path.extension().is_some_and(|ext| ext == "txt")
            })
            .collect();
    assert!(!fixtures.is_empty(), "no fixtures found");
    for path in &fixtures {
        let dir = path.parent().unwrap().file_name().unwrap();
        let text = fs::read_to_string(path).unwrap();
        let lang = detect(path, &text);
        assert_eq!(
            Some(lang.name()),
            dir.to_str(),
            "{} detects as {}",
            path.display(),
            lang.name()
        );
        check_file(path, lang);
    }
}

#[test]
fn line_prefixes() {
    let root =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/golden");
    for path in files(&root) {
        if path.extension().is_some_and(|ext| ext == "golden") {
            continue;
        }
        let dir = path.parent().unwrap().file_name().unwrap();
        let lang = Lang::from_name(dir.to_str().unwrap()).unwrap();
        let text = fs::read_to_string(&path).unwrap();
        let whole = highlight(lang, &text);
        for n in 0..whole.line_count() {
            let line = &text[whole.line_range(n)];
            let ends = line.char_indices().map(|(i, _)| i);
            for end in ends.chain([line.len()]) {
                let mut runs = Vec::new();
                let prefix = &line[..end];
                lex_line(
                    lang,
                    prefix,
                    whole.start_state(n),
                    &mut runs,
                );
                check_line(prefix, &runs).unwrap_or_else(|err| {
                    panic!(
                        "{} line {n} prefix {prefix:?}: {err}",
                        path.display()
                    )
                });
            }
        }
    }
}
