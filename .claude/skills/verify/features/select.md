# Selection in the code view

Every `code_view`, on the `code` page and the `stream` page, selects and copies text. R14 ships it. A selection is two byte ranges into the view's text, so rows that scroll away keep it.

## Sub-features

- `select-drag` selects from the press to the pointer, and scrolls while the pointer is held past the view's top or bottom edge.
- `select-word` takes a run of letters, digits and `_`, a run of spaces, or one other char on a double press.
- `select-line` takes the line, without its line ending, on a triple press.
- `select-extend` moves the selection's end to a shift press.
- `select-all` takes the whole text on `Ctrl+A`, rows built or not.
- `select-copy` puts the selected source bytes on the clipboard on `Ctrl+C`. Blank lines and CRLF stay, and gutter numbers never appear.
- `select-draw` draws a rectangle behind the text of each visual line selected.

## How to get to it (user POV)

- Press, double press, triple press, or drag over the text or the gutter. A press on the gutter lands at the line's start.
- Hold shift and press to extend.
- `Ctrl+A` and `Ctrl+C` act on the view pressed last, or on the only one.

## Driving it with launch.sh

The scripts are `drive/select-*.drive`, written by `gen_drives.py`. Each opens the `code` page on `large.rs.txt` and waits 120 frames. Line `n` of a view at its top shows at y `126 + (n - 1) * 17`, its text starts at x 271, and the gutter is at x 235. `clipboard` prints what the view copied.

- **Drag (lane 1).** `select-drag.drive` drags from line 10 to the end of line 20 and copies. `log selection` prints `range 200..484`, lines 10 to 20 of the file.
- **Auto-scroll (lane 2).** `select-autoscroll.drive` holds the button 19 px below the view for 321 frames, which scrolls 19 px a frame, then moves to the end of line 400 and releases. `log selection` prints `range 200..11911`, lines 10 to 400.
- **Blank line (lane 3).** `select-blank.drive` copies lines 1 to 3. The clipboard keeps the empty line 2.
- **Word (lane 4).** `select-word.drive` double-presses `Duration` on line 3. `expect selection length 8` passes, and `log selection` prints `range 51..59`.
- **Line (lane 5).** `select-line.drive` triple-presses line 6 and copies `use bevy_fynix::tokens::{`.
- **All (lane 6).** `select-all.drive`. `expect selection length 313933` passes, the size of `large.rs.txt`.
- **Survive (lane 7).** `select-survive.drive` selects lines 10 to 20, scrolls 5,000 lines down and back. Both `log selection` lines print `range 200..484`.
- **Extend (lane 8).** `select-extend.drive` presses line 10 and shift-presses the end of line 20. The copy is lines 10 to 20.
- **Gutter and wrap (lane 9).** `select-gutter-wrap.drive` opens `fixtures/long-line.rs`, switches to `wrap on`, and drags from the gutter of line 1 to the end. The copy is the file without its final newline, with no line numbers, and line 3 in one piece.
- **Unicode (lane 10).** `select-unicode.drive` opens `fixtures/unicode.rs` and copies lines 3 and 4, with CJK, an emoji and tabs. `log selection` prints `range 56..148`.

## Log topic

`log selection` prints `selection anchor <a>..<b> focus <c>..<d> range <s>..<e> length <n>` for the first code view, or `selection none`. The anchor is what the first press covered, the focus what the last press or drag covered, both in bytes of the view's text, and the range runs from one to the other.

## Perf

Run `select-autoscroll.drive` and `select-all.drive` with `RUBRIX_NO_VSYNC=1` in `--release` with `frames start` and `frames stop` added around the held drag and around `key ctrl+c`, then `frame-stats.sh`.

## Gotchas

- The rectangles are placed after layout, so they show one frame after the text they cover moves.
- A selected empty line draws no rectangle.
- A double press after a single press at the same place within 500 ms counts as a triple press. Bevy counts presses per entity.
