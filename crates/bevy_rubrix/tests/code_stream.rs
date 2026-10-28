mod support;

use bevy::app::App;
use bevy::ecs::entity::Entity;
use bevy::ecs::hierarchy::Children;
use bevy::text::TextSpan;
use bevy_rubrix::{CodeSource, CodeStream, Kind, Lang};
use support::{
    colour, doc, laid_out, lines, list_state, mount_view, row_spans,
};

const LARGE_RS: &str =
    include_str!("../../../bench/fixtures/rust/large.rs.txt");

fn streaming(lang: Lang) -> (App, CodeStream) {
    let stream = CodeSource::streaming(lang);
    let mut app = laid_out();
    mount_view(&mut app, stream.source(), 600.0, 900.0);
    (app, stream)
}

fn rows(app: &mut App) -> Vec<(String, Vec<(usize, Kind)>)> {
    let doc = doc(app);
    (0..doc.rows())
        .map(|row| {
            let runs = doc
                .runs(row)
                .iter()
                .map(|run| (run.end as usize, run.kind))
                .collect();
            (doc.line(row).expect("a row").to_owned(), runs)
        })
        .collect()
}

#[test]
fn streaming_in_7_byte_chunks_ends_as_loading_whole() {
    let text =
        format!("{}\r\nlet s = \"two\r\nlines\";\r\n", LARGE_RS);
    let (mut app, stream) = streaming(Lang::Rust);
    let chunks = text.as_bytes().chunks(7).collect::<Vec<_>>();
    for frame in chunks.chunks(150) {
        for chunk in frame {
            stream.push(core::str::from_utf8(chunk).expect("ASCII"));
        }
        app.update();
    }
    stream.finish();
    app.update();

    let mut whole = laid_out();
    mount_view(
        &mut whole,
        CodeSource::new(Lang::Rust, text.as_str()),
        600.0,
        400.0,
    );
    assert_eq!(doc(&mut app).rows(), doc(&mut whole).rows());
    assert_eq!(doc(&mut app).committed(), doc(&mut app).rows());
    assert!(
        rows(&mut app) == rows(&mut whole),
        "rows and runs match"
    );
}

#[test]
fn committed_rows_match_the_whole_text_before_the_end() {
    let (mut app, stream) = streaming(Lang::Rust);
    let half = &LARGE_RS[..LARGE_RS.len() / 2];
    for chunk in half.as_bytes().chunks(7) {
        stream.push(core::str::from_utf8(chunk).expect("ASCII"));
    }
    app.update();
    let mut whole = laid_out();
    mount_view(
        &mut whole,
        CodeSource::new(Lang::Rust, LARGE_RS),
        600.0,
        400.0,
    );
    let committed = doc(&mut app).committed();
    assert_eq!(committed, half.matches('\n').count());
    let streamed = rows(&mut app);
    let loaded = rows(&mut whole);
    assert!(
        streamed[..committed] == loaded[..committed],
        "committed rows match"
    );
}

#[test]
fn a_provisional_string_run_is_replaced_when_its_quote_closes() {
    let (mut app, stream) = streaming(Lang::Rust);
    stream.push("let s = \"ab");
    app.update();
    app.update();
    let last = doc(&mut app).rows() - 1;
    assert_eq!(
        doc(&mut app).committed(),
        0,
        "the line is unfinished"
    );
    let open = row_spans(&mut app, last);
    assert_eq!(
        open.last(),
        Some(&("\"ab".to_owned(), colour(Kind::String))),
        "an open string runs to the end"
    );

    stream.push("c\"; let t = 1;");
    app.update();
    app.update();
    let closed = row_spans(&mut app, last);
    assert!(
        closed
            .contains(&("\"abc\"".to_owned(), colour(Kind::String))),
        "{closed:?}"
    );
    assert!(
        closed.contains(&("let ".to_owned(), colour(Kind::Keyword))),
        "the second let is a keyword again: {closed:?}"
    );
}

