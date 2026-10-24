# Shell fixtures

The shell benchmark tiers are generated, not copied. `crates/rubrix_bench/src/bin/gen_shell.rs` writes them from text written for rubrix, with a fixed seed, so every run gives the same bytes.

- Seed: `0x5eed_0007` (`SEED` in `gen_shell.rs`). The seed was picked so the 51-line tier holds the common constructs. The larger tiers cover all of them.
- Licence: `MIT OR Apache-2.0`, the same as rubrix. See `LICENSE-MIT` and `LICENSE-APACHE` at the repository root.
- How the text is built: a shuffled bag of 58 snippets, a small grammar of command lines (arguments, pipes, redirects, `&&` chains, trailing comments), comment blocks, blank lines, and `if`, `for`, `while`, `until`, function, brace, and subshell blocks that nest up to three deep. Names, numbers, paths, and words come from pools in the generator, so lines differ in shape, length, indent, and identifiers. A snippet with a heredoc or a multi-line string is never indented.
- Coverage: every construct `shell.rs` handles. That is each heredoc form (`<<EOF`, `<<'EOF'`, `<<"EOF"`, `<<\EOF`, `<<-EOF`, `<<-'EOF'`, two on one line, one inside `$(...)` or backticks, and a `$(` or `${` that spans lines in the body), here-strings, `$var` and every `${...}` form including `${!ref}`, nested `$(...)`, `$((...))`, backticks, process substitution, quotes, `$'...'`, `$"..."`, comments, `a#b`, `=~`, every redirect and control operator including `|&`, `&>`, `;;&`, and `;&`, arrays, functions, and every keyword (`if`, `case`, `for`, `select`, `while`, `until`, `time`, `local`, `declare`, `typeset`, `readonly`, `export`). The test `tiers_cover_every_construct` lists them and fails when a tier loses one.

| Tier | Lines |
| --- | ---: |
| `tiny.sh.txt` | 51 |
| `small.sh.txt` | 807 |
| `large.sh.txt` | 10,002 |

The tiers share the seed, so `tiny.sh.txt` opens the other two. The generator stops at the first block that reaches the line target. The line counts are `wc -l` counts. `rubrix_highlight` counts one more line per file, the empty line after the last newline. `bash -n` accepts all three files.

To regenerate the tiers, run this from the repository root:

```sh
cargo run -p rubrix_bench --bin gen_shell
```

A test in `gen_shell.rs` fails when the committed files differ from the generator's output.
