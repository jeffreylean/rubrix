use std::hash::{DefaultHasher, Hash, Hasher};
use std::mem;

use bevy::app::{App, PreUpdate, Update};
use bevy::clipboard::{ClipboardError, ClipboardRead};
use bevy::color::{Alpha, Color};
use bevy::ecs::component::Component;
use bevy::ecs::entity::Entity;
use bevy::ecs::lifecycle::Remove;
use bevy::ecs::message::MessageReader;
use bevy::ecs::observer::On;
use bevy::ecs::resource::Resource;
use bevy::ecs::schedule::IntoScheduleConfigs;
use bevy::ecs::schedule::common_conditions::resource_exists;
use bevy::ecs::system::{Commands, Query, Res, ResMut};
use bevy::ecs::world::World;
use bevy::math::Rect;
use bevy::ui::widget::Text;
use bevy::ui::{
    BackgroundColor, ComputedNode, Display, Node, UiGlobalTransform,
    percent, px,
};
use bevy_fynix::tokens::Tone;
use bevy_fynix::views::{
    BehaviorExt, FrameProps, button, column, ghost, label, row,
};
use bevy_fynix::{
    AnyView, ScopedExt, Signal, ViewExt, derived, resource,
};
use bevy_rubrix::{
    Buffer, ClipboardProvider, Edge, EditClipboard, EditDoc, EditKey,
    EditSystems, EditView, Editor, EditorChanged, FollowPlugin, Lang,
    LineAnchors, LineAnnotations, LineNumber, Markers, Placed,
    Ranges, RemoteCaret, RemoteCarets, RemoteView, Severity,
    code_editor,
};

use super::code::{LARGE_RS, stress};
use super::editor::{answer_doc, one_based_line_col};
use super::{Ask, control, wrapping};
use crate::{Build, PageView, Paste};

#[derive(Resource)]
pub struct CollabFile {
    doc: Entity,
    big: bool,
}

