use alloc::string::String;
use alloc::vec::Vec;
use core::ops::Range;

use crate::buffer::Buffer;
use crate::change::{Assoc, Change, Replacement};

/// Byte ranges of a text, each with a payload, that follow the text
/// through each [`Change`], such as a linter's markers.
///
/// Text typed at either end of a range stays outside it, and text
/// typed inside it widens it. A change that deletes or replaces all
/// of a range's bytes drops it. An empty range is a point that text
/// typed at it pushes right.
#[derive(Clone, Debug, PartialEq)]
pub struct Ranges<T> {
    /// Sorted by start.
    items: Vec<(Range<usize>, T)>,
}

impl<T> Ranges<T> {
    pub fn new(
        items: impl IntoIterator<Item = (Range<usize>, T)>,
    ) -> Ranges<T> {
        let mut items: Vec<_> = items.into_iter().collect();
        items.sort_by_key(|(range, _)| range.start);
        Ranges { items }
    }

    pub fn len(&self) -> usize {
        self.items.len()
    }

    pub fn is_empty(&self) -> bool {
        self.items.is_empty()
    }

    /// The ranges by start.
    pub fn iter(
        &self,
    ) -> impl ExactSizeIterator<Item = (Range<usize>, &T)> + '_ {
        self.items.iter().map(|(range, item)| (range.clone(), item))
    }

    /// Moves every range through `change`, which was made to the text
    /// the ranges are in, and drops each range it deleted.
    pub fn map(&mut self, change: &Change) {
        if change.is_empty() {
            return;
        }
        self.items.retain_mut(|(range, _)| {
            if range.start == range.end {
                let at = change.map(range.start, Assoc::After);
                *range = at..at;
                return true;
            }
            let start = change.map(range.start, Assoc::After);
            let end = change.map(range.end, Assoc::Before);
            *range = start..end;
            start < end
        });
    }
}

impl<T> Default for Ranges<T> {
    fn default() -> Ranges<T> {
        Ranges { items: Vec::new() }
    }
}

/// Payloads on the lines of a text, such as review comments, that
/// follow their lines' text through each [`Change`].
///
/// A payload moves with its line's text: an Enter before the line's
/// first byte moves it down with the text, and a Backspace that joins
/// the line onto the one above moves it up onto that line. A change
/// that deletes a line's text with the line break that ends it drops
/// the payload. The change that puts that text back in the same
/// place, such as the undo of the deletion, brings it back to its
/// line, so undo and redo restore payloads as they restore the text.
#[derive(Clone, Debug)]
pub struct LineAnchors<T> {
    /// Sorted by offset. An anchor is the offset of the first byte
    /// of its line's text: the line's start, unless a join moved
    /// that text onto the end of the line above.
    items: Vec<(usize, T)>,
    /// Oldest first, at most [`LineAnchors::MAX_DROPPED`].
    dropped: Vec<Dropped<T>>,
}

/// Payloads whose lines one replacement of a change deleted.
#[derive(Clone, Debug)]
struct Dropped<T> {
    /// The replacement's new text, in the text as it is now: where a
    /// change that puts `text` back would replace.
    at: Range<usize>,
    /// The text the replacement removed.
    text: String,
    /// Each payload with its anchor's offset in `text`.
    items: Vec<(usize, T)>,
}

/// One replacement of a change, with where its new text starts after
/// the change.
struct Rep<'a> {
    range: Range<usize>,
    old: &'a str,
    new: &'a str,
    after: usize,
}

fn reps(change: &Change) -> Vec<Rep<'_>> {
    let mut delta = 0isize;
    change
        .replacements()
        .map(|Replacement { range, old, new }| {
            let after = range.start.wrapping_add_signed(delta);
            delta += new.len() as isize - old.len() as isize;
            Rep {
                range,
                old,
                new,
                after,
            }
        })
        .collect()
}

/// Where an anchor goes through a change.
enum Moved {
    To(usize),
    /// Rep `rep` deleted the anchor's line.
    Dropped {
        rep: usize,
    },
}

fn move_anchor(change: &Change, reps: &[Rep], at: usize) -> Moved {
    let first = reps.partition_point(|rep| rep.range.end < at);
    let starting = reps[first..]
        .iter()
        .position(|rep| rep.range.start == at)
        .map(|i| first + i);
    if let Some(start) = starting {
        let group = reps[start..]
            .iter()
            .take_while(|rep| rep.range.start == at)
            .enumerate();
        let mut to = reps[start].after;
        for (i, rep) in group {
            if rep.old.contains('\n') {
                return Moved::Dropped { rep: start + i };
            }
            if let Some(last) = rep.new.rfind('\n') {
                to = rep.after + last + 1;
            }
        }
        return Moved::To(to);
    }
    match reps.get(first) {
        Some(rep) if rep.range.start < at => {
            if rep.old[at - rep.range.start..].contains('\n') {
                Moved::Dropped { rep: first }
            } else {
                Moved::To(rep.after + rep.new.len())
            }
        }
        _ => Moved::To(change.map(at, Assoc::After)),
    }
}

