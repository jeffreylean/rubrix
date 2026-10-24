# TypeScript fixtures

The TypeScript benchmark tiers come from Pierre's benchmark fixtures and are Microsoft code that Pierre copied. They are kept so that rubrix's throughput compares with Pierre's published number on the same files. The first line of each file names its source URL.

Pierre's copy:

- Repository: https://github.com/pierrecomputer/pierre, path `packages/highlights/benchmark/fixtures/`
- Commit: `f6cccd8a258825af15980e8d80f56d05c8473cd6`
- Licence of Pierre's package: Apache-2.0, `packages/highlights/LICENSE.md` there. Pierre's repository states `Copyright 2025 Pierre Computer Company`.

The origin of each file:

| Tier | Lines | Bytes | Origin repository, commit or tag, and path | Licence |
| --- | ---: | ---: | --- | --- |
| `tiny.ts.txt` | 75 | 1,953 | microsoft/vscode at `d9637b3f2faa8f8ce0636e556291fdb1ba714c0b`, `src/vs/base/common/controlFlow.ts` | MIT |
| `small.ts.txt` | 825 | 31,363 | microsoft/TypeScript at tag `v6.0.3`, `src/compiler/sourcemap.ts` | Apache-2.0 |
| `large.ts.txt` | 10,825 | 528,941 | microsoft/TypeScript at tag `v6.0.3`, `src/compiler/parser.ts` | Apache-2.0 |

What differs from the origin, measured against the files at those commits:

- `small.ts.txt` and `large.ts.txt` are the origin file with its CRLF line ends changed to LF, after a first line holding the source URL and an empty second line.
- `tiny.ts.txt` is the origin file after the same two lines, reformatted by a code formatter. Tab indents became two spaces, long signatures and calls are wrapped over several lines, and the arrow function in `makeExclusiveOrSkip` lost its parentheses. The code is otherwise the origin's.

Copyright and licences:

- microsoft/vscode: MIT, `Copyright (c) 2015 - present Microsoft Corporation`. The permission notice from its `LICENSE.txt` at the commit above follows.
- microsoft/TypeScript: Apache-2.0, Copyright (c) Microsoft Corporation. The repository's `package.json` at `v6.0.3` names the author as `Microsoft Corp.` and the licence as `Apache-2.0`, and its `LICENSE.txt` is the unmodified Apache License 2.0 with no copyright line. The Apache-2.0 text is in `LICENSE-APACHE` at the repository root, and this file records the changes above as the licence asks for modified files. The repository has no `NOTICE` file.

```text
MIT License

Copyright (c) 2015 - present Microsoft Corporation

Permission is hereby granted, free of charge, to any person obtaining a copy
of this software and associated documentation files (the "Software"), to deal
in the Software without restriction, including without limitation the rights
to use, copy, modify, merge, publish, distribute, sublicense, and/or sell
copies of the Software, and to permit persons to whom the Software is
furnished to do so, subject to the following conditions:

The above copyright notice and this permission notice shall be included in all
copies or substantial portions of the Software.

THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE
AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER
LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM,
OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN THE
SOFTWARE.
```

The line counts are `wc -l` counts. `rubrix_highlight` counts one more line per file, the empty line after the last newline.

To rebuild the tiers from a Pierre checkout at the commit above, from the repository root with `PIERRE` set to the checkout:

```sh
for tier in tiny small large; do
  cp "$PIERRE/packages/highlights/benchmark/fixtures/$tier.ts.txt" \
    "bench/fixtures/ts/$tier.ts.txt"
done
```
