# Virtual list

The `virtual_list` page shows one scroll area of numbered rows, of which fynix's `virtual_list` builds only the rows near the view. F2 ships it with 10,000 rows to start. Every row is `Row <n>` on one line, 17 px tall, except that every 100th row, from `Row 0`, has three lines. The buttons above the list change the row count, swap in a plain scroll area, and turn on following the bottom.

## Sub-features

- `vlist-rows` shows `Row 0` at the top and builds only the rows near the view, at most 199.
- `vlist-count` sets 10,000 or 100,000 rows with `10,000 rows` and `100,000 rows`, and adds 1,000 with `append 1,000`.
- `vlist-plain` swaps the list for a plain scroll area that builds a label for every row while `plain on` shows.
- `vlist-follow` keeps the view at the bottom as rows are added, while `follow on` shows and the view is at the bottom.
- `vlist-anchor` keeps the first visible row in place when rows are added below it.
- `vlist-reuse` shows a new row in a slot that scrolls out of sight, so the entity count stays flat as the list scrolls.
- `vlist-thumb` scrolls the list by a drag on the scrollbar thumb.

## How to get to it (user POV)

- Run `cargo run -p bevy_rubrix --example gallery` and click `virtual_list` in the page list.
- Click a button in the row above the list: `10,000 rows`, `100,000 rows`, `append 1,000`, `plain off`, or `follow off`.
- Turn the wheel over the list, or drag the scrollbar thumb at its right edge.

## Driving it with launch.sh

Preconditions: `doctor.sh` is clean and the out dir is empty. For a script that writes a frame CSV, also build in release, set `RUBRIX_NO_VSYNC=1`, and run it alone with no other Bevy window or build running.

- **Open.** Run `launch.sh crates/bevy_rubrix/examples/gallery/drive/virtual-list.drive <out>`. `log rows` prints `rows first_visible 0 spawned 0..=<b> rows <slots> total 10000 following false x 0`, and `log entities` prints a number. `<out>/virtual-list.png` shows the buttons and the list from `Row 0`, with its second and third lines under it.
- **Scroll to the end.** Run `.../virtual-list-end.drive`. It clicks `100,000 rows` and scrolls 2,000,000 px in one step. `accept: rows pass` follows `expect rows ..=99999`, and `virtual-list-end.png` shows the last rows, ending in `Row 99999`.
- **Keep the first row when rows are added.** Run `.../virtual-list-not-pinned.drive`. It turns follow on and scrolls 85,000 px. `log rows` prints `first_visible 4996` and `total 10000 following false`. After `append 1,000` it prints `first_visible 4996` and `total 11000`. All four `accept: rows` lines say `pass`, and `f2-not-pinned.png` shows rows from `Row 4996`.
- **Follow the bottom.** Run `.../virtual-list-pinned.drive`. It turns follow on and scrolls to the bottom, and `log rows` prints `following true`. After `append 1,000` it prints a `spawned` range ending in `..=10999` and `total 11000 following true`. All three `accept: rows` lines say `pass`, and `virtual-list-pinned.png` shows the last rows, ending in `Row 10999`.
- **Land a scroll on a row.** Run `.../virtual-list-anchor.drive`. It scrolls 85,086 px in one step. `log rows` prints `first_visible 5001` right after the scroll step, with no wait, and again 10 frames later. Both `accept: rows` lines say `pass`, and `virtual-list-anchor.png` shows `Row 5001` at the top.
- **Jump.** Run `.../virtual-list-jump.drive`. It scrolls 20,000 px in one step and takes `virtual-list-jump.png` after `wait 0`. `log rows` prints a `first_visible` past 1,000, and the shot shows rows from that index.
- **Drag the thumb.** Run `.../virtual-list-thumb.drive`. It clicks `100,000 rows`, moves onto the bar at 1255 107, waits 20 frames, and drags the thumb to 1255 437. `log rows` prints `total 100000` and a `first_visible` far past 0, and `virtual-list-thumb.png` shows rows from that index.
- **Record the jump and the drag.** Run `.../virtual-list-jump-record.drive` or `.../virtual-list-thumb-record.drive`. Each records the scroll or the drag and the 8 frames after it into `<out>/jump` or `<out>/thumb`. Both run at 10,000 rows. Turn a directory into a video with `video.sh <out>/jump <out>/jump.mp4`. Look at each frame for a blank band or a row cut short.
- **Reuse slots.** Run `.../virtual-list-entities.drive`. It prints `log entities` 31 times, once before each of 30 runs of 100 `scroll 0 -58` steps and once at the end. The counts stay flat and do not grow with the distance scrolled. `virtual-list-entities.png` shows rows near the end of the list.
- **Resize.** Run `.../virtual-list-resize.drive`. It scrolls 85,000 px, then resizes to 800 by 600. It prints `drive: resize ok`, `virtual-list-resize.png` is 800 by 600 and shows rows, and `log rows` prints `total 10000`.
- **Time a scroll.** Run `.../virtual-list-scroll.drive`, `.../virtual-list-scroll-100k.drive`, `.../virtual-list-fast.drive`, or `.../virtual-list-fast-100k.drive`. The `scroll` scripts send 600 steps of `scroll 0 -18`. The `fast` scripts send six legs of 100 steps of 782 px, down then up in turn. The `100k` scripts click `100,000 rows` first. Each writes `<name>.csv` and `<name>.png`, then prints `log rows` and `log entities`. `frame-stats.sh <out>/<name>.csv` prints `median=`, `p95=` and `max=`.
- **Time the plain scroll area.** Run `.../virtual-list-scroll-plain.drive` or `.../virtual-list-scroll-plain-100k.drive`. They click `plain off`, so the button reads `plain on`, and send the same 600 steps. `log rows` prints `rows none`, because a plain scroll area is not a virtual list. Compare their CSVs with the `virtual-list-scroll` ones.
- **Time the first paint.** Run `.../virtual-list-first-paint.drive` or `.../virtual-list-first-paint-100k.drive`. Each starts `frames start` on another page, opens `virtual_list`, and stops the frames 120 frames later. The `100k` script sets `100,000 rows` and leaves the page first, so `log rows` prints `total 100000` when it comes back. The 10,000-row script prints `total 10000`. Each writes `<name>.csv` and `<name>.png`, which shows the list from `Row 0`.

