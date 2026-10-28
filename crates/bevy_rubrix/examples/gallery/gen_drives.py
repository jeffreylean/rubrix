import argparse
from pathlib import Path

parser = argparse.ArgumentParser(
    description="Write every virtual_list drive script but virtual-list.drive, "
    "and every code, stream, select and diff drive script."
)
parser.add_argument("dir", nargs="?", type=Path, default=Path(__file__).parent / "drive")
out = parser.parse_args().dir
CENTER = "move 740 400"
BUTTONS = ["click 290 68", "click 438 68", "click 586 68", "click 734 68", "click 882 68"]
TEN_K, HUNDRED_K, APPEND, PLAIN, FOLLOW = BUTTONS
FOLLOW_ON = [FOLLOW, "wait 2"]


def flat(steps):
    for step in steps:
        if isinstance(step, list):
            yield from flat(step)
        else:
            yield step


def write(name, *steps):
    (out / f"{name}.drive").write_text("\n".join(flat(steps)) + "\n")


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

LARGE_RS, LARGE_TS, STRESS, WRAP = BUTTONS[:4]
THEME = "click 100 771"
SETTLE = "wait 120"
LARGE_RS_LINES = (
    (Path(__file__).resolve().parents[4] / "bench/fixtures/rust/large.rs.txt")
    .read_text()
    .splitlines()
)


def to_line(line_number):
    return f"scroll 0 -{(line_number - 1) * ROW}"


def code(name, *steps):
    write(f"code-{name}", "page code", SETTLE, *steps, "exit")


write(
    "code-large",
    "frames start",
    "wait 70",
    "page code",
    SETTLE,
    "frames stop code-first-paint.csv",
    "screenshot r12-large.png",
    "log page",
    "log rows",
    "log runs 1",
    CENTER,
    "frames start",
    "wait 60",
    back_and_forth(18, 6, 100),
    "frames stop code-scroll.csv",
    "log rows",
    "exit",
)
code(
    "end",
    CENTER,
    to_line(10_000),
    "wait 30",
    "screenshot r12-10000.png",
    "log rows",
    "expect rows first_visible 9999 spawned",
    "scroll 0 -2000000",
    "wait 30",
    "screenshot r12-end.png",
    "log rows",
    f"log runs {len(LARGE_RS_LINES)}",
)
code(
    "late-comment",
    "open ../fixtures/late-comment.rs",
    "wait 30",
    CENTER,
    to_line(60),
    "wait 30",
    "screenshot r12-late-comment.png",
    "log rows",
    "expect rows first_visible 59 spawned",
    "log runs 60",
    "expect runs 60 comment",
    "expect runs 75 comment",
)
code(
    "hscroll",
    "open ../fixtures/long-line.rs",
    "wait 10",
    WRAP,
    "wait 10",
    WRAP,
    "wait 10",
    CENTER,
    "scroll -6000 0",
    "wait 30",
    "screenshot r12-hscroll.png",
    "log rows",
    "log runs 3",
)
code(
    "wrap",
    "open ../fixtures/long-line.rs",
    "wait 10",
    WRAP,
    "wait 10",
    CENTER,
    "wait 30",
    "screenshot r12-wrap.png",
    "log rows",
    "expect rows x 0",
    "log runs 3",
)
code(
    "100k",
    "frames start",
    "wait 70",
    STRESS,
    "wait 60",
    "frames stop code-load-100k.csv",
    CENTER,
    "frames start",
    "wait 70",
    to_line(50_000),
    "wait 2",
    "log rows",
    "expect rows first_visible 49999 spawned",
    "wait 58",
    "frames stop code-jump-100k.csv",
    "screenshot r12-100k.png",
    "log rows",
    "log runs 50000",
    "frames start",
    "wait 60",
    back_and_forth(18, 6, 100),
    "frames stop code-scroll-100k.csv",
    "log rows",
)
code(
    "unicode",
    "open ../fixtures/unicode.rs",
    "wait 30",
    "screenshot r12-unicode.png",
    "log runs 1",
    "log runs 3",
    "log runs 4",
    "log runs 6",
)
code(
    "edges",
    "open ../fixtures/empty.txt",
    "wait 30",
    "screenshot r12-edges-empty.png",
    "log rows",
    "expect rows total 1 following",
    "log runs 1",
    "open ../fixtures/one-line.rs",
    "wait 30",
    "screenshot r12-edges.png",
    "log rows",
    "expect rows total 1 following",
    "log runs 1",
)
quoted = next(
    n for n, line in enumerate(LARGE_RS_LINES, 1) if line.startswith("const ") and '"' in line
)
commented = max(
    n for n, line in enumerate(LARGE_RS_LINES[:quoted], 1) if line.startswith("//")
)
assert quoted - commented < 10, (commented, quoted)
lines = [
    "log rows",
    f"expect rows first_visible {commented - 5} spawned",
    f"log runs {commented}",
    f"log runs {quoted}",
]
code(
    "theme",
    CENTER,
    to_line(commented - 4),
    "wait 30",
    "screenshot r12-theme-dark.png",
    "log theme",
    lines,
    THEME,
    SETTLE,
    "screenshot r12-theme.png",
    "log theme",
    lines,
)
code(
    "plain",
    "open ../fixtures/notes.xyz",
    "wait 30",
    "screenshot r12-plain.png",
    "log runs 1",
    "expect runs 1 plain",
)
code(
    "review-record",
    "record start review",
    "wait 60",
    CENTER,
    scroll(-18, 120),
    "wait 30",
    WRAP,
    "wait 90",
    WRAP,
    "wait 60",
    "open ../fixtures/long-line.rs",
    "wait 30",
    CENTER,
    ["scroll -60 0"] * 40,
    "wait 60",
    LARGE_RS,
    "wait 30",
    CENTER,
    to_line(commented - 4),
    "wait 60",
    THEME,
    "wait 120",
    STRESS,
    "wait 60",
    CENTER,
    to_line(50_000),
    "wait 90",
    scroll(-18, 120),
    "wait 30",
    "record stop",
    "log rows",
)


