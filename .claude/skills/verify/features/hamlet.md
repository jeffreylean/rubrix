# Hamlet pages and the stress page

Three pages rebuild Hamlet's chat blocks from rubrix's views, and a fourth loads both 100,000-line inputs. R17 ships them, with `drive/acceptance.drive` and `scripts/acceptance.sh`. The Hamlet functions named below are at Hamlet 3b8cc93a.

## Sub-features

- `hamlet-rich` draws Hamlet's markdown sample (`rich`, `crates/hamlet_ui/src/markdown/draw.rs:197`). `# Plan` is a 20 px bold heading. The paragraph ``Use **care** with `rm`, ~~old~~ and [docs](http://x).`` is 14 px sans, with `care` bold, `rm` in 13 px mono on the plate fill, `old` struck through, and `docs` underlined in the accent colour.
- `hamlet-rich-stream` streams a second block, one byte every 4 frames, from `Sec` to `Second *grows*`, so `grows` shows in italics. The sample keeps its span entities.
- `hamlet-code` draws a fenced block (`code_view`, `draw.rs:289`): an 11 px `rust` caption over 13 px mono code in a code view that wraps, on a plate 10 by 8 px in with a 1 px border. Line 3 is long, so it wraps onto 2 lines inside the plate. The plate takes the height of every row, so it never scrolls.
- `hamlet-code-stream` streams a fenced block that is still open into a second plate, 3 bytes a frame. The plate grows as lines arrive. The first bytes go with the press, so the plate never shows without a line.
- `hamlet-diff` draws Hamlet's diff block (`block`, `crates/hamlet_ui/src/screens/chat/change.rs:90`) in compact rows with a limit of 16 and `row_colours` on. The left plate is the change from `a b c` to `a B c d`, and the right plate is the 40-line change from `fixtures/diff/forty-old.txt` to `forty-new.txt`. A removed line is red on a deep red row, an added line green on a deep green row, and a line that did not change is dim.
- `stress` loads `large.rs.txt` repeated to 100,000 lines in a code view and `bench/patch/stress.patch.txt` in a diff view, and shows one at a time.

## How to get to it (user POV)

- Click `hamlet_rich` (100 272), `hamlet_code` (100 307), `hamlet_diff` (100 342), or `stress` (100 377) in the page list. They are the last four entries.
- On `hamlet_rich` and `hamlet_code`, click `stream` (290 68). A second click starts the stream again.
- On `hamlet_diff`, click `... 24 more lines` (1003 382) to show all 40 lines.
- On `stress`, click `code` (290 68) or `diff` (438 68) to choose the view.

## Driving it with launch.sh

Run `scripts/acceptance.sh <out dir>`. It runs `drive/acceptance.drive` through `launch.sh`, prints each `accept:` line and the frame stats of each CSV, judges the p95 of each stress CSV, and exits 0 only when the gallery exits 0, every `expect` in the script passes, and both p95 checks pass. It prints `acceptance: <n> of <n> checks pass` last. `gen_drives.py` writes the script.

The script visits each page by a click on the page list, and checks:

- **Rich styles.** `expect rich-styles` passes with every span of the sample as the sub-feature above lists. `hamlet-rich.png` shows the sample.
- **Rich stream.** After `stream` and 80 frames, `expect rich-stream` passes with `"Second " 14 sans | "grows" 14 italic sans sample kept`. The two `log entities` lines list the same `sample` ids. `hamlet-rich-stream.png` shows `Second grows`.
- **Code plate.** `expect code-plate caption above text 13 mono sideways off rows 4 tallest 2 lines` passes. `hamlet-code.png` shows the caption, the code, and line 3 wrapped inside the plate.
- **Copy a line.** A triple press on line 1 (300 127) and `Ctrl+C`. `clipboard` prints `fn main() {` and `expect copied "fn main() {"` passes.
- **Code stream.** After `stream` and 240 frames, `expect code-grow rows 1 to 17 grows yes shrinks 0 empty 0` passes, so the plate grew as lines arrived and never shrank or showed no text. `expect code-blank blank 0` passes, so no frame left a row of the plate's view without a glyph. `hamlet-code-stream.png` shows the 17 streamed lines on the second plate.
- **Diff colours.** `expect diff-colours` passes with the five rows of the small change. `hamlet-diff.png` shows both plates.
- **More lines.** `expect diff-more lines 16 more 24 rows 18 built 18` passes, then after the button `expect diff-more lines 40 more none rows 41 built 41`. `hamlet-diff-more.png` shows the 40 lines down to the window's bottom edge, and `hamlet-diff-more-end.png`, after a 200 px scroll of the page, shows the last of them.
- **Stress.** Each view scrolls 18 px a frame for 600 frames into `stress-code.csv` and `stress-diff.csv`, then jumps to its end. `expect stress code last 100000 of 100000` and `expect stress diff last 97072 of 97072` pass. After each, `expect stress-blank blank 0` passes, so no frame from the first scroll to the end left a row of the view without a glyph.
- **Stress p95.** `acceptance.sh` runs `frame-stats.sh` on `stress-code.csv` and `stress-diff.csv` and prints `accept: stress-code-p95 pass` or `fail`, and the same for `stress-diff-p95`. It passes at a p95 of 3.0 ms or less, the R12 and R15 budget. A machine running another build, gallery, or test fails it, so run `acceptance.sh` alone. `stress-code-end.png` and `stress-diff-end.png` show the last rows.