#[test]
fn a_string_closed_on_the_next_line_ends_there() {
    let (mut app, stream) = streaming(Lang::Rust);
    for byte in "let s = \"ab\nc\"; let t = 1;".as_bytes().chunks(1) {
        stream.push(core::str::from_utf8(byte).expect("ASCII"));
        app.update();
    }
    stream.finish();
    app.update();
    app.update();
    let second = row_spans(&mut app, 1);
    assert_eq!(second[0], ("c\"".to_owned(), colour(Kind::String)));
    assert!(
        second.contains(&("let ".to_owned(), colour(Kind::Keyword))),
        "{second:?}"
    );
}

fn entities(app: &mut App) -> Vec<(usize, Entity, Vec<Entity>)> {
    lines(app)
        .into_iter()
        .map(|(row, root)| {
            let spans = app
                .world()
                .get::<Children>(root)
                .map(|kids| kids.iter().copied().collect())
                .unwrap_or_default();
            (row, root, spans)
        })
        .collect()
}

#[test]
fn earlier_rows_keep_their_entities_across_100_pushes() {
    let (mut app, stream) = streaming(Lang::Rust);
    let text = &LARGE_RS[..1_000];
    let (first, rest) = text.split_at(400);
    stream.push(first);
    app.update();
    app.update();
    let committed = doc(&mut app).committed();
    assert!(committed > 5, "{committed} lines to keep");
    let before = entities(&mut app)
        .into_iter()
        .filter(|&(row, ..)| row < committed)
        .collect::<Vec<_>>();
    let tick = app.world_mut().change_tick();

    for chunk in rest.as_bytes().chunks(6).take(100) {
        stream.push(core::str::from_utf8(chunk).expect("ASCII"));
        app.update();
    }
    assert!(doc(&mut app).committed() > committed, "lines arrived");
    let after = entities(&mut app);
    for (row, root, spans) in &before {
        assert!(
            after.contains(&(*row, *root, spans.clone())),
            "row {row} keeps its root and spans"
        );
        for &span in spans {
            let now = app.world_mut().change_tick();
            let changed = app
                .world()
                .entity(span)
                .get_change_ticks::<TextSpan>()
                .expect("a span")
                .changed;
            assert!(
                !changed.is_newer_than(tick, now),
                "row {row} keeps its text"
            );
        }
    }
}

#[test]
fn the_view_follows_the_newest_line() {
    let (mut app, stream) = streaming(Lang::Rust);
    for chunk in LARGE_RS.as_bytes()[..20_000].chunks(500) {
        stream.push(core::str::from_utf8(chunk).expect("ASCII"));
        app.update();
    }
    app.update();
    let rows = doc(&mut app).rows();
    let state = list_state(&mut app);
    assert!(state.following(), "it follows the bottom");
    assert_eq!(state.count(), rows);
    let shown = state.spawned().expect("rows");
    assert_eq!(*shown.end(), rows - 1, "the newest row is built");
}

#[test]
fn a_stream_faster_than_the_view_follows_to_its_last_line() {
    let (mut app, stream) = streaming(Lang::Rust);
    for chunk in LARGE_RS.as_bytes().chunks(4096) {
        stream.push(core::str::from_utf8(chunk).expect("ASCII"));
        app.update();
        assert!(list_state(&mut app).following(), "it follows");
    }
    stream.finish();
    app.update();
    app.update();
    let rows = doc(&mut app).rows();
    let state = list_state(&mut app);
    assert!(state.following());
    assert_eq!(*state.spawned().expect("rows").end(), rows - 1);
}

#[test]
fn a_finished_empty_stream_counts_the_one_row_its_view_shows() {
    let (mut app, stream) = streaming(Lang::Rust);
    assert_eq!(stream.source().rows(), 0, "no row before a byte");
    stream.finish();
    app.update();
    assert_eq!(doc(&mut app).rows(), 1);
    assert_eq!(stream.source().rows(), doc(&mut app).rows());
    assert_eq!(
        stream.source().rows(),
        CodeSource::new(Lang::Rust, "").rows()
    );
}
