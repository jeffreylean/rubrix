#![cfg(feature = "find")]

use std::ops::Range;
use std::time::Duration;

use rubrix_edit::find::{
    MAX_MATCHES, Query, Search, replace, replace_all, seed,
};
use rubrix_edit::{
    Buffer, Command, Editor, History, Monospace, Selection,
    Selections,
};

mod common;

use common::{Rng, seed as rng_seed};

fn query(text: &str) -> Query {
    Query {
        text: text.to_owned(),
        ..Query::default()
    }
}

fn found(text: &str, query: &Query) -> Vec<Range<usize>> {
    let mut search = Search::new(query);
    search.run(&Buffer::new(text));
    assert!(search.is_done());
    search.matches().to_vec()
}

fn editor(text: &str) -> Editor {
    Editor::new(Buffer::new(text))
}

#[test]
fn plain_ignores_case_unless_match_case() {
    let text = "Theme theme THEME";
    assert_eq!(found(text, &query("theme")), [0..5, 6..11, 12..17]);
    let exact = Query {
        match_case: true,
        ..query("theme")
    };
    assert_eq!(found(text, &exact), vec![6..11]);
}

#[test]
fn plain_escapes_regex_syntax() {
    assert_eq!(found("a.b axb a.b", &query("a.b")), [0..3, 8..11]);
    assert_eq!(found("f(x) (x)", &query("(x)")), [1..4, 5..8]);
}

#[test]
fn whole_word_checks_both_sides_with_the_word_classes() {
    let whole = Query {
        whole_word: true,
        ..query("theme")
    };
    let text = "theme themes _theme theme_x (theme) é theme étheme";
    assert_eq!(found(text, &whole), [0..5, 29..34, 39..44]);
}

#[test]
fn whole_word_retries_inside_a_rejected_match() {
    let whole = Query {
        whole_word: true,
        ..query("aa")
    };
    assert_eq!(found("aaa aa", &whole), vec![4..6]);
}

#[test]
fn regex_matches_with_its_options() {
    let regex = Query {
        regex: true,
        ..query(r"t\w+e")
    };
    assert_eq!(
        found("Theme tree tale", &regex),
        [0..5, 6..10, 11..15]
    );
    let exact = Query {
        match_case: true,
        ..regex.clone()
    };
    assert_eq!(found("Theme tree tale", &exact), [6..10, 11..15]);
    let anchored = Query {
        regex: true,
        ..query("^b")
    };
    assert_eq!(found("ab\nb\r\nbb", &anchored), [3..4, 6..7]);
}

#[test]
fn a_plain_query_with_a_newline_matches_across_two_lines() {
    let text = "let a = {\n    b\n}\nlet c = {\n";
    assert_eq!(found(text, &query("{\n    b")), vec![8..15]);
}

#[test]
fn a_regex_with_a_newline_matches_across_lines() {
    let regex = Query {
        regex: true,
        ..query(r"\{\n\s+let")
    };
    let text = "fn a() {\n    let x;\n}\nfn b() {\n\n  let y;\n}";
    let matches = found(text, &regex);
    assert_eq!(matches, [7..16, 29..37]);
    assert_eq!(&text[29..37], "{\n\n  let");
}

#[test]
fn a_newline_in_a_query_matches_a_crlf_line_break() {
    let text = "let a = {\r\n    let b;\r\n}\r\n[x\ny]";
    assert_eq!(found(text, &query("{\n    let")), vec![8..18]);
    let regex = Query {
        regex: true,
        ..query(r"\{\n\s+let")
    };
    assert_eq!(found(text, &regex), vec![8..18]);
    let class = Query {
        regex: true,
        ..query(r"x[\n]y")
    };
    assert_eq!(found(text, &class), vec![27..30]);
    assert_eq!(
        found(text, &query(r"\n")),
        Vec::<Range<usize>>::new()
    );
}

fn regex_query(text: &str) -> Query {
    Query {
        regex: true,
        ..query(text)
    }
}

