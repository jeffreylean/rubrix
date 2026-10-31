# Diff page

The diff page shows a diff in `diff_view`: a header row per file, then its rows, all in one virtual list. R15 ships stacked rows and R16 split rows. It opens on `bench/patch/large.patch.txt`, 11,095 lines in 93 files.

## Sub-features

- `diff-render` shows each file's header with its name and `+N -M` counts, two line-number columns, and removed and added rows tinted red and green.
- `diff-words` marks the words that changed between a removed line and the added line it pairs with in a stronger tint.
- `diff-headers` shows a rename as `old → new`, a mode change as `mode 100644 → 100755`, and a `binary` note, with no hunks for a mode-only or binary file.
- `diff-collapse` hides a file's rows when its header is clicked, and shows them on a second click.
- `diff-expand` reveals 100 lines of a gap between hunks of a text pair with `↑ up` or `↓ down`, or the whole gap with `↕ all`. A patch's gaps show no buttons.
- `diff-compact` draws stacked rows with no gutter, a `+ `, `- `, or two-space prefix, and wrapped lines, and shows 16 line rows and a `... M more lines` button.
- `diff-split` shows old lines on the left and new lines on the right, with a hatched cell for a side that has no line.
- `diff-switch` keeps the first visible source line at the top when the layout changes.
- `diff-copy` selects as the code view does, and `Ctrl+C` copies the selected lines joined by newlines. A compact line keeps its prefix, and header and gap rows add nothing. In split rows a selection stays on the side its press landed on.
- `diff-stress` shows `bench/patch/stress.patch.txt`, 100,522 lines in 726 files.

## How to get to it (user POV)

- Click `diff` in the page list.
- Click `large.patch`, `100k patch`, or `pair` to show that diff. `pair` is `large.rs.txt` against a copy with a few edits. The label under the buttons names the diff and its file count.
- Click `stacked` to switch to split rows, and `split` to switch back. The button names the layout on show.
- Click `wrap off` to wrap long lines, and `compact off` to turn compact on.
- Click a file header to collapse it. Click a gap row's buttons to reveal its lines.

## Driving it with launch.sh

Preconditions: `doctor.sh` is clean and the out dir is empty. The scripts are in `crates/bevy_rubrix/examples/gallery/drive/`, and `gen_drives.py` writes every `diff-*.drive`. The files they open are in `examples/gallery/fixtures/diff/`.

R15 lanes:

- **Patch.** Run `.../diff-patch.drive`. `log diff` prints `diff files 93 rows 10633 stacked first 0 header 0 Cargo.lock`. `r15-patch.png` shows `Cargo.lock +2 -2`, which matches `git apply --numstat bench/patch/large.patch.txt` for its first file.
- **Last file.** Run `.../diff-last-file.drive`. `expect diff first 10308 header 92 crates/bevy_fynix/src/views/menu.rs` passes, and `r15-last-file.png` shows that header at the top.
- **Expand up.** Run `.../diff-expand.drive`. It opens `gap-old.txt` and `gap-new.txt`, whose second hunk sits 300 lines below the first, and clicks `↑ up`. `expect diff rows 116` passes, 100 rows more than the 16 before. `r15-expand.png` shows `197 hidden lines` and lines 202 to 308 below it, numbered without a break.
- **Expand all.** Run `.../diff-expand-all.drive`. `expect diff rows 312` passes, and `r15-expand-all.png` shows no separator between lines 4 and 5.
- **Collapse.** Run `.../diff-collapse.drive`. `log diff` prints `rows 10621`, 12 fewer, and `r15-collapse.png` shows `▸ Cargo.lock` with the `Cargo.toml` header right under it.
- **Words.** Run `.../diff-words.drive`. `r15-words.png` shows only `sum` and `subtotal` in the stronger tint.
- **Headers.** Run `.../diff-headers.drive`. `r15-headers.png` shows `src/old_name.rs → src/new_name.rs`, `mode 100644 → 100755`, and `binary`, with rows only under `src/lib.rs`.
- **Compact.** Run `.../diff-compact.drive`. `expect diff rows 18 compact` passes, a header, 16 rows, and the button. `r15-compact.png` shows `- ` prefixes and `... 24 more lines`. After the click, `expect diff rows 41 compact` passes.
- **Copy.** Run `.../diff-copy-compact.drive`. It opens `forty-old.txt` and `forty-new.txt`, turns on compact, and drags from the start of the first line to past the end of the third. `clipboard` prints `- old line 1`, `- old line 2` and `- old line 3` on three lines, and `r15-copy.png` shows the three rows selected.
- **100,000 lines.** Run `.../diff-stress.drive`. `log rows` prints `rows 52`, below 200, near row 50,000, and `r15-stress.png` shows rows with no blank band.

