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

## Line commands, carets and brackets (E4)

E4 adds the line commands, the multi-caret gestures, the selection tools and the bracket match. Its scripts are `editor-*.drive` too. `gen_drives.py` writes them from the same model of the text, and also writes the fixtures `fixtures/edit/comment.py` and `fixtures/edit/comment.json`. Run each with `launch.sh <script> <out dir>`. Each prints `drive: <n> of <n> checks pass` and `drive: exit ok`.

`log caret` ends with the bracket match: `caret 1 primary 1 21:42..21:42 visible true bracket 30:5`. The bracket is the one at or just before the primary caret, and `<line>:<col>` is where its match is, the column in chars from 1. A bracket inside a string or a comment does not count. `bracket none` means no bracket is there, or its match is out of reach (1,000 lines or 50,000 chars). With several selections, `<k>` is the newest one's place in buffer order, since the newest is the primary.

Most lanes edit lines 23 to 27 of `large.rs.txt`, the `Tone::` arms. Each starts with 12 spaces, so column 13 is the `T`.

- **Indent (lane 2).** Run `editor-indent.drive`. It selects `23:13..25:42` and presses Tab, Shift+Tab, `mod+]` and `mod+[`. The three lines gain 4 spaces, lose them, gain them, and lose them, and the selection moves 4 columns with them. Then a Backspace at `27:9`, after 8 spaces, deletes back to the stop at `27:5`. The undo depth goes 1 to 5. Save `e4-indent.png`.
- **Move lines (lane 3).** Run `editor-move-lines.drive`. It selects `23:13..24:20` and presses Alt+Down, Alt+Up, Shift+Alt+Down and `mod+enter`, with `mod+z` after each. It checks lines 21 to 27, the selection and the undo depth after each key. The selection moves with its lines, sits on the lower copy after Shift+Alt+Down, and is back at `23:13..24:20` after each undo. `mod+enter` adds a line of 12 spaces below line 24 and puts the caret at `25:13`. Save `e4-move-lines.png`.
- **Comment (lane 4).** Run `editor-comment.drive`. On `23:13..25:42` it presses `mod+/` twice, which adds `// ` at column 13 and takes it away, then Shift+Alt+A, which puts `/* ` before `Tone` on line 23 and ` */` at the end of line 25. Then it opens `comment.py` and presses `mod+/` on line 2, which gains `# ` after its indent. Last, it opens `comment.json` and presses `mod+/` on line 2. The text stays and the undo depth stays 0. Save `e4-comment.png`.
- **Occurrence (lane 5).** Run `editor-occurrence.drive`. It puts line 620 at the top and clicks inside the `let` on line 629. The first `mod+d` selects that `let`. The next two add the ones on lines 630 and 635, which gives 3 selections with the primary at 3. Typing `var` changes all three, and Escape leaves the one caret at `635:20`. Save `e4-occurrence.png`.
- **Column (lane 6).** Run `editor-column.drive`. It runs `drag` with `alt` from column 13 of line 23 to column 19 of line 27, which selects `Tone::` on each of the five lines. Typing `x` puts `x` at column 13 on all five. Save `e4-column.png`.
- **Mod click (lane 7).** Run `editor-mod-click.drive`. It clicks `23:13`, then runs `click mod` at column 13 of lines 24, 25 and 26, then again on line 24. The caret counts are 2, 3, 4 and 3. Save `e4-mod-click.png`.
- **Line clipboard (lane 8).** Run `editor-line-clipboard.drive`. At the bare caret `16:5`, `mod+c` copies line 16 with its line break, and `mod+x` cuts it, so the row total drops to 10,372. Then it makes carets at column 13 of lines 22 to 24 and runs `paste a\nb\nc`, which puts `a`, `b` and `c` at the three carets. Save `e4-line-clipboard.png`.
- **Surround (lane 9).** Run `editor-surround.drive`. It double clicks `Body` on line 23, then types `(`, `[`, `{`, `"`, `'` and a backtick, with `mod+z` after each. Each pair wraps `Body`, which stays selected at `23:20..23:24`, and each undo puts back the text and `23:19..23:23`. Save `e4-surround.png`.
- **Brackets (lane 10).** Run `editor-brackets.drive`. A click after the `{` that ends line 21 prints `bracket 30:5`, the `}` that closes `fn tone`. Then it clicks after the `(` in `"(none)"` on line 9,506. `expect kind 9506 25 string` shows that the lexer put that `(` in a string run, and the caret prints `bracket none`. Save `e4-brackets.png`.

The perf scripts:

- `editor-occurrence-100.drive` clicks the first `let`, on line 16, and presses `mod+d` 100 times, which gives 100 selections from line 16 to line 1,628. In a timed window it sends 100 `type x` steps, and writes `editor-occurrence-100.csv`. Then each `let` is 100 `x`, and the `let` on line 1,631 is not changed.
- `editor-move-1000.drive` opens the 100,000-line input, selects lines 50,000 to 50,999 with a click and a shift click, and presses Alt+Down 100 times in a timed window. It writes `editor-move-1000.csv`. Then the block is at lines 50,100 to 51,099, lines 51,000 to 51,099 are at 50,000 to 50,099, and `seams` is clean.

The perf rule is the plan's. With `RUBRIX_NO_VSYNC=1` in `--release`, `frame-stats.sh` gives a p95 for each CSV. The bar for both is 3.0 ms.

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

## Input method (E3)

While the editor holds the focus, the window takes IME input. A composition shows as underlined text at the primary caret, with the input method's caret inside it, and the buffer and the undo history do not change. A commit goes in at every caret as one undo step. While a composition shows, the editor's keys do nothing, so no key types beside it.