#[test]
fn a_newline_inside_a_character_class_stays_in_the_class() {
    let text = "e1 e] e[ e^ ea eb ex e: e( e) e? e\r e\n[\nx";
    let reference = |pattern: &str| {
        regex::RegexBuilder::new(pattern)
            .case_insensitive(true)
            .multi_line(true)
            .build()
            .expect("a valid pattern")
            .find_iter(text)
            .map(|found| found.range())
            .collect::<Vec<_>>()
    };
    let table = [
        (r"e[[:digit:]\n]", 2),
        (r"e[]\n]", 2),
        (r"e[^]\n]", 11),
        (r"e[\]\n]", 2),
        (r"e[[a]\n]", 2),
        (r"e[a[b]x\n]", 4),
        (r"e[\n[a-z]&&[^b]]", 3),
        (r"e[\[\n]", 2),
        (r"e[\n[:digit:]]", 2),
        (r"\[\n", 1),
    ];
    for (pattern, count) in table {
        let matches = found(text, &regex_query(pattern));
        assert_eq!(matches, reference(pattern), "{pattern}");
        assert_eq!(matches.len(), count, "{pattern}");
    }
    let crlf = "a]\r\nb[\r\nc";
    assert_eq!(found(crlf, &regex_query(r"[\]]\n")), vec![1..4]);
    assert_eq!(found(crlf, &regex_query(r"\[\nc")), vec![5..9]);
}

fn follows_an_edit(
    text: &str,
    query: &Query,
    at: usize,
    typed: &str,
) {
    let mut editor = editor(text);
    let mut search = Search::new(query);
    search.run(editor.buffer());
    editor.set_selections(Selections::new(Selection::caret(at)));
    let change = editor
        .apply(Command::Type(typed), &Monospace::default())
        .expect("a change");
    search.edited(editor.buffer(), &change);
    search.run(editor.buffer());
    let after = editor.buffer().to_string();
    let fresh = found(&after, query);
    assert!(!fresh.is_empty(), "{query:?} matches {after:?}");
    assert_eq!(search.matches(), fresh, "{query:?} in {after:?}");
}

#[test]
fn an_edit_finds_a_match_that_starts_lines_above_it() {
    let text = "share.\n\nx\nfo\n";
    let at = text.find("fo").expect("fo") + 2;
    follows_an_edit(text, &query("share.\n\nx\nfoo"), at, "o");
    follows_an_edit(
        text,
        &regex_query(r"share\.\n\n.*\nfoo"),
        at,
        "o",
    );
    follows_an_edit(
        text,
        &regex_query(r"share\.\s+x\s+foo"),
        at,
        "o",
    );
    let earlier = "a b\na\n\n\n\nc\n";
    let at = earlier.find('c').expect("c");
    follows_an_edit(earlier, &regex_query(r"a\s+b"), at, "b");
}

#[test]
fn edits_keep_a_search_equal_to_a_fresh_one() {
    let mut rng = Rng(rng_seed());
    let alphabet = ["a", "b", "\n", " ", "ab", "\n\n"];
    let queries = [
        query("a\nb"),
        query("b\n\na"),
        query("a b"),
        regex_query(r"a\s+b"),
        regex_query(r"a[^b]*b"),
        regex_query(r"(?s)a.*?b"),
        regex_query(r"\n\n+"),
        regex_query(r"a.*\n.*b"),
        regex_query(r"^b\n(a|\n){2}"),
    ];
    for round in 0..300 {
        let pick = |rng: &mut Rng, n: u32| -> String {
            (0..n)
                .map(|_| {
                    alphabet
                        [rng.below(alphabet.len() as u32) as usize]
                })
                .collect()
        };
        let len = rng.below(60);
        let mut editor = editor(&pick(&mut rng, len));
        let query =
            &queries[rng.below(queries.len() as u32) as usize];
        let mut search = Search::new(query);
        search.run(editor.buffer());
        for _ in 0..4 {
            let end = editor.buffer().len() as u32 + 1;
            let (a, b) =
                (rng.below(end) as usize, rng.below(end) as usize);
            editor.set_selections(Selections::new(Selection {
                anchor: a.min(b),
                head: a.max(b),
            }));
            let typed_len = 1 + rng.below(3);
            let typed = pick(&mut rng, typed_len);
            let change = editor
                .apply(Command::Type(&typed), &Monospace::default())
                .expect("a change");
            search.edited(editor.buffer(), &change);
            if rng.chance(50) {
                search.run(editor.buffer());
            }
        }
        search.run(editor.buffer());
        let text = editor.buffer().to_string();
        assert_eq!(
            search.matches(),
            found(&text, query),
            "round {round}: {query:?} in {text:?}"
        );
    }
}

