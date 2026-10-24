# Patch fixtures

The patch benchmark fixtures are `git log -p` output from fynix's history. They hold fynix's diffs and commit messages, nothing else.

- Upstream: https://github.com/voxell-tech/fynix
- Commit range: all commits reachable from `6aea143ad9fd406e87043e2f9a9b373064378107`, newest first, 57 commits
- Licence: `MIT OR Apache-2.0`, the same as rubrix. See `vendor/fynix/LICENSE-MIT` and `vendor/fynix/LICENSE-APACHE`.

| File | Lines | Files in it | Content |
| --- | ---: | ---: | --- |
| `10k.patch` | 11,095 | 93 | the 9 newest commits, which is the shortest run from the newest commit that reaches 10,000 lines |
| `100k.patch` | 100,522 | 726 | all 57 commits, then whole commits again from the newest, skipping any that would pass 101,000 lines, until the total reaches 100,000 |

Each commit is in `git log --format=email` form: a `From <sha> Mon Sep 17 00:00:00 2001` line, mail headers, the message, then the diffs. The stress patch repeats commits, so it holds the same file diffs more than once. `git apply --numstat` lists each one.

The names avoid the `tiny.`, `small.` and `large.` prefixes and the `.txt` suffix on purpose. `rubrix_bench::fixtures` and `rubrix_highlight`'s `bench_fixtures` test treat each directory under `bench/fixtures` as a language, and `patch` is not one.

`vendor/fynix` holds no history, so the script reads a clone of the upstream repository. Its default location is `~/.cache/rubrix-fixtures/fynix`, made on first use. Pass another clone as the first argument or in `RUBRIX_FYNIX_CLONE`. The script only reads from the clone.

To regenerate both files from the repository root:

```sh
bench/gen-patch.sh
```

The script pins every option that changes `git log -p` output, so a second run, or a run on another clone of the same commits, writes the same bytes.
