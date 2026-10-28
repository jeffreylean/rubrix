mod support;

#[allow(dead_code, reason = "the gallery's drive reads the rest")]
#[path = "../examples/gallery/pages/hamlet_rich.rs"]
mod hamlet_rich;

use core::time::Duration;

use bevy::app::App;
use bevy::color::Color;
use bevy::ecs::entity::Entity;
use bevy::text::{
    FontSize, FontSource, FontStyle, FontWeight, Strikethrough,
    TextBackgroundColor, TextColor, TextFont, TextSpan, Underline,
};
use bevy::time::{TimePlugin, TimeUpdateStrategy};
use bevy::ui::px;
use bevy_fynix::tokens::{SurfaceTokens, TextTokens, Tone};
use bevy_fynix::views::{FrameProps, column};
use bevy_fynix::{FynixPlugin, Theme, ViewExt, mount};
use bevy_rubrix::{
    CODE_FONT, CodeSource, DiffSource, Lang, SyntaxTokens, code_view,
};
use hamlet_rich::{CODE_SIZE, HEADING_SIZE, Sample, Streamed};
use support::diff::{Plain, line_text, set, showing};

const BODY_SIZE: f32 = 14.0;

fn rich_page() -> App {
    let mut app = App::new();
    app.add_plugins((TimePlugin, FynixPlugin::<Plain>::default()))
        .insert_resource(Theme(Plain))
        .insert_resource(TimeUpdateStrategy::ManualDuration(
            Duration::from_millis(16),
        ));
    hamlet_rich::plugin(&mut app);
    app.update();
    mount::<Plain>(
        app.world_mut(),
        hamlet_rich::view::<Plain>(column(()).boxed()),
    );
    app.update();
    app
}

#[derive(Debug, PartialEq)]
struct Drawn {
    text: String,
    size: f32,
    bold: bool,
    italic: bool,
    font: FontSource,
    ink: Color,
    plate: Option<Color>,
    underline: bool,
    struck: bool,
}

fn drawn(app: &App, span: Entity) -> Drawn {
    let entity = app.world().entity(span);
    let font = entity.get::<TextFont>().expect("a span font");
    let FontSize::Px(size) = font.font_size else {
        panic!("a size in px");
    };
    Drawn {
        text: entity.get::<TextSpan>().expect("a span").0.clone(),
        size,
        bold: font.weight == FontWeight::BOLD,
        italic: font.style == FontStyle::Italic,
        font: font.font.clone(),
        ink: entity.get::<TextColor>().expect("an ink").0,
        plate: entity
            .get::<TextBackgroundColor>()
            .map(|plate| plate.0),
        underline: entity.contains::<Underline>(),
        struck: entity.contains::<Strikethrough>(),
    }
}

fn plain(text: &str) -> Drawn {
    Drawn {
        text: text.into(),
        size: BODY_SIZE,
        bold: false,
        italic: false,
        font: FontSource::sans_serif(),
        ink: Plain.tone(Tone::Body),
        plate: None,
        underline: false,
        struck: false,
    }
}

fn spans<M: bevy::ecs::component::Component>(
    app: &mut App,
) -> Vec<Drawn> {
    hamlet_rich::spans_in::<M>(app.world_mut())
        .into_iter()
        .map(|span| drawn(app, span))
        .collect()
}

#[test]
fn the_sample_spans_carry_hamlets_styles() {
    let mut app = rich_page();
    let expected = vec![
        Drawn {
            size: HEADING_SIZE,
            bold: true,
            ..plain("Plan")
        },
        plain("Use "),
        Drawn {
            bold: true,
            ..plain("care")
        },
        plain(" with "),
        Drawn {
            size: CODE_SIZE,
            font: FontSource::Handle(CODE_FONT),
            plate: Some(Plain.fill()),
            ..plain("rm")
        },
        plain(", "),
        Drawn {
            struck: true,
            ..plain("old")
        },
        plain(" and "),
        Drawn {
            ink: Plain.tone(Tone::Accent),
            underline: true,
            ..plain("docs")
        },
        plain("."),
    ];
    assert_eq!(spans::<Sample>(&mut app), expected);
}

#[test]
fn streaming_the_second_block_keeps_the_sample_spans() {
    let mut app = rich_page();
    let before = hamlet_rich::spans_in::<Sample>(app.world_mut());
    hamlet_rich::stream(app.world_mut());
    app.update();
    let first = spans::<Streamed>(&mut app);
    assert_eq!(first, vec![plain("Sec")]);
    for _ in 0..60 {
        app.update();
    }
    assert_eq!(
        spans::<Streamed>(&mut app),
        vec![
            plain("Second "),
            Drawn {
                italic: true,
                ..plain("grows")
            },
        ]
    );
    assert_eq!(
        hamlet_rich::spans_in::<Sample>(app.world_mut()),
        before
    );
}

#[test]
fn a_code_view_draws_its_text_at_its_size() {
    let mut app = support::laid_out();
    mount::<support::Distinct>(
        app.world_mut(),
        column((code_view(CodeSource::new(
            Lang::Rust,
            "fn main() {}",
        ))
        .size(13.0),))
        .width(px(400.0))
        .height(px(200.0)),
    );
    app.update();
    let line = support::line(&mut app, 0);
    let font = app.world().get::<TextFont>(line).expect("a font");
    assert!(
        matches!(font.font_size, FontSize::Px(13.0)),
        "{:?}",
        font.font_size
    );
}

#[test]
fn compact_lines_take_the_colour_of_their_row() {
    let mut app = showing(DiffSource::texts(
        "letters.txt",
        "a\nb\nc\n",
        "a\nB\nc\nd\n",
    ));
    set(&mut app, |showing| showing.compact = true);
    let (removed, added) = (Plain.deletion(), Plain.addition());
    let dim = Plain.tone(Tone::Dim);
    let rows = [
        (1, "  a", dim),
        (2, "- b", removed),
        (3, "+ B", added),
        (4, "  c", dim),
        (5, "+ d", added),
    ];
    for (index, text, ink) in rows {
        let line = line_text(&mut app, index);
        let spans = support::spans_of(&app, line);
        let shown = spans.iter().map(|(text, _)| text.as_str());
        assert_eq!(shown.collect::<String>(), text, "row {index}");
        for (span, colour) in spans {
            assert_eq!(colour, ink, "{span:?} in row {index}");
        }
    }
}

#[test]
fn a_code_view_built_mid_stream_shows_the_text_sent_so_far() {
    let mut app = support::app();
    let stream = CodeSource::streaming(Lang::Rust);
    stream.push("fn a() {}\nfn b");
    mount::<support::Distinct>(
        app.world_mut(),
        column((code_view(stream.source()),)),
    );
    let doc = support::doc(&mut app);
    assert_eq!(doc.text(), "fn a() {}\nfn b");
    assert_eq!(doc.rows(), 2);
}