impl CollabFile {
    fn spawn(world: &mut World, big: bool) -> Entity {
        let (key, text) = if big {
            ("collab/100k", stress())
        } else {
            ("collab/large.rs.txt", LARGE_RS.to_owned())
        };
        world
            .spawn((
                EditDoc::new(Lang::Rust, &text),
                EditKey(key.into()),
                RemoteCarets::default(),
                Markers::default(),
                LineAnnotations::<Badge>(LineAnchors::default()),
            ))
            .id()
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct Badge(&'static str);

const BADGE: Color = Color::srgb(0.55, 0.35, 0.9);

pub fn plugin(app: &mut App) {
    app.add_plugins(FollowPlugin::<LineAnnotations<Badge>>::default())
        .init_resource::<Changes>()
        .add_systems(
            PreUpdate,
            count_changes
                .after(EditSystems::Apply)
                .run_if(resource_exists::<CollabFile>),
        )
        .add_systems(
            Update,
            badges.run_if(resource_exists::<CollabFile>),
        )
        .add_observer(give_back_clipboard);
}

pub fn build() -> PageView {
    AnyView::new(|cx: &mut Build| {
        if !cx.world.contains_resource::<CollabFile>() {
            let doc = CollabFile::spawn(cx.world, false);
            cx.world.insert_resource(CollabFile { doc, big: false });
        }
        let previous = mem::replace(
            &mut *cx.world.resource_mut::<EditClipboard>(),
            EditClipboard::new(CollabClipboard::default()),
        );
        cx.world.insert_resource(Previous(previous));
        let root = cx.build(
            column((
                label("Collab").size(20.0),
                controls(),
                label(size_label()).tone(Tone::Dim),
                code_editor(resource::<CollabFile, _>(|file| {
                    file.doc
                })),
            ))
            .height(percent(100.0))
            .min_height(px(0.0))
            .gap(8.0),
        );
        cx.world.entity_mut(root).insert(CollabRoot);
        root
    })
    .boxed()
}

fn size_label() -> Signal<String> {
    let shown = |world: &World| {
        let file = world.resource::<CollabFile>();
        let rows =
            world.get::<EditDoc>(file.doc).map_or(0, EditDoc::rows);
        (file.big, rows)
    };
    let mut last = None;
    derived(move |world| {
        let (big, rows) = shown(world);
        let name = if big {
            "large.rs.txt repeated"
        } else {
            "large.rs.txt"
        };
        format!("{name}, {rows} lines")
    })
    .when(move |world| {
        let now = Some(shown(world));
        mem::replace(&mut last, now) != now
    })
}

fn controls() -> PageView {
    let buttons = row((
        control(
            resource::<CollabFile, _>(|file| {
                if file.big { "large.rs" } else { "100k lines" }
                    .into()
            }),
            |world| {
                let big = !world.resource::<CollabFile>().big;
                let doc = CollabFile::spawn(world, big);
                let mut file = world.resource_mut::<CollabFile>();
                file.big = big;
                let old = mem::replace(&mut file.doc, doc);
                world.despawn(old);
            },
        ),
        control("markers", |world| {
            let (doc, editor) = on_show(world);
            let markers = Ranges::new(
                [Severity::Error, Severity::Warning, Severity::Info]
                    .into_iter()
                    .zip(129..)
                    .map(|(severity, line)| {
                        (
                            without_indent(editor.buffer(), line),
                            severity,
                        )
                    }),
            );
            world.entity_mut(doc).insert(Markers(markers));
        }),
        control("annotate", |world| {
            let (doc, editor) = on_show(world);
            let anchors = LineAnchors::new(
                editor.buffer(),
                [(199, Badge("review"))],
            );
            world.entity_mut(doc).insert(LineAnnotations(anchors));
        }),
        control("50 carets", fifty_carets),
        control("respawn", respawn),
        wide_control("1,000 markers", |world| {
            let (doc, editor) = on_show(world);
            let buffer = editor.buffer();
            let step = (buffer.line_count() / 1_000).max(1);
            let severities = [
                Severity::Error,
                Severity::Warning,
                Severity::Info,
                Severity::Hint,
            ];
            let markers = Ranges::new((0..1_000).map(|i| {
                (
                    without_indent(buffer, i as u32 * step),
                    severities[i % 4],
                )
            }));
            world.entity_mut(doc).insert(Markers(markers));
        }),
    ))
    .gap(8.0);
    wrapping(buttons.boxed())
}

fn wide_control(
    text: &'static str,
    press: impl Fn(&mut World) + Send + Sync + 'static,
) -> PageView {
    button(label(text))
        .width(px(170.0))
        .rules(ghost)
        .on_activate(press)
        .boxed()
}

fn on_show(world: &World) -> (Entity, &Editor) {
    let doc = world.resource::<CollabFile>().doc;
    let edit = world.get::<EditDoc>(doc).expect("the page's doc");
    (doc, edit.editor())
}

fn without_indent(
    buffer: &Buffer,
    line: u32,
) -> core::ops::Range<usize> {
    let (start, end) =
        (buffer.line_start(line), buffer.line_end(line));
    let text = buffer.slice(start..end);
    start + text.len() - text.trim_start().len()..end
}

fn fifty_carets(world: &mut World) {
    let (doc, editor) = on_show(world);
    let buffer = editor.buffer();
    let head = editor.selections().primary().head;
    let end = buffer.line_end(buffer.line_of(head));
    let carets = (0..50)
        .map(|i| {
            let at = buffer
                .floor_char_boundary(head + (end - head) * i / 49);
            let label = format!("u{}", i + 1);
            RemoteCaret {
                anchor: at,
                head: at,
                color: color_of(&label),
                label,
            }
        })
        .collect();
    world.entity_mut(doc).insert(RemoteCarets(carets));
}

fn respawn(world: &mut World) {
    let file = world.resource::<CollabFile>();
    let (old, big) = (file.doc, file.big);
    let mut taken = world.entity_mut(old);
    let carets = taken.take::<RemoteCarets>().unwrap_or_default();
    let markers = taken.take::<Markers>().unwrap_or_default();
    let notes = taken.take::<LineAnnotations<Badge>>();
    world.despawn(old);
    let doc = CollabFile::spawn(world, big);
    let mut spawned = world.entity_mut(doc);
    spawned.insert((carets, markers));
    if let Some(notes) = notes {
        spawned.insert(notes);
    }
    world.resource_mut::<CollabFile>().doc = doc;
}

fn color_of(name: &str) -> Color {
    let mut hash = DefaultHasher::new();
    name.hash(&mut hash);
    Color::hsl((hash.finish() % 360) as f32, 0.75, 0.55)
}

fn offset_of_place(
    buffer: &Buffer,
    (line, column): (u32, usize),
) -> Result<usize, String> {
    let last = buffer.line_count();
    if line == 0 || line > last {
        return Err(format!("line {line} is not in 1..={last}"));
    }
    let (start, end) =
        (buffer.line_start(line - 1), buffer.line_end(line - 1));
    let text = buffer.slice(start..end);
    let byte = text
        .char_indices()
        .map(|(at, _)| at)
        .chain([text.len()])
        .nth(column.saturating_sub(1))
        .ok_or_else(|| {
            format!("line {line} has no column {column}")
        })?;
    Ok(start + byte)
}

pub fn remote(
    world: &mut World,
    name: &str,
    anchor: (u32, usize),
    head: Option<(u32, usize)>,
) -> Result<(), String> {
    if !world.contains_resource::<CollabFile>() {
        return Err("the remote step needs page collab".into());
    }
    let (doc, editor) = on_show(world);
    let anchor = offset_of_place(editor.buffer(), anchor)?;
    let head = head.map_or(Ok(anchor), |head| {
        offset_of_place(editor.buffer(), head)
    })?;
    let caret = RemoteCaret {
        anchor,
        head,
        color: color_of(name),
        label: name.into(),
    };
    let mut doc = world.entity_mut(doc);
    let mut carets = doc
        .get_mut::<RemoteCarets>()
        .expect("the page spawns its doc with remote carets");
    match carets.0.iter_mut().find(|caret| caret.label == name) {
        Some(held) => *held = caret,
        None => carets.0.push(caret),
    }
    Ok(())
}

pub fn remote_line(world: &mut World) -> String {
    let Some(doc) =
        world.get_resource::<CollabFile>().map(|file| file.doc)
    else {
        return "remote none".into();
    };
    let placed: Vec<Placed> = world
        .query::<(&EditView, &RemoteView)>()
        .iter(world)
        .find(|(view, _)| view.doc() == doc)
        .map(|(_, remote)| remote.placed().to_vec())
        .unwrap_or_default();
    let Some((edit, carets)) = world
        .get::<EditDoc>(doc)
        .zip(world.get::<RemoteCarets>(doc))
    else {
        return "remote none".into();
    };
    let buffer = edit.editor().buffer();
    let each: Vec<String> = carets
        .0
        .iter()
        .enumerate()
        .map(|(i, caret)| {
            let at = if caret.anchor == caret.head {
                one_based_line_col(buffer, caret.head)
            } else {
                format!(
                    "{}..{}",
                    one_based_line_col(buffer, caret.anchor),
                    one_based_line_col(buffer, caret.head)
                )
            };
            let label = &caret.label;
            match placed.get(i) {
                Some(Placed::Bar(_)) => format!("{label} {at} shown"),
                Some(Placed::Edge(Edge::Top)) => {
                    format!("{label} edge top at {at}")
                }
                Some(Placed::Edge(Edge::Bottom)) => {
                    format!("{label} edge bottom at {at}")
                }
                None => format!("{label} {at} unplaced"),
            }
        })
        .collect();
    let carets = carets.0.clone();
    format!(
        "remote {}: {} | {}",
        each.len(),
        each.join(", "),
        painted(world, &carets)
    )
}

/// Where the remote nodes paint: how many of the bars, tints and
/// labels drawn in the rows reach into the line numbers' column, and
/// how many pairs of edge labels overlap.
fn painted(world: &mut World, carets: &[RemoteCaret]) -> String {
    let shown = |node: &Node,
                 size: &ComputedNode,
                 at: &UiGlobalTransform| {
        (node.display != Display::None && size.size().x > 0.0).then(
            || Rect::from_center_size(at.translation, size.size()),
        )
    };
    let numbers = world
        .query::<(&Node, &ComputedNode, &UiGlobalTransform, &LineNumber)>()
        .iter(world)
        .filter_map(|(node, size, at, _)| shown(node, size, at))
        .map(|number| number.max.x)
        .reduce(f32::max)
        .unwrap_or(f32::NEG_INFINITY);
    let colors: Vec<Color> = carets
        .iter()
        .flat_map(|caret| {
            [
                caret.color,
                caret.color.with_alpha(caret.color.alpha() * 0.32),
            ]
        })
        .collect();
    let mut over_gutter = 0;
    let mut edge_labels = Vec::new();
    for (node, size, at, text, background) in world
        .query::<(
            &Node,
            &ComputedNode,
            &UiGlobalTransform,
            Option<&Text>,
            Option<&BackgroundColor>,
        )>()
        .iter(world)
    {
        let Some(area) = shown(node, size, at) else {
            continue;
        };
        let remote = match (text, background) {
            (Some(text), _) => {
                if text.0.starts_with(['↑', '↓', '+']) {
                    edge_labels.push(area);
                    false
                } else {
                    carets.iter().any(|caret| caret.label == text.0)
                }
            }
            (None, Some(background)) => {
                colors.contains(&background.0)
            }
            (None, None) => false,
        };
        if remote && area.min.x < numbers {
            over_gutter += 1;
        }
    }
    let overlapping = edge_labels
        .iter()
        .enumerate()
        .flat_map(|(i, a)| {
            edge_labels[i + 1..].iter().map(move |b| (a, b))
        })
        .filter(|(a, b)| {
            let both = a.intersect(**b);
            both.width() > 0.5 && both.height() > 0.5
        })
        .count();
    format!(
        "over gutter {over_gutter} | edge labels {} overlapping \
         {overlapping}",
        edge_labels.len()
    )
}

#[derive(Resource, Default)]
struct Changes {
    doc: Option<Entity>,
    count: usize,
    consecutive: bool,
    last: u64,
}

fn count_changes(
    mut changed: MessageReader<EditorChanged>,
    file: Res<CollabFile>,
    mut changes: ResMut<Changes>,
) {
    if changes.doc != Some(file.doc) {
        *changes = Changes {
            doc: Some(file.doc),
            consecutive: true,
            ..Changes::default()
        };
    }
    for changed in
        changed.read().filter(|changed| changed.doc == file.doc)
    {
        if changes.count > 0 && changed.seq != changes.last + 1 {
            changes.consecutive = false;
        }
        changes.count += 1;
        changes.last = changed.seq;
    }
}

pub fn changed_line(world: &World) -> String {
    let changes = world.resource::<Changes>();
    format!(
        "changed {} rising {} last {}",
        changes.count, changes.consecutive, changes.last
    )
}

pub fn markers_line(world: &World) -> String {
    let Some(doc) =
        world.get_resource::<CollabFile>().map(|file| file.doc)
    else {
        return "markers none".into();
    };
    let (Some(edit), Some(markers)) =
        (world.get::<EditDoc>(doc), world.get::<Markers>(doc))
    else {
        return "markers none".into();
    };
    let buffer = edit.editor().buffer();
    let each: Vec<String> = markers
        .0
        .iter()
        .take(10)
        .map(|(range, severity)| {
            format!(
                "{} {}..{}",
                format!("{severity:?}").to_lowercase(),
                one_based_line_col(buffer, range.start),
                one_based_line_col(buffer, range.end)
            )
        })
        .collect();
    format!("markers {}: {}", markers.0.len(), each.join(", "))
}

pub fn annotations_line(world: &World) -> String {
    let Some(doc) =
        world.get_resource::<CollabFile>().map(|file| file.doc)
    else {
        return "annotations none".into();
    };
    let (Some(edit), Some(notes)) = (
        world.get::<EditDoc>(doc),
        world.get::<LineAnnotations<Badge>>(doc),
    ) else {
        return "annotations none".into();
    };
    let each: Vec<String> = notes
        .0
        .lines(edit.editor().buffer())
        .map(|(line, badge)| format!("{} {}", line + 1, badge.0))
        .collect();
    format!("annotations {}: {}", each.len(), each.join(", "))
}

fn badges(
    file: Res<CollabFile>,
    docs: Query<(&EditDoc, &LineAnnotations<Badge>)>,
    numbers: Query<(Entity, &LineNumber, Option<&BackgroundColor>)>,
    mut commands: Commands,
) {
    let lines: Vec<u32> = docs
        .get(file.doc)
        .map(|(doc, notes)| {
            notes
                .0
                .lines(doc.editor().buffer())
                .map(|(line, _)| line)
                .collect()
        })
        .unwrap_or_default();
    for (number, at, painted) in &numbers {
        match (lines.contains(&at.line), painted.is_some()) {
            (true, false) => {
                commands
                    .entity(number)
                    .insert(BackgroundColor(BADGE));
            }
            (false, true) => {
                commands.entity(number).remove::<BackgroundColor>();
            }
            _ => {}
        }
    }
}

pub fn answer(world: &mut World, ask: Ask) -> Option<String> {
    let doc = world.get_resource::<CollabFile>()?.doc;
    answer_doc(world, doc, ask)
}

#[derive(Component)]
struct CollabRoot;

#[derive(Resource)]
struct Previous(EditClipboard);

fn give_back_clipboard(
    _: On<Remove<CollabRoot>>,
    mut commands: Commands,
) {
    commands.queue(|world: &mut World| {
        if let Some(Previous(previous)) =
            world.remove_resource::<Previous>()
        {
            world.insert_resource(previous);
        }
    });
}

const SOURCE: &str = "collab: ";

/// Keeps its text in the gallery's paste slot tagged with
/// [`SOURCE`], so `log paste` shows that it served the text. It
/// strips the tag only from the slot text it wrote itself: a drive's
/// `paste` step that writes text starting with the tag gets it back
/// whole.
#[derive(Default)]
struct CollabClipboard {
    tagged: Option<String>,
}

impl CollabClipboard {
    fn hold(&mut self, world: &mut World, text: &str) {
        let tagged = format!("{SOURCE}{text}");
        world.resource_mut::<Paste>().0 = Some(tagged.clone());
        self.tagged = Some(tagged);
    }
}

impl ClipboardProvider for CollabClipboard {
    fn write(
        &mut self,
        world: &mut World,
        text: &str,
    ) -> Result<(), ClipboardError> {
        self.hold(world, text);
        Ok(())
    }

    fn read(&mut self, world: &mut World) -> ClipboardRead {
        let Some(held) = world.resource::<Paste>().0.clone() else {
            return ClipboardRead::Ready(Err(
                ClipboardError::ContentNotAvailable,
            ));
        };
        let text = match &self.tagged {
            Some(tagged) if *tagged == held => {
                held[SOURCE.len()..].to_owned()
            }
            _ => held,
        };
        self.hold(world, &text);
        ClipboardRead::Ready(Ok(text))
    }
}
