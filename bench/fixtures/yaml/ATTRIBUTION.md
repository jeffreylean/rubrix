# YAML fixtures

The YAML benchmark tiers are generated for rubrix. No third-party code is copied.

- Generator: `crates/rubrix_bench/src/bin/gen_yaml.rs`
- Seed: `0x5EED_0009` plus the tier's target line count, so each random tier has its own seed (`0x5EED_0329` for small, `0x5EED_2719` for large). A splitmix64 generator draws every choice from it. The tiny tier has a target of 0 and draws nothing, so it is the fixed 50-line block alone.
- Licence: `MIT OR Apache-2.0`, the same as rubrix. See `LICENSE-MIT` and `LICENSE-APACHE` at the repository root.

| Tier | Target | Lines | Bytes |
| --- | ---: | ---: | ---: |
| `tiny.yaml.txt` | 0 | 50 | 790 |
| `small.yaml.txt` | 800 | 821 | 28,494 |
| `large.yaml.txt` | 10,000 | 10,032 | 364,669 |

The small and large tiers start with the same 141-line block of snippets written for rubrix. It holds every construct and scalar form the lexer handles: plain, quoted, numeric, and merge keys, `? ` complex keys, sequences with the dash inline and on its own line, every scalar form (`true`, `True`, `TRUE` and the `false` spellings; `null`, `Null`, `NULL`, and `~`; `yes`, `no`, `on`, `off`, `y`, and `n` as strings; integers, floats, `0x` and `0o` numbers; `.inf`, `.Inf`, `.INF`, with `+` and `-`; `.nan`, `.NaN`, `.NAN`; plain scalars with colons and hashes), double-quoted escapes, single-quoted `''`, quoted strings across lines, block scalars with the headers `|`, `|+`, `|-`, `>`, `>-`, `|2`, and a header comment, and bodies holding `key: value` and `- item` lines, multi-line flow collections, anchors, aliases, merge keys, tags, a verbatim tag, a `%YAML` directive, and `---` and `...` markers. The header `>+` is in the generated sections of the small and large tiers, not in this block. The test `full_tiers_hold_every_form_and_kind` in `gen_yaml.rs` fails when either tier lacks a listed scalar form or one of the kinds the lexer emits.

The tiny tier is a trimmed 50-line subset of that block: a few keys, scalars, a double-quoted string across lines, the block headers `|`, `|+`, `>-`, and a header comment, flow collections, an anchor, an alias, a merge key, a tag, and a tagged second document. It does not hold every construct.

The generator then adds documents to the small and large tiers until each reaches its target. Each document is a mapping or a sequence nested up to four levels, with keys in several styles and values drawn at random: scalars, nested collections, block scalars with random bodies, flow collections on one line or several, quoted strings across lines, anchors, aliases, merge keys, tags, and comments. Document markers, `...` ends, and tagged documents vary too. Aliases and merges only name anchors defined earlier in the same document.

The large tier compresses 4.0 times under `xz -9`. Rust source compresses about 5 times.

The line counts are `wc -l` counts. `rubrix_highlight` counts one more line per file, the empty line after the last newline. Every tier parses as a YAML stream with PyYAML's `compose_all`. The test `committed_fixtures_match_the_generator` in `gen_yaml.rs` fails when a committed tier differs from the generator's output.

To regenerate the tiers from the repository root:

```sh
cargo run -p rubrix_bench --bin gen_yaml
```
