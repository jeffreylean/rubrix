# Rust fixtures

The Rust benchmark tiers are copies of Rust source files from fynix.

- Upstream: https://github.com/voxell-tech/fynix
- Commit: `6aea143ad9fd406e87043e2f9a9b373064378107`, the copy under `vendor/fynix`
- Licence: `MIT OR Apache-2.0`, the same as rubrix. See `vendor/fynix/LICENSE-MIT` and `vendor/fynix/LICENSE-APACHE`.

| Tier | Lines | Source |
| --- | ---: | --- |
| `tiny.rs.txt` | 74 | `crates/fynix/src/backend.rs` |
| `small.rs.txt` | 813 | `crates/bevy_fynix/src/shortcut.rs` |
| `large.rs.txt` | 10,372 | the first 26 `.rs` files in `git ls-files` order, joined whole |

The fynix sources hold 25,778 lines in 72 files, and the largest file has 1,176 lines. No single file reaches 10,000 lines, so `large.rs.txt` joins whole files in order until the total reaches 10,000 lines. Each file is copied complete, so no string, comment, or attribute is cut at a file boundary, and every line is valid Rust. The result does not compile as one crate, which a lexer benchmark does not need.

The line counts are `wc -l` counts. `rubrix_highlight` counts one more line per file, the empty line after the last newline.

To rebuild the tiers from the repository root:

```sh
cp vendor/fynix/crates/fynix/src/backend.rs bench/fixtures/rust/tiny.rs.txt
cp vendor/fynix/crates/bevy_fynix/src/shortcut.rs bench/fixtures/rust/small.rs.txt
git ls-files 'vendor/fynix/*.rs' \
  | while read -r f; do echo "$(wc -l < "$f") $f"; done \
  | awk '{ print $2; total += $1; if (total >= 10000) exit }' \
  | xargs cat > bench/fixtures/rust/large.rs.txt
```
