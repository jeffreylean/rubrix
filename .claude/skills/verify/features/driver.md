# Drive scripts

A drive script is a text file of steps that the gallery runs one per line, with input injected as Bevy messages. This file is the recipe for checking the driver itself.

## Sub-features

- `drive-parse` rejects a bad line with exit code 2 and its number.
- `drive-input` runs `click`, `scroll`, `key` and `type` and prints `ok` for each.
- `drive-frames` writes a frame-time CSV that `frame-stats.sh` summarises.
- `drive-record` writes one PNG per frame that `video.sh` turns into an mp4.
- `drive-vsync` prints `AutoNoVsync` when `RUBRIX_NO_VSYNC=1`, and exits with code 2 for any value but `0` or `1`.
- `drive-resize` changes the window size through Hyprland and exits with code 2 when the window keeps its size.

## How to get to it (user POV)

- Set `RUBRIX_DRIVE=<script>` and run the gallery, or call `launch.sh`.

## Driving it with launch.sh

- **Bad line.** Run `.../bad-script.drive`. The log says `drive: line 3: unknown step` and `exit=2`. No window opens.
- **Keys.** Run `.../keys.drive`. Every step prints `ok`, and `clipboard` prints `clipboard: (nothing copied)` because no page copies text yet.
- **Frames.** Run `.../frames.drive`, then `frame-stats.sh <out>/frames.csv`. It prints `median=`, `p95=` and `max=`.
- **Record.** Run `.../record.drive`, then `video.sh <out>/video <out>/video.mp4`. `ffprobe -count_frames` reports at least 60 frames.
- **No vsync.** Run `RUBRIX_NO_VSYNC=1 launch.sh .../novsync.drive <out>`. It prints `present_mode requested AutoNoVsync`. With `RUBRIX_NO_VSYNC=true` the gallery prints `RUBRIX_NO_VSYNC must be unset, 0 or 1` and exits with code 2.
- **Screenshot while recording.** Run `.../recshot.drive`. It prints `drive: exit ok` and `exit=0`, and `<out>/inrec.png` exists next to the frames in `<out>/video`.
- **Resize.** Run `.../resize.drive`. It prints `drive: resize ok`, and `resize.png` is 800 by 600.

## Gotchas

- `open` on a page that takes no files exits with code 2.
- `frames stop` before `frames start` exits with code 2.
- The `frames.drive` script scrolls down 100 frames and up 100 frames, three times, so every one of its 600 frames moves the content.