#[test]
fn a_match_that_splits_a_grapheme_cluster_does_not_count() {
    let family = "\u{1f468}\u{200d}\u{1f469}\u{200d}\u{1f467}";
    let text = format!("family {family} end e\u{301} e\r\n");
    assert!(found(&text, &query("\u{1f469}")).is_empty());
    assert!(found(&text, &query("\u{1f468}")).is_empty());
    assert_eq!(
        found(&text, &query(family)),
        vec![7..7 + family.len()]
    );
    let end = text.find("end").expect("end");
    let last = text.rfind("e\r").expect("a last e");
    assert_eq!(
        found(&text, &query("e")),
        vec![end..end + 1, last..last + 1]
    );
    assert!(found(&text, &regex_query(r"e\r")).is_empty());
    assert_eq!(
        found(&text, &regex_query(r"e\r\n")),
        vec![last..last + 3]
    );
}

#[test]
fn an_invalid_regex_finds_nothing_and_says_why() {
    let bad = Query {
        regex: true,
        ..query("(theme")
    };
    let mut search = Search::new(&bad);
    search.run(&Buffer::new("(theme"));
    assert!(search.is_done());
    assert!(search.matches().is_empty());
    let error = search.error().expect("an error");
    assert!(error.contains("unclosed group"), "{error}");
    assert_eq!(Search::new(&query("(theme")).error(), None);
}

#[test]
fn empty_matches_are_skipped_one_character_at_a_time() {
    let empty = Query {
        regex: true,
        ..query("x*")
    };
    assert_eq!(found("éxx🎉x", &empty), [2..4, 8..9]);
    assert!(found("abc", &query("")).is_empty());
}

#[test]
fn the_search_stops_at_the_match_cap() {
    let text = "e".repeat(MAX_MATCHES + 50);
    let mut search = Search::new(&query("e"));
    search.run(&Buffer::new(&text));
    assert!(search.is_done());
    assert!(search.is_capped());
    assert_eq!(search.matches().len(), MAX_MATCHES);
    assert_eq!(search.matches()[MAX_MATCHES - 1], 99_999..100_000);
}

#[test]
fn slices_find_what_one_pass_finds() {
    let text = "theme self.theme\n".repeat(100_000);
    let buffer = Buffer::new(&text);
    let mut sliced = Search::new(&query("theme"));
    let mut slices = 1;
    while !sliced.step(&buffer, Duration::from_micros(200)) {
        slices += 1;
    }
    assert!(slices > 1, "the search took one slice");
    assert_eq!(sliced.matches(), found(&text, &query("theme")));
}

#[test]
fn an_edit_scans_again_from_the_edited_line() {
    let text = "theme\ntheme\nab\ncd\ntheme\n";
    let mut editor = editor(text);
    let plain = query("b\nxcd");
    let mut search = Search::new(&plain);
    search.run(editor.buffer());
    assert!(search.matches().is_empty());
    editor.set_selections(Selections::new(Selection::caret(15)));
    let change =
        editor.apply(Command::Type("x"), &Monospace::default());
    search.edited(editor.buffer(), &change.expect("a change"));
    assert!(!search.is_done());
    search.run(editor.buffer());
    assert_eq!(search.matches(), vec![13..18]);

    let mut theme = Search::new(&query("theme"));
    theme.run(editor.buffer());
    assert_eq!(theme.matches(), [0..5, 6..11, 19..24]);
    editor.set_selections(Selections::new(Selection::caret(19)));
    let change =
        editor.apply(Command::Type("the"), &Monospace::default());
    theme.edited(editor.buffer(), &change.expect("a change"));
    assert_eq!(
        theme.matches(),
        [0..5, 6..11],
        "lines above the edit"
    );
    theme.run(editor.buffer());
    assert_eq!(
        theme.matches(),
        found(&editor.buffer().to_string(), &query("theme"))
    );
}