ONE_B, SIXTY_FOUR_B, FOUR_KIB, SIXTY_FOUR_KIB, PAUSE, RESTART, STREAM_100K = [
    f"click {x} 68" for x in (290, 438, 586, 734, 882, 1030, 1178)
]
LINE_1 = 126
TEXT_X = 271
GUTTER_X = 235
LINE_END_X = 1250


def at_line(line_number, x=TEXT_X):
    return f"{x} {LINE_1 + (line_number - 1) * ROW}"


def stream(name, *steps):
    write(f"stream-{name}", "page stream", SETTLE, *steps, "exit")


def at_rate(rate):
    return [rate, RESTART]


stream(
    "final",
    FOUR_KIB,
    "frames start",
    "wait 60",
    RESTART,
    "wait 85",
    "frames stop stream-4k.csv",
    SIXTY_FOUR_KIB,
    "frames start",
    "wait 60",
    [[RESTART, "wait 8"] for _ in range(10)],
    "frames stop stream-64k.csv",
    "wait 30",
    "screenshot r13-final.png",
    "log stream",
    "log rows",
    "page code",
    SETTLE,
    CENTER,
    "scroll 0 -200000",
    "wait 30",
    "screenshot r13-final-code.png",
    "log rows",
)
stream(
    "mid",
    at_rate(SIXTY_FOUR_B),
    "wait 300",
    "log stream",
    "wait 100",
    "log stream",
    "screenshot r13-mid.png",
    "log rows",
    "expect rows following true",
)
scrolled_up = [
    at_rate(SIXTY_FOUR_B),
    "wait 900",
    CENTER,
    "scroll 0 300",
    "scroll 0 300",
    "wait 10",
    "log rows",
    "expect rows following false",
    "log stream",
    "wait 200",
    "log stream",
    "log rows",
]
stream("scrolled-up", scrolled_up, "screenshot r13-scrolled-up.png")
stream(
    "refollow",
    scrolled_up,
    "wheel 0 -400",
    "wait 400",
    "log rows",
    "expect rows following true",
    "log stream",
    "wait 100",
    "log stream",
    "log rows",
    "screenshot r13-refollow.png",
)
stream(
    "open-string",
    "open ../fixtures/open-string.rs",
    SETTLE,
    "screenshot r13-open-string.png",
    "log stream",
    "log runs 3",
    "expect runs 3 string",
)
stream(
    "closed-string",
    "open ../fixtures/closed-string.rs",
    at_rate(ONE_B),
    SETTLE,
    "screenshot r13-closed-string.png",
    "log stream",
    "log runs 1",
    "log runs 2",
    "expect runs 2 keyword",
)
stream(
    "respawn",
    at_rate(SIXTY_FOUR_B),
    [["wait 100", "log stream"] for _ in range(12)],
    "screenshot r13-respawn.png",
)
stream(
    "restart",
    "log entities",
    PAUSE,
    RESTART,
    "wait 30",
    "screenshot r13-restart-empty.png",
    "log stream",
    PAUSE,
    "wait 200",
    "log entities",
    RESTART,
    "wait 200",
    "log entities",
    RESTART,
    "wait 200",
    "log entities",
    "screenshot r13-restart.png",
)
stream(
    "stress",
    SIXTY_FOUR_KIB,
    STREAM_100K,
    "wait 150",
    "screenshot r13-stress.png",
    "log rows",
    "log stream",
    "expect rows total 100000",
)
stream(
    "crlf",
    "open ../fixtures/crlf.rs",
    at_rate(ONE_B),
    "wait 2000",
    "screenshot r13-crlf.png",
    "log stream",
    "log runs 3",
    "log rows",
)
(out.parent / "fixtures/open-string.rs").write_text(
    'fn main() {\n    let t = 1;\nlet s = "open'
)
(out.parent / "fixtures/closed-string.rs").write_text('let s = "ab\nc"; let t = 1;\n')
(out.parent / "fixtures/crlf.rs").write_bytes(
    "".join(line + "\r\n" for line in LARGE_RS_LINES[:300]).encode()
)


