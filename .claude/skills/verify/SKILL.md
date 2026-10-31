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
- `exit` prints `drive: <passed> of <checks> checks pass` for the script's `expect` steps. When one failed it prints `drive: exit failed` and the gallery exits with code 1. A bad line or a step that cannot run exits with code 2.
- Build a shared target directory per head: `export CARGO_TARGET_DIR=$HOME/.cache/rubrix-target/<head SHA>`. Never put it under `/tmp`.
- A scripted run presents without vsync. An unfocused window under vsync ran at 1 frame per second here (1024 ms per frame, measured), and its screenshots came out black. Set `RUBRIX_NO_VSYNC=0` only to prove the vsync default, and expect slow, black frames. `RUBRIX_NO_VSYNC` takes only `0` or `1`. Any other value exits with code 2 and a message.
- Two galleries can run at once. Give each its own drive script and out dir.
- `TIMEOUT=<seconds>` (default 300) kills a script that hangs.
- `RUBRIX_SCALE=<factor>` draws the gallery at that scale factor, at the same 1280 by 800 logical size, and launch.sh resizes the window to match. `RUBRIX_SCALE=2` runs a script at a Mac's Retina scale on Linux: positions, `expect` lines and `log` values stay in logical pixels, and screenshots come out 2560 by 1600. Any value that is not a positive number exits with code 2.
- `launch.sh --ime-live <drive script> <out dir>` also focuses the gallery window through Hyprland once it exists, so `wtype` steps and a real input method reach it. It warns that the run takes keyboard focus from the desktop, and gives focus back to the window that had it when the gallery exits. Only this mode runs `wtype` steps. Hyprland does not move focus while the display sleeps, and launch.sh then warns that a `wtype` step will exit with code 2.

## Run every script

```sh
.claude/skills/verify/scripts/run-all.sh <out dir>
```

It runs every `drive/*.drive` through `launch.sh`, one at a time, and every replay script in `crates/rubrix_edit/tests/replay/` through `rubrix_edit`'s `examples/replay.rs` once that directory exists. It prints one row per script, `pass`, `fail`, or `skip`, with the exit code and the check count, and ends with `run-all: <passed> of <ran> scripts pass, <skipped> skipped`. It exits 0 only when every script that ran passed. The table at the top of the script lists the scripts that must fail with a given exit code and the ones it skips on macOS, on Linux, or everywhere, each with its reason. A PR that adds such a script adds its row. A second table, `again`, names scripts that run once more with an environment variable, such as `editor-mouse.drive` at `RUBRIX_SCALE=2`; each such run is its own row, `<script>@<VAR>=<value>`. Each script's log is `<out dir>/<script>.txt`.

## Acceptance

```sh
.claude/skills/verify/scripts/acceptance.sh <out dir>
```

It runs `drive/acceptance.drive` through `launch.sh`, which visits the Hamlet pages and the stress page and checks each pass condition with `expect`. It prints one `accept:` line per check, the frame stats of each CSV the script writes, and `acceptance: <n> of <n> checks pass`. Two of the checks compare the stress views' p95 frame times with 3.0 ms, which a loaded machine fails, so run it alone. It exits 0 only when the gallery exits 0 and every check passes. `features/hamlet.md` lists the checks.

## Doctor

```sh
.claude/skills/verify/scripts/doctor.sh
```

Read-only. It checks cargo, jq, hyprctl, ffmpeg, ffprobe, a running Hyprland session, rustc at the pinned version, and lists the galleries running now. Run it first when a launch looks wrong. A line starting `MISSING` is the fix to make. It also prints `wtype`, a running `fcitx5`, and the `fcitx5-chinese-addons` package as `ok` or as an `info` line, since only `launch.sh --ime-live` with a real input method needs them.

## Drive

Scripts for each check are in `crates/bevy_rubrix/examples/gallery/drive/`. One step per line, and a bad line makes the gallery exit with code 2 and print `drive: line <n>: <reason>`.

```
page <name>        wait <frames>        exit
move <x> <y>       click <x> <y> [double|triple] [shift] [mod] [alt]
drag <x0> <y0> <x1> <y1> [shift] [mod] [alt]
press <x> <y>      release              scroll <dx> <dy>     wheel <dx> <dy>
key <combo>        type <text>          paste <text>         wtype <text>
ime enable         ime disable          ime preedit [<text> [<start> <end>]]    ime commit <text>
screenshot <path>  log <topic>          expect <topic> <text>        clipboard
frames start       frames stop <csv>    record start <dir>           record stop
resize <w> <h>     open <path>...
```

