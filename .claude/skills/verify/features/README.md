# Gallery verification map

The gallery lists its pages on the left. Each page has one file here when it ships. Read the index, then drive the page with its recipe.

## Baseline preconditions

- Run `scripts/doctor.sh` and require no `MISSING` line.
- Launch with `scripts/launch.sh <script> <out dir>`. Use an out dir that only this run writes.
- Start every recipe from a fresh gallery. A gallery holds no state between launches.
- Drive only a gallery that this run started.

## Driving conventions

- Input goes in through drive steps only. Read state with `log` and screenshots.
- Wait for `drive: <step> ok` for each step, and `exit=0` at the end.
- Keep screenshots, CSVs and videos in the out dir, and report their paths.

## Proof and skip reporting

- A screenshot goes with the `log` line that names the state it shows.
- Report an unreachable path with the command tried and the exit code.

## Features

- [gallery-shell.md](gallery-shell.md): the page list, the theme toggle, and the `about` page.
- [driver.md](driver.md): the drive script steps themselves.
- [rich.md](rich.md): the `rich` page, rich text in more than one style.
- [virtual-list.md](virtual-list.md): the `virtual_list` page, a list that builds only the rows near its view.
- [code.md](code.md): the `code` page, a highlighted file in a virtual list.
