# Code page

The code page shows a highlighted file in `code_view`: one rich-text row per line in a virtual list, with the line numbers in a gutter. R12 ships it. It opens on `bench/fixtures/rust/large.rs.txt`, 10,372 lines of Rust.

## Sub-features

- `code-render` shows the file's lines in the theme's colours, in the bundled JetBrains Mono.
- `code-gutter` numbers every row, pads each number to the widest, and hides the empty line after a final newline.
- `code-scroll-x` scrolls every row sideways by one offset under `wrap off`, while the gutter stays.
- `code-wrap` breaks a long line inside its row under `wrap on`.
- `code-late-comment` colours a line inside a block comment that opened far above it, in a row built long after the comment's first line.
- `code-stress` shows a 100,000-line input and jumps to its middle in one frame.
- `code-unicode` shows a tab as spaces to the next 4-column stop, CJK text as glyphs, and an emoji in one glyph cell.
- `code-edges` shows an empty file as one row numbered `1`, and a one-line file.
- `code-theme` takes the new theme's colours on a theme switch and keeps its scroll.
- `code-plain` shows a file in no known language in the body colour.

## How to get to it (user POV)

- Click `code` in the page list.
- Click `large.rs`, `large.ts`, or `100k lines` above the view to show that file. The label under the buttons names the file and its line count.
- Click `wrap off` to wrap long lines. Click `wrap on` to scroll them sideways again.
- Turn the wheel over the view to scroll it. A sideways wheel scrolls it sideways under `wrap off`.

## Driving it with launch.sh

Preconditions: `doctor.sh` is clean and the out dir is empty. The scripts are in `crates/bevy_rubrix/examples/gallery/drive/`, and `gen_drives.py` writes every `code-*.drive`. The files they open are in `examples/gallery/fixtures/`.

- **Render.** Run `.../code-large.drive`. `log page` prints `page code`, and `log runs 1` prints `runs 1: 0..33 comment.doc | shown "//! The theme the examples share." #737AA2`. `r12-large.png` shows numbered Rust lines with keywords, strings, and comments in different colours.
- **Line 10,000 and the end.** Run `.../code-end.drive`. `expect rows first_visible 9999 spawned` passes, and `r12-10000.png` shows line 10,000 at the top with no blank band. `r12-end.png` shows the last number, 10372, beside `}`, the file's last line. `log runs 10372` prints `runs 10372: 0..1 punctuation.bracket | shown "}" #E6E6EA`.
- **Late comment.** Run `.../code-late-comment.drive`. It opens `late-comment.rs`, whose block comment opens on line 10, and scrolls line 60 to the top. `expect rows first_visible 59 spawned` and both `expect runs` lines pass. `log runs 60` prints `0..53 comment`, and `r12-late-comment.png` shows the comment lines in the comment colour.
- **Scroll sideways.** Run `.../code-hscroll.drive`. It opens `long-line.rs`, whose line 3 is 400 chars, switches to wrap and back, and scrolls right. `log rows` ends with `x 2337`. `r12-hscroll.png` shows the gutter numbers in place, `END_OF_LONG_LINE` at the right edge, and one bar at the bottom.
- **Wrap.** Run `.../code-wrap.drive`. `expect rows x 0` passes. `r12-wrap.png` shows line 3 broken onto four lines, no sideways bar, and number 4 below the wrapped text.
- **100,000 lines.** Run `.../code-100k.drive`. `expect rows first_visible 49999 spawned` passes two frames after the jump, and `log rows` prints `rows 87`, below 200. `r12-100k.png` shows numbers near 50,000.
- **Unicode.** Run `.../code-unicode.drive`. `r12-unicode.png` shows the tabs as indentation, `日本語のテキスト` as glyphs, and `😀` in one glyph cell, with nothing over the gutter. `log runs 3` shows the tab as four spaces, `shown "    let "`.
- **Edges.** Run `.../code-edges.drive`. Both `expect rows total 1 following` lines pass. `r12-edges-empty.png` shows one row numbered `1`, and `r12-edges.png` shows `1` beside `fn main() {}`.
- **Theme.** Run `.../code-theme.drive`. After `theme dark` and after `theme light`, `expect rows first_visible 739 spawned` passes, and the two `log runs` lines show the same spans in other colours. `r12-theme-dark.png` and `r12-theme.png` show the same lines.
- **Plain.** Run `.../code-plain.drive`. `expect runs 1 plain` passes, and `r12-plain.png` shows `notes.xyz` in the body colour with line numbers.

## Perf

Run each script with `RUBRIX_NO_VSYNC=1` in `--release`, alone, five times, then read the CSVs in the out dir.

- First paint is the largest frame among frames 68 to 80 of `code-first-paint.csv`, from `code-large.drive`. The page opens at frame 70.
- Scroll frame time is `frame-stats.sh` on `code-scroll.csv` and on `code-scroll-100k.csv`. Each scrolls 18 px a frame for 600 frames, after a 60-frame wait that `frame-stats.sh` drops.
- The jump is the largest frame among frames 70 to 74 of `code-jump-100k.csv`. The `expect rows first_visible 49999 spawned` two frames later shows that the rows landed in the jump's frame.
- `code-load-100k.csv` holds the frame that lexes the 100,000 lines, kept apart from the jump.

## Review video

Run `.../code-review-record.drive`, then `video.sh <out>/review <out>/review.mp4 30`. It records about 1,000 frames: a scroll, both wrap switches, a sideways scroll, a theme switch, and the jump into 100,000 lines.

## Gotchas

- The gutter hides the empty line after a final newline, so `large.rs.txt` shows 10,372 rows.
- A row is 17 px under `wrap off`. From the top, `scroll 0 -<(n - 1) * 17>` puts line n first.
- The page starts under `wrap off`. The button names the state, so `wrap off` means that lines scroll sideways.
- A tab shows as spaces to the next 4-column stop. Columns count chars, so a tab after a CJK char stops short of the stop.
- Opening a file or pressing a file button builds a new list at line 1.
- Pressing `100k lines` lexes 100,000 lines in one frame of about 20 ms.
- The theme switch builds the view again and puts its scroll back.
- The list builds the rows within 400 px of the view, about 87 rows, not within the 1,000 px that the `virtual_list` page uses.
