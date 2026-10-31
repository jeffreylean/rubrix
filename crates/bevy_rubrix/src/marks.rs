//! Decorations an app or a feature draws on an editable view's rows.

use core::ops::Range;

use bevy::color::Color;
use bevy::ecs::component::Component;

use crate::damage::Damage;

/// Row decorations on an editable view's root, in layers: a find
/// bar's matches, a bracket pair, an IME preedit. Each layer has one
/// owner, which replaces it whole with [`Marks::set`]. The rows of
/// the lines a set touches draw again in the next fynix Mount.
#[derive(Component, Default)]
pub struct Marks {
    layers: Vec<(MarkLayer, Vec<Mark>)>,
    damage: Damage,
}

/// The name of a layer of [`Marks`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MarkLayer(pub &'static str);

/// One decoration of one line, in source bytes of that line.
#[derive(Clone, Debug, PartialEq)]
pub struct Mark {
    pub line: u32,
    pub kind: MarkKind,
}

#[derive(Clone, Debug, PartialEq)]
pub enum MarkKind {
    /// A background behind the bytes.
    Tint { bytes: Range<usize>, color: Color },
    /// A line under the bytes.
    Underline { bytes: Range<usize>, color: Color },
    /// Text drawn before the byte `at` that the buffer does not hold,
    /// such as an IME preedit, underlined when `underline` is set. A
    /// click on it lands at `at`. While the primary caret sits at
    /// `at`, it is drawn `caret` bytes into the text, or not at all
    /// for `None`.
    Insert {
        at: usize,
        text: String,
        underline: Option<Color>,
        caret: Option<usize>,
    },
}

impl Marks {
    /// Replaces the marks of `layer`.
    pub fn set(&mut self, layer: MarkLayer, mut marks: Vec<Mark>) {
        marks.sort_by_key(|mark| mark.line);
        let old = match self
            .layers
            .iter()
            .position(|(name, _)| *name == layer)
        {
            Some(at) => {
                core::mem::replace(&mut self.layers[at].1, marks)
            }
            None => {
                self.layers.push((layer, marks));
                Vec::new()
            }
        };
        let new = self
            .layers
            .iter()
            .find(|(name, _)| *name == layer)
            .map_or(&[][..], |(_, marks)| marks);
        let mut lines: Vec<u32> =
            old.iter().chain(new).map(|mark| mark.line).collect();
        if lines.is_empty() {
            return;
        }
        lines.sort_unstable();
        lines.dedup();
        self.damage
            .note(&[], lines.into_iter().map(|line| line..line + 1));
    }

    /// The marks of `layer`, by line.
    pub fn layer(&self, layer: MarkLayer) -> &[Mark] {
        self.layers
            .iter()
            .find(|(name, _)| *name == layer)
            .map_or(&[], |(_, marks)| marks)
    }

    /// Every mark on `line`, layer by layer.
    pub fn on(&self, line: u32) -> impl Iterator<Item = &Mark> + '_ {
        self.layers.iter().flat_map(move |(_, marks)| {
            let from = marks.partition_point(|mark| mark.line < line);
            marks[from..]
                .iter()
                .take_while(move |mark| mark.line == line)
        })
    }

    pub(crate) fn damage(&self) -> &Damage {
        &self.damage
    }

    pub(crate) fn settle(&mut self) {
        self.damage.settle();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tint(line: u32) -> Mark {
        Mark {
            line,
            kind: MarkKind::Tint {
                bytes: 0..1,
                color: Color::WHITE,
            },
        }
    }

    #[test]
    fn a_set_dirties_the_lines_of_the_old_and_the_new_marks() {
        const FIND: MarkLayer = MarkLayer("find");
        let mut marks = Marks::default();
        marks.damage.settle();
        marks.set(FIND, vec![tint(9), tint(3)]);
        let read = marks.damage().seq();
        marks.set(FIND, vec![tint(5)]);
        let fires =
            [3, 4, 5, 9].map(|line| marks.damage().fires(line, read));
        assert_eq!(fires, [true, false, true, true]);
        assert_eq!(marks.on(5).count(), 1);
        assert_eq!(marks.on(3).count(), 0);
    }

    #[test]
    fn on_reads_every_layer() {
        let mut marks = Marks::default();
        marks.set(MarkLayer("a"), vec![tint(2), tint(2), tint(4)]);
        marks.set(MarkLayer("b"), vec![tint(2)]);
        assert_eq!(marks.on(2).count(), 3);
        assert_eq!(marks.layer(MarkLayer("a")).len(), 3);
    }
}