def select(name, *steps):
    write(f"select-{name}", "page code", SETTLE, *steps, "exit")


copy = ["key ctrl+c", "log selection", "clipboard"]
select(
    "drag",
    f"drag {at_line(10, GUTTER_X)} {at_line(20, LINE_END_X)}",
    copy,
    "wait 30",
    "screenshot r14-drag.png",
)
# The held drag scrolls past line 400, then a pixel scroll, still
# held, puts line 400 on view row 18, where the pointer lets go.
LINE_400_ROW = 18
select(
    "autoscroll",
    "frames start",
    "wait 60",
    f"press {at_line(10, GUTTER_X)}",
    "move 700 799",
    "wait 360",
    "frames stop select-drag.csv",
    "log rows",
    f"move 700 {LINE_1 + LINE_400_ROW * ROW}",
    "scroll 0 1000000",
    f"scroll 0 -{(400 - 1 - LINE_400_ROW) * ROW}",
    "wait 5",
    f"move {LINE_END_X} {LINE_1 + LINE_400_ROW * ROW}",
    "wait 5",
    "release",
    "log rows",
    copy,
    "wait 30",
    "screenshot r14-autoscroll.png",
)
select(
    "blank",
    f"drag {at_line(1, GUTTER_X)} {at_line(3, LINE_END_X)}",
    copy,
    "wait 30",
    "screenshot r14-blank.png",
)
select(
    "word",
    f"click {at_line(3, TEXT_X + round(19.5 * 8.4))} double",
    "log selection",
    "expect selection length 8",
    copy,
    "wait 30",
    "screenshot r14-word.png",
)
select(
    "line",
    f"click {at_line(6, TEXT_X + 40)} triple",
    copy,
    "wait 30",
    "screenshot r14-line.png",
)
select(
    "all",
    "key ctrl+a",
    "log selection",
    "expect selection length 313933",
    "frames start",
    "wait 60",
    "key ctrl+c",
    "wait 10",
    "frames stop select-copy.csv",
    "clipboard",
    "wait 30",
    "screenshot r14-all.png",
)
select(
    "survive",
    f"drag {at_line(10, GUTTER_X)} {at_line(20, LINE_END_X)}",
    "log selection",
    CENTER,
    f"scroll 0 -{5000 * ROW}",
    "wait 30",
    "log rows",
    f"scroll 0 {5000 * ROW}",
    "wait 30",
    "log rows",
    "log selection",
    "screenshot r14-survive.png",
)
select(
    "extend",
    f"click {at_line(10, GUTTER_X)}",
    f"click {at_line(20, LINE_END_X)} shift",
    copy,
    "wait 30",
    "screenshot r14-extend.png",
)
select(
    "gutter-wrap",
    "open ../fixtures/long-line.rs",
    "wait 30",
    WRAP,
    "wait 60",
    f"drag {at_line(1, GUTTER_X)} {at_line(8, LINE_END_X)}",
    copy,
    "wait 30",
    "screenshot r14-gutter-wrap.png",
)
select(
    "unicode",
    "open ../fixtures/unicode.rs",
    "wait 30",
    f"drag {at_line(3, GUTTER_X)} {at_line(4, LINE_END_X)}",
    copy,
    "wait 30",
    "screenshot r14-unicode.png",
)


