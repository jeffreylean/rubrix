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
- `drive-mod` turns `mod` into Ctrl on Linux and Cmd on macOS in `key` combos, and `click` and `drag` hold `shift`, `mod` and `alt` for the gesture.
- `drive-ime` writes `Ime` messages, and the `ime` topic names the last one the gallery read.
- `drive-paste` hands its text to the gallery's `Paste` resource and sends `mod+v`, never through the shared clipboard.
- `drive-page-topics` asks the page on show for `caret`, `text <line>` and `undo`, and prints `n/a` for a page without an answer.
- `drive-summary` counts every `expect` and ends with `drive: <passed> of <checks> checks pass` and exit code 0 or 1.
- `drive-wtype` runs `wtype` only under `launch.sh --ime-live`.
- `run-all` runs every saved script and prints one row per script.

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
- **Mod chord.** Run `.../driver-mod.drive`. `mod+a` selects the code page and `mod+c` copies it, so `expect copied` matches the first line of `large.rs.txt`, and `g2-mod.png` shows the whole view selected. `mod_holds_ctrl_on_linux_and_cmd_on_macos` covers the macOS key on Linux.
- **Input method.** Run `.../driver-ime.drive`. Its five `expect ime` lines name `enabled`, `preedit "你好" 0..6`, `commit "你好"`, `preedit ""`, and `disabled`, each with `page n/a`. `.../driver-ime-bad.drive` exits with code 2 and `drive: line 3: preedit cursor 2..9 is past the 3 bytes of "abc"`.
- **Paste.** Run `.../driver-paste.drive`. `expect paste` matches `"hello\nworld"`, and `copied` still names the line the script copied before the paste. Run `driver-paste-a.drive` and `driver-paste-b.drive` at once in two out dirs. Every `log paste` names only that gallery's text, and `wl-paste` prints the same text before and after.
- **Page topics.** Run `.../driver-topics.drive`. Its nine `expect` lines match `n/a` on the `code`, `diff` and `virtual_list` pages.
- **Summary.** Run `.../driver-summary.drive`. It ends with `drive: 3 of 3 checks pass`, `drive: exit ok` and `exit=0`. `.../driver-summary-fail.drive` ends with `drive: 2 of 3 checks pass`, `drive: exit failed` and `exit=1`.
- **wtype guard.** Run `.../driver-wtype.drive` without `--ime-live`. It exits with code 2 and `drive: line 3: wtype needs launch.sh --ime-live`.
- **Run all.** Run `scripts/run-all.sh <out dir>`. It ends with `run-all: <n> of <n> scripts pass, <k> skipped`, and each must-fail script exits with the code its table row names.

## Gotchas

- `open` on a page that takes no files exits with code 2. So does an `open` the page cannot read, with `drive: line <n>: cannot read <path>: <error>`.
- `open` reads a relative path from the drive script's directory, not from the out dir the gallery runs in. Fixture files live in `examples/gallery/fixtures/`, not in `drive/`, because a test reads every file in `drive/` as a script.
- `expect` takes a topic's own words before the text. `expect runs 75 comment` checks topic `runs 75` for the text `comment`. `log runs` with no number is a parse error.
- `frames stop` before `frames start` exits with code 2.
- A failed `expect` makes the gallery exit with code 1 after the summary line, so `run-all.sh` lists the script as failed.
- `ime enabled false` in the `ime` line is the window's own request, which the app sets. An `ime enable` step writes the message an input method sends and does not change it, so read the `last` part.
- Under `--ime-live`, wait before the first `wtype` step. Hyprland focuses the window a moment after it opens, and a `wtype` step that runs before then exits with code 2.

- The `frames.drive` script scrolls down 100 frames and up 100 frames, three times, so every one of its 600 frames moves the content.
