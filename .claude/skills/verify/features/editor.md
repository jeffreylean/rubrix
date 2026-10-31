# Editor page

The editor page shows a file in `code_editor`, the editable sibling of `code_view`: the same rows and gutter, with a caret per selection, a pointer that selects, and keys that edit while the view holds focus. E2 ships it. It opens on `bench/fixtures/rust/large.rs.txt`, 10,372 lines of Rust.

## Sub-features

- `editor-caret` places a blinking caret at a click, extends the selection on a shift click, selects on a drag, takes a word on a double click and a line on a triple click.
- `editor-type` inserts typed text at every caret, and the line takes its syntax colours in the frame it is typed.
- `editor-enter` splits a line at the caret and indents the new line with the leading spaces before the caret. The rows below move down by one row with no gap, no overlap, and no jump in the gutter numbers.
- `editor-join` joins a line onto the one above on a Backspace at its start, and the rows below move up.
- `editor-move` moves the caret by word, to the first non-blank and then column 1 on Home, to the line's end on End, and by a page on Page Down, and keeps it in view after each.
- `editor-clipboard` copies, cuts and pastes through the app's `ClipboardProvider`. The gallery's provider keeps its own text, so a script never touches the shared Wayland clipboard.
- `editor-undo` undoes and redoes in Pierre's steps: a run of typing is one step, and an Enter is its own.
- `editor-wrap` wraps a long line under `wrap on`, and the rows below move down by the rows it gained.
- `editor-stress` edits a 100,000-line input at line 50,000 inside the 3.0 ms keystroke budget.

## How to get to it (user POV)

- Click `editor` in the page list. It is the last entry.
- Click into the text to place the caret, and type. `mod+z` undoes and `mod+shift+z` redoes.
- Click `large.rs`, `large.ts`, or `100k lines` above the view to edit that file from the start. Each click starts a new doc, so it drops the edits and the undo history. The label under the buttons names the file and its line count.
- Click `wrap off` to wrap long lines. Click `wrap on` to scroll them sideways again.

## Driving it with launch.sh

Preconditions: `doctor.sh` is clean and the out dir is empty. The scripts are in `crates/bevy_rubrix/examples/gallery/drive/`, and `gen_drives.py` writes every `editor-*.drive` from a model of the text it expects, so each `expect` line comes from the same edits the script sends. Change a script there, not by hand.

The page lays out as the code page does. The buttons sit at y 68: `large.rs` at x 290, `large.ts` at 438, `100k lines` at 586, and the wrap toggle at 734. With line `t` at the top of the view, line `n` shows at y `126 + (n - t) * 17`. The text starts at x 271 under a five-digit gutter, and at x 279.4 under the six-digit gutter of the 100,000-line input. A column is 8.4 px, and the scripts click 2 px right of a column's left edge so that the caret lands before that column. The gutter is at x 235, and a click there puts the caret at column 1. `gen_drives.py` names each of these values once, with three measured live: a Page Down moves the 38 rows that show whole (`PAGE_ROWS`), a wrapped visual line holds 113 chars (`WRAP_COLUMNS`), and a triple click takes the line break (`TRIPLE_TAKES_BREAK`).

The page answers G2's `caret`, `text <line>` and `undo` topics. Lines and columns count from 1, and a column counts chars from the line's start.