The `ime` steps write the messages winit sends, so the scripts need no input method. `ime enable` and `ime disable` write `Ime::Enabled` and `Ime::Disabled`. `ime preedit <text> <start> <end>` writes a preedit with its cursor in bytes of `<text>`, and `ime preedit` alone writes the empty preedit that ends a composition. `ime commit <text>` writes a commit. On Wayland, winit sends an empty preedit, then the commit, then the next preedit, so the scripts commit in that order.

The page answers G2's `ime` topic. `log ime` prints `ime enabled <bool> position <x> <y> last <message> page <answer>`. `enabled` and `position` are the window's `ime_enabled` and `ime_position`, where the input method puts its candidate box. The page's answer is `preedit "<text>" cursor <a>..<b> caret <x> <y>`, with `cursor none` while the input method hides its cursor, or `preedit none caret <x> <y>`. `caret` is the bottom-left of the primary caret, in logical px, or `none` while its row is not built or is clipped. `position` and `caret` print the same digits when the window follows the caret.

`gen_drives.py` writes the `editor-ime-*.drive` scripts from its E3 block. Line 120 is `        .run();`, and the scripts compose at its column 5. `你好` is two chars and six bytes.

- **Regression (lane 1).** Run `editor-acceptance.drive` at trunk and at head, and `editor-ime-commit.drive` at head. Save the head acceptance shot as `e3-regress.png`. Pass when trunk and head print the same `drive: <n> of <n> checks pass` line and `editor-ime-commit.drive` passes.
- **Preedit (lane 2).** Run `editor-ime-preedit.drive`. It clicks line 120 at column 5, writes `ime enable`, and writes `ime preedit 你好 6 6`. `log ime` prints `enabled true` and `page preedit "你好" cursor 6..6`, `log text 120` is unchanged, and `log undo` prints `undo 0 redo 0`. `e3-preedit.png` shows `你好` underlined at the caret, with the caret after it.
- **Commit (lane 3).** Run `editor-ime-commit.drive`. After the preedit it writes an empty preedit, `ime commit 你好`, and an empty preedit. `log text 120` prints `"    你好    .run();"`, the caret is at `120:7`, `log ime` prints `page preedit none`, and the undo depth is 1. `e3-commit.png` shows `你好` with no underline.
- **Undo (lane 4).** Run `editor-ime-undo.drive`. After the commit, one `mod+z` restores line 120 and the caret at `120:5`, and `mod+shift+z` puts `你好` back with the caret at `120:7`. Save `e3-undo.png`.
- **Cancel (lane 5).** Run `editor-ime-cancel.drive`. A preedit then an empty preedit, and a preedit then `ime disable`, each leave line 120 unchanged and `log ime` at `page preedit none`. Save `e3-cancel.png`.
- **Keys (lane 6).** Run `editor-ime-keys.drive`. During a preedit, `type x` and `key backspace` leave line 120 and the undo depth unchanged. After the commit, `type y` makes line 120 `"    你好y    .run();"`. Save `e3-keys.png`.
- **Position (lane 7).** Run `editor-ime-position.drive`. With line 100 at the top, it clicks line 120 at column 5, presses Down 10 times, and scrolls the view 200 px. After each step `log ime` prints the same `position <x> <y>` and `caret <x> <y>`, which the script computes from the caret's line and column. Save `e3-position.png`.
- **Focus (lane 8).** Run `editor-ime-focus.drive`. `log ime` prints `enabled true` after a click in the editor, `enabled false` on the `code` page, and `enabled true` after the click back on the `editor` page. Save `e3-focus.png`.
- **100,000 lines (lane 9).** Run `editor-ime-100k.drive`. It opens the 100,000-line input at line 50,000, grows a 20-char preedit one char a frame five times inside `frames start` and `frames stop editor-ime-100k.csv`, and commits it. `log text 50000` holds the commit once. Save `e3-100k.png`.
- **Live Pinyin (lane 10).** A hand check at milestone 1, until a session can give the gallery the keyboard focus. Run `fcitx5-remote -s pinyin`, then `launch.sh --ime-live drive/editor-ime-live.drive <out>`, then `fcitx5-remote -s keyboard-us`. The script clicks line 120 at column 5, runs `wtype nihao`, and runs `wtype 1`, which picks Pinyin's first candidate. `log ime` shows a preedit before the commit, and `log text 120` holds `你好` once. Save `e3-live-pinyin.png`. `run-all.sh` skips the script.

Perf: run `editor-ime-100k.drive` at head and `editor-100k.drive` at trunk and at head, with `RUBRIX_NO_VSYNC=1` in `--release`, interleaved, five runs each, alone, with the load under 4. Read `editor-ime-100k.csv` and `editor-100k-type.csv` with `frame-stats.sh`. The preedit frame p95 bar is 3.0 ms, and the typing p95 at head may be at most 10% above trunk.

Gotchas:

- Bevy's `TextInputPlugin` turns the window's IME off in `PreUpdate` of every frame in which no `EditableText` has the focus. The editor turns it on again in `PostUpdate`, so a system between them reads `false`. The driver reads the window in `First`, the value winit saw.
- Bevy draws a span's underline 0.7 px thick at scale 1 and fills only the pixels whose centres it covers, so the CJK run of a preedit shows none. The editor draws its own bar under the preedit, at least one device pixel thick.
- `wtype` cannot send a lone space, because the driver trims its text. Lane 10 commits with `1`, Pinyin's first candidate, instead of a space.
