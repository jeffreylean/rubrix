# Drive scripts

A drive script is a text file of steps that the gallery runs one per line, with input injected as Bevy messages. This file is the recipe for checking the driver itself.

## Sub-features

- `drive-parse` rejects a bad line with exit code 2 and its number.
- `drive-input` runs `click`, `scroll`, `key` and `type` and prints `ok` for each.
- `drive-hold` holds the button from `press` to `release`, holds shift for `click <x> <y> shift`, and turns a notched wheel for `wheel <dx> <dy>`.
- `drive-frames` writes a frame-time CSV that `frame-stats.sh` summarises.
- `drive-record` writes one PNG per frame that `video.sh` turns into an mp4.
- `drive-vsync` prints `AutoNoVsync` when `RUBRIX_NO_VSYNC=1`, and exits with code 2 for any value but `0` or `1`.
- `drive-resize` changes the window size through Hyprland and exits with code 2 when the window keeps its size.
- `drive-topics` parses each `log` and `expect` topic with the words it takes. Every topic is one word but `runs`, which takes a line number.
- `drive-open` reads a relative `open` path from the script's directory and exits with code 2 when the page cannot show the file.

## How to get to it (user POV)

- Set `RUBRIX_DRIVE=<script>` and run the gallery, or call `launch.sh`.

## Driving it with launch.sh

- **Bad line.** Run `.../bad-script.drive`. The log says `drive: line 3: unknown step` and `exit=2`. No window opens.
- **Keys.** Run `.../keys.drive`. Every step prints `ok`, and `clipboard` prints `clipboard: (nothing copied)` because the `about` page has no text to copy.
- **Hold and wheel.** Run `cargo test -p bevy_rubrix --example gallery drive::tests`. `every_step_parses` and `a_wheel_turns_in_lines_and_eases_there` pass. `select-autoscroll.drive` and `select-extend.drive` drive `press`, `release` and `click shift` on the code page.
- **Frames.** Run `.../frames.drive`, then `frame-stats.sh <out>/frames.csv`. It prints `median=`, `p95=` and `max=`.
- **Record.** Run `.../record.drive`, then `video.sh <out>/video <out>/video.mp4`. `ffprobe -count_frames` reports at least 60 frames.
- **No vsync.** Run `RUBRIX_NO_VSYNC=1 launch.sh .../novsync.drive <out>`. It prints `present_mode requested AutoNoVsync`. With `RUBRIX_NO_VSYNC=true` the gallery prints `RUBRIX_NO_VSYNC must be unset, 0 or 1` and exits with code 2.
- **Screenshot while recording.** Run `.../recshot.drive`. It prints `drive: exit ok` and `exit=0`, and `<out>/inrec.png` exists next to the frames in `<out>/video`. With `frames start` and `frames stop` around the record, the PNG count trails the frame count by the same two frames as in a record with no screenshot.
- **Resize.** Run `.../resize.drive`. It prints `drive: resize ok`, and `resize.png` is 800 by 600.

- **Topics and open.** Run `cargo test -p bevy_rubrix --example gallery drive::tests`. `log_runs_parses_its_line`, `expect_runs_takes_the_text_after_the_line`, `log_runs_without_a_number_is_refused`, and `an_open_path_is_read_from_the_script_directory` pass.

## Gotchas

- `open` on a page that takes no files exits with code 2. So does an `open` the page cannot read, with `drive: line <n>: cannot read <path>: <error>`.
- `open` reads a relative path from the drive script's directory, not from the out dir the gallery runs in. Fixture files live in `examples/gallery/fixtures/`, not in `drive/`, because a test reads every file in `drive/` as a script.
- `expect` takes a topic's own words before the text. `expect runs 75 comment` checks topic `runs 75` for the text `comment`. `log runs` with no number is a parse error.
- `frames stop` before `frames start` exits with code 2.
- The `frames.drive` script scrolls down 100 frames and up 100 frames, three times, so every one of its 600 frames moves the content.
