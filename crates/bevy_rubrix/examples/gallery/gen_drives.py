import argparse
from pathlib import Path

parser = argparse.ArgumentParser(
    description="Write every virtual_list drive script but virtual-list.drive, "
    "every code, stream, select, diff and editor drive script, and the "
    "Hamlet ones."
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

SPLICE_ON = ["click 1030 68", "wait 2"]
INSERT, REMOVE, INSERT_ABOVE, GROW_ABOVE, JUMP_INSERT, REVEAL_WIDE, SIDEWAYS = (
    f"click {290 + 148 * k} 118" for k in range(7)
)
ROW_4990_AT_TOP = 4990 * ROW + ROWS_MEASURED_AT_TOP * THREE_LINE_EXTRA + 1
SPLICED_PX = 20 * ROW
WIDE = 3000
LIST_WIDTH = 1040
to_row_4990 = [CENTER, f"scroll 0 -{ROW_4990_AT_TOP}", "wait 10", "log rows", "expect rows first_visible 4990"]
to_row_4996 = [CENTER, "scroll 0 -85000", "wait 10", "log rows", "expect rows first_visible 4996"]


def splice(name, *steps):
    write(f"virtual-list-{name}", "page virtual_list", "wait 30", list(steps), "exit")


for name, button, total, shot in [
    ("splice-insert", INSERT, 10020, "f3-insert.png"),
    ("splice-remove", REMOVE, 9980, "f3-remove.png"),
]:
    splice(
        name,
        SPLICE_ON,
        to_row_4990,
        button,
        "wait 10",
        "log rows",
        "expect rows first_visible 4990",
        f"expect rows total {total}",
        f"screenshot {shot}",
    )
splice(
    "splice-above",
    SPLICE_ON,
    to_row_4996,
    "log scroll",
    "expect scroll 85000",
    INSERT_ABOVE,
    "log rows",
    "expect rows first_visible 5016",
    "log scroll",
    f"expect scroll {85000 + SPLICED_PX}",
    "wait 10",
    "log rows",
    "expect rows first_visible 5016",
    "log scroll",
    f"expect scroll {85000 + SPLICED_PX}",
    "screenshot f3-above.png",
)
splice(
    "splice-grow",
    SPLICE_ON,
    to_row_4996,
    GROW_ABOVE,
    "log rows",
    "expect rows first_visible 4996",
    "wait 10",
    "log rows",
    "expect rows first_visible 4996",
    "screenshot f3-grow.png",
)
splice(
    "splice-100k",
    HUNDRED_K,
    "wait 30",
    SPLICE_ON,
    CENTER,
    "scroll 0 -2000000",
    "wait 10",
    "log rows",
    "expect rows ..=99999",
    INSERT,
    "wait 10",
    "log rows",
    "expect rows total 100020",
    "expect rows ..=100019",
    "screenshot f3-100k.png",
)
splice(
    "splice-reuse",
    SPLICE_ON,
    to_row_4990,
    "log entities",
    [[INSERT, "wait 2", REMOVE, "wait 2"] for _ in range(50)],
    "wait 10",
    "log rows",
    "expect rows first_visible 4990",
    "expect rows total 10000",
    "log entities",
    "screenshot f3-reuse.png",
)
splice(
    "splice-follow",
    FOLLOW_ON,
    SPLICE_ON,
    CENTER,
    "scroll 0 -1000000",
    "wait 10",
    "log rows",
    "expect rows following true",
    INSERT,
    "wait 10",
    "log rows",
    "expect rows total 10020 following true",
    APPEND,
    "wait 10",
    "log rows",
    "expect rows total 11020 following true",
    "screenshot f3-follow.png",
)
splice(
    "splice-jump",
    SPLICE_ON,
    JUMP_INSERT,
    "wait 30",
    "log rows",
    "expect rows first_visible 9020",
    "screenshot f3-jump.png",
)
splice(
    "reveal",
    SPLICE_ON,
    SIDEWAYS,
    "wait 2",
    CENTER,
    f"scroll 0 -{ROW_4990_AT_TOP}",
    "wait 10",
    "log rows",
    REVEAL_WIDE,
    "wait 10",
    "log rows",
    f"expect rows x {WIDE - LIST_WIDTH}",
    "screenshot f3-reveal.png",
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


PAGE_LIST = {"hamlet_rich": 272, "hamlet_code": 307, "hamlet_diff": 342, "stress": 377}
STREAM = "click 290 68"
SAMPLE_LINE_1 = "click 300 127 triple"
MORE_LINES = "click 1003 382"
STRESS_DIFF_BUTTON = "click 438 68"
STRESS_CODE_ROWS, STRESS_DIFF_ROWS = 100_000, 97_072
FENCE_ROWS = 17
SAMPLE_STYLES = (
    '"Plan" 20 bold sans | "Use " 14 sans | "care" 14 bold sans | " with " 14 sans | '
    '"rm" 13 mono plate | ", " 14 sans | "old" 14 sans struck | " and " 14 sans | '
    '"docs" 14 sans accent underline | "." 14 sans'
)
SMALL_CHANGE = (
    '"  a" dim no fill, "- b" red on deep red, "+ B" green on deep green, '
    '"  c" dim no fill, "+ d" green on deep green'
)


def first_paint(page):
    return ["frames start", "wait 70", f"click 100 {PAGE_LIST[page]}", SETTLE, f"frames stop first-paint-{page}.csv"]


def checked(topic, text):
    return [f"log {topic}", f"expect {topic} {text}"]


def to_end(csv):
    jump = ["scroll 0 -2000000", SETTLE]
    return ["log stress-blank", CENTER, "frames start", "wait 60", scroll(-18, 600), f"frames stop {csv}", jump, jump]


write(
    "acceptance",
    "wait 30",
    "screenshot pages.png",
    first_paint("hamlet_rich"),
    "screenshot hamlet-rich.png",
    checked("rich-styles", SAMPLE_STYLES),
    "log entities",
    STREAM,
    "wait 80",
    "screenshot hamlet-rich-stream.png",
    "log entities",
    checked("rich-stream", '"Second " 14 sans | "grows" 14 italic sans sample kept'),
    first_paint("hamlet_code"),
    "screenshot hamlet-code.png",
    checked("code-plate", "caption above text 13 mono sideways off rows 4 tallest 2 lines"),
    SAMPLE_LINE_1,
    "key ctrl+c",
    "wait 2",
    "clipboard",
    checked("copied", '"fn main() {"'),
    STREAM,
    "wait 240",
    "screenshot hamlet-code-stream.png",
    checked("code-grow", f"rows 1 to {FENCE_ROWS} grows yes shrinks 0 empty 0"),
    checked("code-blank", "blank 0"),
    first_paint("hamlet_diff"),
    "screenshot hamlet-diff.png",
    checked("diff-colours", SMALL_CHANGE),
    checked("diff-more", "lines 16 more 24 rows 18 built 18"),
    MORE_LINES,
    "wait 30",
    "screenshot hamlet-diff-more.png",
    checked("diff-more", "lines 40 more none rows 41 built 41"),
    "move 1003 400",
    "scroll 0 -200",
    "wait 30",
    "screenshot hamlet-diff-more-end.png",
    f"click 100 {PAGE_LIST['stress']}",
    SETTLE,
    "screenshot stress.png",
    to_end("stress-code.csv"),
    "screenshot stress-code-end.png",
    checked("stress", f"code last {STRESS_CODE_ROWS} of {STRESS_CODE_ROWS}"),
    checked("stress-blank", "blank 0"),
    STRESS_DIFF_BUTTON,
    SETTLE,
    to_end("stress-diff.csv"),
    "screenshot stress-diff-end.png",
    checked("stress", f"diff last {STRESS_DIFF_ROWS} of {STRESS_DIFF_ROWS}"),
    checked("stress-blank", "blank 0"),
    "exit",
)
write(
    "hamlet-review-record",
    "wait 30",
    "record start review",
    f"click 100 {PAGE_LIST['hamlet_rich']}",
    "wait 90",
    STREAM,
    "wait 120",
    f"click 100 {PAGE_LIST['hamlet_code']}",
    "wait 90",
    SAMPLE_LINE_1,
    "wait 60",
    STREAM,
    "wait 300",
    f"click 100 {PAGE_LIST['hamlet_diff']}",
    "wait 120",
    MORE_LINES,
    "wait 120",
    f"click 100 {PAGE_LIST['stress']}",
    "wait 90",
    CENTER,
    scroll(-18, 360),
    STRESS_DIFF_BUTTON,
    "wait 60",
    CENTER,
    scroll(-18, 360),
    "wait 30",
    "record stop",
    "exit",
)


ADVANCE = 8.4
CLICK_INSET = 2
TEXT_X_100K = TEXT_X + ADVANCE
PAGE_ROWS = 38
WRAP_COLUMNS = 113
TRIPLE_TAKES_BREAK = True
TOP = "scroll 0 100000000"
LEFT = "scroll 100000000 0"
STRESS_LINES = [LARGE_RS_LINES[i % len(LARGE_RS_LINES)] for i in range(100_000)]


def editor(name, *steps):
    write(f"editor-{name}", "page editor", SETTLE, *steps, "exit")


def column_x(column, text_x=TEXT_X):
    return round(text_x + (column - 1) * ADVANCE + CLICK_INSET, 1)


class Doc:
    """The editor's text as the scripts expect it, lines and columns from 1."""

    def __init__(self, lines, top=1, text_x=TEXT_X):
        self.lines = list(lines) + [""]
        self.top = top
        self.text_x = text_x
        self.scrolled = False

    def line(self, n):
        return self.lines[n - 1]

    def rows(self):
        return len(self.lines)

    def y(self, n):
        assert self.top <= n < self.top + PAGE_ROWS, (n, self.top)
        return LINE_1 + (n - self.top) * ROW

    def at(self, n, column):
        return f"{column_x(column, self.text_x)} {self.y(n)}"

    def end(self, n):
        return f"{LINE_END_X} {self.y(n)}"

    def gutter(self, n):
        return f"{GUTTER_X} {self.y(n)}"

    def scroll_to(self, n):
        """Puts line `n` first. The view starts at line 1, column 1, so
        a view scrolled since goes back there first."""
        reset = [TOP, LEFT] if self.scrolled else []
        self.top, self.scrolled = n, True
        return [CENTER, reset, to_line(n), "wait 10"]

    def insert(self, at, text):
        n, column = at
        line = self.line(n)
        new = (line[: column - 1] + text + line[column - 1 :]).split("\n")
        self.lines[n - 1 : n] = new
        return (n + len(new) - 1, len(new[-1]) - len(line) + column)

    def between(self, start, end):
        (a, ac), (b, bc) = start, end
        if a == b:
            return self.line(a)[ac - 1 : bc - 1]
        middle = self.lines[a : b - 1]
        return "\n".join([self.line(a)[ac - 1 :], *middle, self.line(b)[: bc - 1]])

    def enter(self, at):
        n, column = at
        line = self.line(n)
        indent = line[: len(line) - len(line.lstrip(" "))][: column - 1]
        return self.insert(at, "\n" + indent)

    def join(self, n):
        column = len(self.line(n - 1)) + 1
        self.lines[n - 2 : n] = [self.line(n - 1) + self.line(n)]
        return (n - 1, column)

    def home(self, n, column):
        line = self.line(n)
        first = len(line) - len(line.lstrip(" ")) + 1
        return 1 if column == first else first

    def text(self, n):
        return [f"log text {n}", f"expect text {n} {rust_debug(self.line(n))}"]


def rust_debug(text):
    assert all(c == "\n" or " " <= c <= "~" for c in text), text
    escaped = text.replace("\\", "\\\\").replace('"', '\\"').replace("\n", "\\n")
    return f'"{escaped}"'


def word_char(c):
    return c.isalnum() or c == "_"


def word_right(line, column):
    i = column - 1
    while i < len(line) and line[i] == " ":
        i += 1
    kind = i < len(line) and word_char(line[i])
    while i < len(line) and line[i] != " " and word_char(line[i]) == kind:
        i += 1
    return i + 1


def word_left(line, column):
    i = column - 1
    while i > 0 and line[i - 1] == " ":
        i -= 1
    kind = i > 0 and word_char(line[i - 1])
    while i > 0 and line[i - 1] != " " and word_char(line[i - 1]) == kind:
        i -= 1
    return i + 1


def word_around(line, column):
    return word_left(line, column + 1), word_right(line, column)


def caret(anchor, head=None, count=1, primary=1):
    head = head or anchor
    anchor, head = min(anchor, head), max(anchor, head)
    return [
        "log caret",
        f"expect caret {count} primary {primary} "
        f"{anchor[0]}:{anchor[1]}..{head[0]}:{head[1]} visible true",
    ]


def undo(depth, redo):
    return ["log undo", f"expect undo {depth} redo {redo}"]


def rows_total(doc):
    return ["log rows", f"expect rows total {doc.rows()} following"]


SEAMS_CLEAN = ["log seams", "expect seams gaps 0 overlaps 0 missing 0 mismatched 0 numbers ok"]


def window(csv, keys):
    assert len(keys) >= 50, csv
    return ["frames start", "wait 60", keys, f"frames stop {csv}"]


doc = Doc(LARGE_RS_LINES)
TYPED = "let x = 1;"
LINE, COLUMN = 120, 5
steps = [doc.scroll_to(LINE), "log rows", f"expect rows first_visible {LINE - 1} spawned"]
steps += [f"click {doc.at(LINE, COLUMN)}", caret((LINE, COLUMN))]
after = doc.insert((LINE, COLUMN), TYPED)
steps += [f"type {TYPED}", "wait 5", doc.text(LINE), f"log runs {LINE}"]
let = COLUMN - 1 + TYPED.index("let")
steps += [[f"log kind {LINE} {byte}", f"expect kind {LINE} {byte} keyword"] for byte in range(let, let + 3)]
editor("type", steps, caret(after), "wait 30", "screenshot e2-type.png")

doc = Doc(LARGE_RS_LINES)
steps = [doc.scroll_to(4990), rows_total(doc), SEAMS_CLEAN]
at = (5000, len(doc.line(5000)) + 1)
steps += [f"click {doc.end(5000)}", caret(at)]
for _ in range(20):
    at = doc.enter(at)
    steps.append("key enter")
steps += ["wait 10", rows_total(doc), SEAMS_CLEAN, caret(at), doc.text(5000), doc.text(5021)]
editor("enter", steps, "wait 30", "screenshot e2-enter.png")


def joins(doc, line, count):
    """`count` Backspaces at the start of the line on `line`'s screen row.
    Home after each takes the view back from the end of the joined line,
    where the Backspace scrolled it sideways, so the next gutter click
    lands at column 1."""
    steps = []
    for _ in range(count):
        at = doc.join(line)
        steps += [f"click {doc.gutter(line)}", "key backspace", "key home"]
    return steps, (at[0], doc.home(at[0], at[1]))


doc = Doc(LARGE_RS_LINES)
steps = [doc.scroll_to(4990), rows_total(doc)]
joined, at = joins(doc, 5001, 20)
steps += [joined, "wait 10", rows_total(doc), SEAMS_CLEAN, caret(at), doc.text(5000), doc.text(5001)]
editor("join", steps, "wait 30", "screenshot e2-join.png")

doc = Doc(LARGE_RS_LINES)
LINE = 5004
line = doc.line(LINE)
steps = [doc.scroll_to(4990)]
column = line.index("Ok") + 1
steps += [f"click {doc.at(LINE, column)}", caret((LINE, column))]
for key, column in [
    ("ctrl+right", word_right(line, column)),
    ("ctrl+left", word_left(line, word_right(line, column))),
]:
    steps += [f"key {key}", caret((LINE, column))]
for _ in range(2):
    column = doc.home(LINE, column)
    steps += ["key home", caret((LINE, column))]
goal = len(line) + 1
steps += ["key end", caret((LINE, goal))]
at = LINE
for _ in range(3):
    at += PAGE_ROWS
    steps += ["key pagedown", caret((at, min(goal, len(doc.line(at)) + 1)))]
editor("move", steps, "wait 30", "screenshot e2-move.png")

doc = Doc(LARGE_RS_LINES)
FROM, ROWS_DOWN = 11, 100
start, end = (FROM, 1), (FROM + 3, 1)
steps = [f"click {doc.at(FROM, 1)}", caret(start)]
for down in range(1, 4):
    steps += ["key shift+down", caret(start, (FROM + down, 1))]
copied = doc.between(start, end)
steps += ["key mod+c", "clipboard", "log copied", f"expect copied {rust_debug(copied)}"]
steps += [doc.scroll_to(doc.top + ROWS_DOWN)]
target = FROM + ROWS_DOWN
steps += [f"click {doc.at(target, 1)}", caret((target, 1))]
after = doc.insert((target, 1), copied)
steps += ["key mod+v", "wait 5", [doc.text(n) for n in range(target, target + 4)], caret(after)]
editor("copy-paste", steps, "wait 30", "screenshot e2-copy-paste.png")

doc = Doc(LARGE_RS_LINES)
LINE = 124
steps = [doc.scroll_to(120), undo(0, 0)]
at = (LINE, len(doc.line(LINE)) + 1)
steps += [f"click {doc.end(LINE)}", caret(at)]
versions = [list(doc.lines)]
at = doc.insert(at, "foo")
steps += ["type foo", doc.text(LINE), undo(1, 0)]
versions.append(list(doc.lines))
at = doc.enter(at)
steps += ["key enter", doc.text(LINE), doc.text(LINE + 1), undo(2, 0)]
versions.append(list(doc.lines))
at = doc.insert(at, "bar")
steps += ["type bar", doc.text(LINE + 1), undo(3, 0)]
versions.append(list(doc.lines))
for key, version, depth in [("mod+z", 2, (2, 1)), ("mod+z", 1, (1, 2)), ("mod+shift+z", 2, (2, 1))]:
    doc.lines = list(versions[version])
    steps += [f"key {key}", doc.text(LINE), doc.text(LINE + 1), undo(*depth)]
editor("undo", steps, "wait 30", "screenshot e2-undo.png")

doc = Doc(LARGE_RS_LINES)
word = doc.line(13).index("struct") + 1
start, end = word_around(doc.line(13), word + 2)
line_end = (13, 1) if TRIPLE_TAKES_BREAK else (12, len(doc.line(12)) + 1)
editor(
    "mouse",
    f"click {doc.at(13, 8)}",
    caret((13, 8)),
    f"click {doc.at(13, 12)} shift",
    caret((13, 8), (13, 12)),
    f"drag {doc.at(11, 5)} {doc.at(12, 10)}",
    caret((11, 5), (12, 10)),
    f"click {doc.at(13, word + 2)} double",
    caret((13, start), (13, end)),
    f"click {doc.at(12, 20)} triple",
    caret((12, 1), line_end),
    "wait 30",
    "screenshot e2-mouse.png",
)


def keystrokes(name, lines, rows, text_x, setup, line):
    doc = Doc(lines, text_x=text_x)
    steps = [setup, doc.scroll_to(line), rows_total(doc)]
    steps += [f"expect rows first_visible {line - 1} spawned"]
    at = (line, len(doc.line(line)) + 1)
    steps += [f"click {doc.end(line)}", caret(at)]
    at = doc.insert(at, "x" * 200)
    steps += [window(f"editor-{name}-type.csv", ["type x"] * 200), doc.text(line), caret(at)]
    for _ in range(50):
        at = doc.enter(at)
    steps += [window(f"editor-{name}-enter.csv", ["key enter"] * 50), rows_total(doc), caret(at)]
    steps += [doc.scroll_to(line)]
    joined, at = joins(doc, line + 1, 50)
    steps += [window(f"editor-{name}-join.csv", joined), rows_total(doc), SEAMS_CLEAN]
    steps += [doc.text(line), doc.text(line + 1), caret(at)]
    assert doc.rows() == rows
    editor(
        name,
        steps,
        "log typing",
        "expect typing hidden 0",
        "log caret",
        "log rows",
        "wait 30",
        f"screenshot e2-{name.removeprefix('editor-')}.png",
    )


keystrokes("10k", LARGE_RS_LINES, len(LARGE_RS_LINES) + 1, TEXT_X, [], 5000)
keystrokes("100k", STRESS_LINES, 100_001, TEXT_X_100K, [STRESS, "wait 60"], 50_000)

doc = Doc(LARGE_RS_LINES)
LINE = 300
assert max(map(len, doc.lines[:LINE])) < WRAP_COLUMNS
WORDS = ["wrap"] * ((5 * WRAP_COLUMNS // 2 - len(doc.line(LINE))) // 5)
HALF = (3 * WRAP_COLUMNS // 2 - len(doc.line(LINE))) // 5
top = (LINE - 1) * ROW


def wrapped(lines):
    return [
        f"log row {LINE}",
        f"expect row {LINE} top {top} height {lines * ROW} lines {lines}",
        f"log row {LINE + 1}",
        f"expect row {LINE + 1} top {top + lines * ROW} height {ROW} lines 1",
    ]


steps = [WRAP, "wait 60", doc.scroll_to(LINE - 10)]
steps += [f"click {doc.end(LINE)}", caret((LINE, len(doc.line(LINE)) + 1)), wrapped(1)]
for part, lines in [(WORDS[:HALF], 2), (WORDS[HALF:], 3)]:
    doc.insert((LINE, len(doc.line(LINE)) + 1), " " + " ".join(part))
    assert (lines - 1) * WRAP_COLUMNS < len(doc.line(LINE)) < lines * WRAP_COLUMNS
    steps += ["key space", f"type {' '.join(part)}", "wait 10"]
    steps += [doc.text(LINE) if lines == 3 else [], wrapped(lines)]
steps += ["log seams", "expect seams gaps 0 overlaps 0"]
editor("wrap", steps, "wait 30", "screenshot e2-wrap.png")


def accept(name, lines, text_x, setup, line):
    """Type, move, select, copy, paste and undo on `line`, checking each."""
    doc = Doc(lines, text_x=text_x)
    steps = [setup, doc.scroll_to(line - 5)]
    at = (line, len(doc.line(line)) + 1)
    steps += [f"click {doc.end(line)}", caret(at)]
    at = doc.insert(at, "x" * 100)
    steps += [window(f"editor-acceptance-{name}.csv", ["type x"] * 100), doc.text(line), caret(at), undo(1, 0)]
    at = (line, doc.home(*at))
    steps += ["key home", caret(at)]
    head = at
    for _ in range(2):
        head = (head[0] + 1, min(at[1], len(doc.line(head[0] + 1)) + 1))
        steps += ["key shift+down", caret(at, head)]
    copied = doc.between(at, head)
    steps += ["key mod+c", "log copied", f"expect copied {rust_debug(copied)}"]
    target = line + 10
    before = list(doc.lines)
    steps += [f"click {doc.gutter(target)}", caret((target, 1))]
    after = doc.insert((target, 1), copied)
    pasted = range(target, target + copied.count("\n") + 2)
    steps += ["key mod+v", "wait 5", [doc.text(n) for n in pasted], caret(after), undo(2, 0)]
    doc.lines = before
    steps += ["key mod+z", "wait 5", [doc.text(n) for n in pasted], undo(1, 1)]
    return steps


editor(
    "acceptance",
    accept("10k", LARGE_RS_LINES, TEXT_X, [], 5004),
    "wait 30",
    "screenshot e2-acceptance-10k.png",
    accept("100k", STRESS_LINES, TEXT_X_100K, [STRESS, "wait 60"], 50_004),
    "wait 30",
    "screenshot e2-acceptance-100k.png",
)


EDIT_FIXTURES = Path(__file__).parent / "fixtures/edit"
COMMENT_PY = ["def greet(name):", '    print("hello", name)']
COMMENT_JSON = ["{", '  "name": "rubrix"', "}"]
INDENT = "    "
TONE_LINES = range(23, 28)
TONE_COLUMN = 13


def selections(sels):
    """`log caret` for several selections, each a (start, end) pair in
    buffer order, the last of them the newest and so the primary."""
    each = " ".join(f"{a[0]}:{a[1]}..{b[0]}:{b[1]}" for a, b in sels)
    return ["log caret", f"expect caret {len(sels)} primary {len(sels)} {each}"]


def carets_at(points):
    return selections([(p, p) for p in points])


def replace(doc, sels, texts):
    """Puts each text in place of its one-line selection, as typing or a
    line-per-caret paste does, and returns the carets after the texts."""
    shift, after = {}, []
    for ((n, a), (m, b)), text in sorted(zip(sels, texts)):
        assert n == m and "\n" not in text, (n, m, text)
        s = shift.get(n, 0)
        line = doc.line(n)
        doc.lines[n - 1] = line[: a - 1 + s] + text + line[b - 1 + s :]
        shift[n] = s + len(text) - (b - a)
        after.append((n, a + s + len(text)))
    return after


def next_occurrence(doc, needle, after):
    """The next `needle` at or after `after`, as `mod+d` finds it. Each
    one found must be a whole word, so a substring search and a
    whole-word search agree on it."""
    n, column = after
    while True:
        line = doc.line(n)
        i = line.find(needle, column - 1)
        if i >= 0:
            j = i + len(needle)
            assert not (i > 0 and word_char(line[i - 1])), (n, line)
            assert not (j < len(line) and word_char(line[j])), (n, line)
            return (n, i + 1), (n, j + 1)
        n, column = n + 1, 1


def leading(line):
    return line[: len(line) - len(line.lstrip())]


def indent_lines(doc, lines):
    for n in lines:
        line = doc.line(n)
        if line.strip():
            doc.lines[n - 1] = ("\t" if line.startswith("\t") else INDENT) + line


def outdent_lines(doc, lines):
    for n in lines:
        line = doc.line(n)
        cut = 1 if line.startswith("\t") else min(len(INDENT), len(line) - len(line.lstrip(" ")))
        doc.lines[n - 1] = line[cut:]


def toggle_comment(doc, lines, token):
    marker = token + " "
    indent = min(len(leading(doc.line(n))) for n in lines)
    commented = all(doc.line(n)[indent:].startswith(marker) for n in lines)
    for n in lines:
        line = doc.line(n)
        if commented:
            doc.lines[n - 1] = line[:indent] + line[indent + len(marker) :]
        else:
            doc.lines[n - 1] = line[:indent] + marker + line[indent:]


def move_lines(doc, first, last, down):
    block = doc.lines[first - 1 : last]
    if down:
        doc.lines[first - 1 : last + 1] = [doc.line(last + 1), *block]
    else:
        doc.lines[first - 2 : last] = [*block, doc.line(first - 1)]


def copy_lines_down(doc, first, last):
    doc.lines[last:last] = doc.lines[first - 1 : last]


def closing_brace(doc, n):
    """The `}` that closes the `{` ending line `n`, where no string,
    char or comment between them holds a brace."""
    depth = 0
    while True:
        line = doc.line(n)
        assert '"' not in line and "'" not in line and "//" not in line, (n, line)
        for i, c in enumerate(line):
            depth += {"{": 1, "}": -1}.get(c, 0)
            if depth == 0 and c == "}":
                return (n, i + 1)
        n += 1


def texts(doc, lines):
    return [doc.text(n) for n in lines]


def select_tone_block(doc):
    """Selects from the `T` of the first `Tone::` line to the end of the
    third, by a click and a shift click."""
    block = TONE_LINES[:3]
    start, end = (block[0], TONE_COLUMN), (block[-1], len(doc.line(block[-1])) + 1)
    return block, start, end, [f"click {doc.at(*start)}", f"click {doc.end(block[-1])} shift", caret(start, end)]


EDIT_FIXTURES.mkdir(exist_ok=True)
(EDIT_FIXTURES / "comment.py").write_text("\n".join(COMMENT_PY) + "\n")
(EDIT_FIXTURES / "comment.json").write_text("\n".join(COMMENT_JSON) + "\n")

doc = Doc(LARGE_RS_LINES)
block, start, end, steps = select_tone_block(doc)
commands = [("tab", indent_lines), ("shift+tab", outdent_lines), ("mod+]", indent_lines), ("mod+[", outdent_lines)]
for depth, (key, command) in enumerate(commands, start=1):
    width = len(doc.line(block[0]))
    command(doc, block)
    by = len(doc.line(block[0])) - width
    start, end = (start[0], start[1] + by), (end[0], end[1] + by)
    steps += [f"key {key}", "wait 5", texts(doc, block), caret(start, end), undo(depth, 0)]
SOFT_TAB = TONE_LINES[-1]
assert doc.line(SOFT_TAB).startswith(" " * 8)
doc.lines[SOFT_TAB - 1] = doc.line(SOFT_TAB)[4:]
steps += [f"click {doc.at(SOFT_TAB, 9)}", caret((SOFT_TAB, 9)), "key backspace", "wait 5"]
steps += [doc.text(SOFT_TAB), caret((SOFT_TAB, 5)), undo(len(commands) + 1, 0)]
editor("indent", steps, "wait 30", "screenshot e4-indent.png")

doc = Doc(LARGE_RS_LINES)
first, last = TONE_LINES[0], TONE_LINES[1]
start, end = (first, TONE_COLUMN), (last, TONE_COLUMN + 7)
steps = [f"click {doc.at(*start)}", f"click {doc.at(*end)} shift", caret(start, end)]
original = list(doc.lines)
around = range(first - 2, last + 4)
for key, by, change in [
    ("alt+down", 1, lambda: move_lines(doc, first, last, True)),
    ("alt+up", -1, lambda: move_lines(doc, first, last, False)),
    ("shift+alt+down", last - first + 1, lambda: copy_lines_down(doc, first, last)),
]:
    change()
    shifted = caret((start[0] + by, start[1]), (end[0] + by, end[1]))
    steps += [f"key {key}", "wait 5", texts(doc, around), shifted, undo(1, 0)]
    doc.lines = list(original)
    steps += ["key mod+z", "wait 5", texts(doc, around), caret(start, end), undo(0, 1)]
indent = leading(doc.line(end[0]))
doc.lines.insert(end[0], indent)
steps += ["key mod+enter", "wait 5", texts(doc, around), caret((end[0] + 1, len(indent) + 1)), undo(1, 0)]
doc.lines = list(original)
steps += ["key mod+z", "wait 5", texts(doc, around), caret(start, end), undo(0, 1)]
editor("move-lines", steps, "wait 30", "screenshot e4-move-lines.png")

doc = Doc(LARGE_RS_LINES)
block, start, end, steps = select_tone_block(doc)
for depth in (1, 2):
    toggle_comment(doc, block, "//")
    steps += ["key mod+/", "wait 5", texts(doc, block), undo(depth, 0)]
doc.lines[block[-1] - 1] += " */"
doc.lines[block[0] - 1] = doc.line(block[0])[: TONE_COLUMN - 1] + "/* " + doc.line(block[0])[TONE_COLUMN - 1 :]
steps += ["key shift+alt+a", "wait 5", texts(doc, block), undo(3, 0)]
py_doc = Doc(COMMENT_PY)
toggle_comment(py_doc, [2], "#")
steps += ["open ../fixtures/edit/comment.py", "wait 10", f"click {py_doc.end(2)}", "key mod+/", "wait 5"]
steps += [texts(py_doc, [1, 2]), undo(1, 0)]
json_doc = Doc(COMMENT_JSON)
steps += ["open ../fixtures/edit/comment.json", "wait 10", f"click {json_doc.end(2)}", "key mod+/", "wait 5"]
steps += [texts(json_doc, [1, 2, 3]), undo(0, 0)]
editor("comment", steps, "wait 30", "screenshot e4-comment.png")

doc = Doc(LARGE_RS_LINES)
LET = 629
steps = [doc.scroll_to(LET - 9)]
found = [next_occurrence(doc, "let", (LET, 1))]
inside = (LET, found[0][0][1] + 1)
steps += [f"click {doc.at(*inside)}", caret(inside), "key mod+d", selections(found)]
for _ in range(2):
    found.append(next_occurrence(doc, "let", found[-1][1]))
    steps += ["key mod+d", selections(found)]
after = replace(doc, found, ["var"] * len(found))
steps += ["type var", "wait 5", texts(doc, [n for (n, _), _ in found]), carets_at(after)]
steps += ["wait 30", "screenshot e4-regress.png", "key escape", caret(after[-1])]
editor("occurrence", steps, "wait 30", "screenshot e4-occurrence.png")

doc = Doc(LARGE_RS_LINES)
left, right = TONE_COLUMN, TONE_COLUMN + len("Tone::")
assert all(doc.line(n)[left - 1 : right - 1] == "Tone::" for n in TONE_LINES)
across = [((n, left), (n, right)) for n in TONE_LINES]
steps = [f"drag {doc.at(TONE_LINES[0], left)} {doc.at(TONE_LINES[-1], right)} alt", "wait 5", selections(across)]
after = replace(doc, across, ["x"] * len(across))
steps += ["type x", "wait 5", texts(doc, TONE_LINES), carets_at(after), undo(1, 0)]
editor("column", steps, "wait 30", "screenshot e4-column.png")

doc = Doc(LARGE_RS_LINES)
points = [(n, TONE_COLUMN) for n in TONE_LINES[:4]]
steps = [f"click {doc.at(*points[0])}", caret(points[0])]
for k in range(2, len(points) + 1):
    steps += [f"click {doc.at(*points[k - 1])} mod", carets_at(points[:k])]
steps += [f"click {doc.at(*points[1])} mod", carets_at([points[0], *points[2:]])]
editor("mod-click", steps, "wait 30", "screenshot e4-mod-click.png")

doc = Doc(LARGE_RS_LINES)
LINE = 16
line = doc.line(LINE) + "\n"
steps = [f"click {doc.at(LINE, 5)}", caret((LINE, 5)), "key mod+c", "clipboard"]
steps += ["log copied", f"expect copied {rust_debug(line)}", undo(0, 0)]
del doc.lines[LINE - 1]
steps += ["key mod+x", "wait 5", doc.text(LINE), rows_total(doc), "log copied", f"expect copied {rust_debug(line)}", undo(1, 0)]
points = [(n - 1, TONE_COLUMN) for n in TONE_LINES[:3]]
steps += [f"click {doc.at(*points[0])}", [f"click {doc.at(*p)} mod" for p in points[1:]], carets_at(points)]
after = replace(doc, [(p, p) for p in points], ["a", "b", "c"])
steps += ["paste a\\nb\\nc", "wait 5", texts(doc, [n for n, _ in points]), carets_at(after), undo(2, 0)]
editor("line-clipboard", steps, "wait 30", "screenshot e4-line-clipboard.png")

doc = Doc(LARGE_RS_LINES)
LINE = TONE_LINES[0]
word = doc.line(LINE).index("Body") + 1
start, end = word_around(doc.line(LINE), word + 1)
steps = [f"click {doc.at(LINE, word + 1)} double", caret((LINE, start), (LINE, end))]
original = doc.line(LINE)
for opening, closing in ["()", "[]", "{}", '""', "''", "``"]:
    doc.lines[LINE - 1] = original[: start - 1] + opening + original[start - 1 : end - 1] + closing + original[end - 1 :]
    steps += [f"type {opening}", "wait 5", doc.text(LINE), caret((LINE, start + 1), (LINE, end + 1)), undo(1, 0)]
    doc.lines[LINE - 1] = original
    steps += ["key mod+z", "wait 5", doc.text(LINE), caret((LINE, start), (LINE, end)), undo(0, 1)]
editor("surround", steps, "wait 30", "screenshot e4-surround.png")

doc = Doc(LARGE_RS_LINES)
FN = 21
assert doc.line(FN).endswith("{")
column = len(doc.line(FN)) + 1
close = closing_brace(doc, FN)
steps = [f"click {doc.end(FN)}", "log caret"]
steps += [f"expect caret 1 primary 1 {FN}:{column}..{FN}:{column} visible true bracket {close[0]}:{close[1]}"]
LINE = 9506
column = doc.line(LINE).index('"(') + 3
steps += [doc.scroll_to(LINE - 6), f"click {doc.at(LINE, column)}"]
steps += [f"log kind {LINE} {column - 2}", f"expect kind {LINE} {column - 2} string"]
steps += ["log caret", f"expect caret 1 primary 1 {LINE}:{column}..{LINE}:{column} visible true bracket none"]
editor("brackets", steps, "wait 30", "screenshot e4-brackets.png")

doc = Doc(LARGE_RS_LINES)
CARETS = 100
found = [next_occurrence(doc, "let", (1, 1))]
while len(found) <= CARETS:
    found.append(next_occurrence(doc, "let", found[-1][1]))
beyond = found.pop()
inside = (found[0][0][0], found[0][0][1] + 1)
steps = [f"click {doc.at(*inside)}", caret(inside), ["key mod+d"] * CARETS, "wait 5", selections(found)]
after = replace(doc, found, ["x" * CARETS] * CARETS)
steps += [window("editor-occurrence-100.csv", ["type x"] * CARETS), "wait 5", carets_at(after)]
steps += [texts(doc, sorted({n for (n, _), _ in [found[0], found[-1], beyond]}))]
editor("occurrence-100", steps, "wait 30", "screenshot e4-occurrence-100.png")

doc = Doc(STRESS_LINES, text_x=TEXT_X_100K)
FIRST, LINES, MOVES = 50_000, 1_000, 100
last = FIRST + LINES - 1
assert len(doc.line(last)) < (LINE_END_X - TEXT_X_100K) / ADVANCE
end = (last, len(doc.line(last)) + 1)
steps = [STRESS, "wait 60", doc.scroll_to(FIRST), f"click {doc.at(FIRST, 1)}", doc.scroll_to(last - 9)]
steps += [f"click {doc.end(last)} shift", caret((FIRST, 1), end), rows_total(doc)]
for move in range(MOVES):
    move_lines(doc, FIRST + move, last + move, True)
steps += [window("editor-move-1000.csv", ["key alt+down"] * MOVES), "wait 5", rows_total(doc)]
steps += [selections([((FIRST + MOVES, 1), (last + MOVES, end[1]))])]
steps += [texts(doc, [FIRST, FIRST + MOVES - 1, FIRST + MOVES, last + MOVES, last + MOVES + 1]), SEAMS_CLEAN]
editor("move-1000", steps, "wait 30", "screenshot e4-move-1000.png")


IME_LINE, IME_COLUMN = 120, 5
COMPOSED = "你好"
CARET_BOTTOM = 9


def ime_debug(text):
    """`{:?}` of text with no control character, as Rust prints it."""
    assert all(c >= " " and c != "\x7f" for c in text), text
    return '"' + text.replace("\\", "\\\\").replace('"', '\\"') + '"'


def ime_text(doc, n):
    return [f"log text {n}", f"expect text {n} {ime_debug(doc.line(n))}"]


def ime(*expects):
    return ["log ime", [f"expect ime {expect}" for expect in expects]]


def preedit(text, cursor=None):
    cursor = len(text.encode()) if cursor is None else cursor
    return f"ime preedit {text} {cursor} {cursor}"


def committed(text):
    """winit's order on Wayland: an empty preedit, the commit, and the
    next preedit, here empty."""
    return ["ime preedit", f"ime commit {text}", "ime preedit"]


def ime_editor(name, *steps):
    editor(f"ime-{name}", *steps)


def at_ime_line(doc):
    return [
        doc.scroll_to(IME_LINE),
        f"click {doc.at(IME_LINE, IME_COLUMN)}",
        caret((IME_LINE, IME_COLUMN)),
    ]


def composing(doc):
    """A preedit of `COMPOSED` at the caret, which the text does not hold."""
    shown = f'preedit {ime_debug(COMPOSED)} cursor 6..6'
    return [preedit(COMPOSED), "wait 5", ime("enabled true", f"page {shown}"), ime_text(doc, IME_LINE), undo(0, 0)]


doc = Doc(LARGE_RS_LINES)
steps = [at_ime_line(doc), undo(0, 0), "ime enable", ime("enabled true", "page preedit none")]
steps += [composing(doc), caret((IME_LINE, IME_COLUMN))]
ime_editor("preedit", steps, "wait 30", "screenshot e3-preedit.png")

doc = Doc(LARGE_RS_LINES)
steps = [at_ime_line(doc), composing(doc), committed(COMPOSED), "wait 5"]
after = doc.insert((IME_LINE, IME_COLUMN), COMPOSED)
steps += [ime_text(doc, IME_LINE), caret(after), ime("page preedit none"), undo(1, 0)]
ime_editor("commit", steps, "wait 30", "screenshot e3-commit.png")

doc = Doc(LARGE_RS_LINES)
steps = [at_ime_line(doc), composing(doc), committed(COMPOSED), "wait 5"]
before = list(doc.lines)
after = doc.insert((IME_LINE, IME_COLUMN), COMPOSED)
steps += [ime_text(doc, IME_LINE), undo(1, 0)]
done = list(doc.lines)
doc.lines = before
steps += ["key mod+z", "wait 5", ime_text(doc, IME_LINE), caret((IME_LINE, IME_COLUMN)), undo(0, 1)]
doc.lines = done
steps += ["key mod+shift+z", "wait 5", ime_text(doc, IME_LINE), caret(after), undo(1, 0)]
ime_editor("undo", steps, "wait 30", "screenshot e3-undo.png")

doc = Doc(LARGE_RS_LINES)
steps = [at_ime_line(doc), composing(doc), "ime preedit", "wait 5"]
steps += [ime_text(doc, IME_LINE), ime("page preedit none"), undo(0, 0)]
steps += [composing(doc), "ime disable", "wait 5"]
steps += [ime_text(doc, IME_LINE), ime("page preedit none"), undo(0, 0)]
ime_editor("cancel", steps, "wait 30", "screenshot e3-cancel.png")

doc = Doc(LARGE_RS_LINES)
steps = [at_ime_line(doc), composing(doc), "type x", "key backspace", "wait 5"]
steps += [ime_text(doc, IME_LINE), caret((IME_LINE, IME_COLUMN)), undo(0, 0)]
after = doc.insert((IME_LINE, IME_COLUMN), COMPOSED)
steps += [committed(COMPOSED), "type y", "wait 5"]
after = doc.insert(after, "y")
steps += [ime_text(doc, IME_LINE), caret(after)]
ime_editor("keys", steps, "wait 30", "screenshot e3-keys.png")


def candidate_box(doc, at, dy=0):
    """The window's IME position and the page's caret, both at the
    bottom-left of the caret at `at` on an ASCII line."""
    n, column = at
    x = round(doc.text_x + (column - 1) * ADVANCE)
    y = doc.y(n) + CARET_BOTTOM + dy
    return ime("enabled true", f"position {x} {y} last", f"caret {x} {y}")


IME_TOP = IME_LINE - 20
SCROLLED = 200
doc = Doc(LARGE_RS_LINES)
steps = [doc.scroll_to(IME_TOP), f"click {doc.at(IME_LINE, IME_COLUMN)}", "wait 2"]
at = (IME_LINE, IME_COLUMN)
steps += [caret(at), candidate_box(doc, at)]
for _ in range(10):
    n = at[0] + 1
    at = (n, min(IME_COLUMN, len(doc.line(n)) + 1))
    steps += ["key down", "wait 2", caret(at), candidate_box(doc, at)]
steps += [CENTER, f"scroll 0 -{SCROLLED}", "wait 10", candidate_box(doc, at, -SCROLLED)]
ime_editor("position", steps, "wait 30", "screenshot e3-position.png")

steps = [CENTER, "click 740 400", "wait 2", ime("enabled true")]
steps += ["page code", "wait 10", ime("enabled false"), "page editor", SETTLE]
steps += ["click 740 400", "wait 2", ime("enabled true")]
ime_editor("focus", steps, "wait 30", "screenshot e3-focus.png")

IME_100K_LINE = 50_000
GROWN = "你好世界" * 5
ROUNDS = 5
doc = Doc(STRESS_LINES, text_x=TEXT_X_100K)
steps = [STRESS, "wait 60", doc.scroll_to(IME_100K_LINE)]
at = (IME_100K_LINE, len(doc.line(IME_100K_LINE)) + 1)
steps += [f"click {doc.end(IME_100K_LINE)}", caret(at)]
grow = [[preedit(GROWN[:k]) for k in range(1, len(GROWN) + 1)], "ime preedit"] * ROUNDS
steps += ["frames start", "wait 60", grow, "frames stop editor-ime-100k.csv"]
steps += [preedit(GROWN), committed(GROWN), "wait 5"]
at = doc.insert(at, GROWN)
steps += [ime_text(doc, IME_100K_LINE), caret(at), undo(1, 0)]
ime_editor("100k", steps, "wait 30", "screenshot e3-100k.png")

doc = Doc(LARGE_RS_LINES)
steps = [at_ime_line(doc), "wtype nihao", "wait 30", ime("last preedit"), "wtype 1", "wait 30"]
doc.insert((IME_LINE, IME_COLUMN), COMPOSED)
steps += [ime_text(doc, IME_LINE), ime("page preedit none")]
write(
    "editor-ime-live",
    "# Needs fcitx5 Pinyin and launch.sh --ime-live, which gives the gallery the keyboard focus.",
    "page editor",
    SETTLE,
    steps,
    "wait 30",
    "screenshot e3-live-pinyin.png",
    "exit",
)

import re

FIND_FIELD = "click 745 151"
MATCH_CASE, WHOLE_WORD, REGEX = "click 873 151", "click 902 151", "click 930 151"
REPLACE_FIELD, REPLACE_ONE, REPLACE_ALL = "click 745 179", "click 894 179", "click 986 179"
SPAWNED_ABOVE, SPAWNED_BELOW = 24, 62
LARGE_RS_TEXT = "\n".join(LARGE_RS_LINES) + "\n"
STRESS_TEXT = "\n".join(STRESS_LINES) + "\n"
MAX_MATCHES = 100_000


def find_all(text, query, case=False, word=False, regex=False):
    flags = re.MULTILINE | (0 if case else re.IGNORECASE)
    pattern = re.compile(query if regex else re.escape(query), flags)
    found, at = [], 0
    while len(found) < MAX_MATCHES and (m := pattern.search(text, at)):
        s, e = m.span()
        whole = not word or not (s > 0 and word_char(text[s - 1]) or e < len(text) and word_char(text[e]))
        if s < e and whole:
            found.append((s, e))
            at = e
        else:
            at = s + 1
            if at > len(text):
                break
    return found


def find_place(text, offset):
    line = text.count("\n", 0, offset) + 1
    return (line, offset - (text.rfind("\n", 0, offset) + 1) + 1)


def selected(text, span):
    return caret(find_place(text, span[0]), find_place(text, span[1]))


def find_log(query, index, count, capped=False, marked=None):
    shown = f'open query {rust_debug(query)} {index} of {count}{" capped" if capped else ""}'
    return ["log find", f"expect find {shown} marked {marked}" if marked is not None else f"expect find {shown}"]


def counted(query, count):
    return ["log find", f"expect find open query {rust_debug(query)}", f"expect find of {count} marked"]


def marked(text, found, first_visible):
    lines = range(first_visible - SPAWNED_ABOVE + 1, first_visible + SPAWNED_BELOW + 2)
    hit = set()
    for s, e in found:
        a, b = find_place(text, s)[0], find_place(text, max(s, e - 1))[0]
        hit.update(n for n in range(a, b + 1) if n in lines)
    return len(hit)


def index_at(found, offset):
    return next(i for i, (s, _) in enumerate(found) if s >= offset)


def find_script(name, *steps):
    editor(f"find-{name}", *steps)


THEMES = find_all(LARGE_RS_TEXT, "theme")
GREP_THEME = f"# grep -oi theme large.rs.txt | wc -l -> {len(THEMES)}"


def opened_on_theme(line, column):
    doc = Doc(LARGE_RS_LINES)
    at = sum(len(l) + 1 for l in LARGE_RS_LINES[: line - 1]) + column - 1
    current = next(i for i, (s, e) in enumerate(THEMES) if s <= at < e)
    steps = [doc.scroll_to(line - 6), f"click {doc.at(line, column + 2)}", "key mod+f", "wait 10"]
    return doc, current, steps


doc, current, steps = opened_on_theme(676, 16)
top = 676 - 6
steps += [GREP_THEME, find_log("theme", current + 1, len(THEMES), marked=marked(LARGE_RS_TEXT, THEMES, top - 1))]
steps += [selected(LARGE_RS_TEXT, THEMES[current]), "wait 30", "screenshot e5-open.png"]
find_script("open", steps)

doc = Doc(LARGE_RS_LINES)
first = THEMES[0]
line, column = find_place(LARGE_RS_TEXT, first[0])
steps = [f"click {doc.at(line, column + 2)}", "key mod+f", "wait 10", GREP_THEME, find_log("theme", 1, len(THEMES))]
for key, index in [("enter", 2), ("mod+g", 3), ("shift+enter", 2), ("mod+shift+g", 1)]:
    steps += [f"key {key}", "wait 10", find_log("theme", index, len(THEMES)), selected(LARGE_RS_TEXT, THEMES[index - 1])]
find_script("next", steps, "wait 30", "screenshot e5-next.png")

OPTIONS_QUERY = "tab."
steps = [f"click {Doc(LARGE_RS_LINES).at(1, 1)}", "key mod+f", "wait 5", f"type {OPTIONS_QUERY}", "wait 10"]
options = [
    ("", {}, "grep -oiF"),
    (MATCH_CASE, {"case": True}, "grep -oF"),
    (WHOLE_WORD, {"case": True, "word": True}, "grep -owF"),
    (REGEX, {"case": True, "word": True, "regex": True}, "grep -owE"),
]
counts = [len(find_all(LARGE_RS_TEXT, OPTIONS_QUERY, **flags)) for _, flags, _ in options]
assert len(set(counts)) == len(counts), "each toggle changes the count"
for (click, _, grep), count in zip(options, counts):
    steps += [click or [], "wait 10", f"# {grep} '{OPTIONS_QUERY}' large.rs.txt | wc -l -> {count}", "log find"]
    steps += [f"expect find open query {rust_debug(OPTIONS_QUERY)}", f"expect find of {count} marked"]
find_script("options", steps, "wait 30", "screenshot e5-options.png")

MULTILINE = r"\{\n\s+let"
LETS = find_all(LARGE_RS_TEXT, MULTILINE, regex=True)
first = LETS[0]
assert find_place(LARGE_RS_TEXT, first[1])[0] > find_place(LARGE_RS_TEXT, first[0])[0]
steps = [f"click {Doc(LARGE_RS_LINES).at(1, 1)}", "key mod+f", "wait 5", REGEX, "wait 5", f"type {MULTILINE}", "wait 20"]
steps += [f"# grep -Pzo '{MULTILINE}' large.rs.txt | grep -c let -> {len(LETS)}"]
steps += [find_log(MULTILINE, 1, len(LETS)), selected(LARGE_RS_TEXT, first)]
find_script("multiline", steps, "wait 30", "screenshot e5-multiline.png")

SELFS = find_all(LARGE_RS_TEXT, "self")
SCROLLED, MARKS_TOP = 4000, 3943
doc = Doc(LARGE_RS_LINES)
near = MARKS_TOP - 1
far = (near * ROW + SCROLLED) // ROW
steps = [doc.scroll_to(MARKS_TOP), f"click {doc.at(MARKS_TOP + 1, 1)}", "key mod+f", "wait 5", "type self", "wait 10"]
steps += [f"# grep -oi self large.rs.txt | wc -l -> {len(SELFS)}"]
current = index_at(SELFS, sum(len(l) + 1 for l in LARGE_RS_LINES[:MARKS_TOP]))
assert find_place(LARGE_RS_TEXT, SELFS[current][0])[0] < MARKS_TOP + 38
SPAWNED_ROWS = SPAWNED_ABOVE + SPAWNED_BELOW + 1
for dy, first_visible in [(-SCROLLED, far), (SCROLLED, near)]:
    spawned_window = f"{first_visible - SPAWNED_ABOVE}..={first_visible + SPAWNED_BELOW} rows {SPAWNED_ROWS} total"
    rows_marked = marked(LARGE_RS_TEXT, SELFS, first_visible)
    assert 0 < rows_marked <= SPAWNED_ROWS
    steps += [CENTER, f"scroll 0 {dy}", "wait 20", "log rows", f"expect rows first_visible {first_visible} spawned {spawned_window}"]
    steps += [find_log("self", current + 1, len(SELFS), marked=rows_marked), f"expect find marked {rows_marked} final"]
find_script("marks", steps, "wait 30", "screenshot e5-marks.png")

doc, current, steps = opened_on_theme(284, 24)
steps[2] = "key mod+alt+f"
replaced = "        let panel = cx.palette().panel();"
steps += ["type palette", "wait 5", "key enter", "wait 10", GREP_THEME]
steps += [counted("theme", len(THEMES) - 1), "log text 284", f"expect text 284 {rust_debug(replaced)}", undo(1, 0)]
steps += [f"click {doc.at(300, 1)}", "key mod+z", "wait 10", counted("theme", len(THEMES)), doc.text(284), undo(0, 1)]
steps += [REPLACE_ALL, "wait 10", counted("theme", 0), 'expect find label "No results"', undo(1, 0)]
steps += [f"click {doc.at(300, 1)}", "key mod+z", "wait 10", counted("theme", len(THEMES)), doc.text(284), undo(0, 1)]
find_script("replace", steps, "wait 30", "screenshot e5-replace.png")


def regex_replace(text, query, template, offset):
    found = find_all(text, query, regex=True)
    s, e = found[index_at(found, offset)]
    m = re.compile(query, re.MULTILINE | re.IGNORECASE).match(text, s)
    new = re.sub(r"\$(\$|&|\d+)", lambda g: "$" if g[1] == "$" else m[0] if g[1] == "&" else (m[int(g[1])] or ""), template)
    return text[:s] + new + text[e:], s, s + len(new)


doc = Doc(LARGE_RS_LINES)
start = 290
offset = sum(len(l) + 1 for l in LARGE_RS_LINES[: start - 1])
SWAP, FNS = r"(\w+)_(\w+)", "fn"
text, s, after = regex_replace(LARGE_RS_TEXT, SWAP, "$2_$1", offset)
swapped = find_place(text, s)[0]
text, s2, _ = regex_replace(text, FNS, "$&$&", after)
doubled = find_place(text, s2)[0]
lines = text.split("\n")
steps = [doc.scroll_to(start), f"click {doc.at(start, 1)}", "key mod+alt+f", "wait 5", REGEX, "wait 5"]
steps += [FIND_FIELD, "key mod+a", f"type {SWAP}", "wait 10", REPLACE_FIELD, "type $2_$1", "key enter", "wait 10"]
steps += [FIND_FIELD, "key mod+a", f"type {FNS}", "wait 10", REPLACE_FIELD, "key mod+a", "type $&$&", REPLACE_ONE, "wait 10"]
steps += [f"log text {swapped}", f"expect text {swapped} {rust_debug(lines[swapped - 1])}"]
steps += [f"log text {doubled}", f"expect text {doubled} {rust_debug(lines[doubled - 1])}"]
find_script("regex-replace", steps, "wait 30", "screenshot e5-regex-replace.png")

FIND_100K = "cx.theme()"
assert len(FIND_100K) == 10
HITS_100K = find_all(STRESS_TEXT, FIND_100K)
steps = [STRESS, "wait 60", f"click {TEXT_X_100K + CLICK_INSET} {LINE_1}", "key mod+f", "wait 10"]
steps += ["frames start", "wait 60", [[f"type {c}", "wait 9"] for c in FIND_100K], "wait 20", "frames stop editor-find-100k.csv"]
steps += ["wait 30", f"# grep -oiF '{FIND_100K}' 100k | wc -l -> {len(HITS_100K)}"]
steps += [find_log(FIND_100K, 1, len(HITS_100K))]
find_script("100k", steps, "wait 30", "screenshot e5-100k.png")

ES = find_all(STRESS_TEXT, "e")
assert len(ES) == MAX_MATCHES
line, column = find_place(STRESS_TEXT, ES[0][0])
typed = STRESS_LINES[line - 1][: column - 1] + "x" + STRESS_LINES[line - 1][column:]
steps = [STRESS, "wait 60", f"click {TEXT_X_100K + CLICK_INSET} {LINE_1}", "key mod+f", "wait 5", "type e", "wait 60"]
steps += [find_log("e", 1, MAX_MATCHES, capped=True), f'expect find label "1 of {MAX_MATCHES}+"']
steps += ["key escape", "wait 10", "log find", 'expect find closed', "expect find marked 0", "type x", "wait 5"]
steps += [f"log text {line}", f"expect text {line} {rust_debug(typed)}"]
find_script("close", steps, "wait 30", "screenshot e5-close.png")

TOGGLE_100K, MARKERS, ANNOTATE, FIFTY_CARETS, RESPAWN = BUTTONS
THOUSAND_MARKERS = "click 1041 68"


def collab(name, *steps):
    write(f"collab-{name}", "page collab", SETTLE, *steps, "exit")


def remote(*expected):
    return ["log remote", [f"expect remote {text}" for text in expected]]


def place(at):
    return f"{at[0]}:{at[1]}"


def trimmed(doc, n):
    line = doc.line(n)
    return (n, len(line) - len(line.lstrip(" ")) + 1), (n, len(line) + 1)


doc = Doc(LARGE_RS_LINES)
ada, bo = (120, 5), ((123, 3), (123, 12))
tinted = ((120, 5), (120, 12))
collab(
    "caret",
    doc.scroll_to(115),
    f"remote Ada {place(ada)}",
    f"remote Bo {place(bo[0])} {place(bo[1])}",
    "wait 5",
    remote(f"Ada {place(ada)} shown", f"Bo {place(bo[0])}..{place(bo[1])} shown"),
    "screenshot e6-caret.png",
    f"remote Ada {place(tinted[0])} {place(tinted[1])}",
    "wait 5",
    remote(f"Ada {place(tinted[0])}..{place(tinted[1])} shown"),
    "screenshot e6-tint.png",
    f"click {doc.at(120, 1)}",
    "type ab",
    "wait 5",
    remote(f"Ada 120:7..120:14 shown"),
    "screenshot e6-tint-pushed.png",
)

doc = Doc(LARGE_RS_LINES)
remap = [doc.scroll_to(95), "remote Ada 120:5", "wait 5", remote("Ada 120:5 shown")]
remap += [f"click {doc.at(120, 1)}", "type xy", "wait 5", remote("Ada 120:7 shown")]
remap += [f"click {doc.at(120, 7)}", "type z", "wait 5", remote("Ada 120:8 shown")]
remap += [f"click {doc.at(100, 1)}", "key enter", "wait 5", remote("Ada 121:8 shown"), undo(3, 0)]
remap += ["key mod+z", "key mod+z", "key mod+z", "wait 5", remote("Ada 120:5 shown"), undo(0, 3)]
collab("remap", remap, "screenshot e6-remap.png")

doc = Doc(LARGE_RS_LINES)
collab(
    "edge",
    "remote Ada 9000:1",
    "wait 5",
    remote("Ada edge bottom at 9000:1"),
    "screenshot e6-edge-bottom.png",
    doc.scroll_to(8990),
    "wait 5",
    remote("Ada 9000:1 shown"),
    doc.scroll_to(9020),
    "wait 5",
    remote("Ada edge top at 9000:1"),
    "screenshot e6-edge.png",
)

doc = Doc(LARGE_RS_LINES)
changed = [f"click {doc.end(10)}", "type a", "type b", "type c", "key enter", "key mod+z", "wait 5"]
collab(
    "changed",
    changed,
    "log changed",
    "expect changed changed 5 rising true",
    undo(1, 1),
    "screenshot e6-changed.png",
)


def marker(severity, span):
    return f"{severity} {place(span[0])}..{place(span[1])}"


doc = Doc(LARGE_RS_LINES)
first, second, third = (trimmed(doc, n) for n in (130, 131, 132))
moved = ((130, first[0][1] + 2), (130, first[1][1] + 2))
collab(
    "markers",
    doc.scroll_to(120),
    MARKERS,
    "wait 5",
    "log markers",
    f"expect markers markers 3: {marker('error', first)}, {marker('warning', second)}, {marker('info', third)}",
    f"click {doc.at(130, 1)}",
    "type ab",
    f"click {doc.at(*second[0])}",
    "key shift+end",
    "key backspace",
    "wait 5",
    "log markers",
    f"expect markers markers 2: {marker('error', moved)}, {marker('info', third)}",
    "screenshot e6-markers.png",
)

doc = Doc(LARGE_RS_LINES)
collab(
    "annotations",
    ANNOTATE,
    "wait 5",
    "log annotations",
    "expect annotations annotations 1: 200 review",
    doc.scroll_to(140),
    f"click {doc.end(150)}",
    "key enter",
    "wait 5",
    "log annotations",
    "expect annotations annotations 1: 201 review",
    "key mod+z",
    "wait 5",
    "log annotations",
    "expect annotations annotations 1: 200 review",
    doc.scroll_to(185),
    "screenshot e6-annotations.png",
)

doc = Doc(LARGE_RS_LINES)
original = [doc.text(120), doc.text(121)]
steps = [doc.scroll_to(110), f"click {doc.end(120)}"]
at = doc.insert((120, len(doc.line(120)) + 1), "abc")
at = doc.enter(at)
steps += ["type abc", "key enter", "wait 5", doc.text(120), doc.text(121), undo(2, 0)]
steps += [RESPAWN, "wait 30", doc.text(120), doc.text(121), undo(2, 0)]
doc.top, doc.scrolled = 1, False
steps += [f"click {doc.at(5, 1)}", "key mod+z", "key mod+z", "wait 5", original, undo(0, 2)]
collab("respawn", steps, "screenshot e6-respawn.png")

doc = Doc(LARGE_RS_LINES)
copied = doc.line(120)
steps = [doc.scroll_to(115), f"click {doc.at(120, 1)}", "key shift+end", "key mod+c", "clipboard"]
steps += ["log copied", f"expect copied {rust_debug(copied)}"]
steps += ["log paste", f"expect paste {rust_debug('collab: ' + copied)}"]
steps += [f"click {doc.end(125)}"]
doc.insert((125, len(doc.line(125)) + 1), "one\ntwo")
steps += ["paste one\\ntwo", "wait 5", doc.text(125), doc.text(126)]
steps += ["log paste", f"expect paste {rust_debug('collab: one' + chr(10) + 'two')}"]
collab("clipboard", steps, "screenshot e6-clipboard.png")

doc = Doc(STRESS_LINES, text_x=TEXT_X_100K)
LINE = 50_000
start = (LINE, 9)
line_end = len(doc.line(LINE)) + 1
carets = [start[1] + (line_end - start[1]) * i // 49 for i in range(50)]
steps = [TOGGLE_100K, "wait 60", doc.scroll_to(LINE), rows_total(doc)]
steps += [f"click {doc.at(*start)}", caret(start), FIFTY_CARETS, THOUSAND_MARKERS, "wait 5"]
steps += [f"click {doc.at(LINE + 1, 1)}", f"click {doc.at(*start)}", caret(start)]
steps += [remote("remote 50: ", *(f"u{i + 1} {LINE}:{column} shown" for i, column in enumerate(carets)))]
steps += ["log markers", "expect markers markers 1000: "]
steps += [window("collab-100k-type.csv", ["type x"] * 200)]
doc.insert(start, "x" * 200)
steps += [doc.text(LINE), caret((LINE, start[1] + 200))]
steps += [remote("remote 50: ", *(f"u{i + 1} {LINE}:{column + 200} " for i, column in enumerate(carets)))]
collab("100k", steps, "log typing", "log rows", "wait 30", "screenshot e6-100k.png")
