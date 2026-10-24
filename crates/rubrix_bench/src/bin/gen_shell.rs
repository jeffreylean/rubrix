//! Writes `bench/fixtures/shell/{tiny,small,large}.sh.txt`, the shell
//! benchmark tiers of about 50, 800, and 10,000 lines.
//!
//! The text is the same on every run. A fixed seed picks the names
//! and numbers that fill a cycle of snippets written for rubrix,
//! and the cycle covers each construct the shell lexer handles. The
//! three tiers share the seed, so a smaller tier opens every larger
//! one.

use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::{fs, io};

use rubrix_highlight::{Lang, highlight};

const SEED: u64 = 0x5eed_5ee5;

const TIERS: [(&str, usize); 3] =
    [("tiny", 50), ("small", 800), ("large", 10_000)];

const NOUNS: [&str; 12] = [
    "backup", "build", "cache", "config", "deploy", "log", "release",
    "report", "source", "target", "temp", "user",
];
const SUFFIXES: [&str; 8] = [
    "dir", "file", "path", "name", "count", "list", "size", "mode",
];
const EXTENSIONS: [&str; 6] =
    ["conf", "log", "txt", "json", "toml", "sh"];
const COMMANDS: [&str; 6] =
    ["make", "cargo", "git", "curl", "tar", "ssh"];

struct Rng(u64);

impl Rng {
    fn next(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9e37_79b9_7f4a_7c15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
        z ^ (z >> 31)
    }

    fn below(&mut self, n: usize) -> usize {
        (self.next() % n as u64) as usize
    }

    fn pick<'a>(&mut self, items: &[&'a str]) -> &'a str {
        items[self.below(items.len())]
    }
}

struct Gen {
    rng: Rng,
    out: String,
    lines: usize,
}

impl Gen {
    fn name(&mut self) -> String {
        let noun = self.rng.pick(&NOUNS);
        let suffix = self.rng.pick(&SUFFIXES);
        format!("{noun}_{suffix}")
    }

    fn snippet(&mut self, template: &str) {
        let mut text = template.to_owned();
        for key in ["a", "b", "c", "f"] {
            let name = self.name();
            text = text.replace(&format!("@{key}@"), &name);
        }
        let number = self.rng.below(90) + 2;
        let extension = self.rng.pick(&EXTENSIONS);
        let command = self.rng.pick(&COMMANDS);
        text = text
            .replace("@n@", &number.to_string())
            .replace("@w@", extension)
            .replace("@x@", command);
        self.out.push_str(&text);
        self.out.push('\n');
        self.lines += text.matches('\n').count() + 1;
    }
}

const SNIPPETS: [&str; 18] = [
    "\
@a@=\"$HOME/@w@\"
@b@=\"${@a@:-/var/tmp}\"
echo \"@a@ is ${@a@} and ${#@a@} long\"
echo \"${@a@##*/}\" \"${@a@%.*}\" \"${@a@//-/_}\" \"${@a@^^}\"
: \"${@b@:=@n@}\"
if [ -z \"$@b@\" ]; then echo \"empty @b@\"; fi
",
    "\
@a@=\"$(basename \"$(pwd)\")\"
echo \"$@a@\" a#b # a comment with $(nothing run)
@b@=`date +%s`
echo \"$@a@\" \"`echo nested`\" \"$(echo \"$(echo deep)\")\"
echo $# $? $$ $@ $* $1 ${10}
",
    "\
cat <<EOF > \"$@a@.@w@\"
# generated for $USER on $(date +%F)
@b@=${@b@:-@n@}
path=$(dirname \"$0\")/@c@ and `hostname`
EOF
",
    "\
cat <<'EOF' >> @c@.@w@
export PATH=\"$HOME/bin:$PATH\"
alias ll='ls -la'   # kept as is: $(not run) `not run` ${not}
EOF
",
    "\
@f@() {
\tcat <<-EOF
\t\tname: $@a@
\t\ttotal: $(( @n@ + 1 ))
\t\thome: ${HOME:-/root}
\tEOF
}
",
    "\
while IFS= read -r @a@; do
  case \"$@a@\" in
    '#'*|'') continue ;;
    @w@=*) @b@=${@a@#*=} ;;
    *) echo \"skip: $@a@\" ;;
  esac
