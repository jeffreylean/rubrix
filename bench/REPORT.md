# Benchmark report

This report puts each `rubrix_bench` number next to the number Pierre's Highlights README publishes for the same tier. It was written for R17, the last PR of wave 1, at commit `8cecded`.

The two machines and runtimes differ, so a row compares tiers, not implementations. Pierre ran Bun 1.4.0 with a Wasm build on an Apple M4 Pro, and its timings include JavaScript objects or HTML that `rubrix_highlight` never builds. Read a gap as a scale check, not as a speedup.

## Machine and method

- rubrix: an AMD Ryzen AI 7 PRO 350 with an AMD Radeon 860M iGPU, 16 logical CPUs (8 cores), 54 GiB RAM, Linux 7.2.11 (Omarchy 4.0.6). rustc 1.97.1, release profile.
- Pierre: an Apple M4 Pro, 14 cores, 48 GiB RAM, macOS 27.0.1, Bun 1.4.0, results dated 2026-10-05 in `packages/highlights/benchmark/README.md` at Pierre `f6cccd8a`.
- Each binary ran three times, in rounds of all six binaries, on its own with no cargo, gallery or other build running. The load average before each run was 1.06 to 1.24 on 16 CPUs. Every run exited 0. Each table gives the median of the three runs, and its range column the lowest and highest of the three.
- `rubrix_bench` times a case as Pierre's `benchmark/measure.ts` does: a 200 ms warm-up that sizes batches to about 5 ms, then a 1.5 s budget of at least 20 batches. Each sample is one batch's mean time per call, and a case's number is the median of its samples. The live-editing cases time one call per sample, as `measure.ts` does with `{ batch: false }`.
- The TypeScript and JSONC tiers are Pierre's own fixtures, byte for byte. The other tiers are rubrix's generated fixtures and the vendored fynix (see each `bench/fixtures/<lang>/ATTRIBUTION.md`). Running `gen_go`, `gen_python`, `gen_shell`, `gen_toml` and `gen_yaml` at this commit left every fixture unchanged.

## Rerun

From the repository root, with nothing else running:

```sh
cargo run --release -p rubrix_bench --bin highlight -- --stress
cargo run --release -p rubrix_bench --bin stream
cargo run --release -p rubrix_bench --bin live
cargo run --release -p rubrix_bench --bin diff
cargo run --release -p rubrix_bench --bin patch
cargo run --release -p rubrix_bench --bin words
```

`highlight`, `live`, `diff`, `patch` and `words` also take `--json <path>`. `diff` first runs `bench/gen-edits.sh`, which writes its pairs under `target/edits`.

## One-shot highlighting

`highlight` lexes a whole file into runs. Pierre's nearest table is Tokens, which times `codeToTokens` on its TypeScript tiers. Its HTML table also covers JSONC, and its `highlights (bytes)` column times HTML output from bytes, which is more work than runs. Pierre publishes no Go, Python, Rust, shell, TOML, YAML or diff tier.