#[test]
fn seed_takes_the_word_at_or_before_a_bare_caret() {
    let buffer = Buffer::new("let theme = x.theme;\n  ");
    let at = |offset| {
        seed(&buffer, &Selections::new(Selection::caret(offset)))
    };
    assert_eq!(at(6), Some(4..9));
    assert_eq!(at(9), Some(4..9));
    assert_eq!(at(19), Some(14..19));
    assert_eq!(at(10), None);
    assert_eq!(at(22), None);
    let range = Selections::new(Selection { anchor: 4, head: 9 });
    assert_eq!(seed(&buffer, &range), None);
}

#[test]
fn replace_puts_the_caret_after_the_new_text() {
    let mut editor = editor("theme and theme");
    let mut search = Search::new(&query("theme"));
    let change = replace(&mut editor, &mut search, 1, "palette");
    assert!(change.is_some());
    assert_eq!(editor.buffer().to_string(), "theme and palette");
    assert_eq!(editor.selections().primary(), Selection::caret(17));
    assert_eq!(replace(&mut editor, &mut search, 9, "x"), None);
}

#[test]
fn replace_all_is_one_undo_step() {
    let text = "theme a theme\ntheme";
    let mut editor = Editor::from_parts(
        Buffer::new(text),
        Selections::new(Selection::caret(0)),
        History::new(),
    );
    let _ = editor.apply(Command::Type("x"), &Monospace::default());
    let mut search = Search::new(&query("theme"));
    replace_all(&mut editor, &mut search, "palette")
        .expect("a change");
    assert_eq!(
        editor.buffer().to_string(),
        "xpalette a palette\npalette"
    );
    assert_eq!(editor.history().undo_depth(), 2);
    let _ = editor.apply(Command::Undo, &Monospace::default());
    assert_eq!(editor.buffer().to_string(), format!("x{text}"));
    let _ = editor.apply(Command::Undo, &Monospace::default());
    assert_eq!(editor.buffer().to_string(), text);
}

#[test]
fn a_regex_replacement_expands_groups_the_match_and_dollars() {
    let swap = Query {
        regex: true,
        ..query(r"(\w+)_(\w+)")
    };
    let mut editor = editor("theme_color fn x");
    replace_all(&mut editor, &mut Search::new(&swap), "$2_$1")
        .unwrap();
    assert_eq!(editor.buffer().to_string(), "color_theme fn x");

    let word = Query {
        regex: true,
        ..query("fn")
    };
    replace_all(
        &mut editor,
        &mut Search::new(&word),
        "$&$& $$1 $9 $",
    )
    .unwrap();
    assert_eq!(
        editor.buffer().to_string(),
        "color_theme fnfn $1  $ x"
    );

    let plain = query("x");
    replace_all(&mut editor, &mut Search::new(&plain), "$&").unwrap();
    assert_eq!(
        editor.buffer().to_string(),
        "color_theme fnfn $1  $ $&"
    );
}

#[test]
fn plain_search_agrees_with_match_indices_on_random_text() {
    let mut rng = Rng(rng_seed());
    let alphabet = ['a', 'b', 'é', '\n', ' '];
    for round in 0..200 {
        let pick = |rng: &mut Rng, n: u32| -> String {
            (0..n)
                .map(|_| {
                    alphabet
                        [rng.below(alphabet.len() as u32) as usize]
                })
                .collect()
        };
        let len = rng.below(400);
        let text = pick(&mut rng, len);
        let needle_len = 1 + rng.below(3);
        let needle = pick(&mut rng, needle_len);
        let exact = Query {
            match_case: true,
            ..query(&needle)
        };
        let expected: Vec<Range<usize>> = text
            .match_indices(&needle)
            .map(|(at, hit)| at..at + hit.len())
            .collect();
        assert_eq!(
            found(&text, &exact),
            expected,
            "round {round}: {needle:?} in {text:?}"
        );
    }
}
