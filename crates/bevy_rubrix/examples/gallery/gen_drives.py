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


# The editor page lays out as the code page does, so its rows sit
# where the code page's do: LINE_1, TEXT_X and GUTTER_X carry over.
ADVANCE = 8.4
# A click this far right of a column's left edge lands before that
# column at scale 1 and at scale 2.
CLICK_INSET = 2
# A 100,001-row doc has a six-digit gutter, one advance wider.
TEXT_X_100K = TEXT_X + ADVANCE
# Measured live at 1280 by 800: a Page Down moves the 38 rows that
# show whole, a visual line under wrap on holds 113 chars, and a
# triple click takes the line break.
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
WORDS = " ".join(["wrap"] * ((5 * WRAP_COLUMNS // 2 - len(doc.line(LINE))) // 5))
steps = [WRAP, "wait 60", doc.scroll_to(LINE - 10)]
steps += [f"click {doc.end(LINE)}", caret((LINE, len(doc.line(LINE)) + 1))]
doc.insert((LINE, len(doc.line(LINE)) + 1), " " + WORDS)
assert 2 * WRAP_COLUMNS < len(doc.line(LINE)) < 3 * WRAP_COLUMNS
top = (LINE - 1) * ROW
steps += ["key space", f"type {WORDS}", "wait 10", doc.text(LINE)]
steps += [f"log row {LINE}", f"expect row {LINE} top {top} height {3 * ROW} lines 3"]
steps += [f"log row {LINE + 1}", f"expect row {LINE + 1} top {top + 3 * ROW} height {ROW} lines 1"]
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
