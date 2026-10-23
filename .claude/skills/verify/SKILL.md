---
name: verify
description: Launch the rubrix gallery (a Bevy window), drive it with a script, and capture screenshots, frame times and video as proof. Use to see a bevy_rubrix change working, to confirm a gallery page behaves, or to take review evidence. Linux with Hyprland only.
---

# Verify rubrix in the gallery

The gallery is `cargo run -p bevy_rubrix --example gallery`, a 1280 by 800 window with a page list on the left and the selected page on the right. You never touch it with the OS. A drive script, named by `RUBRIX_DRIVE`, puts clicks, keys and scrolls into the app as Bevy messages, and reads state back through `log`. Hyprland gives the window no focus, so an OS pointer warp or a typed key never arrives.

Feature recipes are in `features/README.md`. Read the one for the page you are checking before you drive it.

## Launch

```sh
.claude/skills/verify/scripts/launch.sh <drive script> <out dir>
```

It builds the gallery in release, runs it from `<out dir>`, floats and places its window at 1280 by 800 with Hyprland's Lua dispatchers, waits for the script to finish, prints the gallery's log, and exits with the gallery's exit code. Relative paths in a script, such as `screenshot shot.png`, land in `<out dir>`. The gallery's pid is in `<out dir>/pid` while it runs.

- Ready means `drive: exit ok` and `exit=0`. A script ends with `exit`. Without it the window stays open.
- Build a shared target directory per head: `export CARGO_TARGET_DIR=$HOME/.cache/rubrix-target/<head SHA>`. Never put it under `/tmp`.
- A scripted run presents without vsync. An unfocused window under vsync ran at 1 frame per second here (1024 ms per frame, measured), and its screenshots came out black. Set `RUBRIX_NO_VSYNC=0` only to prove the vsync default, and expect slow, black frames.
- Two galleries can run at once. Give each its own drive script and out dir.
- `TIMEOUT=<seconds>` (default 300) kills a script that hangs.

## Doctor

```sh
.claude/skills/verify/scripts/doctor.sh
```

Read-only. It checks cargo, jq, hyprctl, ffmpeg, ffprobe, a running Hyprland session, rustc at the pinned version, and lists the galleries running now. Run it first when a launch looks wrong. A line starting `MISSING` is the fix to make.

## Drive

Scripts for each check are in `crates/bevy_rubrix/examples/gallery/drive/`. One step per line, and a bad line makes the gallery exit with code 2 and print `drive: line <n>: <reason>`.

```
page <name>        wait <frames>        exit
move <x> <y>       click <x> <y> [double|triple]       drag <x0> <y0> <x1> <y1>
scroll <dx> <dy>   key <combo>          type <text>
screenshot <path>  log <topic>          expect <topic> <text>        clipboard
frames start       frames stop <csv>    record start <dir>           record stop
resize <w> <h>     open <path>...
```

- Positions are logical pixels. `scroll 0 -400` scrolls the page down. The pointer starts in the middle of the page area, at 740 400.
- Stable handles. The page list entry `about` is at 100 62. The theme toggle is at 100 771.
- `log` topics are `entities`, `page`, `present_mode`, `theme`. Each prints one line such as `page about`. `expect page about` prints `accept: page pass` or `accept: page fail`.
- `clipboard` prints what this gallery last copied, never the shared system clipboard.
- Each step prints `drive: <step> ok`. Wait for every one, and for `screenshot` the file exists when it prints.
- Never warp the OS pointer or use wtype. They act on the focused window, and the gallery has none.

Example, the smoke script:

```sh
export CARGO_TARGET_DIR=$HOME/.cache/rubrix-target/$(git rev-parse HEAD)
.claude/skills/verify/scripts/launch.sh crates/bevy_rubrix/examples/gallery/drive/smoke.drive /tmp/swarm-g1/worker-1
```

## Evidence

- A screenshot with `screenshot <path>`, taken by Bevy's own API from inside the app. Look at it. A black frame means nothing rendered yet, so add a `wait`.
- A state line with `log <topic>` next to the screenshot, so the image and the state agree.
- Frame times: `frames start`, the scroll steps, `frames stop frames.csv`, then `.claude/skills/verify/scripts/frame-stats.sh <out dir>/frames.csv`. It drops the first 60 frames and prints the median, p95 and max in milliseconds. Run perf with `RUBRIX_NO_VSYNC=1` in `--release`, alone, with no other Bevy window or build running.
- Video: `record start <dir>`, the steps, `record stop`, then `.claude/skills/verify/scripts/video.sh <dir> <out.mp4> [fps]`. One PNG is saved per frame, so the video plays at the fps you pass (default 60), not at the speed of the capture. `ffprobe -count_frames` reports the frame count.
- Proof standard. Drive the real path (a `click` on the page list, not a resource write), capture the action and the state after it, and keep evidence outside the checkout. Do not weaken a script to make it pass.

## Cleanup

```sh
.claude/skills/verify/scripts/cleanup.sh <out dir>...
```

It stops the gallery whose pid `launch.sh` wrote into each out dir, only if that pid is still a gallery, and removes the pid file. It never touches screenshots, CSVs, videos or logs. Never `pkill gallery`: another lane may own that process. After a run, `pgrep -x gallery` should list none of yours.
