# Stream page

The stream page streams a file into `code_view` through `CodeSource::streaming`, a chunk of a chosen size each frame, and follows the newest line. R13 ships it. It opens on `bench/fixtures/rust/large.rs.txt`, 313,933 bytes in 10,372 lines, at 64 bytes a frame.

## Sub-features

- `stream-grow` adds each line as its `\n` arrives and colours it then. The unfinished last line shows with the colours it would have if the text ended there.
- `stream-follow` keeps the newest line at the bottom while the view is at the bottom, stops when you scroll up, and starts again when you scroll back to the bottom, by trackpad or by wheel.
- `stream-rows` changes only the rows whose text changed. A committed row keeps its entities.
- `stream-controls` sets the rate, pauses, resumes, restarts, and streams the 100,000-line input.
- `stream-open` streams a file given by `open <path>`.

## How to get to it (user POV)

- Click `stream` in the page list. It streams only while it shows.
- The buttons at y 68 are `1 B` (290), `64 B` (438), `4 KiB` (586), `64 KiB` (734), `pause` or `resume` (882), `restart` (1030), and `100k lines` (1178). A rate button sets the rate from the next frame. `restart` starts again from the first byte at the same rate, paused if the page was paused, with an empty view. `100k lines` starts the 100,000-line input at the same rate.
- The label under the buttons names the file, the bytes sent, and the rate.

## Driving it with launch.sh

The scripts are `drive/stream-*.drive`, written by `gen_drives.py`. Each opens the page and waits 120 frames.

- **Final screen and perf (lane 1).** `stream-final.drive` streams at 4 KiB a frame into `stream-4k.csv`, then restarts ten times at 64 KiB a frame into `stream-64k.csv`, saves `r13-final.png`, then shows the `code` page scrolled to its end and saves `r13-final-code.png`. Both `log rows` lines print `first_visible 10333`.
- **Mid-stream (lane 2).** `stream-mid.drive`. The two `log stream` lines show `committed` growing, and `expect rows following true` passes.
- **Scrolled up (lane 3).** `stream-scrolled-up.drive` scrolls up 600 px. `expect rows following false` passes, and `first_visible` is the same before and after 200 more frames while `committed` grows.
- **Refollow (lane 4).** `stream-refollow.drive` then turns the wheel 400 lines down with `wheel 0 -400`. The wheel eases to the bottom in about 0.6 s of wall time, which is about 400 frames at 1.4 ms a frame without vsync, so `wait 3000 until rows following true` waits until the list follows, whatever the frame rate. Its 3,000 frames end while the stream still sends, so a list that follows again only once the stream stops fails it. `expect rows following true` passes and `first_visible` keeps growing.
- **Open string (lane 5).** `stream-open-string.drive` opens `fixtures/open-string.rs`, whose last line is `let s = "open`. `expect runs 3 string` passes.
- **Closed string (lane 6).** `stream-closed-string.drive` streams `fixtures/closed-string.rs` at 1 byte a frame. `log runs 2` shows `let ` in the keyword colour, and `expect runs 2 keyword` passes.
- **Respawns (lane 7).** `stream-respawn.drive` logs `stream` every 100 frames for 1,200 frames at 64 bytes a frame, past 2,000 lines.
- **Restart (lane 8).** `stream-restart.drive` pauses and restarts, so `r13-restart-empty.png` shows an empty view and `log stream` prints `committed 0`, then restarts twice more, logging `entities` each time.
- **100,000 lines (lane 9).** `stream-stress.drive` streams the 100,000-line input at 64 KiB a frame. `expect rows total 100000` passes and `r13-stress.png` shows `100000` in the last gutter row.
- **CRLF (lane 10).** `stream-crlf.drive` streams `fixtures/crlf.rs`, the first 300 lines of `large.rs.txt` with CRLF endings, at 1 byte a frame for 2,000 frames. For the whole file, write `sed 's/$/\r/' bench/fixtures/rust/large.rs.txt > <out>/large-crlf.rs` and a copy of the script that opens it.

## Log topic

`log stream` prints `stream committed <n> provisional "<line>" respawned <n> sent <bytes>/<bytes>`. It reads the first code view on the page. `committed` counts the lines whose runs are final, `provisional` is the unfinished last line or `none`, `respawned` counts row text entities spawned since the last `log stream`, and `sent` is the bytes the page has sent of the file. A slot that shows another row is not respawned, so `respawned` counts only rows that enter the view while the list grows its rows to fill it.

## Perf

Run `stream-final.drive` with `RUBRIX_NO_VSYNC=1` in `--release`, alone, five times, then `frame-stats.sh` on `stream-4k.csv` and `stream-64k.csv`. Each starts with a 60-frame wait that `frame-stats.sh` drops.

## Gotchas

- A streaming view has no rows before its first byte, so it starts at its bottom and follows text that arrives faster than a view a frame.
- A streaming list builds no rows outside its view (an overscan of 0), because at 4 KiB a frame every row it builds is laid out again each frame.
- The gutter widens when the line count gains a digit, which moves the text right by one cell.
- `restart` and `100k lines` build a new list. The old list's entities go.