done < \"$@b@\"
",
    "\
@f@() {
  local @a@=$1 @b@=\"${2:-@n@}\"
  if [ -z \"$@a@\" ]; then
    echo \"usage: @f@ <@a@> [@b@]\" >&2
    return 1
  elif [[ $@a@ == *.@w@ ]]; then
    printf '%s\\n' \"$@a@\"
  else
    echo \"$@a@ $@b@\"
  fi
}
",
    "\
for @a@ in *.@w@ \"$@b@\"/*; do
  [ -e \"$@a@\" ] || continue
  @c@=$(basename \"$(dirname \"$(readlink -f \"$@a@\")\")\")
  echo \"$@c@: $(wc -l < \"$@a@\") lines\" # count the lines
done
",
    "\
case \"$1\" in
  start|go) @f@ \"$2\" ;;
  stop) kill \"$(cat \"$@a@.pid\")\" 2>/dev/null || true ;;
  *) echo \"usage: $0 {start|stop}\" >&2; exit @n@ ;;
esac
",
    "\
echo 'single $quoted \\n' \"double $@a@ \\\"esc\\\" \\$lit\" \
     $'tab\\there'
@a@=\"multi
line string $@b@\"
msg='it'\\''s'
",
    "\
grep -rn \"@w@\" \"$@a@\" 2>/dev/null | sort -u | head -n @n@ > \
     \"$@b@.out\"
@x@ --verbose --flag=@n@ && echo ok || echo fail >&2
tail -f \"$@a@\" &
wait $!
diff <(sort a) <(sort b) > /dev/null
read -r @c@ <<< \"$@a@\"
",
    "\
@a@=(@n@ 2 3 \"$@b@\")
@a@+=(four)
for ((i = 0; i < ${#@a@[@]}; i++)); do
  echo \"${@a@[$i]}: $(( i * @n@ + 1 ))\"
done
(( count++ ))
",
    "\
until [ \"$@a@\" -ge @n@ ]; do @a@=$((@a@ + 1)); done
while :; do sleep 1; done & trap 'kill $!' EXIT
",
    "\
function @f@ {
  declare -r @a@=\"@w@\"
  export @b@=@n@
  { echo a; echo b; } | sort
  ( cd \"$@a@\" && pwd )
}
",
    "\
if [[ -n \"$@a@\" && ! -d \"$@b@\" ]] || test -f \"@c@.sh\"; then
  mkdir -p \"$@b@\"
fi
[ \"$@a@\" = \"@w@\" ] && echo same
",
    "\
@x@ --option-one \"$@a@\" \\
    --option-two \"$@b@\" \\
    --number @n@
",
    "\
@a@=$(cat <<EOF
inside $(echo @w@)
EOF
)
",
    "\
cat <<ONE; cat <<'TWO'
first $@a@
ONE
second $@a@
TWO
",
];

fn generate(target: usize) -> String {
    let mut g = Gen {
        rng: Rng(SEED),
        out: String::new(),
        lines: 0,
    };
    g.snippet("#!/usr/bin/env bash\nset -euo pipefail\n");
    let mut next = 0;
    while g.lines < target {
        g.snippet(SNIPPETS[next % SNIPPETS.len()]);
        next += 1;
    }
    g.out
}

fn fixture_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../bench/fixtures/shell")
}

fn main() -> ExitCode {
    if std::env::args().len() > 1 {
        eprintln!("usage: gen_shell");
        return ExitCode::FAILURE;
    }
    match write_all() {
        Ok(()) => ExitCode::SUCCESS,
        Err(err) => {
            eprintln!("cannot write the fixtures: {err}");
            ExitCode::FAILURE
        }
    }
}

fn write_all() -> io::Result<()> {
    let dir = fixture_dir();
    for (tier, target) in TIERS {
        let text = generate(target);
        if let Err(err) = highlight(Lang::Shell, &text).check(&text) {
            return Err(io::Error::other(format!("{tier}: {err}")));
        }
        let path = dir.join(format!("{tier}.sh.txt"));
        fs::write(&path, &text)?;
        println!(
            "{}: {} lines",
            path.display(),
            text.matches('\n').count()
        );
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tiers_cover_every_construct() {
        let needles = [
            "<<EOF", "<<'EOF'", "<<-EOF", "${", ":-", "$(", "`",
            "a#b", "'", "\"", " # ", "if ", "case ", "for ",
            "while ",
        ];
        let later = ["<<<", "until ", "(( "];
        for (tier, target) in TIERS {
            let text = generate(target);
            let wanted =
                if tier == "tiny" { &[][..] } else { &later };
            for &needle in needles.iter().chain(wanted) {
                assert!(
                    text.contains(needle),
                    "{tier} lacks {needle}"
                );
            }
        }
    }

    #[test]
    fn committed_files_match_the_generator() {
        for (tier, target) in TIERS {
            let path = fixture_dir().join(format!("{tier}.sh.txt"));
            let committed = fs::read_to_string(&path).unwrap();
            assert!(
                committed == generate(target),
                "{tier}.sh.txt differs; run `cargo run -p \
                 rubrix_bench --bin gen_shell`"
            );
        }
    }
}
