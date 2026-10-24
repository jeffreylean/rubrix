# Go fixtures

The Go benchmark tiers are copies of source files from the Go standard library, as packaged by Arch Linux (`go 1.27.3`, installed under `/usr/lib/go/src`).

- Upstream: https://go.googlesource.com/go, directory `src/`
- Licence: `BSD-3-Clause`, "Copyright 2009 The Go Authors". The licence text is the `LICENSE` file at the root of the Go repository.
- Each file keeps its original text, including its copyright header. rubrix changes nothing in them.

| Tier | Lines | Source (under `src/`) |
| --- | ---: | --- |
| `tiny.go.txt` | 56 | `syscall/rlimit.go` |
| `small.go.txt` | 842 | `bufio/bufio.go` |
| `large.go.txt` | 10,111 | `strings/strings.go`, `net/url/url.go`, `text/template/exec.go`, `flag/flag.go`, `encoding/json/decode.go`, `bytes/bytes.go`, `go/scanner/scanner.go`, `regexp/regexp.go`, joined whole in that order |

No single file reaches 10,000 lines, so `large.go.txt` joins whole files. Each file is copied complete, so no string, raw string, or comment is cut at a file boundary. The result does not compile as one package, which a lexer benchmark does not need.

The line counts are `wc -l` counts. `rubrix_highlight` counts one more line per file, the empty line after the last newline.

To rebuild the tiers from a Go source tree:

```sh
src=/usr/lib/go/src
cp "$src/syscall/rlimit.go" bench/fixtures/go/tiny.go.txt
cp "$src/bufio/bufio.go" bench/fixtures/go/small.go.txt
(cd "$src" && cat strings/strings.go net/url/url.go text/template/exec.go flag/flag.go \
  encoding/json/decode.go bytes/bytes.go go/scanner/scanner.go regexp/regexp.go) \
  > bench/fixtures/go/large.go.txt
```
