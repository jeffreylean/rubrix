# Python fixtures

The Python benchmark tiers are generated for rubrix. No third-party code is copied.

- Generator: `crates/rubrix_bench/src/bin/gen_python.rs`
- Seed: `0x5EED_0001`, the `SEED` constant in the generator
- Regenerate: `cargo run -p rubrix_bench --bin gen_python`
- Licence: `MIT OR Apache-2.0`, the same as rubrix. See `LICENSE-MIT` and `LICENSE-APACHE` at the repository root.

| Tier | Lines | Target |
| --- | ---: | ---: |
| `tiny.py.txt` | 47 | 50 |
| `small.py.txt` | 795 | 800 |
| `large.py.txt` | 9,999 | 10,000 |

The generator writes the same bytes on every run. It starts each tier with twelve fixed snippets in a set order, so every tier of about 800 lines or more holds triple-quoted strings across lines, f-strings with nested quotes and format specs, decorators, `def` and `class` names, numbers with underscores and the `j` suffix, comments, line continuations, and the keywords `yield`, `nonlocal`, `del`, `finally`, `Ellipsis`, `NotImplemented`, `__debug__`, and `...`. It then fills the tier with snippets from a seeded random generator, which builds Python functions, types, and statements from banks of names, types, and prose. Nesting depth, line length, identifiers, and statement shape change from one snippet to the next, so the large tier does not repeat a few lines. About one snippet in ten repeats a fixed snippet with a new name.

The line counts are `wc -l` counts. `rubrix_highlight` counts one more line per file, the empty line after the last newline. Every file compiles with Python 3.14.

To regenerate the tiers from the repository root:

```sh
cargo run -p rubrix_bench --bin gen_python
```
