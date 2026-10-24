# JSON fixtures

The JSON benchmark tiers are byte-for-byte copies of Pierre's benchmark fixtures, kept so that rubrix's throughput compares with Pierre's published number on the same files.

- Copied from: `packages/highlights/benchmark/fixtures/` in https://github.com/pierrecomputer/pierre
- Commit: `f6cccd8a258825af15980e8d80f56d05c8473cd6`
- Licence of the copy: Apache-2.0, the licence of Pierre's `packages/highlights`. The licence text is in `LICENSE-APACHE` at the repository root.

| Tier | Lines | Bytes | Origin, from the first line of the file |
| --- | ---: | ---: | --- |
| `tiny.jsonc.txt` | 65 | 1,979 | an excerpt of `defaults.json` in microsoft/terminal at `093e49e29a9f806ff83025c49be5d0c970673b00` (MIT) |
| `small.jsonc.txt` | 1,104 | 34,090 | `defaults.json` in microsoft/terminal at `093e49e29a9f806ff83025c49be5d0c970673b00` (MIT) |
| `large.jsonc.txt` | 8,560 | 299,186 | `src/compiler/diagnosticMessages.json` in microsoft/TypeScript at `b465fdbfe175304d9b977da137b2c178ae1091d3` (Apache-2.0) |

The terminal files are MIT licensed: Copyright (c) Microsoft Corporation. Permission is granted to use, copy, modify, and distribute the software without restriction, subject to keeping that copyright notice and the MIT permission notice in copies or substantial portions of it. The TypeScript file is Copyright (c) Microsoft Corporation under Apache-2.0.

The perf rule compares rubrix's JSON throughput with Pierre's published number on the same files, so these files stay as Pierre ships them.

`rubrix_highlight` counts one more line per file than `wc -l`, the empty line after the last newline. The files mix LF and CRLF line ends, as Pierre's copies do.

To check the copies against Pierre's, from a clone of Pierre at the commit above:

```sh
for f in tiny small large; do
  cmp packages/highlights/benchmark/fixtures/$f.jsonc.txt \
    bench/fixtures/json/$f.jsonc.txt
done
```
