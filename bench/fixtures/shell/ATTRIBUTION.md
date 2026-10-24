# Shell fixtures

The shell benchmark tiers are generated, not copied. `crates/rubrix_bench/src/bin/gen_shell.rs` writes them from snippets written for rubrix, with a fixed seed, so every run gives the same bytes.

- Licence: `MIT OR Apache-2.0`, the same as rubrix. See `LICENSE-MIT` and `LICENSE-APACHE` at the repository root.
- Coverage: the snippet cycle holds each heredoc form across lines (`<<EOF`, `<<'EOF'`, `<<-EOF`, two on one line, and one inside `$(...)`), `$var`, `${var:-x}` and the other expansions, nested `$(...)`, backticks, single and double quotes, `$'...'`, comments, `a#b`, functions, `if`, `case`, `for`, `while`, `until`, redirects, and arrays.

| Tier | Lines |
| --- | ---: |
| `tiny.sh.txt` | 55 |
| `small.sh.txt` | 801 |
| `large.sh.txt` | 10,005 |

The tiers share the seed, so `tiny.sh.txt` opens the other two. The generator stops at the first snippet that reaches the line target. The line counts are `wc -l` counts. `rubrix_highlight` counts one more line per file, the empty line after the last newline.

To regenerate the tiers, run this from the repository root:

```sh
cargo run -p rubrix_bench --bin gen_shell
```

A test in `gen_shell.rs` fails when the committed files differ from the generator's output.
