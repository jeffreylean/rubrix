# JSON fixtures

The JSON benchmark tiers are byte-identical copies of Pierre's `packages/highlights/benchmark/fixtures/` files at commit `f6cccd8a258825af15980e8d80f56d05c8473cd6`. Pierre copied them from the Microsoft source URL on each file's first line.

| Tier | Lines | Source |
| --- | ---: | --- |
| `tiny.jsonc.txt` | 65 | https://github.com/microsoft/terminal/blob/093e49e29a9f806ff83025c49be5d0c970673b00/src/cascadia/TerminalSettingsModel/defaults.json |
| `small.jsonc.txt` | 1104 | https://github.com/microsoft/terminal/blob/093e49e29a9f806ff83025c49be5d0c970673b00/src/cascadia/TerminalSettingsModel/defaults.json |
| `large.jsonc.txt` | 8560 | https://github.com/microsoft/TypeScript/blob/b465fdbfe175304d9b977da137b2c178ae1091d3/src/compiler/diagnosticMessages.json |

The line counts are `wc -l` counts. `rubrix_highlight` counts one more line per file, the empty line after the last newline.

To rebuild the tiers from a Pierre checkout at the commit above, from the repository root with `PIERRE` set to the checkout:

```sh
for tier in tiny small large; do
  cp "$PIERRE/packages/highlights/benchmark/fixtures/$tier.jsonc.txt" \
    "bench/fixtures/json/$tier.jsonc.txt"
done
```
