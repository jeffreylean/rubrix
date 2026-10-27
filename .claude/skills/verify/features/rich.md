# Rich text

The `rich` page shows fynix's `rich` view, text built from styled spans, in one scroll area. F1 ships it with six blocks, top to bottom. They are a line of styles, a line of accented Latin, Japanese, and emoji, a paragraph that wraps, a hit line, a long line of 200 coloured spans, and 65 rows of 10 spans that show only while `rows on` shows. The buttons above the scroll area change the long line, the plate, and the rows.

## Sub-features

- `rich-styles` shows `plain`, `bold`, `italic`, `mono`, `larger`, `underlined` with an accent line, `struck` with a critical line, ` plate ` on a fill, and `accent` in the accent colour.
- `rich-unicode` shows `café déjà naïve 日本語のテキスト 😀 after the emoji` on one line.
- `rich-wrap` breaks the paragraph at word boundaries, and each bold, coloured, or italic run keeps its style across the breaks.
- `rich-hit` records the byte of the character under a press on the line `click: héllo wörld 日本 x`.
- `rich-long` shows 200 spans, `s000` to `s199`, each in its own hue.
- `rich-recolour` gives every span of the long line a new hue and keeps the same span entities.
- `rich-count` sets the long line to 0, 5, or 200 spans.
- `rich-animate` recolours the long line on every frame while `animate on` shows.
- `rich-plate` takes the fill off ` plate ` and puts it back.
- `rich-rows` adds 65 rows, `00` to `64`, each followed by `tok1` to `tok9` in their own hues.

## How to get to it (user POV)

- Run `cargo run -p bevy_rubrix --example gallery` and click `rich` in the page list.
- Click a button in the row above the text: `recolour`, `animate off`, `0 spans`, `5 spans`, `200 spans`, `plate`, or `rows off`.
- Click a character on the hit line.
- Turn the wheel over the page.

## Driving it with launch.sh

Preconditions: `doctor.sh` is clean and the out dir is empty. For a script that writes a frame CSV, also build in release, set `RUBRIX_NO_VSYNC=1`, and run it alone with no other Bevy window or build running.

- **Open.** Run `launch.sh crates/bevy_rubrix/examples/gallery/drive/rich.drive <out>`. It prints `page rich`, and `log spans` prints a line that starts `spans 200: `, then lists each span's id, its text from `"s000 "` to `"s199 "`, and its colour. `<out>/rich.png` shows the page from the line of styles down, with no rows under the long line.
- **Recolour.** Run `.../rich-recolour.drive`. Both `log spans` lines start `spans 200: `, list the same ids in the same order, and give every span a new colour. `rich-recolour.png` shows the long line in different hues from `rich-before.png`.
- **Empty the long line and grow it back.** Run `.../rich-empty.drive`. After `0 spans` the line starts `spans 1: ` and holds one span with the text `""`, and `rich-empty.png` shows no long line text. After `5 spans` the line starts `spans 5: `, its first id is the first id from before `0 spans`, and `rich-five.png` shows `s000` to `s004`.
- **Hit a character.** Run `.../rich-hit.drive`. `log hit` prints `hit none` before any press. `click 342 233` makes it print `hit 15 'ö'`, and `click 387 233` makes it print `hit 21 '日'`. Both `accept: hit` lines say `pass`.
- **Lines in both themes.** Run `.../rich-lines.drive`. It prints `page rich` and `theme dark`, then `theme light` after the toggle. `rich-lines.png` and `rich-lines-light.png` show the accent line under `underlined` and the critical line through `struck`, dark then light.
- **Theme.** Run `.../rich-theme.drive`. `log spans` starts `spans 200: ` before and after the toggle, and `log theme` prints `theme light` after it. The colours in the two lines differ, since each hue is lighter on the dark theme. `rich-dark.png` and `rich-light.png` show the same text, dark then light.
- **Plate.** Run `.../rich-plate.drive`. `rich-plate-on.png` shows ` plate ` on a fill, and `rich-plate-off.png` shows it with no fill and the rest of the page unchanged.
- **Wrap.** Run `.../rich-wrap.drive`. It prints `page rich`, resizes to 900 by 800, and prints `drive: resize ok` and `page rich` again. `rich-wrap-narrow.png` shows the paragraph on more lines than `rich-wrap.png`, with the bold, coloured, and italic runs styled on each line they cover.
- **Animate.** Run `.../rich-animate.drive`. It clicks `animate off`, so the button reads `animate on`, and writes `rich-animate.csv` over 600 frames. `frame-stats.sh <out>/rich-animate.csv` prints `median=`, `p95=` and `max=`. `log spans` still starts `spans 200: `.
- **Rows.** Run `.../rich-rows.drive`. It clicks `rows off`, so the button reads `rows on`, inside `frames start` and `frames stop rich-rows.csv`, then `log entities` prints `entities <n>`. `frame-stats.sh <out>/rich-rows.csv` prints `median=`, `p95=` and `max=`. It scrolls 2,000 px, and `rich-rows.png` shows rows of `tok1` to `tok9` in their own hues.

## Gotchas

- The rich scripts open the page with `click 100 96` on the page list and wait 120 frames. A `scroll` after that needs `move 740 400` first, as `rich-rows.drive` does.
- The hit positions 342 233 and 387 233 hold only at 1280 by 800 with the page scrolled to the top. A press that lands on no character prints `hit none`.
- The byte in `hit` counts UTF-8 bytes from the start of the line, not characters. `ö` in `wörld` is byte 15 and `日` is byte 21, because `é` and `ö` take two bytes each.
- `0 spans` leaves one empty span, so `log spans` prints `spans 1: `, not `spans 0: `. Growing the line back keeps the first span of the 200.
- The `plate` button keeps its label, and no log topic reads the plate. `rich-plate.drive` proves it by screenshots only.
- The theme toggle builds the page again. Do not compare span ids across a toggle. Compare the count and the colours.
- `log spans` reads the long line. On another page it prints `spans none`.
- The button row wraps. The y 68 positions hold only at 1280 by 800. At 900 by 800 the buttons move.
- The long line's span count, `animate`, `plate`, and `rows` stay set when you leave the page and come back in the same run.
