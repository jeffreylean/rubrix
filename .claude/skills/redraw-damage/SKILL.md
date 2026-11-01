---
name: redraw-damage
description: Model-check when an editable view's rows read their spans again, with TLA+ and TLC. Use when a change touches Damage, row_edits, splice_rows or shift_line in crates/bevy_rubrix/src/damage.rs; EditDoc::run, catch_up, settle or EditFeed::reread in editor.rs; frame::apply or EditView::splice in frame.rs; Marks::set; rows.rs `line`; or fynix's VirtualList::splice, fill, RowSlot::index or Mount. Also use when a row shows stale text or highlighting after an edit, a scroll or a marks change.
---

# Redraw damage

`Damage` (crates/bevy_rubrix/src/damage.rs) is the log that tells a row of an editable view whether to read its spans again. `RedrawDamage.tla` models it together with the row splices, fynix's slot placement and fynix's Mount, and TLC checks every order of those steps within the bounds below.

**Property.** After each Mount, every row of an editable view shows the spans its line holds now, whatever order changes, catch-ups, marks sets, frame starts, slot placements and Mounts came in.

**Harm.** When it fails, the editor shows a line's old text or highlighting at a row that should show something else, and keeps showing it until that row reads for another reason.

## Run it

```sh
.claude/skills/redraw-damage/run.sh                # damage, then rows
.claude/skills/redraw-damage/run.sh damage-deep    # one check by name
```

The script downloads tla2tools.jar v1.7.4 into `~/.cache/tla2tools/` once, checks its sha256, and runs TLC with 4 workers and `-Xmx4g`. Logs and TLC's state files go to `~/.cache/redraw-damage/<check>/`. Java 11 or newer must be on `PATH`. It prints PASS or FAIL per check, with the time and the states found, and exits nonzero when any fails.

| Check | Invariants | Bounds | Time |
| --- | --- | --- | --- |
| `damage` | `DamageSound` | 3 lines growing to 4, 3 frames, 2 notes, up to 2 line edits a change | about 12 s |
| `rows` | `ListCountIsDocRows`, `RowsFresh` | 3 lines, 2 slots, 3 frames, 2 notes, up to 2 line edits a change | about 45 s |
| `damage-deep` | `DamageSound` | as `damage`, with 3 notes | about 8 min |

Run `damage` and `rows` on every change that touches the covered code. Run `damage-deep` before you move the sync commit.

- `DamageSound` is the promise in `Damage`'s doc comment, on `Damage` alone. For every line and every generation `read` up to `seq`, a line whose spans a note changed after `read` gets `fires(line, read) == true`. It holds for the doc's damage and for the marks' damage. The `damage` checks fingerprint states by `DamageView`, which leaves out the slots and rows, because nothing that `DamageSound` reads depends on them.
- `RowsFresh` is the end-to-end property above. It needs `DamageSound`, and it also needs fynix's slot bookkeeping to tell a row that its slot moved, which F1 broke.
- `ListCountIsDocRows` says that `splice_rows` never meets a list whose count differs from the doc's rows, so its error branch never runs.

## Status at the sync commit

`damage`, `rows` and `damage-deep` pass at `278caa5`. Any `rows` failure is new.

The model was first confirmed at `24b97b0`, on the F1 fix branch. #36 and #37 then changed `editor.rs`, `frame.rs` and `change.rs`. None of those changes alters a step that the model takes, The `Change` and `CatchUp` rows of the coverage table and the list of assumptions say why.

**F1, fixed. A row could keep drawing a line that moved away from its slot.** `rows.rs` `line` learns that its slot moved only from `RowSlot::index`'s check, which is `follow` in virtual_list.rs. Before the fix, that check compared the slot's index with the one it saw at the last Mount. When the index changed and came back to the same number between two Mounts, the row saw no move. Its line number then held another line, and `Damage` had no entry for it, because that line's spans did not change. TLC found three paths.

1. A join in one frame. `crates/bevy_rubrix/tests/row_redraw.rs` replays it on the real code.
   1. A doc of three lines, `aaa`, `bbb` and `ccc`, is shown. A slot shows row 1, and its row read `bbb`.
   2. Backspace at the start of `bbb` gives `LineEdit { first: 0, removed: 2, inserted: 1 }`. `row_edits` makes it `(1, 1, 0)`, and `VirtualList::splice(1, 1, 0)` sets the slot's index to `usize::MAX`, because its row was removed.
   3. `Damage::note` records line 0, the edit's inserted line. Line 1 is now `ccc`, whose spans did not change.
   4. In the same frame, `fill` gives the free slot the first missing index, which is 1 again.
   5. At Mount, `RowSlot::index` reported no move, and `fires(1, read)` was false. The row kept drawing `bbb` where `ccc` belongs.
