# TOML fixtures

The TOML benchmark tiers are generated for rubrix. No third-party code is copied.

- Generator: `crates/rubrix_bench/src/bin/gen_toml.rs`
- Seed: `0x5EED_0008` plus the tier's target line count, so each random tier has its own seed (`0x5EED_0328` for small, `0x5EED_2718` for large). A splitmix64 generator draws every choice from it. The tiny tier has a target of 0 and draws nothing, so it is the fixed 50-line block alone.
- Licence: `MIT OR Apache-2.0`, the same as rubrix. See `LICENSE-MIT` and `LICENSE-APACHE` at the repository root.

| Tier | Target | Lines | Bytes |
| --- | ---: | ---: | ---: |
| `tiny.toml.txt` | 0 | 50 | 874 |
| `small.toml.txt` | 800 | 822 | 32,173 |
| `large.toml.txt` | 10,000 | 10,007 | 389,551 |

The small and large tiers start with the same 104-line block of snippets written for rubrix. It holds every construct and scalar form the lexer handles: bare, dotted, quoted, numeric, and keyword-shaped keys, all four string forms, escapes, multi-line strings with `[table]` headers and `#` inside them, line-ending backslashes, the quote runs that close a multi-line string, every number form (decimal, `+` and `-` signs, `_` separators, `0x`, `0o`, `0b`, floats, exponents), `inf`, `+inf`, `-inf`, `nan`, `+nan`, and `-nan`, every date and time form, inline tables, multi-line arrays with comments, and `[table]` and `[[array]]` headers. The test `full_tiers_hold_every_form_and_kind` in `gen_toml.rs` fails when either tier lacks a listed scalar form or one of the kinds the lexer emits.

The tiny tier is a trimmed 50-line subset of that block: a few keys of each style, a handful of numbers and dates, one multi-line basic and one multi-line literal string, inline tables, a multi-line array, and the two header forms. It does not hold every construct.

The generator then adds sections to the small and large tiers until each reaches its target. Each section has a table or array-of-tables header of one to four segments and two to fourteen entries. An entry draws its key style, value type, array length, inline-table nesting, comment, and string length at random, and some entries are multi-line arrays or multi-line strings. Header segments start with a capital letter and keys do not, so a key never collides with a sub-table.

The large tier compresses 3.3 times under `xz -9`. Rust source compresses about 5 times.

The line counts are `wc -l` counts. `rubrix_highlight` counts one more line per file, the empty line after the last newline. Every tier is valid TOML 1.0, checked with Python's `tomllib`. The test `committed_fixtures_match_the_generator` in `gen_toml.rs` fails when a committed tier differs from the generator's output.

To regenerate the tiers from the repository root:

```sh
cargo run -p rubrix_bench --bin gen_toml
```
