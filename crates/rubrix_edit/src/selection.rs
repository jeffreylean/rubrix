use alloc::vec::Vec;
use core::ops::Range;

use crate::change::{Assoc, Change};

/// A selected range of a text. `head` is where the caret is drawn and
/// what motions move, and `anchor` is the end that stays when Shift
/// extends. A caret is a selection with `anchor == head`. Byte
/// offsets.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Selection {
    pub anchor: usize,
    pub head: usize,
}

impl Selection {
    pub const fn caret(at: usize) -> Selection {
        Selection {
            anchor: at,
            head: at,
        }
    }

    pub fn start(&self) -> usize {
        self.anchor.min(self.head)
    }

    pub fn end(&self) -> usize {
        self.anchor.max(self.head)
    }

    pub fn range(&self) -> Range<usize> {
        self.start()..self.end()
    }

    pub fn is_caret(&self) -> bool {
        self.anchor == self.head
    }

    /// Whether `self` and `next`, which starts at or after it,
    /// overlap under the rule in [`Selections`]' doc.
    fn overlaps(&self, next: &Selection) -> bool {
        if self.is_caret() || next.is_caret() {
            next.start() <= self.end()
        } else {
            next.start() < self.end()
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
struct Item {
    selection: Selection,
    /// When the selection was added. The primary has the greatest.
    age: u32,
    /// The x of the last vertical motion, which the next one aims
    /// for. Every other command clears it.
    goal: Option<f32>,
}

impl Item {
    /// The union of two overlapping items, in the newer one's slot.
    fn merge(self, other: Item) -> Item {
        let (older, newer) = if self.age < other.age {
            (self, other)
        } else {
            (other, self)
        };
        let start =
            older.selection.start().min(newer.selection.start());
        let end = older.selection.end().max(newer.selection.end());
        let own = newer.selection;
        let forward = if own.is_caret() {
            own.head != start
        } else {
            own.anchor < own.head
        };
        let selection = if forward {
            Selection {
                anchor: start,
                head: end,
            }
        } else {
            Selection {
                anchor: end,
                head: start,
            }
        };
        Item { selection, ..newer }
    }
}

/// The selections of one text: at least one, sorted by position, none
/// overlapping.
///
/// Each selection remembers when it was added, and the primary is the
/// one added last, as in Pierre. Selections that come to overlap
/// merge into their union, which takes the newest member's slot: its
/// age, its direction and its goal x. So a merge never loses the
/// primary. As in Pierre, two ranges overlap when they share a byte,
/// so ranges that only touch stay apart, which lets adjacent
/// occurrences be selected at once. A caret overlaps a range that
/// holds it, its ends included, and another caret at the same offset.
#[derive(Clone, Debug, PartialEq)]
pub struct Selections {
    items: Vec<Item>,
    next_age: u32,
}

impl Selections {
    /// One selection, which is the primary.
    pub fn new(selection: Selection) -> Selections {
        Selections {
            items: Vec::from([Item {
                selection,
                age: 0,
                goal: None,
            }]),
            next_age: 1,
        }
    }

    /// At least 1.
    #[expect(
        clippy::len_without_is_empty,
        reason = "a `Selections` is never empty"
    )]
    pub fn len(&self) -> usize {
        self.items.len()
    }

    /// The selections in buffer order.
    pub fn iter(
        &self,
    ) -> impl ExactSizeIterator<Item = Selection> + '_ {
        self.items.iter().map(|item| item.selection)
    }

    /// The selection added last.
    pub fn primary(&self) -> Selection {
        self.items[self.primary_index()].selection
    }

    /// The primary's index in [`Selections::iter`] order.
    pub fn primary_index(&self) -> usize {
        (0..self.items.len())
            .max_by_key(|&i| self.items[i].age)
            .unwrap_or(0)
    }

    /// Adds `selection` as the newest, so it becomes the primary,
    /// then merges it with any selection it overlaps.
    pub fn add(&mut self, selection: Selection) {
        self.items.push(Item {
            selection,
            age: self.next_age,
            goal: None,
        });
        self.next_age += 1;
        self.normalize();
    }

    /// Moves every caret and the start of every range through
    /// `change` with [`Assoc::After`], and the end of every range
    /// with [`Assoc::Before`], keeps each age, then merges. Carets
    /// that typed the same text land after it, and carets whose
    /// deletions met become one. A range typed over ends after its
    /// own text, not after the text typed over a range that touches
    /// it.
    pub(crate) fn map_through(&mut self, change: &Change) {
        self.update(|selection, _| {
            let start = change.map(selection.start(), Assoc::After);
            let end = if selection.is_caret() {
                start
            } else {
                change.map(selection.end(), Assoc::Before)
            };
            if selection.anchor <= selection.head {
                Selection {
                    anchor: start,
                    head: end,
                }
            } else {
                Selection {
                    anchor: end,
                    head: start,
                }
            }
        });
    }

    /// Replaces every selection with `f(selection, goal)`. `f` reads
    /// and may set the goal x. Then merges.
    pub(crate) fn update(
        &mut self,
        mut f: impl FnMut(Selection, &mut Option<f32>) -> Selection,
    ) {
        for item in &mut self.items {
            item.selection = f(item.selection, &mut item.goal);
        }
        self.normalize();
    }

    /// Keeps only the primary.
    pub(crate) fn keep_primary(&mut self) {
        let primary = self.primary_index();
        self.items.swap(0, primary);
        self.items.truncate(1);
    }

    pub(crate) fn clear_goals(&mut self) {
        for item in &mut self.items {
            item.goal = None;
        }
    }

    /// Sorts the items and merges overlapping neighbours in one
    /// sweep.
    fn normalize(&mut self) {
        self.items.sort_by_key(|item| {
            (item.selection.start(), item.selection.end())
        });
        self.items.dedup_by(|next, kept| {
            let overlaps = kept.selection.overlaps(&next.selection);
            if overlaps {
                *kept = kept.merge(*next);
            }
            overlaps
        });
    }
}
