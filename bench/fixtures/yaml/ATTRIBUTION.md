# YAML fixtures

The YAML benchmark tiers are generated for rubrix. No third-party code is copied.

- Generator: `crates/rubrix_bench/src/bin/gen_yaml.rs`
- Seed: `0x5EED_0009` plus the tier's target line count, so each tier has its own seed (`0x5EED_0045` for tiny, `0x5EED_0329` for small, `0x5EED_2719` for large). A splitmix64 generator draws every choice from it.
- Licence: `MIT OR Apache-2.0`, the same as rubrix. See `LICENSE-MIT` and `LICENSE-APACHE` at the repository root.

| Tier | Target | Lines | Bytes |
| --- | ---: | ---: | ---: |
| `tiny.yaml.txt` | 60 | 135 | 2,113 |
| `small.yaml.txt` | 800 | 815 | 28,237 |
| `large.yaml.txt` | 10,000 | 10,026 | 364,306 |

Every tier starts with the same 135-line block of snippets written for rubrix. It holds every construct the lexer handles: plain, quoted, numeric, and merge keys, `? ` complex keys, sequences with the dash inline and on its own line, every scalar form (booleans, nulls, numbers, `inf`, `nan`, plain scalars with colons and hashes), double-quoted escapes, single-quoted `''`, quoted strings across lines, block scalars with every header (`|`, `>`, `|-`, `>+`, `|2`, and a header comment) and bodies holding `key: value` and `- item` lines, multi-line flow collections, anchors, aliases, merge keys, tags, a verbatim tag, a `%YAML` directive, and `---` and `...` markers. Because of that block, the tiny tier has 135 lines, not its target of 60.

The generator then adds documents until the tier reaches its target. Each document is a mapping or a sequence nested up to four levels, with keys in several styles and values drawn at random: scalars, nested collections, block scalars with random bodies, flow collections on one line or several, quoted strings across lines, anchors, aliases, merge keys, tags, and comments. Document markers, `...` ends, and tagged documents vary too. Aliases and merges only name anchors defined earlier in the same document.

The large tier compresses 4.0 times under `xz -9`. Rust source compresses about 5 times.

The line counts are `wc -l` counts. `rubrix_highlight` counts one more line per file, the empty line after the last newline. Every tier parses as a YAML stream with PyYAML's `compose_all`. The test `committed_fixtures_match_the_generator` in `gen_yaml.rs` fails when a committed tier differs from the generator's output.

To regenerate the tiers from the repository root:

```sh
cargo run -p rubrix_bench --bin gen_yaml
```