- `log caret` prints `caret <n> primary <k> <sel> ... visible <bool>`. `<n>` is the selection count, `<k>` is the primary's place among them in buffer order, from 1, and each `<sel>` is `<line>:<col>..<line>:<col>`, its start then its end, so a selection made backwards prints the same as one made forwards. `visible` says whether the primary caret is inside the view.
- `log text <line>` prints the line quoted as Rust debug text, so leading spaces show, or `none` past the last line.
- `log undo` prints `undo <depth> redo <depth>`.
- `log runs <line>` reads the editor's doc when the page shows no code view. `log rows` reads the editor's list, and its `total` is the editor's row count.
- `log kind <line> <byte>` prints `kind <line> <byte> <kind>`, the kind of the run that holds that byte of the line, counted from 0 as `runs` counts them, or `none`. A script checks a word's kind byte by byte with it, without naming where the lexer ends the word's run: the lexer folds the spaces around `let` into its keyword run.
- `log seams` prints `seams shown <a>..=<b> gaps <g> overlaps <o> missing <m> mismatched <x> numbers <ok|jump <line>>` over the rows of the view that show. A gap or an overlap is more than 0.5 px between one row's bottom and the next row's top. `missing` counts shown rows whose lines do not follow on, `mismatched` counts rows whose text is not their line's, and `numbers` names the first row whose line number is not its line's or does not sit at the row's top. The editor draws its line numbers in one layer over the gutters, one text per line, so a number stays put while an Enter moves the rows below it.
- `log row <line>` prints `row <line> top <px> height <px> lines <k>`: where the list puts the line's row, and the visual lines its text takes, or `lines unbuilt` while no row is built for it.
- `log typing` prints `typing edits <e> hidden <h>`: the frames in which the doc on show changed since it was made, and how many of those ended with the primary caret out of view.

The recipes:

- **Regression (lane 1).** Run `code-large.drive` at trunk and at head, and `editor-type.drive` at head. Save the head `r12-large.png` as `e2-code-regress.png`. Pass when both `log rows` lines are the same at trunk and head, and `editor-type.drive` passes.
- **Type (lane 2).** Run `editor-type.drive`. It puts line 120 at the top, clicks column 5, and types `let x = 1;`. `expect text 120 "    let x = 1;    .run();"` passes, `expect kind 120 <byte> keyword` passes for bytes 4, 5 and 6, the bytes of `let`, and the caret ends at `120:15`. `e2-type.png` shows `let` in the keyword colour.
- **Enter (lane 3).** Run `editor-enter.drive`. It clicks the end of line 5,000 and presses Enter 20 times. `expect rows total 10373 following` passes before and `expect rows total 10393 following` after, both `seams` checks print `gaps 0 overlaps 0 missing 0 mismatched 0 numbers ok`, and the caret ends at `5020:1`. `e2-enter.png` shows 20 empty numbered lines after line 5,000.
- **Join (lane 4).** Run `editor-join.drive`. Twenty times, it clicks the gutter of line 5,001's screen row, presses Backspace, and presses Home. `expect rows total 10353 following` and the `seams` check pass, and `log text 5000` holds lines 5,000 to 5,020 run together. `e2-join.png` shows the joined line.
- **Move (lane 5).** Run `editor-move.drive`. From `5004:13`, before `Ok`, it presses `ctrl+right`, `ctrl+left`, Home twice, End, and Page Down three times. The carets are `5004:15`, `5004:13`, `5004:9`, `5004:1`, `5004:63`, then `5042:29`, `5080:33` and `5118:2` at 38 rows a page, each with `visible true`. Save `e2-move.png`.
- **Copy and paste (lane 6).** Run `editor-copy-paste.drive`. It selects lines 11 to 13 with three `shift+down`, copies, scrolls 100 lines down, clicks line 111 at column 1, and pastes. `expect copied` matches the three lines with their line breaks, and `log text 111` to `log text 113` print them. Save `e2-copy-paste.png`.
- **Undo (lane 7).** Run `editor-undo.drive`. At the end of line 124 it types `foo`, presses Enter, types `bar`, then presses `mod+z` twice and `mod+shift+z` once. Every `expect text` and `expect undo` passes: the depth goes 1, 2, 3, then 2, 1, and 2 again. Save `e2-undo.png`.
- **Mouse (lane 8).** Run `editor-mouse.drive`, and again with `RUBRIX_SCALE=2`, which `run-all.sh` does as `editor-mouse.drive@RUBRIX_SCALE=2`. The carets are `13:8`, then `13:8..13:12` after the shift click, `11:5..12:10` after the drag, `13:5..13:11` after a double click on `struct`, and `12:1..13:1` after a triple click on line 12. Each run saves `e2-mouse.png` in its own out dir. Save the scale-2 one as `e2-mouse-scale2.png`. It is 2560 by 1600, twice the scale-1 shot, with the caret where the click landed.
- **Keystrokes at 10,000 and 100,000 lines (lane 9).** Run `editor-10k.drive` and `editor-100k.drive`. Each puts line 5,000 or 50,000 at the top and clicks its end, then runs three timed windows: 200 `type x` steps, 50 Enters, and 50 joins. Each window writes `editor-10k-type.csv`, `-enter.csv` and `-join.csv`, or the `editor-100k-` ones. After the windows, `expect typing hidden 0` passes, and the row total is back to 10,373 or 100,001. `e2-10k.png` and `e2-100k.png` show the edited line.
- **Wrap (lane 10).** Run `editor-wrap.drive`. It turns wrap on and clicks the end of line 300. It records rows 300 and 301 (`top 5083 height 17 lines 1`, `top 5100`), types `wrap` words until line 300 wraps onto a second visual line (`top 5083 height 34 lines 2`, row 301 at `top 5117`), then types more until it wraps onto a third (`top 5083 height 51 lines 3`, row 301 at `top 5134`). Row 300's top stays, and each new visual line moves row 301 down by one row height, 17 px. `seams` prints `gaps 0 overlaps 0`. Save `e2-wrap.png`.
- **Acceptance.** Run `editor-acceptance.drive`. On line 5,004 of the 10,000-line file and on line 50,004 of the 100,000-line input, it types 100 `x` in a timed window, presses Home, selects two lines down, copies, pastes 10 lines below, and undoes the paste, with an `expect` after each step. It prints `drive: <n> of <n> checks pass` and saves `e2-acceptance-10k.png` and `e2-acceptance-100k.png`.