R16 lanes:

- **Split.** Run `.../diff-split.drive`. `expect diff files 93 rows 9829 split` passes, and `r16-split.png` shows old lines left and new lines right.
- **Realign.** Run `.../diff-realign.drive`. `r16-realign.png` shows `let total = prices.iter().copied()...` beside the second removed line, and hatched new cells beside the first and third.
- **Added file.** Run `.../diff-added.drive`. `r16-added.png` shows every old cell hatched and the new gutter counting from 1.
- **Last file.** Run `.../diff-split-last.drive`. `expect diff first 9520 header 92 crates/bevy_fynix/src/views/menu.rs` passes.
- **Sideways.** Run `.../diff-hscroll.drive`. `log rows` ends with `x 400`, and `r16-hscroll.png` shows both sides moved by the same amount with both gutters in place.
- **Wrap.** Run `.../diff-wrap.drive`. `r16-wrap.png` shows a row as tall as the wrapped new line, and `}` below it on both sides.
- **Switch.** Run `.../diff-switch.drive`. Both `log diff` lines name the same line, as `line 32 old 1478 new 1676`.
- **Copy one side.** Run `.../diff-copy-side.drive`. It opens `realign-old.rs` and `realign-new.rs` in split rows and drags from the old gutter of line 2 to the new side of the fourth row. `clipboard` prints old lines 2 to 4, from `    let started` to `started.elapsed());`, and no new line. `r16-copy-side.png` shows the selection on the left side only.
- **Expand in split.** Run `.../diff-split-expand.drive`. `log diff` prints `rows 114`, and `r16-expand.png` shows the revealed lines on both sides with matching numbers.
- **100,000 lines split.** Run `.../diff-split-stress.drive`. `log rows` prints `rows 52`, below 200, and `r16-stress.png` shows rows with no blank band.

## Perf

Run each script with `RUBRIX_NO_VSYNC=1` in `--release`, alone, five times, then read the CSVs in the out dir.

- First paint is the largest frame among frames 68 to 80 of `diff-first-paint.csv` from `diff-patch.drive`, and of `diff-split-first-paint.csv` from `diff-split.drive`. The page opens at frame 70.
- Scroll frame time is `frame-stats.sh` on `diff-scroll.csv`, `diff-scroll-100k.csv` from `diff-stress.drive`, and `diff-split-scroll.csv`. Each scrolls 18 px a frame for 600 frames, after a 60-frame wait that `frame-stats.sh` drops.

## Review video

Run `.../diff-review-record.drive` for R15 and `.../diff-split-review-record.drive` for R16, then `video.sh <out>/review <out>/review.mp4 30`.

## Gotchas

- A line row is 17 px, a header row 33 px, and a gap row with buttons 33 px. A patch's gap row has no buttons and is 17 px.
- The buttons sit at y 68, at x 290, 438, 586, 734, 882, and 1030. A click leaves the pointer there, so move it back over the view before a scroll.
- Opening a diff or pressing a diff button builds a new list at the first row, with every file expanded and every gap collapsed.
- Pressing `100k patch` parses and highlights 100,522 lines in one frame.
- Any change to the rows, such as a collapse, a reveal, a layout or compact switch, or a new diff, drops the selection.

