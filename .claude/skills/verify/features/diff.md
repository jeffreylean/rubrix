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
- `diff-stress` shows `bench/patch/stress.patch.txt`, 100,522 lines in 726 files.

## How to get to it (user POV)

- Click `diff` in the page list.
- Click `large.patch`, `100k patch`, or `pair` to show that diff. `pair` is `large.rs.txt` against a copy with a few edits. The label under the buttons names the diff and its file count.
- Click `stacked` to switch to split rows, and `split` to switch back. The button names the layout on show.
- Click `wrap off` to wrap long lines, and `compact off` to turn compact on.
- Click a file header to collapse it. Click a gap row's buttons to reveal its lines.

## Driving it with launch.sh

Preconditions: `doctor.sh` is clean and the out dir is empty. The scripts are in `crates/bevy_rubrix/examples/gallery/drive/`, and `gen_diff_drives.py` writes every `diff-*.drive`. The files they open are in `examples/gallery/fixtures/diff/`.

R15 lanes:

- **Patch.** Run `.../diff-patch.drive`. `log diff` prints `diff files 93 rows 10633 stacked first 0 header 0 Cargo.lock`. `r15-patch.png` shows `Cargo.lock +2 -2`, which matches `git apply --numstat bench/patch/large.patch.txt` for its first file.
- **Last file.** Run `.../diff-last-file.drive`. `expect diff first 10308 header 92 crates/bevy_fynix/src/views/menu.rs` passes, and `r15-last-file.png` shows that header at the top.
- **Expand up.** Run `.../diff-expand.drive`. It opens `gap-old.txt` and `gap-new.txt`, whose second hunk sits 300 lines below the first, and clicks `↑ up`. `expect diff rows 116` passes, 100 rows more than the 16 before. `r15-expand.png` shows `197 hidden lines` and lines 202 to 308 below it, numbered without a break.
- **Expand all.** Run `.../diff-expand-all.drive`. `expect diff rows 312` passes, and `r15-expand-all.png` shows no separator between lines 4 and 5.
- **Collapse.** Run `.../diff-collapse.drive`. `log diff` prints `rows 10621`, 12 fewer, and `r15-collapse.png` shows `▸ Cargo.lock` with the `Cargo.toml` header right under it.
- **Words.** Run `.../diff-words.drive`. `r15-words.png` shows only `sum` and `subtotal` in the stronger tint.
- **Headers.** Run `.../diff-headers.drive`. `r15-headers.png` shows `src/old_name.rs → src/new_name.rs`, `mode 100644 → 100755`, and `binary`, with rows only under `src/lib.rs`.
- **Compact.** Run `.../diff-compact.drive`. `expect diff rows 18 compact` passes, a header, 16 rows, and the button. `r15-compact.png` shows `- ` prefixes and `... 24 more lines`. After the click, `expect diff rows 41 compact` passes.
- **Copy.** Not built yet. It needs R14's selection.
- **100,000 lines.** Run `.../diff-stress.drive`. `log rows` prints `rows 52`, below 200, near row 50,000, and `r15-stress.png` shows rows with no blank band.

R16 lanes:

- **Split.** Run `.../diff-split.drive`. `expect diff files 93 rows 9829 split` passes, and `r16-split.png` shows old lines left and new lines right.
- **Realign.** Run `.../diff-realign.drive`. `r16-realign.png` shows `let total = prices.iter().copied()...` beside the second removed line, and hatched new cells beside the first and third.
- **Added file.** Run `.../diff-added.drive`. `r16-added.png` shows every old cell hatched and the new gutter counting from 1.
- **Last file.** Run `.../diff-split-last.drive`. `expect diff first 9520 header 92 crates/bevy_fynix/src/views/menu.rs` passes.
- **Sideways.** Run `.../diff-hscroll.drive`. `log rows` ends with `x 400`, and `r16-hscroll.png` shows both sides moved by the same amount with both gutters in place.
- **Wrap.** Run `.../diff-wrap.drive`. `r16-wrap.png` shows a row as tall as the wrapped new line, and `}` below it on both sides.
- **Switch.** Run `.../diff-switch.drive`. Both `log diff` lines name the same line, as `line 32 old 1478 new 1676`.
- **Copy one side.** Not built yet. It needs R14's selection.
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
- The copy lanes wait for R14's selection.
