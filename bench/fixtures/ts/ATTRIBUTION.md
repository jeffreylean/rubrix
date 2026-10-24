# TypeScript fixtures

The TypeScript benchmark tiers are byte-identical copies of Pierre's `packages/highlights/benchmark/fixtures/` files at commit `f6cccd8a258825af15980e8d80f56d05c8473cd6`. Pierre copied them from the Microsoft source URL on each file's first line.

| Tier | Lines | Source |
| --- | ---: | --- |
| `tiny.ts.txt` | 75 | https://github.com/microsoft/vscode/blob/d9637b3f2faa8f8ce0636e556291fdb1ba714c0b/src/vs/base/common/controlFlow.ts |
| `small.ts.txt` | 825 | https://github.com/microsoft/TypeScript/blob/v6.0.3/src/compiler/sourcemap.ts |
| `large.ts.txt` | 10825 | https://github.com/microsoft/TypeScript/blob/v6.0.3/src/compiler/parser.ts |

The line counts are `wc -l` counts. `rubrix_highlight` counts one more line per file, the empty line after the last newline.

To rebuild the tiers from a Pierre checkout at the commit above, from the repository root with `PIERRE` set to the checkout:

```sh
for tier in tiny small large; do
  cp "$PIERRE/packages/highlights/benchmark/fixtures/$tier.ts.txt" \
    "bench/fixtures/ts/$tier.ts.txt"
done
```