| File | Lines | rubrix µs/call | rubrix MiB/s | range MiB/s | Pierre, same file |
| --- | ---: | ---: | ---: | ---: | --- |
| `diff/tiny.diff.txt` | 48 | 0.722 | 2,155 | 2,117 to 2,530 | — |
| `diff/small.diff.txt` | 770 | 10.9 | 2,384 | 2,357 to 2,805 | — |
| `diff/large.diff.txt` | 11,101 | 167 | 2,071 | 2,055 to 2,073 | — |
| `go/tiny.go.txt` | 50 | 1.72 | 641 | 627 to 655 | — |
| `go/small.go.txt` | 801 | 43.9 | 464 | 461 to 465 | — |
| `go/large.go.txt` | 9,998 | 950 | 302 | 300 to 306 | — |
| `json/tiny.jsonc.txt` | 66 | 1.52 | 1,239 | 1,192 to 1,251 | 798 MiB/s (HTML, bytes) |
| `json/small.jsonc.txt` | 1,105 | 32.4 | 1,005 | 964 to 1,010 | 702 MiB/s (HTML, bytes) |
| `json/large.jsonc.txt` | 8,561 | 224 | 1,271 | 1,269 to 1,272 | 1,031 MiB/s (HTML, bytes) |
| `python/tiny.py.txt` | 48 | 2.57 | 579 | 569 to 585 | — |
| `python/small.py.txt` | 796 | 56.7 | 505 | 504 to 508 | — |
| `python/large.py.txt` | 10,000 | 1,178 | 334 | 334 to 334 | — |
| `rust/tiny.rs.txt` | 75 | 2.34 | 1,201 | 1,201 to 1,250 | — |
| `rust/small.rs.txt` | 814 | 48.3 | 483 | 482 to 484 | — |
| `rust/large.rs.txt` | 10,373 | 949 | 316 | 315 to 316 | — |
| `shell/tiny.sh.txt` | 52 | 4.50 | 527 | 524 to 529 | — |
| `shell/small.sh.txt` | 808 | 87.1 | 396 | 393 to 396 | — |
| `shell/large.sh.txt` | 10,003 | 1,377 | 285 | 282 to 286 | — |
| `toml/tiny.toml.txt` | 51 | 1.09 | 763 | 758 to 803 | — |
| `toml/small.toml.txt` | 823 | 25.0 | 1,228 | 1,222 to 1,300 | — |
| `toml/large.toml.txt` | 10,008 | 586 | 634 | 630 to 635 | — |
| `ts/tiny.ts.txt` | 76 | 2.75 | 676 | 672 to 703 | 6.94 µs, 268 MiB/s (Tokens) |
| `ts/small.ts.txt` | 826 | 50.0 | 598 | 594 to 607 | 133 µs, 224 MiB/s (Tokens) |
| `ts/large.ts.txt` | 10,826 | 1,146 | 440 | 436 to 441 | 2,240 µs, 225 MiB/s (Tokens) |
| `yaml/tiny.yaml.txt` | 51 | 1.30 | 580 | 575 to 633 | — |
| `yaml/small.yaml.txt` | 822 | 25.9 | 1,049 | 1,042 to 1,105 | — |
| `yaml/large.yaml.txt` | 10,033 | 514 | 677 | 676 to 685 | — |
| `diff/large.diff.txt x10` | 111,001 | 1,902 | 1,818 | 1,794 to 1,822 | — |
| `go/large.go.txt x11` | 109,968 | 10,509 | 301 | 299 to 302 | — |
| `json/large.jsonc.txt x12` | 102,721 | 2,696 | 1,270 | 1,253 to 1,276 | — |
| `python/large.py.txt x11` | 109,990 | 13,035 | 332 | 331 to 333 | — |
| `rust/large.rs.txt x10` | 103,721 | 9,612 | 311 | 307 to 314 | — |
| `shell/large.sh.txt x10` | 100,021 | 13,725 | 286 | 285 to 286 | — |
| `toml/large.toml.txt x10` | 100,071 | 5,956 | 624 | 619 to 626 | — |
| `ts/large.ts.txt x10` | 108,251 | 11,509 | 438 | 437 to 440 | 39.0 ms (Live editing, eager init, 100k lines) |
| `yaml/large.yaml.txt x10` | 100,321 | 5,196 | 669 | 667 to 672 | — |

## Streaming

`stream` feeds each file to a new `Stream` in the chunk count Pierre's streaming table gives its TypeScript tier of the same size: 1, 8 and 130. Pierre cuts chunks of 4,096 UTF-16 code units. Ratio is streaming time over one-shot time in the same run.