## Gotchas

- The list builds at most 199 rows. It cuts its overscan first, the 1,000 px above and below the view that this page leaves at the default. A view more than 199 rows high shows a blank band below them.
- `follow on` does not scroll. It keeps the view at the bottom only once the view is there. `log rows` prints `following true` only while follow is on and the view is at the bottom. A scroll up stops it, and a scroll back to the bottom starts it again.
- Every 100th row, from `Row 0`, is three lines, about 51 px tall once laid out. Until then the list counts it as 17 px. On the frame a row first shows, it shows only 17 px of itself, so a three-line row can show one line in a jump shot or in the first record frames.
- The anchor distance is arithmetic in `gen_drives.py`. 85,086 px is 5,001 rows of 17 px, plus 34 px for each of the two three-line rows laid out before the scroll, plus 1 px. The 85,000 px scroll lands on `first_visible 4996` for the same reason. A change to the row height, the row text, or the font breaks both numbers.
- A change in the list's width forgets the heights of rows out of sight. After `resize` each three-line row out of sight counts as 17 px again until it shows.
- The button row wraps. The y 68 positions hold only at 1280 by 800. At 800 by 600 the buttons move.
- The row count, `plain`, and `follow` stay set when you leave the page and come back in the same run.
- `log rows` reads the first virtual list in the world. With `plain on`, or on a page without a list, it prints `rows none`.
- The virtual-list scripts open the page with `page virtual_list`. All but the two record scripts and the two first-paint scripts wait 30 frames, not 120, before the first screenshot. The highlight on the `virtual_list` entry can be between colours in their shots. Judge the rows, not the page list.
- Drag the thumb after a `move` onto the bar and a wait, as `virtual-list-thumb.drive` does with `move 1255 107` and `wait 20`.
- `gen_drives.py` writes every virtual-list script except `virtual-list.drive`. Edit the generator and run it again, not the generated files.
