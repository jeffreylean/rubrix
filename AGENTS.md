# rubrix

Code, diff and file-tree UI kit for Bevy, built on fynix.

## Setup

```sh
git clone https://github.com/jeffreylean/rubrix.git
cd rubrix
```

`rust-toolchain.toml` pins Rust 1.97.1 with `rustfmt`, `clippy`, and the `x86_64-unknown-none` target. `rustup` installs them on first use. Formatting also needs a nightly `rustfmt`: `rustup toolchain install nightly --profile minimal --component rustfmt`.

The CI jobs that build Bevy install `libudev-dev`, `libwayland-dev`, and `libxkbcommon-dev` (the `apt-get install` line in `.github/workflows/rust.yml`). `scripts/check-no-std.sh` also needs `jq`, which the GitHub runner image already has. On Debian or Ubuntu:

```sh
sudo apt-get update
sudo apt-get install --no-install-recommends libudev-dev libwayland-dev libxkbcommon-dev jq
```

## Commands that CI runs

```sh
cargo +nightly fmt --all -- --check
cargo check --workspace --all-features --all-targets
cargo check --workspace --no-default-features
cargo clippy --workspace --all-features --all-targets -- -D warnings
RUSTDOCFLAGS="-D warnings" cargo doc --workspace --all-features --no-deps --document-private-items
cargo test --workspace
scripts/check-no-std.sh
scripts/check-bevy-pin.sh
(cd vendor/fynix && cargo test --workspace)
```

Keep build output out of `/tmp`. Set `CARGO_TARGET_DIR` to a directory under `~/.cache/`.

## no_std crates

A crate opts into the no_std check with `no_std = true` under `[package.metadata.rubrix]` in its `Cargo.toml`. `scripts/check-no-std.sh` builds each such crate for `x86_64-unknown-none`. No later PR needs to edit `.github/`.

## Vendoring rule

`vendor/fynix` is a plain copy of the upstream commit that `vendor/README.md` records. It is not a submodule, a subtree, or a fork.

- fynix changes are rubrix PRs that edit the files under `vendor/fynix`.
- Nothing is pushed to voxell-tech/fynix or to any fork of it.
- `vendor/fynix` is excluded from the root workspace and has its own `Cargo.lock`. Run its tests from inside that directory.
- Pin `bevy` to one exact version in the root `Cargo.toml`. `scripts/check-bevy-pin.sh` fails when two versions of a `bevy*` crate resolve on any target, when `bevy` resolves at another version, or when `cargo tree` itself fails.

## References

Local clones for reading prior art (outside this repo):

- Pierre (`highlights`, `diffs`, `trees`): `~/Projects/opensources/pierre`
- Zed (GPUI text layout): `~/Projects/opensources/zed`
- fynix: `~/Projects/fynix`