Each Hamlet page's visit is timed from 70 frames before the click to 120 frames after it, into `first-paint-<page>.csv`. `frame-stats.sh` drops the first 60 frames, so its `max` is the frame of the page switch.

`drive/hamlet-review-record.drive` records about 30 s of the four pages into `review/` for `video.sh`.

## Log topics

- `rich-styles` lists the sample's spans, `|` between them. Each is `"<text>" <size>`, then `bold`, `italic`, the font (`mono`, `sans` or `default`), the ink (`accent`, `dim`, or a hex colour when not the body's), `plate` for the theme's fill behind it, `underline`, and `struck`.
- `rich-stream` lists the second block's spans the same way, then `sample kept` when the sample has the span entities it had when `stream` was pressed, `sample changed` when not, or `sample not streamed`.
- `entities` adds `sample <id>...` on `hamlet_rich`, the ids of the sample's spans in order.
- `code-plate` reads the sample plate: `caption above` or `below`, the code's size and font, `sideways on` or `off`, its rows, and the most lines one row wraps onto.
- `code-grow` reads the streamed plate since `stream`: `frames <n> from <px> to <px> rows <n> to <n> grows <yes|no> shrinks <n> empty <n>`. `grows yes` means the last frame is taller and has more rows than the first. A shrink is a frame laid out shorter than the frame before. An empty frame shows no text.
- `code-blank` reads the streamed plate since `stream`: `frames <n> blank <n>`, where a blank frame has a row in the plate's view, as that frame lays it out, with no glyph on it. A row's line number counts, so an empty line is not blank.
- `copied` prints `copied "<text>"`, quoted as Rust debug text, or `copied none`. It reads what `clipboard` prints, so `expect copied` can match the whole copy.
- `diff-colours` reads each line row of the small change: its text, its ink (`red`, `green`, `dim`, `body`, or a hex colour), and its fill (`on deep red`, `on deep green`, or `no fill`).
- `diff-more` reads the 40-line change: `lines <n> more <n|none> rows <n> built <n>`, where `built` counts the rows the list built.
- `stress` prints `stress code last <row> of <rows>` or `stress diff last <row> of <rows>`, the last row of the view on show that shows, counted from 1.
- `stress-blank` prints `stress-blank frames <n> blank <n>` for the view on show, blank as in `code-blank`, and starts counting again. It counts from the view's first frame or the last `stress-blank` read, whichever is later. The acceptance script reads it once before each view's scroll, so the frames it judges start at the scroll.

## Gotchas

- The diff view's `diff` topic and `log selection` read the first diff view and the first code view. On `hamlet_code` the first code view is the sample, and on `hamlet_diff` the first diff view is the small change.
- `stress` builds both sources on its first visit, so that click takes longer than one frame. It is not a Hamlet page, and the 16.7 ms first-paint rule does not apply to it.
- The first frame of the stress diff view, after `diff`, builds 12 rows of a 687 px view, so `stress-blank` read before the scroll prints `blank 1`. It does not show in the screenshots, and the acceptance script does not judge it.
- A stream keeps going when you leave its page, and `stream` starts it again from the start.
- After `... 24 more lines`, a band of 9 to 43 px shows between old lines 16 and 17, where the button was, and narrows over the next frames. The `diff` page's compact rows show it too.
- The positions above hold only at 1280 by 800 with the page scrolled to the top.
