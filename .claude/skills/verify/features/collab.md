# Collab page

The collab page shows `bench/fixtures/rust/large.rs.txt` in `code_editor`, as the editor page does, with the hooks an app hangs on the doc: other users' carets (`RemoteCarets`), diagnostics (`Markers`), per-line payloads (`LineAnnotations<T>`), edit state kept under a key across a respawn (`EditKey`, `EditStates`), and a clipboard provider of the page's own. E6 ships it.

## Sub-features

- `collab-caret` draws a remote caret as a 2 px bar in its colour with its label above it, and a ranged one also tints its range at 32% of the colour. Users cannot click or move one.
- `collab-remap` moves each remote caret through every change, undo and redo included. Text typed exactly at a caret pushes it right.
- `collab-edge` shows a remote caret outside the viewport as a marker at the view's top or bottom edge, `↑ <label>` or `↓ <label>`.
- `collab-changed` writes one `EditorChanged` per change, undo and redo included, each with the doc's revision one past the last.
- `collab-markers` underlines each marker in its severity's colour in the rows near the viewport. A marker follows edits, and a change that deletes its whole range drops it.
- `collab-annotations` keeps a payload on its line's text through edits, drops it with a deleted line, and brings it back on the undo. The page shows each as a purple gutter number.
- `collab-respawn` despawns the doc and spawns one with the same key from the original text. The key's saved state wins, so text, selections and undo come back.
- `collab-clipboard` serves copy, cut and paste through the page's own `ClipboardProvider`, which keeps its text in the gallery's paste slot tagged `collab: `. The shared Wayland clipboard is never written.
- `collab-100k` types at line 50,000 of the 100,000-line input with 50 remote carets and 1,000 markers, inside the 3.0 ms keystroke budget.

## How to get to it (user POV)

- Click `collab` in the page list. It is the last entry.
- The buttons above the view: `100k lines` swaps in the 100,000-line input (and then reads `large.rs` to swap back), `markers` puts an error, a warning and an info marker on the text of lines 130 to 132, `annotate` puts a `review` badge on line 200, `50 carets` puts 50 remote carets from the primary caret to the end of its line, `respawn` despawns and respawns the doc, and `1,000 markers` spreads 1,000 markers over the file.
- Remote carets come only from a drive script's `remote` step: the page has no user to stand for.

## Driving it with launch.sh

Preconditions: `doctor.sh` is clean and the out dir is empty. `gen_drives.py` writes every `collab-*.drive` in its `e6` block. Change a script there, not by hand.

The page lays out as the editor page does: the buttons sit at y 68: `100k lines` at x 290, `markers` at 438, `annotate` at 586, `50 carets` at 734, `respawn` at 882, and the wider `1,000 markers` at 1041; line `n` shows at y `126 + (n - t) * 17` with line `t` at the top, and the text starts at x 271, or 279.4 under the 100,000-line input's six-digit gutter. The page answers `caret`, `text <line>` and `undo` as the editor page does.

- `remote <name> <line>:<col> [<line>:<col>]` sets the caret labelled `<name>`: a caret at the first place, or a range from the first (anchor) to the second (head). Its colour comes from a hash of the name, the same each run. A second `remote` with the same name replaces that caret.
- `log remote` prints `remote <n>: <caret>, ... | over gutter <k> | edge labels <e> overlapping <o>`, each caret `<label> <place> shown` for a bar or `<label> edge <top|bottom> at <place>` for an edge marker, where `<place>` is `<line>:<col>` or `<anchor>..<head>`. A caret drawn by no view yet prints `unplaced`. `<k>` counts the remote bars, tints and labels on screen whose left edge is left of the line numbers' right edge, `<e>` the edge labels on screen (`↑ `, `↓ ` or `+<n>`), and `<o>` the pairs of those that overlap.
- `log changed` prints `changed <n> rising <bool> last <seq>`: the `EditorChanged` messages for the doc on show since it was spawned, whether each revision was one past the one before, and the last revision.
- `log markers` prints `markers <n>: <severity> <from>..<to>, ...`, the first ten by start.
- `log annotations` prints `annotations <n>: <line> <badge>, ...`.
- `log paste` prints the page's clipboard text, `collab: ` first once the page's provider holds or served it.

The recipes:

