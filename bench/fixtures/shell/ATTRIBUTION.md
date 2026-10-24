# Shell fixtures

The shell benchmark tiers are copies of scripts from three MIT or MIT/Apache-2.0 projects. Each file is copied whole, so no heredoc, string, or `$(...)` is cut at a file boundary. The scripts are fixtures for a lexer and are not meant to run as one file.

| Project | Upstream | Commit | Licence |
| --- | --- | --- | --- |
| nvm | https://github.com/nvm-sh/nvm | `e780535240ee3ba67048b434cec6f7f509ee7b31` | MIT, Copyright (c) 2010 Tim Caswell |
| bats-core | https://github.com/bats-core/bats-core | `5b61d8f22b46abf001e218cdd63f2938c356fa93` | MIT, Copyright (c) 2017 bats-core contributors and (c) 2014 Sam Stephenson |
| rustup | https://github.com/rust-lang/rustup | `5c8ff3f907c37688d20d9e7038a2f826042cd763` | `MIT OR Apache-2.0`, Copyright (c) 2016 The Rust Project Developers |

MIT and `MIT OR Apache-2.0` are the licences rubrix uses. See `LICENSE-MIT` and `LICENSE-APACHE` at the repository root. The MIT notices above keep each upstream's copyright line.

| Tier | Lines | Source |
| --- | ---: | --- |
| `tiny.sh.txt` | 89 | bats-core `libexec/bats-core/bats-format-tap13` |
| `small.sh.txt` | 933 | rustup `rustup-init.sh` |
| `large.sh.txt` | 10,428 | nvm `nvm.sh`, then every file in bats-core `lib/bats-core/` and `libexec/bats-core/`, each in `LC_ALL=C sort` order |

No single script reaches 10,000 lines, so `large.sh.txt` joins whole files. The line counts are `wc -l` counts. `rubrix_highlight` counts one more line per file, the empty line after the last newline.

To rebuild the tiers, with the three projects cloned at the commits above:

```sh
cp bats-core/libexec/bats-core/bats-format-tap13 tiny.sh.txt
cp rustup/rustup-init.sh small.sh.txt
{
  cat nvm/nvm.sh
  for f in $(ls bats-core/lib/bats-core/*.bash | LC_ALL=C sort); do cat "$f"; done
  for f in $(ls bats-core/libexec/bats-core/* | LC_ALL=C sort); do cat "$f"; done
} > large.sh.txt
```