LARGE, STRESS_PATCH, PAIR, LAYOUT, DIFF_WRAP, COMPACT = (f"click {x} 68" for x in range(290, 1031, 148))
FIRST_HEADER = "click 400 134"
UP, ALL = "click 252 252", "click 420 252"
SPLIT_UP = "click 252 235"
MORE = "click 740 439"
HEADER = 33
LAST_FILE_ROWS_ABOVE_END_STACKED = 284
LAST_FILE_ROWS_ABOVE_END_SPLIT = 268
LAST_HEADER_STACKED = HEADER + LAST_FILE_ROWS_ABOVE_END_STACKED * ROW
LAST_HEADER_SPLIT = HEADER + LAST_FILE_ROWS_ABOVE_END_SPLIT * ROW
STRESS_MIDDLE = 850_000


def timed_scroll(csv):
    return [CENTER, "frames start", "wait 60", back_and_forth(18, 6, 100), f"frames stop {csv}"]


def opened(*paths):
    return ["page diff", "wait 30", "open " + " ".join(f"../fixtures/diff/{p}" for p in paths), SETTLE]


def to_last_file(up):
    return [CENTER, "scroll 0 -2000000", "wait 10", f"scroll 0 {up}", SETTLE]


def to_stress_middle():
    return [STRESS_PATCH, SETTLE, CENTER, f"scroll 0 -{STRESS_MIDDLE}", SETTLE]


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
    "diff-copy-compact",
    opened("forty-old.txt", "forty-new.txt"),
    COMPACT,
    SETTLE,
    "drag 221 159 700 193",
    "key ctrl+c",
    "clipboard",
    "wait 30",
    "screenshot r15-copy.png",
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
    "diff-copy-side",
    opened("realign-old.rs", "realign-new.rs"),
    LAYOUT,
    SETTLE,
    "drag 230 176 1000 210",
    "key ctrl+c",
    "clipboard",
    "wait 30",
    "screenshot r16-copy-side.png",
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
write("diff-wrap", opened("wrap-old.rs", "wrap-new.rs"), LAYOUT, DIFF_WRAP, SETTLE, "screenshot r16-wrap.png", "log diff", "exit")
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
    DIFF_WRAP,
    "wait 90",
    DIFF_WRAP,
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