- Positions are logical pixels. `scroll 0 -400` scrolls the page down. `scroll` turns in pixels, as a trackpad does, and lands at once. `wheel 0 -3` turns 3 notches of a mouse wheel, which ease there.
- `press` moves the pointer and holds the button down until `release`, so a script can `move` and `wait` while it is held. The pointer starts in the middle of the page area, at 740 400.
- `mod` is the command key, Ctrl on Linux and Cmd on macOS, so one script runs on both. Write `key mod+c`, not `key ctrl+c`, for a command. `key` combos take `mod`, `ctrl`, `alt`, `super` and `shift`.
- `click` and `drag` hold `shift`, `mod` and `alt` for the whole gesture, each named once after the points, as in `click 300 200 mod` or `drag 300 200 500 260 alt`. A count, `double` or `triple`, comes before them.
- `ime` writes the `Ime` messages an input method sends: `ime enable`, `ime disable`, `ime preedit <text> <start> <end>` with the cursor in bytes of the text, `ime preedit <text>` with no cursor, `ime preedit` alone for the empty preedit that cancels, and `ime commit <text>`. The last two words of a preedit are its cursor when both are numbers. A cursor past the text or inside a character is a bad line.
- `paste <text>` puts the text in the gallery's own `Paste` resource and sends `mod+v`. `\n`, `\t` and `\\` stand for a newline, a tab and a backslash. It never writes the shared Wayland clipboard, so two galleries never read each other's paste.
- `wtype <text>` types through the OS with `wtype`. It runs only under `launch.sh --ime-live` and only while the gallery has focus. Otherwise it exits with code 2, as `drive: line <n>: wtype needs launch.sh --ime-live`.
- A `scroll` goes to whatever is under the pointer. After a `click` on the page list or the theme toggle the pointer stays there, and a `scroll` does nothing until a `move` puts it back on the page, such as `move 740 400`.
- After a `page` change or a click on the page list, `wait 120` before a `screenshot`. The highlight on the list entry fades between colors, and a shot taken 10 to 30 frames in shows the colors in between. Wait 30 frames before the first screenshot of a run, since the first frames can be black.
- Stable handles, at 1280 by 800. The page list entry `about` is at 100 62. The theme toggle is at 100 771. They move when `resize` changes the window.
- `resize <w> <h>` asks Hyprland for the size, and prints `ok` once the window reports it. Without Hyprland, or when the window does not take the size within 2 seconds, it exits with code 2 and names the size the window has.
- The theme toggle builds the page again and puts each scroll offset back, so a page keeps its scroll through a toggle.
- `log` topics are `entities`, `page`, `present_mode`, `scroll`, `theme`, `rows`, `spans`, `hit`, `diff`, `runs <line>`, `stream`, `selection`, `copied`, `paste`, `ime`, `caret`, `text <line>`, `undo`, and the Hamlet pages' `rich-styles`, `rich-stream`, `code-plate`, `code-grow`, `code-blank`, `diff-colours`, `diff-more`, `stress`, and `stress-blank`, which `features/hamlet.md` describes. Each prints one line such as `page about`.
  - `caret`, `text <line>`, and `undo` are answered by the page on show, through `Page::answer` in `pages.rs`, as `<topic> <answer>`. A page without an answer prints `<topic> n/a`, so every read-only page prints `caret n/a`. `text` counts lines from 1.
  - `ime` prints `ime enabled <bool> position <x> <y> last <message> page <answer>`. The first half is the window's `ime_enabled` and `ime_position`. `<message>` is the last `Ime` message the gallery read, `enabled`, `disabled`, `preedit "<text>" <start>..<end>`, `preedit "<text>"` with no cursor, `commit "<text>"`, or `none`. `<answer>` is the page's, or `n/a`.
  - `paste` prints what the last `paste` step handed the gallery, as `paste "<text>"` quoted as Rust debug text, or `paste none`.
  - `present_mode` prints the mode the window asked for, as `present_mode requested AutoNoVsync`, not the mode the driver settled on.
  - `scroll` prints the offset of the first scroll area on the current page, in pixels.
  - `rows` reads the first virtual list: `rows first_visible <n> spawned <a>..=<b> rows <slots> total <count> following <bool> x <px>`, where `x` is its sideways offset. It prints `rows empty total 0` for a list with no rows and `rows none` with no list.
  - `spans` lists the spans of the rich page's long line, `spans <n>: <id> "<text>" <hex>, ...`. `hit` prints the byte and character under the last press on its hit line, `hit 15 'ö'`, or `hit none`.
  - `stream` and `selection` read the first code view, as `features/stream.md` and `features/select.md` describe.
  - `runs <line>` reads the code page's line, counted from 1: `runs <line>: <a>..<b> <kind>, ... | shown "<text>" <hex>, ...`. The part before `|` is the runs the line was lexed into, in bytes of the line, or `empty` for an empty line. The part after is each span its row shows, or `not shown` while no row is built for it. Past the last line it prints `runs <line> none`.
  - `copied` prints what this gallery last copied as `copied "<text>"`, quoted as Rust debug text, or `copied none`, so `expect copied` can match a whole copy.
  - `diff` reads the first diff view: `diff files <n> rows <count> <stacked|split|compact> first <index> <row>`, where `<row>` is the first visible row, as `header <file> <name>`, `line <file> old <n> new <n>` with `-` for a side without a line, `gap <file> hidden <n>`, or `more <n>`. It prints `diff none` with no diff view.
- `expect <topic> <text>` prints `accept: <topic> pass` when the topic's line holds the text, else `accept: <topic> fail`, and counts toward the summary that `exit` prints. A topic that takes a line number takes it before the text: `expect runs 75 comment` checks `runs 75` for `comment`.
- `open <path>` gives a file to the page on show. A relative path is read from the drive script's directory, so `open ../fixtures/notes.xyz` in a script in `drive/` opens `examples/gallery/fixtures/notes.xyz`. It exits with code 2 when the page takes no files or cannot read the file.
- `clipboard` prints what this gallery last copied, never the shared system clipboard.
- Each step prints `drive: <step> ok`. Wait for every one, and for `screenshot` the file exists when it prints.
- Never warp the OS pointer. Use `wtype` only through a `wtype` step under `launch.sh --ime-live`. Both act on the focused window, which the gallery has only in that mode.

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
