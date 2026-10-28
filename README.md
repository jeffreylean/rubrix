<img src="assets/icon.svg" width="64" height="64" alt="">

# rubrix

Code, diff and file-tree UI kit for Bevy, built on [fynix](https://github.com/voxell-tech/fynix).

## Layout

- `crates/` holds the rubrix crates. `rubrix_highlight` is a `no_std` stub for now.
- `vendor/fynix` is a plain copy of fynix at a pinned upstream commit. See `vendor/README.md`.
- `scripts/` holds the checks that CI runs.

## Build

The repository pins Rust 1.97.1 in `rust-toolchain.toml`, so `rustup` installs it on first use.

```sh
cargo test --workspace
scripts/check-no-std.sh
scripts/check-bevy-pin.sh
```

## Licence

Licensed under either of [Apache License, Version 2.0](LICENSE-APACHE) or [MIT licence](LICENSE-MIT) at your option.

`vendor/fynix` keeps its own licence files, also `MIT OR Apache-2.0`.
