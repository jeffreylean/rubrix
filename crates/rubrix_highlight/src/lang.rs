/// One list makes the enum, `ALL`, the names, and the extension
/// table. A name is also the language's directory under
/// `tests/golden/` and `bench/fixtures/`.
macro_rules! langs {
    ($(
        $(#[doc = $doc:literal])*
        $variant:ident = $name:literal [$($ext:literal),*],
    )*) => {
        /// A language `rubrix_highlight` can lex. Each one has its own
        /// lexer module.
        #[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
        pub enum Lang {
            #[default]
            $($(#[doc = $doc])* $variant,)*
        }

        impl Lang {
            /// Every language, in declaration order.
            pub const ALL: [Lang; [$($name,)*].len()] =
                [$(Lang::$variant,)*];

            pub const fn name(self) -> &'static str {
                match self {
                    $(Lang::$variant => $name,)*
                }
            }

            const fn extensions(self) -> &'static [&'static str] {
                match self {
                    $(Lang::$variant => &[$($ext),*],)*
                }
            }
        }
    };
}

langs! {
    Plain = "plain" [],
    Rust = "rust" ["rs"],
    /// TypeScript and JavaScript share one lexer.
    TypeScript = "ts" ["ts", "mts", "cts", "tsx", "js", "mjs", "cjs", "jsx"],
    Python = "python" ["py"],
    Go = "go" ["go"],
    Shell = "shell" ["sh", "bash", "zsh"],
    Diff = "diff" ["diff", "patch"],
    Json = "json" ["json", "jsonc"],
    Toml = "toml" ["toml"],
    Yaml = "yaml" ["yaml", "yml"],
}

impl Lang {
    /// The inverse of [`Lang::name`].
    pub fn from_name(name: &str) -> Option<Lang> {
        Lang::ALL.into_iter().find(|lang| lang.name() == name)
    }

    /// The language a path's extension names.
    ///
    /// Reads the file name after the last `/` or `\` and ignores
    /// ASCII case. One trailing `.txt` is dropped first, so
    /// `small.ts.txt` is TypeScript. A leading dot does not start an
    /// extension, so `.bashrc` names no language.
    pub fn from_path(path: &str) -> Option<Lang> {
        let name = path.rsplit(['/', '\\']).next().unwrap_or(path);
        let name = strip_suffix_ignore_case(name, ".txt")
            .filter(|stem| !stem.is_empty())
            .unwrap_or(name);
        let (stem, ext) = name.rsplit_once('.')?;
        if stem.is_empty() {
            return None;
        }
        Lang::ALL.into_iter().find(|lang| {
            lang.extensions()
                .iter()
                .any(|known| known.eq_ignore_ascii_case(ext))
        })
    }

    /// The language to highlight a file with: its extension, then a
    /// `#!` interpreter on its first line, then [`Lang::Plain`].
    ///
    /// The interpreters are `sh`, `bash`, and `zsh` for shell,
    /// `python` and `python3` for Python, and `node` for
    /// JavaScript, named directly or through `env`.
    pub fn detect(path: &str, first_line: &str) -> Lang {
        Lang::from_path(path)
            .or_else(|| from_shebang(first_line))
            .unwrap_or(Lang::Plain)
    }
}

fn strip_suffix_ignore_case<'a>(
    text: &'a str,
    suffix: &str,
) -> Option<&'a str> {
    let split = text.len().checked_sub(suffix.len())?;
    let (stem, tail) = text.split_at_checked(split)?;
    tail.eq_ignore_ascii_case(suffix).then_some(stem)
}

fn from_shebang(line: &str) -> Option<Lang> {
    let mut words = line.strip_prefix("#!")?.split_ascii_whitespace();
    let mut program = base_name(words.next()?);
    if program == "env" {
        let word = words.find(|word| {
            !word.starts_with('-') && !word.contains('=')
        })?;
        program = base_name(word);
    }
    match program {
        "sh" | "bash" | "zsh" => Some(Lang::Shell),
        "node" => Some(Lang::TypeScript),
        _ if is_python(program) => Some(Lang::Python),
        _ => None,
    }
}

fn base_name(path: &str) -> &str {
    path.rsplit('/').next().unwrap_or(path)
}

/// `python`, `python3`, or a versioned name such as `python3.12`.
fn is_python(program: &str) -> bool {
    program.strip_prefix("python").is_some_and(|version| {
        version.bytes().all(|b| b.is_ascii_digit() || b == b'.')
    })
}

#[cfg(test)]
mod tests {
    use super::Lang;

    #[test]
    fn names_round_trip() {
        for lang in Lang::ALL {
            assert_eq!(Lang::from_name(lang.name()), Some(lang));
        }
    }

    #[test]
    fn extensions() {
        let cases = [
            ("src/lib.rs", Some(Lang::Rust)),
            ("a/b/app.tsx", Some(Lang::TypeScript)),
            ("index.CJS", Some(Lang::TypeScript)),
            (
                "bench/fixtures/ts/small.ts.txt",
                Some(Lang::TypeScript),
            ),
            ("C:\\code\\main.go", Some(Lang::Go)),
            ("fix.patch", Some(Lang::Diff)),
            ("tsconfig.jsonc", Some(Lang::Json)),
            ("Cargo.toml", Some(Lang::Toml)),
            ("ci.yml", Some(Lang::Yaml)),
            ("run.zsh", Some(Lang::Shell)),
            ("notes.txt", None),
            ("notes.xyz", None),
            (".bashrc", None),
            ("Makefile", None),
            (".txt", None),
        ];
        for (path, lang) in cases {
            assert_eq!(Lang::from_path(path), lang, "{path}");
        }
    }

    #[test]
    fn shebangs() {
        let cases = [
            ("#!/bin/sh", Lang::Shell),
            ("#!/usr/bin/env bash", Lang::Shell),
            ("#!/usr/bin/env -S zsh -e", Lang::Shell),
            ("#! /bin/bash -x", Lang::Shell),
            ("#!/usr/bin/env python3", Lang::Python),
            ("#!/usr/bin/python3.12", Lang::Python),
            ("#!/usr/bin/env node", Lang::TypeScript),
            ("#!/usr/bin/env perl", Lang::Plain),
            ("# not a shebang", Lang::Plain),
            ("", Lang::Plain),
        ];
        for (line, lang) in cases {
            assert_eq!(Lang::detect("script", line), lang, "{line}");
        }
        assert_eq!(Lang::detect("a.rs", "#!/bin/sh"), Lang::Rust);
    }
}