| File | Chunks | rubrix µs/call | rubrix MiB/s | range MiB/s | ratio | Pierre, same file |
| --- | ---: | ---: | ---: | ---: | ---: | --- |
| `diff/tiny.diff.txt` | 1 | 0.850 | 1,830 | 1,830 to 1,841 | 1.15 | — |
| `diff/small.diff.txt` | 8 | 11.2 | 2,322 | 2,321 to 2,331 | 1.01 | — |
| `diff/large.diff.txt` | 130 | 183 | 1,886 | 1,869 to 1,895 | 1.08 | — |
| `go/tiny.go.txt` | 1 | 1.96 | 562 | 558 to 565 | 1.12 | — |
| `go/small.go.txt` | 8 | 45.6 | 447 | 444 to 450 | 1.03 | — |
| `go/large.go.txt` | 130 | 989 | 290 | 288 to 291 | 1.03 | — |
| `json/tiny.jsonc.txt` | 1 | 1.76 | 1,073 | 1,072 to 1,076 | 1.15 | 7.81 µs, 242 MiB/s |
| `json/small.jsonc.txt` | 8 | 34.1 | 953 | 951 to 956 | 1.05 | 136 µs, 239 MiB/s |
| `json/large.jsonc.txt` | 130 | 235 | 1,214 | 1,208 to 1,216 | 1.04 | 860 µs, 332 MiB/s |
| `python/tiny.py.txt` | 1 | 2.92 | 511 | 511 to 518 | 1.10 | — |
| `python/small.py.txt` | 8 | 58.9 | 487 | 484 to 488 | 1.03 | — |
| `python/large.py.txt` | 130 | 1,201 | 327 | 325 to 328 | 1.01 | — |
| `rust/tiny.rs.txt` | 1 | 2.56 | 1,098 | 1,094 to 1,101 | 1.07 | — |
| `rust/small.rs.txt` | 8 | 49.6 | 471 | 469 to 472 | 1.04 | — |
| `rust/large.rs.txt` | 130 | 1,067 | 281 | 279 to 284 | 1.13 | — |
| `shell/tiny.sh.txt` | 1 | 4.77 | 497 | 493 to 504 | 1.07 | — |
| `shell/small.sh.txt` | 8 | 89.5 | 385 | 385 to 388 | 1.02 | — |
| `shell/large.sh.txt` | 130 | 1,420 | 277 | 276 to 281 | 1.02 | — |
| `toml/tiny.toml.txt` | 1 | 1.34 | 623 | 619 to 628 | 1.24 | — |
| `toml/small.toml.txt` | 8 | 25.4 | 1,208 | 1,178 to 1,217 | 1.06 | — |
| `toml/large.toml.txt` | 130 | 615 | 604 | 600 to 617 | 1.04 | — |
| `ts/tiny.ts.txt` | 1 | 3.07 | 607 | 606 to 610 | 1.10 | 9.09 µs, 205 MiB/s |
| `ts/small.ts.txt` | 8 | 51.5 | 581 | 576 to 581 | 1.03 | 162 µs, 184 MiB/s |
| `ts/large.ts.txt` | 130 | 1,155 | 437 | 433 to 438 | 1.01 | 2,728 µs, 185 MiB/s |
| `yaml/tiny.yaml.txt` | 1 | 1.55 | 486 | 481 to 491 | 1.18 | — |
| `yaml/small.yaml.txt` | 8 | 27.0 | 1,007 | 1,006 to 1,019 | 1.04 | — |
| `yaml/large.yaml.txt` | 130 | 535 | 650 | 649 to 661 | 1.04 | — |

## Live editing

