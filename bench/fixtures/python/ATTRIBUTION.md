# Python fixtures

The Python benchmark tiers are generated for rubrix. No third-party code is copied.

- Generator: `crates/rubrix_bench/src/bin/gen_python.rs`
- Licence: `MIT OR Apache-2.0`, the same as rubrix. See `LICENSE-MIT` and `LICENSE-APACHE` at the repository root.

| Tier | Lines | Target |
| --- | ---: | ---: |
| `tiny.py.txt` | 47 | 50 |
| `small.py.txt` | 798 | 800 |
| `large.py.txt` | 9,998 | 10,000 |

The generator joins snippets written for rubrix in an order fixed by a seed, so each run writes the same bytes. The first eleven snippets run in a set order, so every tier of about 800 lines or more holds every string prefix, triple-quoted strings across lines, f-strings with nested quotes and format specs, decorators, `def` and `class` names, numbers with underscores and the `j` suffix, comments, and line continuations. Later snippets repeat in a seeded order with a different name and number each time, so the large tier repeats the same constructs.

The line counts are `wc -l` counts. `rubrix_highlight` counts one more line per file, the empty line after the last newline. Every file is valid Python 3.14 syntax.

To regenerate the tiers from the repository root:

```sh
cargo run -p rubrix_bench --bin gen_python
```