2. A splice, then a scroll back, in one frame. An Enter above a slot moves the slot's index from 2 to 3. A scroll in the same frame makes `fill` place the slot at 2 again, and line 2 now holds the line that was line 1. With many slots, this needs an insert of more than half the built rows above them, such as a paste.
3. A scroll away and back across a frame whose Mount does not run, for example while `Theme<T>` is absent. `fires`'s `read < since` arm does not catch it when nothing was noted between the row's read and the start of the skipped frame.

The fix gives each slot `Placed::filled_at`, the list's `fills` count when a fill last gave the slot a row. `RowSlot::index` compares the index together with `filled_at`, so every slot that a fill gives a row reads as moved, even at the index it had. A splice leaves `filled_at` alone, because a splice moves a slot together with its row. In the model, `Fill` sets `filled_at` to `fills + 1`, `SpliceSlots` keeps it, and `Checked` compares the whole slot record with the one the row saw.

Two fynix tests in `vendor/fynix/crates/bevy_fynix/src/tests/virtual_list.rs` check the slot side on a plain list. In `a_slot_freed_and_given_its_index_again_shows_the_new_row`, a removal frees a slot and fill gives it the same index, as in path 1. In `a_slot_moved_off_its_index_and_back_shows_the_new_row`, an insert at the last built row moves its slot off and fill puts it back, as in path 2 without the scroll. Path 3 needs a skipped Mount, and only the model checks it.

## Shown to fail

At `24b97b0`, each of these breaks of the code, mirrored in the model, made a check fail. The model and the code these breaks change are the same at the sync commit.

- `Damage::settle` drops the last frame's entries as well: `damage` fails with a trace of 3 states, and `rows` with one of 8.
- `Damage::note` stops moving kept entries through the edits: `damage` fails with a trace of 4 states.
- `Damage::fires` loses its `read < self.since` arm: `damage` fails with a trace of 6 states, and `rows` with one of 12. At `218d8a3`, this break was also made in damage.rs and caught by following this skill.
- `Shift::line` forgets the delta of earlier edits: `damage` fails with a trace of 4 states.
- `RowSlot::index` compares the index only, as before the F1 fix: `rows` fails in 8 s with a trace of 8 states.
- `fill` stops setting `filled_at`: `rows` fails in 9 s. This break was also made in virtual_list.rs, where the two fynix tests above fail on it.
- rows.rs `line` ignores `moved`: `rows` fails in 4 s.

## Tell a real bug from a modeling error

Read the trace in `~/.cache/redraw-damage/<check>/tlc.log`. Each state names the action that produced it. Map each action to its code with the table below, and replay the steps against the code at the current commit.

- If the code takes the same steps and ends in the same state, the bug is real. Report the trace in the code's words, as F1 does. Do not weaken the property or the model to make it pass.
- If a step cannot happen in the code, the model is wrong. Examples are a schedule order that Bevy rules out, or an edit that `Change::line_edits` never produces. Fix the model to match the code and say what you changed.
- An `Assertion failed` or a TLC evaluation error is a modeling error, or a splice past the list's count. That splice would panic in `VirtualList::splice`.

## Does a change touch the verified code?

```sh
git diff --stat 278caa5060c96ae222c50cc5435f6858943775e9 -- \
  crates/bevy_rubrix/src/damage.rs crates/bevy_rubrix/src/editor.rs \
  crates/bevy_rubrix/src/frame.rs crates/bevy_rubrix/src/marks.rs \
  crates/bevy_rubrix/src/rows.rs crates/bevy_rubrix/src/lib.rs \
  crates/rubrix_edit/src/change.rs \
  vendor/fynix/crates/bevy_fynix/src/views/virtual_list.rs \
  vendor/fynix/crates/bevy_fynix/src/mounted.rs \
  vendor/fynix/crates/bevy_fynix/src/lib.rs \
  vendor/fynix/crates/fynix/src/mounted.rs
```

If the diff touches a function in the table, update the matching operator so that it does what the code does now, then run both checks. Treat a change to a system's schedule, set or run condition in `lib.rs` or fynix's `lib.rs` as a change to the phases.

## Coverage

Sync commit: `278caa5060c96ae222c50cc5435f6858943775e9`. At this commit the model was confirmed to describe the code below. Move the sync commit forward only after you confirm that the model describes the code at the new commit, and record the new status.