## Editing the new side (E7)

With editing on, a press on a new-side line puts a caret there, and keys edit that file's new side. Each file of a text pair has its own doc and undo history, and each keystroke re-diffs only the hunks within reach. Old-side lines stay read-only, selectable and copyable, and so does every line of a patch and every line in compact rows. A press on an old-side line drops the caret, and `log diff-edit` prints `diff-edit none` again.

Controls:

- `edit off` is the seventh button in the control row, at 1178 68. A press flips it to `edit on`. Edits stay shown when editing turns off.
- `pairs` and `100k pair` sit in the title row, right of `Diff`, at 304 32 and 380 32. The control row fits seven buttons in one line, and an eighth would wrap it and move the view under every read-only diff script, so these two are tinted text buttons and the title row keeps its height.
- `pairs` shows nine files: `large.rs` with the `pair` edits, then the generated `merge.rs` and `retype.rs`, then `forty.txt`, `gap.txt`, `long.rs`, `realign.rs`, `words.rs` and `wrap.rs`, each from its `-old` and `-new` fixture.
- `100k pair` shows `large.rs.txt` ten times, 103,720 lines, against the same with ` // edit` after every line whose 0-based index is a multiple of 100: 1,038 hunks in 9,342 rows. It matches `hundred_k_pair()` in `crates/rubrix_diff/tests/rediff.rs`.

`log diff-edit` prints `diff-edit file <n> hunks <count> undo <depth> rediff <us> marks <ranges> line <l> hunk <header> shown <rows> stale <rows>` for the first diff view, or `diff-edit none` while no new-side press has placed a caret. The fields up to `hunk` come from `DiffDoc::edit_status`: `file` counts from 0, `rediff` is the last re-diff of that file in whole microseconds or `-`, `marks` is the caret row's new-side word ranges as `a..b,c..d` or `-`, `line` counts from 1, and `hunk` is the `@@` header of the hunk that holds the caret's line, or `-`. `shown` counts the stacked line rows the list has built, and `stale` those whose gutter shows numbers other than their row's, so `expect diff-edit stale 0` fails on a row the list did not read again after an edit. Split rows are not counted.

`gen_drives.py` writes the `diff-edit-*.drive` scripts and the `merge` and `retype` fixtures from its E7 block. Its `Pair` class models each file's stacked or split rows from its changes, and `reach` scrolls from the top to a row by pixels and checks the row on top with `expect diff first`. In `pair`, line 120 is `        .run();`, which holds no `theme`, so the pair deletes old lines 2,001 to 2,006, adds two lines before old line 5,001, and changes old line 8,001 (new 7,997). The lanes edit new line 7,997 as the changed line.

Lanes, each a `diff-edit-<name>.drive` that ends `drive: <n> of <n> checks pass`:

