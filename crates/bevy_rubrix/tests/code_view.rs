//! The code view, headless with no layout: what its rows show.
//!
//! With no layout the view is 0 px high, so its list builds the rows
//! within its 400 px overscan of the scroll offset, 48 at the top.

use core::time::Duration;

use bevy::app::App;
use bevy::color::Color;
use bevy::ecs::entity::Entity;
use bevy::ecs::hierarchy::Children;
use bevy::ecs::query::With;
use bevy::ecs::resource::Resource;
use bevy::text::{LineBreak, TextColor, TextLayout, TextSpan};
use bevy::time::{TimePlugin, TimeUpdateStrategy};
use bevy::ui::widget::Text;
use bevy::ui::{Node, OverflowAxis};
use bevy_fynix::tokens::{SpacingTokens, TextTokens, Tone};
use bevy_fynix::views::VirtualList;
use bevy_fynix::{FynixPlugin, Theme, mount, resource};
use bevy_rubrix::{
    CodeLine, CodeSource, Kind, Lang, Overflow, RubrixPlugin,
    SyntaxStyle, SyntaxTokens, code_view, resolve,
};
use rubrix_highlight::{highlight, spans};

const BODY: Color = Color::WHITE;

/// A theme that gives every root kind a colour of its own, so a span
/// in the wrong kind shows in the wrong colour.
struct Distinct;

fn root_colour(kind: Kind) -> Color {
    Color::srgb_u8(kind as u8 * 4, 64, 128)
}

impl TextTokens for Distinct {
    fn tone(&self, tone: Tone) -> Color {
        match tone {
            Tone::Body => BODY,
            _ => Color::BLACK,
        }
    }

    fn body_size(&self) -> f32 {
        14.0
    }

    fn small_size(&self) -> f32 {
        11.0
    }
}

impl SpacingTokens for Distinct {
    fn gap(&self) -> f32 {
        8.0
    }

    fn row(&self) -> f32 {
        20.0
    }

    fn radius(&self) -> f32 {
        3.0
    }
}

impl SyntaxTokens for Distinct {
    fn style(&self, kind: Kind) -> SyntaxStyle {
        match kind.parent() {
            None => SyntaxStyle::color(root_colour(kind)),
            Some(_) => SyntaxStyle::default(),
        }
    }
}

fn colour(kind: Kind) -> Color {
    resolve(&Distinct, kind).0
}

fn app() -> App {
    let mut app = App::new();
    app.add_plugins((
        TimePlugin,
        FynixPlugin::<Distinct>::default(),
        RubrixPlugin,
    ))
    .insert_resource(Theme(Distinct))
    .insert_resource(TimeUpdateStrategy::ManualDuration(
        Duration::from_millis(50),
    ));
    // The first update only starts the clock.
    app.update();
    app
}

#[derive(Resource)]
struct Showing(CodeSource);

#[derive(Resource)]
struct Wrapping(Overflow);

/// An app showing the source in [`Showing`], which a test may swap.
fn showing(source: CodeSource) -> App {
    let mut app = app();
    app.insert_resource(Showing(source))
        .insert_resource(Wrapping(Overflow::Scroll));
    mount::<Distinct>(
        app.world_mut(),
        code_view(resource::<Showing, _>(|showing| {
            showing.0.clone()
        }))
        .overflow(resource::<Wrapping, _>(|wrapping| wrapping.0)),
    );
    app.update();
    app
}

fn show(app: &mut App, source: CodeSource) {
    app.world_mut().resource_mut::<Showing>().0 = source;
    app.update();
}

fn list(app: &mut App) -> Entity {
    app.world_mut()
        .query_filtered::<Entity, With<VirtualList>>()
        .single(app.world())
        .expect("one list")
}

fn list_state(app: &mut App) -> &VirtualList {
    let list = list(app);
    app.world().get::<VirtualList>(list).expect("a list")
}

/// The spans of the rich text `root`: their text and colour.
fn spans_of(app: &App, root: Entity) -> Vec<(String, Color)> {
    let world = app.world();
    world
        .get::<Children>(root)
        .map(|kids| {
            kids.iter()
                .copied()
                .filter_map(|span| {
                    let text = world.get::<TextSpan>(span)?;
                    let colour = world.get::<TextColor>(span)?;
                    Some((text.0.clone(), colour.0))
                })
                .collect()
        })
        .unwrap_or_default()
}

fn text_of(app: &App, root: Entity) -> String {
    spans_of(app, root)
        .into_iter()
        .map(|(text, _)| text)
        .collect()
}

