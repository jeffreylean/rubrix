# Diff fixtures

The diff benchmark tiers are `git log -p` output from fynix, cut at commit boundaries so each tier holds whole commits.

- Upstream: https://github.com/voxell-tech/fynix
- History ends at commit `6aea143ad9fd406e87043e2f9a9b373064378107`, the commit `vendor/fynix` copies. `vendor/fynix` holds no history, so the tiers come from a separate read-only clone.
- Licence: `MIT OR Apache-2.0`, the same as rubrix. See `LICENSE-MIT` and `LICENSE-APACHE` at the repository root. The patches contain fynix source lines and commit metadata (author names, GitHub no-reply emails, dates, messages), all public in the upstream history.

| Tier | Lines | Command, run in the clone |
| --- | ---: | --- |
| `tiny.diff.txt` | 47 | `git log -p -1 fa17239` |
| `small.diff.txt` | 769 | `git log -p -1 647e649` |
| `large.diff.txt` | 11,100 | `git log -p -n 9 6aea143` |

`fa17239` is "Update to use published `motiongfx_interp` crate". It is an ancestor of `6aea143` and touches three files with one hunk each. `647e649` is "Backend hooks for retained hosts (#72)". Only one fynix commit passes 10,000 diff lines, `e9fe487` "Fynix v3 (#63)" with 29,046, so `large.diff.txt` joins the newest nine commits instead. The line counts are `wc -l` counts. `rubrix_highlight` counts one more line per file, the empty line after the last newline.

To rebuild the tiers:

```sh
git clone https://github.com/voxell-tech/fynix.git fynix-ro
cd fynix-ro
git log -p -1 fa17239 > tiny.diff.txt
git log -p -1 647e649 > small.diff.txt
git log -p -n 9 6aea143 > large.diff.txt
```
