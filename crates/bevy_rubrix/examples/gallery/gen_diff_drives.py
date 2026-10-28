import argparse
from pathlib import Path

parser = argparse.ArgumentParser(description="Write every diff drive script.")
parser.add_argument("dir", nargs="?", type=Path, default=Path(__file__).parent / "drive")
out = parser.parse_args().dir

CENTER = "move 740 400"
SETTLE = "wait 120"
LARGE, STRESS, PAIR, LAYOUT, WRAP, COMPACT = (f"click {x} 68" for x in range(290, 1031, 148))
FIRST_HEADER = "click 400 134"
UP, ALL = "click 252 252", "click 420 252"
SPLIT_UP = "click 252 235"
MORE = "click 740 439"
ROW = 17
HEADER = 33
LAST_FILE_ROWS_ABOVE_END_STACKED = 284
LAST_FILE_ROWS_ABOVE_END_SPLIT = 268
LAST_HEADER_STACKED = HEADER + LAST_FILE_ROWS_ABOVE_END_STACKED * ROW
LAST_HEADER_SPLIT = HEADER + LAST_FILE_ROWS_ABOVE_END_SPLIT * ROW
STRESS_MIDDLE = 850_000


def flat(steps):
    for step in steps:
        if isinstance(step, list):
            yield from flat(step)
        else:
            yield step


def write(name, *steps):
    (out / f"{name}.drive").write_text("\n".join(flat(steps)) + "\n")


def back_and_forth(dy, legs, frames):
    steps = []
    for leg in range(legs):
        steps += [f"scroll 0 {-dy if leg % 2 == 0 else dy}"] * frames
    return steps


def timed_scroll(csv):
    return [CENTER, "frames start", "wait 60", back_and_forth(18, 6, 100), f"frames stop {csv}"]


def opened(*paths):
    return ["page diff", "wait 30", "open " + " ".join(f"../fixtures/diff/{p}" for p in paths), SETTLE]


def to_last_file(up):
    return [CENTER, "scroll 0 -2000000", "wait 10", f"scroll 0 {up}", SETTLE]


def to_stress_middle():
    return [STRESS, SETTLE, CENTER, f"scroll 0 -{STRESS_MIDDLE}", SETTLE]


write(
    "diff-patch",
    "frames start",
    "wait 70",
    "page diff",
    SETTLE,
    "frames stop diff-first-paint.csv",
    "screenshot r15-patch.png",
    "log page",
    "log diff",
    "log rows",
    timed_scroll("diff-scroll.csv"),
    "log rows",
    "exit",
)
write(
    "diff-last-file",
    "page diff",
    SETTLE,
    to_last_file(LAST_HEADER_STACKED),
    "screenshot r15-last-file.png",
    "log diff",
    "expect diff first 10308 header 92 crates/bevy_fynix/src/views/menu.rs",
    "exit",
)
write("diff-expand", opened("gap-old.txt", "gap-new.txt"), UP, SETTLE, "screenshot r15-expand.png", "log diff", "expect diff rows 116", "exit")
write("diff-expand-all", opened("gap-old.txt", "gap-new.txt"), ALL, SETTLE, "screenshot r15-expand-all.png", "log diff", "expect diff rows 312", "exit")
write("diff-collapse", "page diff", SETTLE, FIRST_HEADER, SETTLE, "screenshot r15-collapse.png", "log diff", "exit")
write("diff-words", opened("words-old.rs", "words-new.rs"), "screenshot r15-words.png", "exit")
write("diff-headers", opened("headers.patch"), "screenshot r15-headers.png", "log diff", "expect diff files 4", "exit")
write(
    "diff-compact",
    opened("forty-old.txt", "forty-new.txt"),
    COMPACT,
    SETTLE,
    "screenshot r15-compact.png",
    "log diff",
    "expect diff rows 18 compact",
    MORE,
    SETTLE,
    "log diff",
    "expect diff rows 41 compact",
    "screenshot r15-compact-all.png",
    "exit",
)
write(
    "diff-stress",
    "page diff",
    "wait 30",
    to_stress_middle(),
    "screenshot r15-stress.png",
    "log rows",
    "log diff",
    timed_scroll("diff-scroll-100k.csv"),
    "log rows",
    "exit",
)