## Perf

Run `editor-10k.drive`, `editor-100k.drive`, `code-large.drive` and `code-100k.drive` with `RUBRIX_NO_VSYNC=1` in `--release`, at trunk and at head, interleaved, five runs each, alone, with the load under 4. Read each CSV with `frame-stats.sh`, which drops the 60-frame wait at the start of each window.

- Keystroke frame time is the p95 of `editor-10k-type.csv`, `-enter.csv` and `-join.csv`, and of the `editor-100k-` ones. The bar is 3.0 ms.
- `expect typing hidden 0` checks that no frame that changed the doc hid the primary caret. It counts frames, not keystrokes, so it needs no `log` step between keys.
- The `code` page's first paint and scroll p95, from `code-large.drive` and `code-100k.drive`, must stay within 10% of trunk.

## Review video

`editor-acceptance.drive` is the milestone run. Add `record start review` after its `page` step and `record stop` before `exit` in a copy, then `video.sh <out>/review <out>/review.mp4 30`.

## Gotchas

- An editable view shows one more row than the read-only view of the same file: the empty line after a final newline, where a caret can sit. `large.rs.txt` shows 10,373 rows here and 10,372 on the `code` page, and the 100,000-line input shows 100,001.
- `type` sends a whole word in one frame, every key pressed and released at once. A timed window that needs one keystroke a frame uses one `type x` step a frame.
- `type` drops the text's leading and trailing spaces. Send a space first with `key space`.
- Word motion is Ctrl+Arrow on Linux and Option+Arrow on macOS. The driver sends `ctrl` as Control on both, so `editor-move.drive` runs on Linux only as written, and the Mac phase reruns that lane by hand with Option.
- Typing at the end of a long line scrolls the view sideways to keep the caret in view. A gutter click then lands on the text under the gutter, not at column 1, so the join scripts press Home after each Backspace to bring the view back to column 1.
- A file button or `open` starts a new doc and drops the old one, so its edits and undo history go.
- The theme toggle builds the view again around the same doc, so the text, the carets and the undo history stay.

## Find and replace

E5 adds a find bar, `find_bar(doc)`, which the editor page lays over the view's top right. `mod+f` opens it on the word at a bare caret, and `mod+alt+f` opens it with its replace row, the focus in the replace field. It searches plain or as a regex, with toggles for match case (`Aa`), whole word (`W`) and regex (`.*`). A search scans at most 1 ms a frame, so the count reads `searching <n>` until it ends. It stops at 100,000 matches and reads `<i> of 100000+`. Each built row tints its matches, and the current match takes a stronger tint.