- **Regression (lane 1).** Run `diff-stress.drive`, `diff-split.drive` and `diff-words.drive` at trunk and at head, and `diff-edit-type.drive` at head, which saves `e7-regress.png`. Pass when every `log diff` and `log rows` line matches between trunk and head and `diff-edit-type.drive` passes.
- **Type (lane 2).** `diff-edit-type.drive` shows `pair`, turns editing on, clicks the end of new line 7,997, and types ` x`. `log diff-edit` prints `hunks 3` and `line 7997 hunk @@ -7998,7 +7994,7 @@` before and after, and `marks` grows from `49..60` to `49..62`. `expect diff files 1 rows 33 stacked` passes before and after, so the row stays in its hunk. `e7-type.png` shows ` x` on the added line.
- **Unchanged line (lane 3).** `diff-edit-unchanged.drive` clicks `↕ all` on the 2,988-line gap, scrolls to new line 3,000, and types `y` at its end. `hunks 3` becomes `hunks 4`, and the line's `hunk -` becomes `hunk @@ -3003,7 +2997,7 @@`, a hunk inside the old gap with the gap's revealed lines on both sides. The rows grow from 3,020 to 3,021. Save `e7-unchanged.png`.
- **Revert (lane 4).** `diff-edit-revert.drive` deletes the ` // changed` on new line 7,997 with 11 Backspaces. `hunks 3` becomes `hunks 2`, the line prints `marks - line 7997 hunk -`, and the rows drop from 33 to 32 as the removed and added rows become one context row. `stale 0` checks that the rows below show their own numbers. Save `e7-revert.png`.
- **Merge (lane 5).** `diff-edit-merge.drive` shows `pairs`, scrolls to `merge.rs`, whose changes on lines 5 and 14 sit 8 unchanged lines apart, and types ` // edited` at the end of context line 8. `file 1 hunks 2` becomes `hunks 1`, and line 8's hunk goes from `@@ -2,7 +2,7 @@` to `@@ -2,16 +2,16 @@`, one header over both changes. Save `e7-merge.png`.
- **Split (lane 6).** `diff-edit-split.drive` switches `pair` to split rows, clicks the new side of the 8,001 / 7,997 row, presses End, and types ` x`. Shift+Home and `mod+c` copy the new line with ` x`. A click on the old side and `type z` change nothing: a drag over the old line with `mod+c` copies it as it was, and the new line copies again without a `z`. Save `e7-split.png`.
- **Enter (lane 7).** `diff-edit-enter.drive` shows `pairs`, clicks the end of the added new line 4,996, and presses Enter 20 times. The rows grow from 142 to 162. With the view scrolled from the top by the model's pixels, `expect diff first 40 line 0 old 5001 new 5017` passes: the old number stays and the new one runs on by 20. `expect rows first_visible 40` checks that the rows above add up to those pixels, with no gap and no overlap. Save `e7-enter.png`.
- **Undo (lane 8).** `diff-edit-undo.drive` shows `pairs`, types ` one` on new line 7,997 of `large.rs` and ` two` on line 5 of `merge.rs`, then presses `mod+z` with the caret in `merge.rs`. `merge.rs` prints `undo 0` and copies its line as it was, and `large.rs` still prints `undo 1` and copies its line with ` one`. Save `e7-undo.png`.
- **Words (lane 9).** `diff-edit-words.drive` shows `pairs`, clicks the end of `retype.rs`'s added line `    cost * count + tot`, whose old line ends `total`, and types `a` then `l`. `marks` goes from `4..8,19..22` to `4..8,19..23` to `4..8`. Save `e7-words.png`.
- **100,000 lines (lane 10).** `diff-edit-100k.drive` shows `100k pair`, turns editing on, scrolls to new line 50,000, and types 200 `x` inside `frames start` and `frames stop diff-edit-100k.csv`. The count stays at `hunks 1038`, and the line's hunk goes from `@@ -49998,7 +49998,7 @@` to `@@ -49997,8 +49997,8 @@`. Pass when `log diff-edit` prints a `rediff` under 100 µs, which a reader checks, since `expect` matches text, and the CSV meets the perf rule.

Perf probe: `diff-edit-pairs.drive` shows `pairs`, clicks the end of added line 4,996, and types 100 `x` inside `frames start` and `frames stop diff-edit-pairs.csv`. Run it and `diff-edit-100k.drive` with `RUBRIX_NO_VSYNC=1` in `--release`, and read each CSV with `frame-stats.sh`, which drops the 60-frame wait.

Gotchas:

- A gap of one hidden line shows that line as a context row, not a gap row.
- The list places a row it has not built at 17 px, so a scroll far down lands by that estimate, and only the rows the list has built take their real heights. `reach` models it with the 51 rows the list builds on show.
- Where a line that ` // edit` changes repeats its neighbour, the diff matches the old line to the neighbour, which adds a context row between the removed and the added line. `100k pair` has two such lines, at 0-based indexes 47,000 and 71,700.
- `type` drops leading spaces, so the lanes send `key space` first. A click at x 1250 can land inside a long split-row line, so the lanes press End after each click.
