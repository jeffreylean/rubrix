# JSON fixtures

The JSON benchmark tiers come from Pierre's benchmark fixtures and are Microsoft code that Pierre copied. They are kept so that rubrix's throughput compares with Pierre's published number on the same files, which is why they stay as Pierre ships them. The first line of each file names its source URL.

Pierre's copy:

- Repository: https://github.com/pierrecomputer/pierre, path `packages/highlights/benchmark/fixtures/`
- Commit: `f6cccd8a258825af15980e8d80f56d05c8473cd6`
- Licence of Pierre's package: Apache-2.0, `packages/highlights/LICENSE.md` there. Pierre's repository states `Copyright 2025 Pierre Computer Company`.

The origin of each file:

| Tier | Lines | Bytes | Origin repository, commit, and path | Licence |
| --- | ---: | ---: | --- | --- |
| `tiny.jsonc.txt` | 65 | 1,979 | microsoft/terminal at `093e49e29a9f806ff83025c49be5d0c970673b00`, `src/cascadia/TerminalSettingsModel/defaults.json` | MIT |
| `small.jsonc.txt` | 1,104 | 34,090 | microsoft/terminal at `093e49e29a9f806ff83025c49be5d0c970673b00`, `src/cascadia/TerminalSettingsModel/defaults.json` | MIT |
| `large.jsonc.txt` | 8,560 | 299,186 | microsoft/TypeScript at `b465fdbfe175304d9b977da137b2c178ae1091d3`, `src/compiler/diagnosticMessages.json` | Apache-2.0 |

What differs from the origin, measured against the files at those commits:

- `large.jsonc.txt` is the origin file after a first line holding the source URL. The bytes of the origin file follow unchanged, CRLF line ends included.
- `small.jsonc.txt` is the terminal file after the same first line, reformatted by a code formatter. Indents are two spaces, objects written on one line are split over several, and trailing commas are added. Its keys and values equal the origin's once comments and trailing commas are removed.
- `tiny.jsonc.txt` is an excerpt of the terminal file, with the same reformatting. Its second line reads `// Excerpt: global settings and profiles.`

Copyright and licences:

- microsoft/terminal: MIT, `Copyright (c) Microsoft Corporation. All rights reserved.` The licence text from its `LICENSE` at the commit above follows.
- microsoft/TypeScript: Apache-2.0, Copyright (c) Microsoft Corporation. The repository's `package.json` at that commit names the author as `Microsoft Corp.` and the licence as `Apache-2.0`, and its `LICENSE.txt` is the unmodified Apache License 2.0 with no copyright line. The Apache-2.0 text is in `LICENSE-APACHE` at the repository root. The repository has no `NOTICE` file.

```text
Copyright (c) Microsoft Corporation. All rights reserved.

MIT License

Permission is hereby granted, free of charge, to any person obtaining a copy
of this software and associated documentation files (the "Software"), to deal
in the Software without restriction, including without limitation the rights
to use, copy, modify, merge, publish, distribute, sublicense, and/or sell
copies of the Software, and to permit persons to whom the Software is
furnished to do so, subject to the following conditions:

The above copyright notice and this permission notice shall be included in all
copies or substantial portions of the Software.

THE SOFTWARE IS PROVIDED *AS IS*, WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE
AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER
LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM,
OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN THE
SOFTWARE.
```

The line counts are `wc -l` counts. `rubrix_highlight` counts one more line per file, the empty line after the last newline. The files mix LF and CRLF line ends, as Pierre's copies do.

To rebuild the tiers from a Pierre checkout at the commit above, from the repository root with `PIERRE` set to the checkout:

```sh
for tier in tiny small large; do
  cp "$PIERRE/packages/highlights/benchmark/fixtures/$tier.jsonc.txt" \
    "bench/fixtures/json/$tier.jsonc.txt"
done
```