/// Each built row's text rich, by the row it shows.
fn lines(app: &mut App) -> Vec<(usize, Entity)> {
    let mut lines = app
        .world_mut()
        .query::<(Entity, &CodeLine)>()
        .iter(app.world())
        .map(|(entity, line)| (line.row, entity))
        .collect::<Vec<_>>();
    lines.sort();
    lines
}

fn line(app: &mut App, row: usize) -> Entity {
    lines(app)
        .into_iter()
        .find(|&(shown, _)| shown == row)
        .unwrap_or_else(|| panic!("row {row} is built"))
        .1
}

/// The text of every rich text that is all digits and spaces: the
/// line numbers.
fn numbers(app: &mut App) -> Vec<String> {
    let roots = app
        .world_mut()
        .query_filtered::<Entity, With<Text>>()
        .iter(app.world())
        .collect::<Vec<_>>();
    let mut numbers = roots
        .into_iter()
        .map(|root| text_of(app, root))
        .filter(|text| {
            !text.trim().is_empty()
                && text
                    .chars()
                    .all(|c| c == ' ' || c.is_ascii_digit())
        })
        .collect::<Vec<_>>();
    numbers.sort_by_key(|number| number.trim().parse::<usize>().ok());
    numbers
}

/// What `highlight` says a row shows: the text of each run in its
/// kind's colour, runs of one colour joined.
fn expected(
    lang: Lang,
    text: &str,
    row: usize,
) -> Vec<(String, Color)> {
    let highlights = highlight(lang, text);
    let content = &text[highlights.line_range(row as u32)];
    let mut out: Vec<(String, Color)> = Vec::new();
    for (range, kind) in spans(highlights.stored_runs(row as u32)) {
        let piece = &content[range];
        match out.last_mut() {
            Some((text, last)) if *last == colour(kind) => {
                text.push_str(piece);
            }
            _ => out.push((piece.to_owned(), colour(kind))),
        }
    }
    out
}

fn numbered(count: usize) -> String {
    (1..=count).map(|n| format!("line {n}\n")).collect()
}

const SNIPPET: &str = "\
// Counts the words in a line.
fn count(line: &str) -> usize {
    let mut words = 0;
    for word in line.split(' ') {
        if !word.is_empty() {
            words += 1;
        }
    }
    words
}
const NAME: &str = \"count\"; // the name
";

#[test]
fn spawned_rows_show_the_runs_highlight_gives() {
    let text = SNIPPET.repeat(4);
    let mut app = showing(CodeSource::new(Lang::Rust, text.as_str()));
    let built = lines(&mut app);
    assert_eq!(built.len(), 44, "every row of a short text is built");
    for (row, root) in built {
        assert_eq!(
            spans_of(&app, root),
            expected(Lang::Rust, &text, row),
            "row {row}"
        );
    }
    let at = |app: &mut App, row| {
        let root = line(app, row);
        spans_of(app, root)
    };
    let s = |text: &str, kind| (text.to_owned(), colour(kind));
    assert_eq!(
        at(&mut app, 2),
        [
            s("    let mut ", Kind::Keyword),
            s("words ", Kind::Variable),
            s("= ", Kind::Operator),
            s("0", Kind::Number),
            s(";", Kind::Punctuation),
        ]
    );
    assert_eq!(
        at(&mut app, 10),
        [
            s("const ", Kind::Keyword),
            s("NAME", Kind::Constant),
            s(": ", Kind::Punctuation),
            s("&", Kind::Operator),
            s("str ", Kind::Type),
            s("= ", Kind::Operator),
            s("\"count\"", Kind::String),
            s("; ", Kind::Punctuation),
            s("// the name", Kind::Comment),
        ]
    );
}

#[test]
fn replacing_the_source_replaces_the_rows() {
    let mut app = showing(CodeSource::new(Lang::Plain, "a\nb\nc\n"));
    let before = list(&mut app);
    show(&mut app, CodeSource::new(Lang::Plain, "x\ny\nz\n"));
    let shown = lines(&mut app)
        .into_iter()
        .map(|(_, root)| text_of(&app, root))
        .collect::<Vec<_>>();
    assert_eq!(shown, ["x", "y", "z"]);
    assert_ne!(list(&mut app), before, "a new list");

    show(&mut app, CodeSource::new(Lang::Plain, numbered(10_000)));
    let ten_k = list(&mut app);
    app.world_mut()
        .get_mut::<VirtualList>(ten_k)
        .expect("a list")
        .scroll_to(5_000);
    for _ in 0..5 {
        app.update();
    }
    assert_eq!(list_state(&mut app).first_visible(), 5_000);

    show(&mut app, CodeSource::new(Lang::Plain, "one\ntwo\nthree\n"));
    let state = list_state(&mut app);
    assert_eq!(state.count(), 3);
    assert_eq!(state.first_visible(), 0);
    let shown = lines(&mut app)
        .into_iter()
        .map(|(row, root)| (row, text_of(&app, root)))
        .collect::<Vec<_>>();
    assert_eq!(
        shown,
        [(0, "one".into()), (1, "two".into()), (2, "three".into())]
    );
    assert_eq!(numbers(&mut app), ["1", "2", "3"]);
}

