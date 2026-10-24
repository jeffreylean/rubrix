# Go fixtures

The Go benchmark tiers are generated for rubrix. No third-party code is copied.

- Generator: `crates/rubrix_bench/src/bin/gen_go.rs`
- Licence: `MIT OR Apache-2.0`, the same as rubrix. See `LICENSE-MIT` and `LICENSE-APACHE` at the repository root.

| Tier | Lines | Target |
| --- | ---: | ---: |
| `tiny.go.txt` | 49 | 50 |
| `small.go.txt` | 798 | 800 |
| `large.go.txt` | 9,998 | 10,000 |

The generator joins snippets written for rubrix in an order fixed by a seed, so each run writes the same bytes. The first eleven snippets run in a set order, so every tier of about 800 lines or more holds raw strings across lines, runes, block comments across lines, `func` and method names, type names, built-in types, hex, octal, binary, and imaginary numbers, and struct tags. Later snippets repeat in a seeded order with a different name and number each time, so the large tier repeats the same constructs.

The line counts are `wc -l` counts. `rubrix_highlight` counts one more line per file, the empty line after the last newline. Every file parses with `gofmt -e`. The result does not compile as one package, which a lexer benchmark does not need.

To regenerate the tiers from the repository root:

```sh
cargo run -p rubrix_bench --bin gen_go
```
