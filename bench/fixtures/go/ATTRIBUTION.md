# Go fixtures

The Go benchmark tiers are generated for rubrix. No third-party code is copied.

- Generator: `crates/rubrix_bench/src/bin/gen_go.rs`
- Seed: `0x5EED_0002`, the `SEED` constant in the generator
- Regenerate: `cargo run -p rubrix_bench --bin gen_go`
- Licence: `MIT OR Apache-2.0`, the same as rubrix. See `LICENSE-MIT` and `LICENSE-APACHE` at the repository root.

| Tier | Lines | Target |
| --- | ---: | ---: |
| `tiny.go.txt` | 49 | 50 |
| `small.go.txt` | 800 | 800 |
| `large.go.txt` | 9,997 | 10,000 |

The generator writes the same bytes on every run. It starts each tier with twelve fixed snippets in a set order, so every tier of about 800 lines or more holds raw strings across lines, runes, block comments across lines, `func` and method names, type names, built-in types, hex, octal, binary, and imaginary numbers, struct tags, and `fallthrough`. It then fills the tier with snippets from a seeded random generator, which builds Go functions, types, and statements from banks of names, types, and prose. Nesting depth, line length, identifiers, and statement shape change from one snippet to the next, so the large tier does not repeat a few lines. About one snippet in ten repeats a fixed snippet with a new name.

The line counts are `wc -l` counts. `rubrix_highlight` counts one more line per file, the empty line after the last newline. Every file parses with `gofmt -e`. The result does not compile as one package, which a lexer benchmark does not need.

To regenerate the tiers from the repository root:

```sh
cargo run -p rubrix_bench --bin gen_go
```
