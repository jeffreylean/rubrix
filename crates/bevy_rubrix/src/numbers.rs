//! An editable view's line numbers, kept by line.

use core::mem;

use bevy::camera::visibility::Visibility;
use bevy::color::Color;
use bevy::ecs::change_detection::DetectChangesMut;
use bevy::ecs::component::Component;
use bevy::ecs::entity::Entity;
use bevy::ecs::hierarchy::ChildOf;
use bevy::ecs::query::Without;
use bevy::ecs::system::{Commands, Query};
use bevy::ecs::world::World;
use bevy::math::BVec2;
use bevy::picking::Pickable;
use bevy::text::{
    FontSize, FontSource, TextColor, TextFont, TextLayout,
};
use bevy::ui::widget::Text;
use bevy::ui::{IgnoreScroll, Node, PositionType, Val, px};

use crate::editor::EditDoc;
use crate::rows::{NumberedRow, RowStyle, digits};

/// On each line number an editable view shows: the view, and the
/// line it numbers, from 0. A number hidden for reuse keeps its last
/// line.
#[derive(Component, Clone, Copy, Debug, PartialEq, Eq)]
pub struct LineNumber {
    pub view: Entity,
    pub line: u32,
}

/// The line numbers over an editable list's gutters, one text per
/// line instead of one per row slot. A splice moves each row below
/// it to another line, so a number kept in the row's slot would be
/// laid out again for every such row on each Enter; a number kept by
/// its line stays where it was.
#[derive(Component)]
pub(crate) struct Numbers {
    view: Entity,
    doc: Entity,
    font: FontSource,
    size: f32,
    color: Color,
    width: usize,
    /// Sorted by line.
    shown: Vec<(usize, Entity)>,
    spare: Vec<Entity>,
}

pub(crate) fn spawn(
    world: &mut World,
    list: Entity,
    view: Entity,
    doc: Entity,
    style: &RowStyle,
) {
    world.spawn((
        Node {
            position_type: PositionType::Absolute,
            top: px(0.0),
            left: px(0.0),
            ..Node::default()
        },
        IgnoreScroll(BVec2::new(true, false)),
        Pickable::IGNORE,
        Numbers {
            view,
            doc,
            font: style.font.clone(),
            size: style.size,
            color: style.faint,
            width: 0,
            shown: Vec::new(),
            spare: Vec::new(),
        },
        ChildOf(list),
    ));
}

/// Gives each built row's line a number at the row's top: a line
/// that keeps its number keeps its text, a new line takes a spare
/// number, and a line no longer built gives its number back.
pub(crate) fn place(
    mut layers: Query<(Entity, &mut Numbers)>,
    rows: Query<(&NumberedRow, &ChildOf)>,
    slots: Query<&Node, Without<LineNumber>>,
    mut labels: Query<(
        &mut LineNumber,
        &mut Text,
        &mut Node,
        &mut Visibility,
    )>,
    docs: Query<&EditDoc>,
    mut commands: Commands,
) {
    for (layer, mut numbers) in &mut layers {
        let numbers = &mut *numbers;
        let width =
            docs.get(numbers.doc).map_or(1, |doc| digits(doc.rows()));
        let relabel =
            mem::replace(&mut numbers.width, width) != width;
        let text = |line: usize| format!("{:>width$}", line + 1);
        let mut wanted = rows
            .iter()
            .filter(|(row, _)| row.view == numbers.view)
            .filter_map(|(row, slot)| {
                match slots.get(slot.parent()).ok()?.top {
                    Val::Px(top) => Some((row.line, top)),
                    _ => None,
                }
            })
            .collect::<Vec<_>>();
        wanted.sort_unstable_by_key(|&(line, _)| line);
        wanted.dedup_by_key(|&mut (line, _)| line);
        let mut old =
            mem::take(&mut numbers.shown).into_iter().peekable();
        let mut fresh = Vec::new();
        for (line, top) in wanted {
            while let Some((_, gone)) =
                old.next_if(|&(shown, _)| shown < line)
            {
                numbers.spare.push(gone);
            }
            let Some((_, label)) =
                old.next_if(|&(shown, _)| shown == line)
            else {
                fresh.push((line, top));
                continue;
            };
            if let Ok((_, mut shown, mut node, _)) =
                labels.get_mut(label)
            {
                if relabel {
                    shown.0 = text(line);
                }
                if node.top != px(top) {
                    node.top = px(top);
                }
            }
            numbers.shown.push((line, label));
        }
        numbers.spare.extend(old.map(|(_, gone)| gone));
        for (line, top) in fresh {
            let tag = LineNumber {
                view: numbers.view,
                line: line as u32,
            };
            let reused = numbers.spare.pop().filter(|&label| {
                let Ok((mut at, mut shown, mut node, mut visible)) =
                    labels.get_mut(label)
                else {
                    return false;
                };
                *at = tag;
                shown.0 = text(line);
                node.top = px(top);
                *visible = Visibility::Inherited;
                true
            });
            let label = reused.unwrap_or_else(|| {
                commands
                    .spawn((
                        tag,
                        Text::new(text(line)),
                        TextFont {
                            font: numbers.font.clone(),
                            font_size: FontSize::Px(numbers.size),
                            ..TextFont::default()
                        },
                        TextColor(numbers.color),
                        TextLayout::no_wrap(),
                        Node {
                            position_type: PositionType::Absolute,
                            top: px(top),
                            left: px(0.0),
                            ..Node::default()
                        },
                        Pickable::IGNORE,
                        ChildOf(layer),
                    ))
                    .id()
            });
            numbers.shown.push((line, label));
        }
        numbers.shown.sort_unstable_by_key(|&(line, _)| line);
        for &spare in &numbers.spare {
            if let Ok((_, _, _, mut visible)) = labels.get_mut(spare)
            {
                visible.set_if_neq(Visibility::Hidden);
            }
        }
    }
}
