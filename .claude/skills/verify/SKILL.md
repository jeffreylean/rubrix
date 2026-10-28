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

It creates `<out dir>` if it is missing, builds the gallery in release, runs it from `<out dir>`, floats and places its window at 1280 by 800 with Hyprland's Lua dispatchers, waits for the script to finish, prints the gallery's log, and exits with the gallery's exit code. Relative paths in a script, such as `screenshot shot.png`, land in `<out dir>`. The gallery's pid is in `<out dir>/pid` while it runs.

- Ready means `drive: exit ok` and `exit=0`. A script ends with `exit`. Without it the window stays open.
- Build a shared target directory per head: `export CARGO_TARGET_DIR=$HOME/.cache/rubrix-target/<head SHA>`. Never put it under `/tmp`.
- A scripted run presents without vsync. An unfocused window under vsync ran at 1 frame per second here (1024 ms per frame, measured), and its screenshots came out black. Set `RUBRIX_NO_VSYNC=0` only to prove the vsync default, and expect slow, black frames. `RUBRIX_NO_VSYNC` takes only `0` or `1`. Any other value exits with code 2 and a message.
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
move <x> <y>       click <x> <y> [double|triple|shift] drag <x0> <y0> <x1> <y1>
press <x> <y>      release              scroll <dx> <dy>     wheel <dx> <dy>
key <combo>        type <text>
screenshot <path>  log <topic>          expect <topic> <text>        clipboard
frames start       frames stop <csv>    record start <dir>           record stop
resize <w> <h>     open <path>...
```

- Positions are logical pixels. `scroll 0 -400` scrolls the page down. `scroll` turns in pixels, as a trackpad does, and lands at once. `wheel 0 -3` turns 3 notches of a mouse wheel, which ease there.
- `press` moves the pointer and holds the button down until `release`, so a script can `move` and `wait` while it is held. `click <x> <y> shift` holds shift for the click. The pointer starts in the middle of the page area, at 740 400.
- A `scroll` goes to whatever is under the pointer. After a `click` on the page list or the theme toggle the pointer stays there, and a `scroll` does nothing until a `move` puts it back on the page, such as `move 740 400`.
- After a `page` change or a click on the page list, `wait 120` before a `screenshot`. The highlight on the list entry fades between colors, and a shot taken 10 to 30 frames in shows the colors in between. Wait 30 frames before the first screenshot of a run, since the first frames can be black.
- Stable handles, at 1280 by 800. The page list entry `about` is at 100 62. The theme toggle is at 100 771. They move when `resize` changes the window.
- `resize <w> <h>` asks Hyprland for the size, and prints `ok` once the window reports it. Without Hyprland, or when the window does not take the size within 2 seconds, it exits with code 2 and names the size the window has.
- The theme toggle builds the page again and puts each scroll offset back, so a page keeps its scroll through a toggle.
- `log` topics are `entities`, `page`, `present_mode`, `scroll`, `theme`, `rows`, `spans`, `hit`, `runs <line>`, `stream`, and `selection`. Each prints one line such as `page about`.
  - `present_mode` prints the mode the window asked for, as `present_mode requested AutoNoVsync`, not the mode the driver settled on.
  - `scroll` prints the offset of the first scroll area on the current page, in pixels.
  - `rows` reads the first virtual list: `rows first_visible <n> spawned <a>..=<b> rows <slots> total <count> following <bool> x <px>`, where `x` is its sideways offset. It prints `rows empty total 0` for a list with no rows and `rows none` with no list.
  - `spans` lists the spans of the rich page's long line, `spans <n>: <id> "<text>" <hex>, ...`. `hit` prints the byte and character under the last press on its hit line, `hit 15 'ö'`, or `hit none`.
  - `stream` and `selection` read the first code view, as `features/stream.md` and `features/select.md` describe.
  - `runs <line>` reads the code page's line, counted from 1: `runs <line>: <a>..<b> <kind>, ... | shown "<text>" <hex>, ...`. The part before `|` is the runs the line was lexed into, in bytes of the line, or `empty` for an empty line. The part after is each span its row shows, or `not shown` while no row is built for it. Past the last line it prints `runs <line> none`.
- `expect <topic> <text>` prints `accept: <topic> pass` when the topic's line holds the text, else `accept: <topic> fail`. A topic that takes a line number takes it before the text: `expect runs 75 comment` checks `runs 75` for `comment`.
- `open <path>` gives a file to the page on show. A relative path is read from the drive script's directory, so `open ../fixtures/notes.xyz` in a script in `drive/` opens `examples/gallery/fixtures/notes.xyz`. It exits with code 2 when the page takes no files or cannot read the file.
- `clipboard` prints what this gallery last copied, never the shared system clipboard.
- Each step prints `drive: <step> ok`. Wait for every one, and for `screenshot` the file exists when it prints.
- Never warp the OS pointer or use wtype. They act on the focused window, and the gallery has none.

Example, the smoke script:

```sh
export CARGO_TARGET_DIR=$HOME/.cache/rubrix-target/$(git rev-parse HEAD)
out=$(mktemp -d)
.claude/skills/verify/scripts/launch.sh crates/bevy_rubrix/examples/gallery/drive/smoke.drive "$out"
```

`$out` is an empty directory only this run writes. Two runs that share one directory overwrite each other's screenshots and pid file.

## Evidence

- A screenshot with `screenshot <path>`, taken by Bevy's own API from inside the app. Look at it. A black frame means nothing rendered yet, so add a `wait`. Every script in `drive/` waits 30 frames before its first screenshot, and a test fails if one does not.
- A state line with `log <topic>` next to the screenshot, so the image and the state agree.
- Frame times: `frames start`, the scroll steps, `frames stop frames.csv`, then `.claude/skills/verify/scripts/frame-stats.sh <out dir>/frames.csv`. It drops the first 60 frames and prints the median, p95 and max in milliseconds. Run perf with `RUBRIX_NO_VSYNC=1` in `--release`, alone, with no other Bevy window or build running.
- Video: `record start <dir>`, the steps, `record stop`, then `.claude/skills/verify/scripts/video.sh <dir> <out.mp4> [fps]`. One PNG is saved per frame, so the video plays at the fps you pass (default 60), not at the speed of the capture. A `screenshot` and an `exit` finish while a record runs, and `exit` stops it. A `screenshot` step saves its image as that frame's record frame too, so the video loses no frame to it. `ffprobe -count_frames` reports the frame count.
- Proof standard. Drive the real path (a `click` on the page list, not a resource write), capture the action and the state after it, and keep evidence outside the checkout. Do not weaken a script to make it pass.

## Cleanup

```sh
.claude/skills/verify/scripts/cleanup.sh <out dir>...
```

It stops the gallery whose pid `launch.sh` wrote into each out dir, only if that pid is still a gallery, and removes the pid file. It never touches screenshots, CSVs, videos or logs. A `launch.sh` still waiting on a gallery that cleanup stops prints `exit=143` and exits with 143, the code of a process ended by SIGTERM. The drive script had left the gallery open, so this is not a failed run. Never `pkill gallery`: another lane may own that process. After a run, `pgrep -x gallery` should list none of yours.
