# Python fixtures

The Python benchmark tiers are copies of source files from the CPython 3.14.8 standard library, as packaged by Arch Linux (`python 3.14.8-1`, installed under `/usr/lib/python3.14`).

- Upstream: https://github.com/python/cpython, tag `v3.14.8`, directory `Lib/`
- Licence: `PSF-2.0`, the Python Software Foundation License Version 2 (a permissive licence). Its text is `LICENSE.txt` in the CPython repository and in `/usr/lib/python3.14`.
- Each file keeps its original text. rubrix changes nothing in them.

| Tier | Lines | Source (under `Lib/`) |
| --- | ---: | --- |
| `tiny.py.txt` | 56 | `tkinter/scrolledtext.py` |
| `small.py.txt` | 814 | `contextlib.py` |
| `large.py.txt` | 10,064 | `argparse.py`, `dataclasses.py`, `functools.py`, `enum.py`, `shutil.py`, `textwrap.py`, joined whole in that order |

No single file reaches 10,000 lines, so `large.py.txt` joins whole files. Each file is copied complete, so no string or comment is cut at a file boundary. The result is one valid Python module text, because each file is valid on its own and none ends inside a statement.

The line counts are `wc -l` counts. `rubrix_highlight` counts one more line per file, the empty line after the last newline.

To rebuild the tiers from a CPython 3.14 `Lib/` directory:

```sh
lib=/usr/lib/python3.14
cp "$lib/tkinter/scrolledtext.py" bench/fixtures/python/tiny.py.txt
cp "$lib/contextlib.py" bench/fixtures/python/small.py.txt
(cd "$lib" && cat argparse.py dataclasses.py functools.py enum.py shutil.py textwrap.py) \
  > bench/fixtures/python/large.py.txt
```