- In the find field, Enter and `mod+g` go to the next match, Shift+Enter and `mod+shift+g` to the previous one, and Escape closes the bar and gives the focus back to the editor. Enter and Escape do nothing while the field holds an IME preedit. Enter in the replace field replaces the current match.
- `Replace` replaces the current match and puts the caret after it. `Replace All` replaces every match as one undo step. With the regex toggle on, `$1` to `$n`, `$&` and `$$` in the replacement expand.
- The bar sits at y 151 for its find row and 179 for its replace row. The find field is at x 745, `Aa` at 873, `W` at 902, `.*` at 930, `Replace` at 894 and `Replace All` at 986. `gen_drives.py` names each once.
- `log find` prints `find <open|closed> query "<text>" <index> of <count>[ capped] marked <rows> final <ms|pending> label "<label>"`. The index counts from 1, and 0 means no current match. `marked` counts the built rows with a tinted match, and `final` is the time from the last change to the query until its count was final.

The recipes. `gen_drives.py` writes every `editor-find-*.drive`, and each count is the one `grep` gives with the same options, named in a `# grep` line of the script.

- **Regression (lane 1).** Run `editor-acceptance.drive` at trunk and at head, and `editor-find-open.drive` at head. Save the head acceptance shot as `e5-regress.png`. Pass when both print the same `drive: <n> of <n> checks pass` line and `editor-find-open.drive` passes.
- **Open (lane 2).** `editor-find-open.drive` clicks inside `theme` on line 676 and presses `mod+f`. `log find` prints `open query "theme" 13 of 132 marked 9`, and the caret selects `676:16..676:21`. `e5-open.png` shows every visible `theme` tinted.
- **Next and previous (lane 3).** `editor-find-next.drive` opens on the first `theme` and presses Enter, `mod+g`, Shift+Enter and `mod+shift+g`. The index goes 2, 3, 2, 1, and `log caret` selects each match with `visible true`. Save `e5-next.png`.
- **Options (lane 4).** `editor-find-options.drive` types `Theme`, then turns on match case, whole word and regex in turn. The counts are 132, 21, 21 and 21. Save `e5-options.png`.
- **Across lines (lane 5).** `editor-find-multiline.drive` turns on regex and types `\{\n\s+let`. The count is 281, and the caret selects the first match, `15:27..16:8`. Save `e5-multiline.png`.
- **Marks (lane 6).** `editor-find-marks.drive` searches `self` from line 3,944, scrolls 4,000 px down and back. `marked` is 25 and then 4, each no more than the 87 rows built. `e5-marks.png` shows the tints and the current match at line 3,970.
- **Replace (lane 7).** `editor-find-replace.drive` presses `mod+alt+f` on `theme`, types `palette` and presses Enter. The count drops to 131, a `mod+z` in the editor brings 132 back, `Replace All` reads `No results`, and one `mod+z` brings 132 back. Save `e5-replace.png`.
- **Regex replace (lane 8).** `editor-find-regex-replace.drive` replaces `(\w+)_(\w+)` with `$2_$1` once, then `fn` with `$&$&` once. `log text 305` shows `list_keyed` and `log text 320` shows `fnfn`. Save `e5-regex-replace.png`.
- **100,000 lines (lane 9).** `editor-find-100k.drive` opens the 100,000-line input and types `cx.theme()` one char every 10 frames inside `frames start` and `frames stop editor-find-100k.csv`. `log find` prints `1 of 150` and the `final` time. Save `e5-100k.png`.
- **Close (lane 10).** `editor-find-close.drive` searches `e` on the 100,000-line input. The count reads `1 of 100000 capped` with the label `1 of 100000+`. Escape prints `closed` and `marked 0`, and a typed `x` replaces the selected `e` on line 1. Save `e5-close.png`.

Perf: run `editor-find-100k.drive`, and `editor-100k.drive` at trunk and at head, with `RUBRIX_NO_VSYNC=1` in `--release`, interleaved, five runs each, alone, through `perf-run.sh`. The find-field frame p95 is the p95 of `editor-find-100k.csv`, and its bar is 3.0 ms. The `final` time of a plain count must stay at or under 250 ms. The `editor-100k-type.csv` p95 at head must stay within 10% of trunk.