- **Regression (lane 1).** Run `editor-acceptance.drive` at trunk and at head, and `collab-caret.drive` at head. Pass when trunk and head print the same `drive: <n> of <n> checks pass` line with every check passing and `collab-caret.drive` passes. Save `e6-regress.png`.
- **Caret (lane 2).** Run `collab-caret.drive`. It puts line 115 at the top and sets `remote Ada 120:5` and `remote Bo 123:3 123:12` (the plan says line 122, which is empty in `large.rs.txt`). `log remote` lists `Ada 120:5 shown` and `Bo 123:3..123:12 shown`, and `e6-caret.png` shows two labelled bars with Bo's range tinted. It then sets `remote Ada 120:5 120:12` (`e6-tint.png`, seven bytes tinted) and types `ab` at `120:1`: `Ada 120:7..120:14 shown`, and `e6-tint-pushed.png` shows the bar and tint two columns right.
- **Remap (lane 3).** Run `collab-remap.drive`. It sets `remote Ada 120:5`, types `xy` at `120:1`, `z` at `120:7`, presses Enter at `100:1`, and undoes all three. `log remote` reads `Ada 120:7`, `Ada 120:8`, `Ada 121:8`, then `Ada 120:5`. Save `e6-remap.png`.
- **Edge (lane 4).** Run `collab-edge.drive`. It sets `remote Ada 9000:1` with the view at the top (`Ada edge bottom at 9000:1`, `e6-edge-bottom.png`), puts line 8,990 at the top (`Ada 9000:1 shown`), then line 9,020 (`Ada edge top at 9000:1`). Save `e6-edge.png`.
- **Changed (lane 5).** Run `collab-changed.drive`. It types three characters, presses Enter, and undoes once. `expect changed changed 5 rising true` passes. Save `e6-changed.png`.
- **Markers (lane 6).** Run `collab-markers.drive`. It clicks `markers`, types `ab` at `130:1`, selects the second marker's range with `shift+end` and deletes it. `log markers` shows the first moved two columns right, the second gone and the third unchanged, and `e6-markers.png` shows two underlines.
- **Annotations (lane 7).** Run `collab-annotations.drive`. It clicks `annotate`, presses Enter at the end of line 150, and undoes. `log annotations` reads `201 review` after the Enter and `200 review` after the undo, and `e6-annotations.png` shows the purple number 200.
- **Respawn (lane 8).** Run `collab-respawn.drive`. It types `abc` at the end of line 120, presses Enter, clicks `respawn`, clicks into the new view, and presses `mod+z` twice. `log text 120`, `log text 121` and `log undo` match before and after the respawn, and the two undos restore the original lines with `undo 0 redo 2`. Save `e6-respawn.png`.
- **Clipboard (lane 9).** Run `wl-paste -n` before and after, read only. Run `collab-clipboard.drive`. It copies line 120 with `shift+end` and `mod+c`, then runs `paste one\ntwo` at the end of line 125. `log copied` prints the line, `log paste` prints it tagged `collab: `, `log text 125` and `log text 126` show `one` and `two`, and the last `log paste` prints `collab: one\ntwo`. The two `wl-paste` outputs match. Save `e6-clipboard.png`.
- **100k (lane 10).** Run `collab-100k.drive`. It opens the 100,000-line input, clicks `50 carets` and `1,000 markers` with the caret at `50000:9`, and types 200 `x` inside `frames start` and `frames stop collab-100k-type.csv`. `log remote` lists 50 carets, each 200 columns right of where it started. Save `e6-100k.png`.

The milestone 3 fixes, written by the last block of `gen_drives.py`, each failing at 218d8a3:

- `collab-m3-move-annotation.drive` moves line 201 up onto the annotated empty line 200, undoes and redoes it, then moves line 200 down: the badge follows line 200's text to 201 each time and comes back to 200 on each undo.
- `collab-m3-move-marker.drive` clicks `markers`, moves line 132 up, undoes, moves line 130 down, undoes: all three markers move with their lines and come back on each undo.
- `collab-m3-move-remote.drive` sets `remote W 119:12` and `remote P 120:12` and moves line 120 up: the two carets swap lines with their text, undo and redo swap them back and forth, and a second move takes P on to 118:12 and leaves W.
- `collab-m3-multibyte.drive` pastes ten `李` at the end of line 120 and clicks `50 carets` from column 1: the gallery does not panic, every caret sits on a character boundary, and typing `ab` then undoing moves all 50 right by two and back.
- `collab-m3-paste-tag.drive` runs `paste collab: hi`: the text arrives whole, `.run();collab: hi`, and `log paste` shows it tagged once more.
- `collab-m3-revision.drive` types `abc` and Enter, clicks `respawn`, and undoes: the undo's `EditorChanged` carries revision 5, one past the last before the respawn.
- `collab-m3-gutter.drive` pastes a 200-character run on line 120, scrolls 30 px sideways, and sets carets at 120:1, 121:1 and a range from 122:1 to 120:3: `over gutter 0`, nothing remote paints over the line numbers (`m3fa-gutter.png`).
- `collab-m3-edge-labels.drive` opens the 100,000-line input, clicks `50 carets` on line 50,000, and scrolls 100 lines past it each way: every caret is at the edge and `overlapping 0` at both edges, with the labels that do not fit folded into one `+N` (`m3fa-edge-top.png`, `m3fa-edge-bottom.png`).

## Perf

Run `collab-100k.drive` at head and `editor-100k.drive` at trunk and at head, with `RUBRIX_NO_VSYNC=1` in `--release`, interleaved, five runs each, alone, with the load under 4. Read each CSV with `frame-stats.sh`. The `collab-100k-type.csv` p95 is at most 3.0 ms, and the `editor-100k-type.csv` p95 at head is at most 10% worse than trunk.
