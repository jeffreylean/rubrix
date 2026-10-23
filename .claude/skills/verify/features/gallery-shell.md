# Gallery shell

The gallery shell is the window that holds every page: a page list on the left, the selected page on the right, and a theme toggle under the list. G1 ships one page, `about`, a scrollable list of 200 labels.

## Sub-features

- `shell-pages` lists every page by name, and selecting one shows it.
- `shell-theme` flips between a dark and a light theme, builds the page again, and keeps its scroll offsets.
- `about-scroll` scrolls the 200 labels by wheel.
- `about-font` shows accented Latin, CJK and emoji text on row `Label 001`.

## How to get to it (user POV)

- Run `cargo run -p bevy_rubrix --example gallery`. The window opens on `about`.
- Click a name in the page list.
- Click `theme: dark` at the bottom of the list.
- Turn the wheel over the page.

## Driving it with launch.sh

Preconditions: `doctor.sh` is clean and the out dir is empty.

- **Open.** Run `launch.sh crates/bevy_rubrix/examples/gallery/drive/smoke.drive <out>`. It prints `page about` and `drive: exit ok`, and `<out>/smoke.png` shows the list, `about` in the accent colour, and labels from `Label 000`.
- **Select a page.** Run `.../click.drive`. `log page` prints `page about` after `click 100 62`.
- **Toggle the theme.** In the same script `click 100 771` makes `log theme` print `theme light`, and `click-theme.png` is light.
- **Theme keeps the scroll.** Run `.../theme.drive`. `log scroll` prints `scroll 1000` before and after the click, both `accept:` lines say `pass`, and `theme-before.png` and `theme-after.png` show the same labels, dark then light.
- **Scroll.** Run `.../scroll.drive`. `scrolled-after.png` shows labels from further down than `scrolled-before.png`, and `log entities` prints a number.
- **Font.** In `smoke.png` row `Label 001` shows `héllo wörld · 日本語 · 😀` with no boxes.

## Gotchas

- A screenshot taken in the first frames can be black. Scripts `wait` before the first one.
- The page list highlight fades. After a page change `wait 120` before a screenshot.
- A `scroll` goes to what is under the pointer. After a click on the list, `move 740 400` first.
- Under vsync an unfocused window runs at about 1 frame per second and screenshots come out black, so a scripted run turns vsync off unless `RUBRIX_NO_VSYNC=0`.
- The label rows are 24 px tall by design. Scroll distances in pixels map to rows at 24 px each.