`live` runs Pierre's live-editing scenarios on the Rust large tier and on that tier repeated to 103,721 lines. Pierre runs them on `large.ts.txt` (10,826 lines) and on it repeated to 108,251 lines, so each row compares tiers, not files. Both distant-viewport runs printed `distant read ok` in every round. Pierre's edit also updates stored token records, and rubrix's edit updates runs only. A time under 0.1 µs is near the timer's resolution, since an empty pair of `Instant` reads takes about 0.03 µs on this machine (R4's build report).

| Input | Scenario | rubrix median | rubrix p95 | rebuild median | changed lines | Pierre median | Pierre p95 | Pierre rebuild | Pierre changed |
| --- | --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| `rust/large.rs.txt` | eager init | 941 µs | 953 µs | — | — | 4,717 µs | 5,089 µs | — | — |
| `rust/large.rs.txt` | edit top | 0.050 µs | 0.060 µs | 938 µs | 1 | 0.54 µs | 0.75 µs | 2,124 µs | 1 |
| `rust/large.rs.txt` | edit middle | 0.050 µs | 0.060 µs | 928 µs | 1 | 0.75 µs | 0.96 µs | 2,132 µs | 1 |
| `rust/large.rs.txt` | edit end | 0.050 µs | 0.060 µs | 930 µs | 1 | 0.67 µs | 0.87 µs | 2,132 µs | 2 |
| `rust/large.rs.txt` | insert line | 0.090 µs | 0.110 µs | 928 µs | 2 | 1.21 µs | 1.50 µs | 2,141 µs | 3 |
| `rust/large.rs.txt` | delete line | 0.141 µs | 0.161 µs | 934 µs | 1 | 0.75 µs | 0.96 µs | 2,138 µs | 1 |
| `rust/large.rs.txt` | template propagation | 332 µs | 336 µs | 197 µs | 10,371 | 1,320 µs | 1,387 µs | 408 µs | 10,824 |
| `rust/large.rs.txt` | template + viewport | 2.90 µs | 3.01 µs | 196 µs | 119 | 47.1 µs | 66.2 µs | 421 µs | 118 |
| `rust/large.rs.txt` | template + distant viewport | 0.040 µs | 0.060 µs | 196 µs | 0 | 1,001 µs | 1,008 µs | 409 µs | 0 |
| `rust/large.rs.txt x10` | eager init | 9,347 µs | 9,419 µs | — | — | 39.0 ms | 39.6 ms | — | — |
| `rust/large.rs.txt x10` | edit top | 0.050 µs | 0.060 µs | 9,377 µs | 1 | 0.63 µs | 0.79 µs | 22.0 ms | 1 |
| `rust/large.rs.txt x10` | edit middle | 0.050 µs | 0.060 µs | 9,354 µs | 1 | 0.71 µs | 0.87 µs | 21.8 ms | 1 |
| `rust/large.rs.txt x10` | edit end | 0.050 µs | 0.060 µs | 9,349 µs | 1 | 0.71 µs | 0.87 µs | 21.9 ms | 2 |
| `rust/large.rs.txt x10` | insert line | 0.190 µs | 0.391 µs | 9,383 µs | 2 | 1.12 µs | 1.37 µs | 22.5 ms | 4 |
| `rust/large.rs.txt x10` | delete line | 0.090 µs | 0.191 µs | 9,419 µs | 1 | 0.67 µs | 0.83 µs | 22.2 ms | 2 |
| `rust/large.rs.txt x10` | template propagation | 3,589 µs | 3,835 µs | 2,180 µs | 103,719 | 30.1 ms | 31.2 ms | 20.6 ms | 108,249 |
| `rust/large.rs.txt x10` | template + viewport | 3.77 µs | 4.51 µs | 2,181 µs | 119 | 73.5 µs | 98.0 µs | 20.5 ms | 118 |
| `rust/large.rs.txt x10` | template + distant viewport | 0.150 µs | 0.381 µs | 2,174 µs | 0 | 1,014 µs | 1,025 µs | 20.4 ms | 0 |

## Diff, patches and changed words

Pierre publishes no timing for diffing, patch parsing or word marks. Its `packages/diffs/benchmarks` holds only a CSS rendering runbook, `CSS_PERFORMANCE_BENCHMARK.md`, with no results, so these three tables have no Pierre column. `range ms` is the range of the three runs' medians.

### Diff

`diff` times `diff_texts` and `rows` on the old and new pairs that `bench/gen-edits.sh` writes from the Rust tiers, with 1%, 10% and 50% of lines changed, and on the 100k-line stress pair.

| pair | lines | algorithm | ms | p95 ms | rows | added | removed | range ms |
| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| `tiny-1` | 74 | myers | 0.002 | 0.002 | 5 | 0 | 1 | 0.002 to 0.002 |
| `tiny-10` | 74 | myers | 0.005 | 0.005 | 51 | 10 | 7 | 0.005 to 0.005 |
| `tiny-50` | 74 | myers | 0.020 | 0.020 | 106 | 32 | 37 | 0.020 to 0.020 |
| `small-1` | 813 | myers | 0.021 | 0.022 | 62 | 8 | 8 | 0.021 to 0.022 |
| `small-10` | 813 | myers | 0.079 | 0.081 | 533 | 77 | 81 | 0.078 to 0.079 |
| `small-50` | 813 | myers | 1.64 | 1.66 | 1,248 | 437 | 407 | 1.64 to 1.65 |
| `large-1` | 10,372 | myers | 1.01 | 1.03 | 909 | 103 | 104 | 1.01 to 1.02 |
| `large-10` | 10,372 | myers | 2.26 | 2.28 | 7,035 | 1,047 | 1,040 | 2.26 to 2.27 |
| `large-50` | 10,372 | myers | 6.57 | 6.67 | 15,636 | 5,293 | 5,284 | 6.56 to 6.59 |
| `stress-1` | 103,720 | myers | 14.8 | 15.2 | 9,079 | 1,051 | 1,037 | 14.7 to 15.0 |

### Patch

`patch` times `parse_patch` and `rows` for every file of each patch under `bench/patch`.

| patch | lines | files | ms | p95 ms | rows | added | removed | range ms |
| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| `large.patch.txt` | 11,095 | 93 | 0.827 | 0.836 | 10,540 | 5,454 | 1,194 | 0.824 to 0.832 |
| `stress.patch.txt` | 100,522 | 726 | 6.87 | 7.06 | 96,346 | 59,200 | 17,651 | 6.85 to 6.94 |

### Changed words

`words` times `realign` and `Row::words` for every paired line, on `bench/patch/large.patch.txt` and on the large pair with 50% of lines changed.

| case | lines | pairs | marked | realign ms | ms | p95 ms | range ms |
| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| `large.patch.txt` | 11,095 | 804 | 804 | 0.162 | 1.65 | 1.67 | 1.64 to 1.65 |
| `large-50` | 10,372 | 4,305 | 4,305 | 0.653 | 7.12 | 7.16 | 7.08 to 7.15 |

