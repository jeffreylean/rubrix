# rubrix

Code, diff and file-tree UI kit for Bevy, built on [fynix](https://github.com/voxell-tech/fynix).

rubrix draws highlighted code and diffs in a Bevy UI. It has a read-only code view and a read-only diff view. The file tree is not built yet.

## Crates

- `rubrix_highlight` highlights text with hand-written lexers for Rust, TypeScript and JavaScript, Python, Go, shell, unified diffs, JSON and JSONC, TOML, and YAML, with a plain-text fallback. `highlight` lexes a whole text. `Highlights::edit` lexes again only the lines that an edit can reach. `Stream` highlights text that arrives in chunks. The crate is `no_std`.
- `rubrix_diff` diffs two texts into hunks with `similar` (`diff_texts`) and parses multi-file patches (`parse_patch`). `realign` pairs the most similar removed and added lines, and `word_changes` marks the words that changed between them. `rows` turns a diff into stacked or split rows (`Layout`), and `Expansion` reveals the unchanged lines between hunks. The crate is `no_std`.
- `bevy_rubrix` holds the Bevy views. Add `RubrixPlugin` to the app, then build a `code_view` from a `CodeSource` or a `diff_view` from a `DiffSource`. A code view shows a whole text, or text that arrives in chunks. For chunks, `CodeSource::streaming` returns a `CodeStream`. Give `CodeStream::source` to `code_view`, then send the text with `CodeStream::push` and end it with `CodeStream::finish`. A diff view shows a patch or two texts, stacked or split. Both views build only the rows near the visible area, wrap or scroll long lines (`Overflow`), and let you select text with the mouse and copy it with `Ctrl+C`. A theme colours them through `SyntaxTokens`.
- `rubrix_bench` holds the benchmarks. `bench/REPORT.md` has the results and the commands that run them again.

## Layout

- `crates/` holds the four crates above.
- `bench/` holds the benchmark fixtures and patches, and `REPORT.md`.
- `vendor/fynix` holds fynix. It started as a copy of an upstream commit and now carries rubrix's changes, such as the `rich` element and the virtual list. See `vendor/README.md`.
- `scripts/` holds the checks that CI runs.
- `.claude/skills/verify/` holds the scripts that launch the gallery with a drive script and check its pages.

## Build

The repository pins Rust 1.97.1 in `rust-toolchain.toml`, so `rustup` installs it on first use. On Debian or Ubuntu, Bevy and `scripts/check-no-std.sh` need these packages:

```sh
sudo apt-get update
sudo apt-get install --no-install-recommends libudev-dev libwayland-dev libxkbcommon-dev libfontconfig1-dev jq
```

## Run the gallery

The `gallery` example opens a 1280 by 800 window with a page list on the left.

```sh
cargo run -p bevy_rubrix --example gallery
```

The page list has these pages, in this order:

- `about` scrolls 200 labels.
- `rich` shows the fynix `rich` element: span styles, a wrapped paragraph, and a line of 200 spans.
- `virtual_list` scrolls a fynix virtual list of 10,000 or 100,000 rows.
- `code` shows `large.rs.txt` or `large.ts.txt` from `bench/fixtures` in a code view, or the Rust file repeated to 100,000 lines.
- `stream` streams a file into a code view at 1 B, 64 B, 4 KiB, or 64 KiB a frame.
- `diff` shows `bench/patch/large.patch.txt`, the 100,000-line `stress.patch.txt`, or an edited pair of texts in a diff view, stacked or split.
- `hamlet_rich`, `hamlet_code`, and `hamlet_diff` rebuild the rich text, the code block, and the diff block that Hamlet's chat draws.
- `stress` shows the 100,000-line file and the 100,000-line patch, one view at a time.

The button at the bottom of the page list switches between the dark and the light theme.

To run a drive script, set `RUBRIX_DRIVE` to its path. A drive script puts input into the app one step per line, and the module doc of `crates/bevy_rubrix/examples/gallery/drive.rs` lists the steps. The scripts are in `crates/bevy_rubrix/examples/gallery/drive/`.

```sh
RUBRIX_DRIVE=crates/bevy_rubrix/examples/gallery/drive/smoke.drive cargo run -p bevy_rubrix --example gallery
```

A run with a drive script presents frames without vsync. Set `RUBRIX_NO_VSYNC=0` to keep vsync on, or `RUBRIX_NO_VSYNC=1` to turn it off in a run without a script.

## Run the headless examples

`ansi` prints a file with ANSI colours. `print` prints the line diff of two files, or of the files in a patch, as numbered rows.

```sh
cargo run -p rubrix_highlight --example ansi -- <file>
cargo run -p rubrix_diff --example print -- <old> <new>
cargo run -p rubrix_diff --example print -- --patch <file>
```

The module doc of each example lists its flags, such as `--split` for `print`.

## Run the checks

```sh
cargo test --workspace
scripts/check-no-std.sh
scripts/check-bevy-pin.sh
(cd vendor/fynix && cargo test --workspace)
```

`vendor/fynix` is not in the root workspace, so its tests run from inside that directory. `AGENTS.md` lists every command that CI runs.

## Licence

Licensed under either of [Apache License, Version 2.0](LICENSE-APACHE) or [MIT licence](LICENSE-MIT) at your option.

`vendor/fynix` keeps its own licence files, also `MIT OR Apache-2.0`.