impl<T> LineAnchors<T> {
    /// The most deletions kept for their text to come back, as many
    /// as `History` keeps undo steps by default.
    pub const MAX_DROPPED: usize =
        crate::History::DEFAULT_MAX_ENTRIES;

    /// `items`, each on a line of `buffer`. A line past the last
    /// counts as the last.
    pub fn new(
        buffer: &Buffer,
        items: impl IntoIterator<Item = (u32, T)>,
    ) -> LineAnchors<T> {
        let last = buffer.line_count() - 1;
        let mut items: Vec<_> = items
            .into_iter()
            .map(|(line, item)| {
                (buffer.line_start(line.min(last)), item)
            })
            .collect();
        items.sort_by_key(|(at, _)| *at);
        LineAnchors {
            items,
            dropped: Vec::new(),
        }
    }

    pub fn len(&self) -> usize {
        self.items.len()
    }

    pub fn is_empty(&self) -> bool {
        self.items.is_empty()
    }

    /// Each payload with its line in `buffer`, the text the anchors
    /// follow, by line.
    pub fn lines<'a>(
        &'a self,
        buffer: &'a Buffer,
    ) -> impl ExactSizeIterator<Item = (u32, &'a T)> + 'a {
        self.items
            .iter()
            .map(|(at, item)| (buffer.line_of(*at), item))
    }

    /// Moves every payload through `change`, made to the text the
    /// anchors follow: drops the payloads of the lines it deleted,
    /// and brings back those it puts back.
    pub fn map(&mut self, change: &Change) {
        if change.is_empty() {
            return;
        }
        let reps = reps(change);
        let mut back = Vec::new();
        self.dropped.retain_mut(|dropped| {
            let put = reps.iter().find(|rep| rep.range == dropped.at);
            let Some((rep, found)) = put.and_then(|rep| {
                Some((rep, rep.new.find(dropped.text.as_str())?))
            }) else {
                return true;
            };
            let base = rep.after + found;
            back.extend(
                core::mem::take(&mut dropped.items)
                    .into_iter()
                    .map(|(rel, item)| (base + rel, item)),
            );
            false
        });
        self.dropped.retain_mut(|dropped| {
            let at = &dropped.at;
            let touched = reps.iter().any(|rep| {
                if at.is_empty() {
                    rep.range.start < at.start
                        && at.start < rep.range.end
                } else {
                    rep.range.start < at.end
                        && at.start < rep.range.end
                }
            });
            if touched {
                return false;
            }
            dropped.at = if at.is_empty() {
                let to = change.map(at.start, Assoc::After);
                to..to
            } else {
                change.map(at.start, Assoc::After)
                    ..change.map(at.end, Assoc::Before)
            };
            true
        });
        let mut gone: Vec<(usize, Dropped<T>)> = Vec::new();
        let mut kept = Vec::with_capacity(self.items.len());
        for (at, item) in core::mem::take(&mut self.items) {
            match move_anchor(change, &reps, at) {
                Moved::To(to) => kept.push((to, item)),
                Moved::Dropped { rep: i } => {
                    let rep = &reps[i];
                    let rel = at - rep.range.start;
                    match gone.iter_mut().find(|(j, _)| *j == i) {
                        Some((_, dropped)) => {
                            dropped.items.push((rel, item))
                        }
                        None => gone.push((
                            i,
                            Dropped {
                                at: rep.after
                                    ..rep.after + rep.new.len(),
                                text: rep.old.into(),
                                items: Vec::from([(rel, item)]),
                            },
                        )),
                    }
                }
            }
        }
        self.dropped
            .extend(gone.into_iter().map(|(_, dropped)| dropped));
        let excess =
            self.dropped.len().saturating_sub(Self::MAX_DROPPED);
        self.dropped.drain(..excess);
        kept.extend(back);
        kept.sort_by_key(|(at, _)| *at);
        self.items = kept;
    }
}

impl<T> Default for LineAnchors<T> {
    fn default() -> LineAnchors<T> {
        LineAnchors {
            items: Vec::new(),
            dropped: Vec::new(),
        }
    }
}