write(
    "diff-split",
    "page diff",
    "wait 30",
    LAYOUT,
    "wait 30",
    "page about",
    "wait 30",
    "frames start",
    "wait 70",
    "page diff",
    SETTLE,
    "frames stop diff-split-first-paint.csv",
    "screenshot r16-split.png",
    "log diff",
    "expect diff files 93 rows 9829 split",
    timed_scroll("diff-split-scroll.csv"),
    "log rows",
    "exit",
)
write("diff-realign", opened("realign-old.rs", "realign-new.rs"), LAYOUT, SETTLE, "screenshot r16-realign.png", "log diff", "exit")
write("diff-added", opened("added.patch"), LAYOUT, SETTLE, "screenshot r16-added.png", "log diff", "exit")
write(
    "diff-split-last",
    "page diff",
    "wait 30",
    LAYOUT,
    SETTLE,
    to_last_file(LAST_HEADER_SPLIT),
    "screenshot r16-last.png",
    "log diff",
    "expect diff first 9520 header 92 crates/bevy_fynix/src/views/menu.rs",
    "exit",
)
write(
    "diff-hscroll",
    opened("long-old.rs", "long-new.rs"),
    LAYOUT,
    SETTLE,
    "move 740 176",
    "scroll -400 0",
    "wait 30",
    "screenshot r16-hscroll.png",
    "log rows",
    "exit",
)
write("diff-wrap", opened("wrap-old.rs", "wrap-new.rs"), LAYOUT, WRAP, SETTLE, "screenshot r16-wrap.png", "log diff", "exit")
write(
    "diff-switch",
    "page diff",
    SETTLE,
    CENTER,
    f"scroll 0 -{2_999 * ROW}",
    SETTLE,
    "log diff",
    LAYOUT,
    SETTLE,
    "log diff",
    "screenshot r16-switch.png",
    "exit",
)
write("diff-split-expand", opened("gap-old.txt", "gap-new.txt"), LAYOUT, SETTLE, SPLIT_UP, SETTLE, "screenshot r16-expand.png", "log diff", "exit")
write("diff-split-stress", "page diff", "wait 30", LAYOUT, "wait 30", to_stress_middle(), "screenshot r16-stress.png", "log rows", "log diff", "exit")


def review(name, *steps):
    write(name, "page diff", SETTLE, "record start review", "wait 60", *steps, "wait 30", "record stop", "log diff", "exit")


review(
    "diff-review-record",
    CENTER,
    ["scroll 0 -18"] * 120,
    "wait 30",
    FIRST_HEADER,
    "wait 60",
    FIRST_HEADER,
    "wait 60",
    "open ../fixtures/diff/gap-old.txt ../fixtures/diff/gap-new.txt",
    "wait 60",
    UP,
    "wait 90",
    "open ../fixtures/diff/forty-old.txt ../fixtures/diff/forty-new.txt",
    "wait 30",
    COMPACT,
    "wait 90",
    MORE,
    "wait 60",
    COMPACT,
    "wait 30",
)
review(
    "diff-split-review-record",
    LAYOUT,
    "wait 90",
    CENTER,
    ["scroll 0 -18"] * 120,
    "wait 30",
    "open ../fixtures/diff/realign-old.rs ../fixtures/diff/realign-new.rs",
    "wait 90",
    "open ../fixtures/diff/added.patch",
    "wait 90",
    "open ../fixtures/diff/wrap-old.rs ../fixtures/diff/wrap-new.rs",
    "wait 60",
    WRAP,
    "wait 90",
    WRAP,
    LARGE,
    "wait 30",
    CENTER,
    f"scroll 0 -{2_999 * ROW}",
    "wait 60",
    LAYOUT,
    "wait 90",
    LAYOUT,
    "wait 60",
)
