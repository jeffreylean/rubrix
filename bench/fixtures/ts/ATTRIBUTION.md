# TypeScript fixtures

The TypeScript benchmark tiers are copies of Pierre's `@pierre/highlights` benchmark fixtures. The files are unchanged.

- Upstream: https://github.com/pierrecomputer/pierre, package `packages/highlights`
- Path: `packages/highlights/benchmark/fixtures/`
- Commit: `f6cccd8a258825af15980e8d80f56d05c8473cd6`
- Licence: Apache-2.0. See `packages/highlights/LICENSE.md` in the upstream repository.

| Tier | Lines | Source |
| --- | ---: | --- |
| `tiny.ts.txt` | 75 | `tiny.ts.txt` |
| `small.ts.txt` | 825 | `small.ts.txt` |
| `large.ts.txt` | 10,825 | `large.ts.txt` |

The line counts are `wc -l` counts. `rubrix_highlight` counts one more line per file, the empty line after the last newline.

To rebuild the tiers from a Pierre checkout at the commit above:

```sh
for tier in tiny small large; do
  cp "$PIERRE/packages/highlights/benchmark/fixtures/$tier.ts.txt" "bench/fixtures/ts/$tier.ts.txt"
done
```