| Model | Code |
| --- | --- |
| `ShiftLine`, `ShiftRanges` | `Shift::line`, `Shift::range`, damage.rs |
| `Merged`, `SortByStart` | `merged`, and `sort_unstable_by_key` in `Damage::note` |
| `Note`, `Settle`, `Fires` | `Damage::note`, `Damage::settle`, `Damage::fires` |
| `RowSplices`, `SpliceRows` | `row_edits`, `row_splices`, `splice_rows`, damage.rs |
| `SpliceSlots` | `VirtualList::splice`, its loop over `self.slots`, which leaves `Placed::filled_at` as it is (vendor/fynix/crates/bevy_fynix/src/views/virtual_list.rs) |
| `EditsOn`, `Changes` | the contract on `Change::line_edits` (crates/rubrix_edit/src/change.rs): at least one line removed and one inserted, ascending, `first + inserted <= next.first` |
| `Change` | `Tx::transact` (frame.rs): `EditDoc::run`, which lexes and calls `Damage::note`, then `EditView::splice`, which calls `splice_rows`. `Tx::lay_out` changes only the layout that the commands read, so it changes which edit is made, not the steps |
| `CatchUp` | `EditDoc::catch_up`, from the end of `frame::apply` and from `diff_edit::catch_up`. `frame::catch_up_window` sets the last line to lex, and the model leaves it free, because a lex may return any set of lines |
| `SetMarks` | `Marks::set` (marks.rs), from `match_brackets`, `draw_markers`, `find::follow`, ime and app code, in any schedule |
| `StartFrame` | the settle loop at the top of `frame::apply`, PreUpdate in `EditSystems::Apply` |
| `Fill` | `read_sources` then `place_rows`, which calls `VirtualList::place` and `fill`, in fynix's Update `Prepare` set. `fill` adds one to `fills` and gives each slot it places that count as `filled_at` |
| `Mount`, `Built`, `Checked` | fynix's `mounted::update`, an exclusive system in Update's `Mount` set: `update_structure` builds and drops the `each` rows keyed by slot, `update_elements` polls every row. Each row's `line` signal in rows.rs checks `RowSlot::index` (`follow` on the slot's index and `filled_at`) and `EditFeed::reread` (editor.rs) |
| `SkipMount` | a frame whose Mount does not run, for example while `Theme<T>` is absent |
| `Content` | `EditFeed::spans`: `EditDoc::line` and `runs`, and `Marks::on` by line number |

Each action is one indivisible step because the code makes it one. `frame::apply` and fynix's Mount are exclusive systems, and `Marks::set` takes `&mut Marks`, so no note can land inside a Mount or inside a transaction. A row's read and its stamp happen inside the same Mount.

## Assumptions and what it leaves out

- The ground truth for a line's spans is its text, the runs `Highlights` stores for it, and the marks on its line number. The model trusts `edit_many_lines` and `catch_up` to return every line whose stored runs changed, and lets a lex return any set of lines. Whether those runs are the right highlighting is not checked.
- The model has one view and one list. Heights, scrolling and the `MAX_SLOTS` cap appear only as a free choice of the window's first row and size at each `fill`.
- Edits come from the `Change::line_edits` contract. `line_edits` reads only the change's replacements, so the moves that a line move carries do not change the edits. `Change::map`, `Change::map_range` and `Ranges::map` are left out, because they move offsets and marker ranges, and those reach the rows only through `Marks::set`, which `SetMarks` models with any set of lines.
- Diff rows are not modeled. `diff_edit::fires` gates its `Damage::fires` call on `Session.reads`, and its rows map through `rubrix_diff` rows and their gap expansion. The find scan's pause and resume are left out because seeded differential tests cover them.
- `Marks` and `EditDoc` are never replaced on their entities. A new `Damage` restarts `seq` at 0, and that case is not checked. `EditDoc::restored`, which `restore_state` in hooks.rs calls when an `EditDoc` is added, also starts a new `Damage`. The revision it carries over is the `EditorChanged` count, not `Damage`'s `seq`.
- The bounds are small: at most 4 lines, 2 slots, 3 frames and 3 notes. A pass covers those bounds only. The `rows` check stops at 3 lines, because at 4 lines a run of an earlier scratch fix had not finished after 5 minutes.
- `fills` is a `u64` in the code and is never near its limit, so the model does not wrap it.
- A count per slot, which starts again when `fill` drops a slot and builds it again, also passes `rows` in these bounds. The code keeps one count for the list because that count never repeats a value a row saw, not because the check tells the two apart.
- The gutter's line number in rows.rs reads the same `RowSlot::index` check, and the model leaves it out.
