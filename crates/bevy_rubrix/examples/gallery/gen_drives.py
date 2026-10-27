import argparse
from pathlib import Path

parser = argparse.ArgumentParser(
    description="Write every virtual_list drive script but virtual-list.drive."
)
parser.add_argument("dir", nargs="?", type=Path, default=Path(__file__).parent / "drive")
out = parser.parse_args().dir
CENTER = "move 740 400"
TEN_K, HUNDRED_K, APPEND, PLAIN, FOLLOW = (
    "click 290 68", "click 438 68", "click 586 68", "click 734 68", "click 882 68"
)
FOLLOW_ON = [FOLLOW, "wait 2"]


def write(name, *steps):
    lines = []
    for step in steps:
        lines.extend(step if isinstance(step, list) else [step])
    (out / f"{name}.drive").write_text("\n".join(lines) + "\n")


def scroll(dy, frames):
    return [f"scroll 0 {dy}"] * frames


def back_and_forth(dy, legs, frames):
    steps = []
    for leg in range(legs):
        steps += scroll(-dy if leg % 2 == 0 else dy, frames)
    return steps


def timed(name, setup, moves):
    write(
        name,
        "page virtual_list",
        "wait 30",
        setup,
        "wait 60",
        CENTER,
        "frames start",
        moves,
        f"frames stop {name}.csv",
        f"screenshot {name}.png",
        "log rows",
        "log entities",
        "exit",
    )


slow = scroll(-18, 600)
fast = back_and_forth(782, 6, 100)
timed("virtual-list-scroll", [], slow)
timed("virtual-list-scroll-100k", HUNDRED_K, slow)
timed("virtual-list-scroll-plain", PLAIN, slow)
timed("virtual-list-scroll-plain-100k", [HUNDRED_K, "wait 30", PLAIN], slow)
timed("virtual-list-fast", [], fast)
timed("virtual-list-fast-100k", HUNDRED_K, fast)

for name, setup in [
    ("virtual-list-first-paint", []),
    (
        "virtual-list-first-paint-100k",
        ["page virtual_list", "wait 30", HUNDRED_K, "wait 30", "page about", "wait 30"],
    ),
]:
    write(
        name,
        setup,
        "frames start",
        "wait 70",
        "page virtual_list",
        "wait 120",
        f"frames stop {name}.csv",
        f"screenshot {name}.png",
        "log rows",
        "exit",
    )

write(
    "virtual-list-end",
    "page virtual_list",
    "wait 30",
    HUNDRED_K,
    "wait 30",
    CENTER,
    "scroll 0 -2000000",
    "wait 10",
    "screenshot virtual-list-end.png",
    "log rows",
    "expect rows ..=99999",
    "exit",
)
write(
    "virtual-list-thumb",
    "page virtual_list",
    "wait 30",
    HUNDRED_K,
    "wait 30",
    "move 1255 107",
    "wait 20",
    "drag 1255 107 1255 437",
    "wait 10",
    "screenshot virtual-list-thumb.png",
    "log rows",
    "exit",
)
write(
    "virtual-list-jump",
    "page virtual_list",
    "wait 30",
    CENTER,
    "scroll 0 -20000",
    "wait 0",
    "screenshot virtual-list-jump.png",
    "log rows",
    "exit",
)
ROW = 17
THREE_LINE_EXTRA = 2 * ROW
ROWS_MEASURED_AT_TOP = 2
ROW_5000_ABOVE_VIEW = 5001 * ROW + ROWS_MEASURED_AT_TOP * THREE_LINE_EXTRA + 1
write(
    "virtual-list-anchor",
    "page virtual_list",
    "wait 30",
    CENTER,
    f"scroll 0 -{ROW_5000_ABOVE_VIEW}",
    "log rows",
    "expect rows first_visible 5001",
    "wait 10",
    "log rows",
    "expect rows first_visible 5001",
    "screenshot virtual-list-anchor.png",
    "exit",
)
write(
    "virtual-list-pinned",
    "page virtual_list",
    "wait 30",
    FOLLOW_ON,
    CENTER,
    "scroll 0 -1000000",
    "wait 10",
    "log rows",
    "expect rows following true",
    APPEND,
    "wait 10",
    "screenshot virtual-list-pinned.png",
    "log rows",
    "expect rows ..=10999",
    "expect rows total 11000 following true",
    "exit",
)
write(
    "virtual-list-not-pinned",
    "page virtual_list",
    "wait 30",
    FOLLOW_ON,
    CENTER,
    "scroll 0 -85000",
    "wait 10",
    "log rows",
    "expect rows first_visible 4996",
    "expect rows total 10000 following false",
    APPEND,
    "wait 10",
    "log rows",
    "expect rows first_visible 4996",
    "expect rows total 11000",
    "screenshot f2-not-pinned.png",
    "exit",
)
write(
    "virtual-list-jump-record",
    "page virtual_list",
    "wait 120",
    CENTER,
    "record start jump",
    "scroll 0 -20000",
    "wait 8",
    "record stop",
    "log rows",
    "exit",
)
write(
    "virtual-list-thumb-record",
    "page virtual_list",
    "wait 120",
    "move 1255 107",
    "wait 20",
    "record start thumb",
    "drag 1255 107 1255 437",
    "wait 8",
    "record stop",
    "log rows",
    "exit",
)
steps = []
for _ in range(30):
    steps += ["log entities"] + scroll(-58, 100)
write(
    "virtual-list-entities",
    "page virtual_list",
    "wait 30",
    CENTER,
    steps,
    "log entities",
    "log rows",
    "screenshot virtual-list-entities.png",
    "exit",
)
write(
    "virtual-list-resize",
    "page virtual_list",
    "wait 30",
    CENTER,
    "scroll 0 -85000",
    "wait 10",
    "resize 800 600",
    "wait 30",
    "screenshot virtual-list-resize.png",
    "log rows",
    "exit",
)