#[test]
fn a_row_built_late_inside_a_block_comment_shows_comment() {
    let text = format!(
        "/*\n{}*/\nfn main() {{}}\n",
        " * a comment line\n".repeat(300)
    );
    let mut app = showing(CodeSource::new(Lang::Rust, text.as_str()));
    let list = list(&mut app);
    app.world_mut()
        .get_mut::<VirtualList>(list)
        .expect("a list")
        .scroll_to(200);
    for _ in 0..5 {
        app.update();
    }
    let spawned = list_state(&mut app).spawned().expect("rows");
    assert!(*spawned.start() > 0, "row 0 is not built: {spawned:?}");
    let row = line(&mut app, 200);
    let spans = spans_of(&app, row);
    assert_eq!(
        spans,
        [(" * a comment line".to_owned(), colour(Kind::Comment))]
    );
}

#[test]
fn a_final_newline_hides_the_empty_last_line() {
    let mut app = showing(CodeSource::new(Lang::Plain, "a\nb\n"));
    assert_eq!(list_state(&mut app).count(), 2);
    assert_eq!(numbers(&mut app), ["1", "2"]);
}

#[test]
fn an_empty_text_shows_one_row() {
    let mut app = showing(CodeSource::empty());
    assert_eq!(list_state(&mut app).count(), 1);
    assert_eq!(numbers(&mut app), ["1"]);
    let row = line(&mut app, 0);
    assert_eq!(text_of(&app, row), "");
}

#[test]
fn a_crlf_line_shows_no_carriage_return() {
    let mut app = showing(CodeSource::new(Lang::Plain, "a\r\nb\r\n"));
    let shown = lines(&mut app)
        .into_iter()
        .map(|(_, root)| text_of(&app, root))
        .collect::<Vec<_>>();
    assert_eq!(shown, ["a", "b"]);
}

#[test]
fn every_gutter_has_one_width() {
    let mut app =
        showing(CodeSource::new(Lang::Plain, numbered(10_000)));
    let widths = |numbers: Vec<String>| {
        numbers
            .iter()
            .map(|n| n.chars().count())
            .collect::<Vec<_>>()
    };
    let ten_k = numbers(&mut app);
    assert!(ten_k.len() > 40, "{} rows built", ten_k.len());
    assert!(widths(ten_k).iter().all(|&width| width == 5));
    assert_eq!(numbers(&mut app)[0], "    1");

    show(&mut app, CodeSource::new(Lang::Plain, numbered(100_000)));
    assert!(
        widths(numbers(&mut app)).iter().all(|&width| width == 6)
    );
}

#[test]
fn tabs_show_as_spaces_to_four_column_stops() {
    let mut app =
        showing(CodeSource::new(Lang::Plain, "\tx\n日\ty\n"));
    let shown = lines(&mut app)
        .into_iter()
        .map(|(_, root)| text_of(&app, root))
        .collect::<Vec<_>>();
    assert_eq!(shown, ["    x", "日   y"]);
}

#[test]
fn switching_overflow_keeps_the_list_and_flips_wrap() {
    let mut app = showing(CodeSource::new(Lang::Plain, numbered(20)));
    let state = |app: &mut App| {
        let list = list(app);
        let axis =
            app.world().get::<Node>(list).expect("a node").overflow.x;
        let breaks = lines(app)
            .into_iter()
            .map(|(_, root)| {
                app.world()
                    .get::<TextLayout>(root)
                    .expect("text")
                    .linebreak
            })
            .collect::<Vec<_>>();
        (list, axis, breaks)
    };
    let (before, axis, breaks) = state(&mut app);
    assert_eq!(axis, OverflowAxis::Scroll);
    assert!(breaks.iter().all(|&b| b == LineBreak::NoWrap));

    app.world_mut().resource_mut::<Wrapping>().0 = Overflow::Wrap;
    app.update();
    app.update();
    let (after, axis, breaks) = state(&mut app);
    assert_eq!(after, before, "the same list");
    assert_eq!(axis, OverflowAxis::Clip);
    assert_eq!(breaks.len(), 20);
    assert!(breaks.iter().all(|&b| b == LineBreak::WordBoundary));
}
