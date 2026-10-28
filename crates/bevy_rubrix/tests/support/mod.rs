#![allow(dead_code)]

use core::time::Duration;

use bevy::app::App;
use bevy::color::Color;
use bevy::ecs::entity::Entity;
use bevy::ecs::hierarchy::{ChildOf, Children};
use bevy::ecs::resource::Resource;
use bevy::text::TextSpan;
use bevy::time::{TimePlugin, TimeUpdateStrategy};
use bevy::ui::widget::Text;
use bevy::ui_widgets::{Activate, Button};
use bevy_fynix::tokens::{
    Curve, Motion, MotionTokens, SpacingTokens, SurfaceTokens,
    TextTokens, Tone,
};
use bevy_fynix::views::VirtualList;
use bevy_fynix::{FynixPlugin, Theme, mount, resource};
use bevy_rubrix::{
    DiffDoc, DiffRow, DiffRowAt, DiffSource, Layout, Overflow,
    RubrixPlugin, SyntaxTokens, diff_view,
};
use rubrix_diff::{Expansion, FileDiff, Row, rows};

pub const LARGE_PATCH: &str =
    include_str!("../../../../bench/patch/large.patch.txt");

pub struct Plain;

impl TextTokens for Plain {
    fn tone(&self, tone: Tone) -> Color {
        match tone {
            Tone::Body => Color::WHITE,
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

impl SpacingTokens for Plain {
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

impl SurfaceTokens for Plain {
    fn fill(&self) -> Color {
        Color::srgb(0.2, 0.2, 0.2)
    }

    fn hover(&self) -> Color {
        Color::srgb(0.25, 0.25, 0.25)
    }

    fn panel(&self) -> Color {
        Color::srgb(0.1, 0.1, 0.1)
    }

    fn accent(&self) -> Color {
        Color::srgb(0.4, 0.6, 1.0)
    }
}

impl MotionTokens for Plain {
    fn motion(&self, _: Motion) -> Curve {
        Curve {
            duration: Duration::ZERO,
            ease: |t| t,
        }
    }
}

impl SyntaxTokens for Plain {}

#[derive(Resource)]
pub struct Showing {
    pub source: DiffSource,
    pub layout: Layout,
    pub compact: bool,
    pub limit: Option<u32>,
}

pub fn showing(source: DiffSource) -> App {
    let mut app = App::new();
    app.add_plugins((
        TimePlugin,
        FynixPlugin::<Plain>::default(),
        RubrixPlugin,
    ))
    .insert_resource(Theme(Plain))
    .insert_resource(TimeUpdateStrategy::ManualDuration(
        Duration::from_millis(50),
    ))
    .insert_resource(Showing {
        source,
        layout: Layout::Stacked,
        compact: false,
        limit: None,
    });
    app.update();
    mount::<Plain>(
        app.world_mut(),
        diff_view(resource::<Showing, _>(|s| s.source.clone()))
            .layout(resource::<Showing, _>(|s| s.layout))
            .compact(resource::<Showing, _>(|s| s.compact))
            .limit(resource::<Showing, _>(|s| s.limit))
            .overflow(Overflow::Scroll),
    );
    settle(&mut app);
    app
}

pub fn settle(app: &mut App) {
    for _ in 0..5 {
        app.update();
    }
}

pub fn set(app: &mut App, change: impl FnOnce(&mut Showing)) {
    change(&mut app.world_mut().resource_mut::<Showing>());
    settle(app);
}

pub fn doc(app: &mut App) -> &DiffDoc {
    app.world_mut()
        .query::<&DiffDoc>()
        .single(app.world())
        .expect("one diff view")
}

pub fn list(app: &mut App) -> &mut VirtualList {
    let list = app
        .world_mut()
        .query::<(Entity, &VirtualList)>()
        .single(app.world())
        .expect("one list")
        .0;
    app.world_mut()
        .get_mut::<VirtualList>(list)
        .expect("a list")
        .into_inner()
}

pub fn scroll_to(app: &mut App, index: usize) {
    list(app).scroll_to(index);
    settle(app);
}

pub fn first_visible(app: &mut App) -> usize {
    list(app).first_visible()
}

pub fn slots(app: &mut App) -> Vec<(usize, Entity)> {
    let mut slots = app
        .world_mut()
        .query::<(Entity, &DiffRowAt)>()
        .iter(app.world())
        .map(|(entity, at)| (at.index, entity))
        .collect::<Vec<_>>();
    slots.sort();
    slots
}

pub fn slot(app: &mut App, index: usize) -> Entity {
    slots(app)
        .into_iter()
        .find(|&(shown, _)| shown == index)
        .unwrap_or_else(|| panic!("row {index} is built"))
        .1
}

fn descendants(app: &App, root: Entity) -> Vec<Entity> {
    let world = app.world();
    let mut found = Vec::new();
    let mut stack = vec![root];
    while let Some(node) = stack.pop() {
        found.push(node);
        if let Some(children) = world.get::<Children>(node) {
            stack.extend(children.iter().rev());
        }
    }
    found
}

pub fn texts(app: &App, root: Entity) -> Vec<String> {
    let world = app.world();
    descendants(app, root)
        .into_iter()
        .filter(|&node| world.get::<Text>(node).is_some())
        .map(|node| {
            let own = world.get::<Text>(node).map_or("", |t| &t.0);
            let spans = world
                .get::<Children>(node)
                .into_iter()
                .flat_map(|kids| kids.iter())
                .filter_map(|&span| world.get::<TextSpan>(span))
                .map(|span| span.0.as_str())
                .collect::<String>();
            format!("{own}{spans}")
        })
        .collect()
}

pub fn press(app: &mut App, root: Entity, text: &str) {
    let world = app.world();
    let node = descendants(app, root)
        .into_iter()
        .find(|&node| {
            world.get::<Text>(node).is_some()
                && texts(app, node).first().is_some_and(|t| t == text)
        })
        .unwrap_or_else(|| panic!("a `{text}` under {root}"));
    let mut at = node;
    while world.get::<Button>(at).is_none() {
        at = world
            .get::<ChildOf>(at)
            .expect("a button above")
            .parent();
    }
    app.world_mut().trigger(Activate { entity: at });
    settle(app);
}

pub fn expected(
    source: &DiffSource,
    layout: Layout,
    expansions: &[Expansion],
) -> Vec<DiffRow> {
    let none = Expansion::new();
    source
        .files()
        .enumerate()
        .flat_map(|(file, diff)| {
            let expansion = expansions.get(file).unwrap_or(&none);
            let rows = rows(diff, layout, expansion);
            core::iter::once(DiffRow::Header { file }).chain(
                rows.range(..)
                    .map(move |row| DiffRow::Body { file, row })
                    .collect::<Vec<_>>(),
            )
        })
        .collect()
}

pub fn shown(line: &str) -> String {
    let mut out = String::new();
    for c in line.chars() {
        if c == '\t' {
            let spaces = 4 - out.chars().count() % 4;
            out.extend(core::iter::repeat_n(' ', spaces));
        } else {
            out.push(c);
        }
    }
    out
}

pub fn old_text(diff: &FileDiff, row: Row) -> Option<String> {
    row.old_line()
        .map(|line| shown(diff.old_lines().line(line.index)))
}

pub fn new_text(diff: &FileDiff, row: Row) -> Option<String> {
    row.new_line()
        .map(|line| shown(diff.new_lines().line(line.index)))
}

pub fn numbered(count: usize) -> String {
    (1..=count).map(|n| format!("line {n}\n")).collect()
}
