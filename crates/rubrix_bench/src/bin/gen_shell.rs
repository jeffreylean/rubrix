//! Writes `bench/fixtures/shell/{tiny,small,large}.sh.txt`, the shell
//! benchmark tiers of about 50, 800, and 10,000 lines.

use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::{fs, io};

use rubrix_highlight::{Lang, highlight};

const SEED: u64 = 0x5eed_0007;

const TIERS: [(&str, usize); 3] =
    [("tiny", 50), ("small", 800), ("large", 10_000)];

const MAX_DEPTH: usize = 3;

const NOUNS: [&str; 36] = [
    "archive", "backup", "build", "bundle", "cache", "client",
    "config", "deploy", "image", "index", "input", "log", "module",
    "output", "package", "plugin", "profile", "project", "queue",
    "record", "release", "report", "result", "route", "server",
    "session", "socket", "staging", "store", "target", "temp",
    "token", "upload", "user", "version", "worker",
];
const SUFFIXES: [&str; 16] = [
    "dir", "file", "path", "name", "count", "list", "size", "mode",
    "id", "url", "host", "port", "root", "tmp", "old", "max",
];
const PREFIXES: [&str; 8] =
    ["my", "tmp", "old", "new", "raw", "app", "sys", "opt"];
const SHORT_NAMES: [&str; 9] =
    ["i", "j", "k", "n", "m", "fd", "rc", "ec", "ts"];
const WORDS: [&str; 40] = [
    "alpha", "bravo", "charlie", "delta", "echo", "foxtrot", "golf",
    "hotel", "india", "juliet", "kilo", "lima", "mike", "november",
    "oscar", "papa", "quebec", "romeo", "sierra", "tango", "uniform",
    "victor", "whiskey", "xray", "yankee", "zulu", "start", "stop",
    "retry", "skip", "done", "failed", "ready", "pending", "ok",
    "warn", "debug", "trace", "fatal", "idle",
];
const EXTENSIONS: [&str; 12] = [
    "conf", "log", "txt", "json", "toml", "sh", "yaml", "csv", "tmp",
    "bak", "pid", "lock",
];
const COMMANDS: [&str; 12] = [
    "make", "cargo", "git", "curl", "tar", "ssh", "rsync", "docker",
    "kubectl", "npm", "jq", "python3",
];
const TOOLS: [&str; 18] = [
    "echo",
    "printf '%s\\n'",
    "cd",
    "mkdir -p",
    "rm -f",
    "cp -r",
    "mv",
    "ls -la",
    "chmod +x",
    "touch",
    "grep -rn",
    "find .",
    "sed -i",
    "ln -s",
    "kill",
    "sleep 1",
    "install -m 644",
    "stat -c %s",
];
const FILTERS: [&str; 10] = [
    "sort",
    "sort -u",
    "uniq -c",
    "head -n 5",
    "tail -n +2",
    "wc -l",
    "cut -d: -f1",
    "tr a-z A-Z",
    "sed 's/a/b/'",
    "awk '{ print $1 }'",
];
const FLAGS: [&str; 10] = [
    "--verbose",
    "--quiet",
    "--force",
    "--dry-run",
    "-v",
    "-q",
    "-f",
    "-n",
    "--jobs=4",
    "--output=out",
];
const PATHS: [&str; 8] = [
    "/var/log",
    "/etc/hosts",
    "/usr/local/bin",
    "$HOME/.config",
    "/opt/app",
    "./build",
    "../dist",
    "/srv/data",
];
const REDIRECTS: [&str; 7] = [
    "> /dev/null",
    ">> run.log",
    "2>&1",
    "2> /dev/null",
    "< /dev/null",
    "&> all.log",
    ">&2",
];

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

    fn chance(&mut self, percent: usize) -> bool {
        self.below(100) < percent
    }

    fn pick<'a>(&mut self, items: &[&'a str]) -> &'a str {
        items[self.below(items.len())]
    }

    fn shuffle(&mut self, items: &mut [usize]) {
        for i in (1..items.len()).rev() {
            items.swap(i, self.below(i + 1));
        }
    }
}

struct Gen {
    rng: Rng,
    out: String,
    lines: usize,
    bag: Vec<usize>,
}

impl Gen {
    fn name(&mut self) -> String {
        let noun = self.rng.pick(&NOUNS);
        let suffix = self.rng.pick(&SUFFIXES);
        let prefix = self.rng.pick(&PREFIXES);
        match self.rng.below(10) {
            0..=3 => format!("{noun}_{suffix}"),
            4 | 5 => format!("{prefix}_{noun}_{suffix}"),
            6 => noun.to_owned(),
            7 => format!("{prefix}_{noun}"),
            8 => self.rng.pick(&SHORT_NAMES).to_owned(),
            _ => format!("{noun}{}", self.rng.below(9) + 1),
        }
    }

    fn number(&mut self) -> usize {
        match self.rng.below(4) {
            0 => self.rng.below(8),
            1 => self.rng.below(90) + 2,
            2 => self.rng.below(2_000) + 100,
            _ => self.rng.below(100_000),
        }
    }

    fn value(&mut self, key: char) -> String {
        match key {
            'a' | 'b' | 'c' | 'f' => self.name(),
            'u' => self.name().to_uppercase(),
            'n' => self.number().to_string(),
            'w' => self.rng.pick(&EXTENSIONS).to_owned(),
            'x' => self.rng.pick(&COMMANDS).to_owned(),
            'p' => self.rng.pick(&PATHS).to_owned(),
            's' => self.rng.pick(&WORDS).to_owned(),
            _ => "\t".to_owned(),
        }
    }

    fn fill(&mut self, template: &str) -> String {
        let mut text = template.to_owned();
        for key in
            ['a', 'b', 'c', 'f', 'u', 'n', 'w', 'x', 'p', 's', 't']
        {
            let tag = format!("@{key}@");
            if text.contains(&tag) {
                let value = self.value(key);
                text = text.replace(&tag, &value);
            }
        }
        text
    }

    fn line(&mut self, depth: usize, text: &str) {
        for row in text.split('\n') {
            if !row.is_empty() {
                self.out.push_str(&"  ".repeat(depth));
            }
            self.out.push_str(row);
            self.out.push('\n');
            self.lines += 1;
        }
    }

    fn snippet(&mut self, depth: usize) {
        if self.bag.is_empty() {
            self.bag = (0..NESTABLE.len() + FLAT.len()).collect();
            self.rng.shuffle(&mut self.bag);
        }
        let pick = self.bag.pop().unwrap_or_default();
        let (template, depth) = match NESTABLE.get(pick) {
            Some(template) => (template, depth),
            None => (&FLAT[pick - NESTABLE.len()], 0),
        };
        let text = self.fill(template);
        self.line(depth, text.trim_matches('\n'));
    }

    fn arg(&mut self, depth: usize) -> String {
        let (name, word) = (self.name(), self.rng.pick(&WORDS));
        let extension = self.rng.pick(&EXTENSIONS);
        match self.rng.below(15) {
            0 => word.to_owned(),
            1 => self.rng.pick(&FLAGS).to_owned(),
            2 => format!("\"${name}\""),
            3 => match self.rng.below(6) {
                0 => format!("\"${{{name}:-{word}}}\""),
                1 => format!("\"${{{name}%.{extension}}}\""),
                2 => format!("\"${{{name}##*/}}\""),
                3 => format!("\"${{{name}/{word}/_}}\""),
                4 => format!("${{#{name}}}"),
                _ => {
                    format!("\"${{{name}:0:{}}}\"", self.below_nine())
                }
            },
            4 if depth < 2 => {
                format!("\"$({})\"", self.command(depth + 1))
            }
            5 => self.rng.pick(&PATHS).to_owned(),
            6 => self.number().to_string(),
            7 => format!("'{word} {extension}'"),
            8 => format!("{name}={word}"),
            9 => format!("*.{extension}"),
            10 => self
                .rng
                .pick(&["$1", "$2", "$@", "$#", "$?", "$$"])
                .to_owned(),
            11 => format!("\"{word} ${name} {extension}\""),
            12 if depth < 2 => {
                format!("`{}`", self.command(2))
            }
            13 => format!("$(( {name} + {} ))", self.below_nine()),
            _ => word.to_owned(),
        }
    }

    fn below_nine(&mut self) -> usize {
        self.rng.below(9) + 1
    }

    fn command(&mut self, depth: usize) -> String {
        let mut text = if self.rng.chance(35) {
            self.rng.pick(&COMMANDS).to_owned()
        } else {
            self.rng.pick(&TOOLS).to_owned()
        };
        for _ in 0..self.rng.below(5) {
            text.push(' ');
            text.push_str(&self.arg(depth));
        }
        text
    }

    fn pipeline(&mut self) -> String {
        let mut text = self.command(0);
        for _ in 0..self.rng.below(4) {
            let sep =
                if self.rng.chance(10) { " |& " } else { " | " };
            text.push_str(sep);
            text.push_str(self.rng.pick(&FILTERS));
        }
        if self.rng.chance(30) {
            text.push(' ');
            text.push_str(self.rng.pick(&REDIRECTS));
        }
        text
    }

    fn condition(&mut self) -> String {
        let (name, other) = (self.name(), self.name());
        let word = self.rng.pick(&WORDS);
        let extension = self.rng.pick(&EXTENSIONS);
        let command = self.rng.pick(&COMMANDS);
        match self.rng.below(10) {
            0 => format!("[ -f \"${name}\" ]"),
            1 => format!("[[ ${name} == *.{extension} ]]"),
            2 => format!("[[ -n \"${name}\" && ! -d \"${other}\" ]]"),
            3 => format!("! grep -q {word} \"${name}\""),
            4 => format!("(( {name} > {} ))", self.number()),
            5 => format!("[ \"${name}\" -eq {} ]", self.number()),
            6 => format!("command -v {command} > /dev/null 2>&1"),
            7 => format!("[[ \"${name}\" =~ ^[0-9]+$ ]]"),
            8 => format!("test -z \"${{{name}:-}}\""),
            _ => format!(
                "[ \"${name}\" = \"{word}\" ] || [ -e {word} ]"
            ),
        }
    }

    fn statement(&mut self) -> String {
        let (name, word) = (self.name(), self.rng.pick(&WORDS));
        let mut text = match self.rng.below(12) {
            0..=3 => self.pipeline(),
            4 => format!("{name}=$({})", self.pipeline()),
            5 => format!("{name}=\"{word} ${{{}}}\"", self.name()),
            6 => {
                let (cond, first) =
                    (self.condition(), self.command(0));
                format!("{cond} && {first} || echo {word}")
            }
            7 => {
                let parts: Vec<String> = (0..self.rng.below(4) + 1)
                    .map(|_| self.string_piece())
                    .collect();
                format!("echo {}", parts.join(" "))
            }
            8 => format!(
                "printf '%s: %d\\n' \"${name}\" {}",
                self.number()
            ),
            9 => format!("{} &", self.command(0)),
            10 => format!("{name}+=({word} {})", self.number()),
            _ => {
                let mut long = self.command(0);
                for _ in 0..self.rng.below(14) + 8 {
                    long.push(' ');
                    long.push_str(&self.arg(0));
                }
                long
            }
        };
        if self.rng.chance(12) {
            text.push_str(" # ");
            let words = self.rng.below(9) + 1;
            text.push_str(&self.prose(words));
        }
        text
    }

    fn string_piece(&mut self) -> String {
        let (name, word) = (self.name(), self.rng.pick(&WORDS));
        match self.rng.below(5) {
            0 => {
                let words = self.rng.below(5) + 2;
                format!("\"{}: ${name}\"", self.prose(words))
            }
            1 => format!("'{word} $literal'"),
            2 => format!("\"${{{name}:-{word}}}\""),
            3 => format!("\"$({})\"", self.command(1)),
            _ => format!("${name}"),
        }
    }

    fn prose(&mut self, words: usize) -> String {
        let words: Vec<&str> =
            (0..words).map(|_| self.rng.pick(&WORDS)).collect();
        words.join(" ")
    }

    fn comment(&mut self, depth: usize) {
        for _ in 0..self.rng.below(4) + 1 {
            let words = self.rng.below(12) + 5;
            let text = format!("# {}", self.prose(words));
            self.line(depth, &text);
        }
    }

    fn statements(&mut self, depth: usize) {
        for _ in 0..self.rng.below(3) + 1 {
            let text = self.statement();
            self.line(depth, &text);
        }
    }

    fn body(&mut self, depth: usize) {
        if self.rng.chance(50) {
            self.snippet(depth + 1);
        } else {
            self.statements(depth + 1);
        }
        for _ in 0..self.rng.below(3) {
            self.block(depth + 1);
        }
    }

    fn block(&mut self, depth: usize) {
        match self.rng.below(100) {
            0..=44 => self.snippet(depth),
            45..=64 => self.statements(depth),
            65..=84 => self.comment(depth),
            85..=87 => self.line(depth, ""),
            _ if depth < MAX_DEPTH => self.group(depth),
            _ => self.statements(depth),
        }
    }

    fn group(&mut self, depth: usize) {
        let (name, other) = (self.name(), self.name());
        let word = self.rng.pick(&WORDS);
        let extension = self.rng.pick(&EXTENSIONS);
        match self.rng.below(9) {
            0 => {
                let cond = self.condition();
                self.line(depth, &format!("if {cond}; then"));
                self.body(depth);
                if self.rng.chance(35) {
                    let cond = self.condition();
                    self.line(depth, &format!("elif {cond}; then"));
                    self.body(depth);
                }
                if self.rng.chance(45) {
                    self.line(depth, "else");
                    self.body(depth);
                }
                self.line(depth, "fi");
            }
            1 => {
                let list = match self.rng.below(5) {
                    0 => format!("*.{extension}"),
                    1 => format!("\"${{{other}[@]}}\""),
                    2 => format!("$({})", self.command(1)),
                    3 => format!("{{1..{}}}", self.below_nine() + 1),
                    _ => format!("{word} {} {other}", self.arg(2)),
                };
                self.line(
                    depth,
                    &format!("for {name} in {list}; do"),
                );
                self.body(depth);
                self.line(depth, "done");
            }
            2 => {
                let cond = self.condition();
                self.line(depth, &format!("while {cond}; do"));
                self.body(depth);
                self.line(depth, "done");
            }
            3 => {
                let source = if self.rng.chance(50) {
                    format!("< \"${other}\"")
                } else {
                    format!("< <({})", self.command(1))
                };
                self.line(
                    depth,
                    &format!("while IFS= read -r {name}; do"),
                );
                self.body(depth);
                self.line(depth, &format!("done {source}"));
            }
            4 => {
                let head = if self.rng.chance(50) {
                    format!("{name}() {{")
                } else {
                    format!("function {name} {{")
                };
                self.line(depth, &head);
                self.body(depth);
                self.line(depth, "}");
            }
            5 => {
                let redirect = self.rng.pick(&REDIRECTS);
                self.line(depth, "{");
                self.body(depth);
                self.line(depth, &format!("}} {redirect}"));
            }
            6 => {
                self.line(depth, "(");
                self.body(depth);
                self.line(depth, ")");
            }
            7 => {
                let n = self.number();
                self.line(
                    depth,
                    &format!(
                        "for (( {name} = 0; {name} < {n}; {name}++ \
                         )); do"
                    ),
                );
                self.body(depth);
                self.line(depth, "done");
            }
            _ => {
                let cond = self.condition();
                self.line(depth, &format!("until {cond}; do"));
                self.body(depth);
                self.line(depth, "done");
            }
        }
    }
}
const NESTABLE: [&str; 46] = [
    r#"
@a@="$HOME/@w@"
@b@="${@a@:-/var/tmp}"
echo "@a@ is ${@a@} and ${#@a@} long"
echo "${@a@##*/}" "${@a@%.*}" "${@a@//-/_}" "${@a@^^}"
: "${@b@:=@n@}"
if [ -z "$@b@" ]; then echo "empty @b@"; fi
"#,
    r#"
@a@="$(basename "$(pwd)")"
echo "$@a@" a#b # a comment with $(nothing run)
@b@=`date +%s`
echo "$@a@" "`echo nested`" "$(echo "$(echo deep)")"
echo $# $? $$ $@ $* $1 ${10}
"#,
    r#"
while IFS= read -r @a@; do
  case "$@a@" in
    '#'*|'') continue ;;
    @w@=*) @b@=${@a@#*=} ;;
    *) echo "skip: $@a@" ;;
  esac
done < "$@b@"
"#,
    r#"
@f@() {
  local @a@=$1 @b@="${2:-@n@}"
  if [ -z "$@a@" ]; then
    echo "usage: @f@ <@a@> [@b@]" >&2
    return 1
  elif [[ $@a@ == *.@w@ ]]; then
    printf '%s\n' "$@a@"
  else
    echo "$@a@ $@b@"
  fi
}
"#,
    r#"
for @a@ in *.@w@ "$@b@"/*; do
  [ -e "$@a@" ] || continue
  @c@=$(basename "$(dirname "$(readlink -f "$@a@")")")
  echo "$@c@: $(wc -l < "$@a@") lines" # count the lines
done
"#,
    r#"
case "$1" in
  start|go) @f@ "$2" ;;
  stop) kill "$(cat "$@a@.pid")" 2>/dev/null || true ;;
  *) echo "usage: $0 {start|stop}" >&2; exit @n@ ;;
esac
"#,
    r#"
grep -rn "@w@" "$@a@" 2>/dev/null | sort -u | head -n @n@ > \
     "$@b@.out"
@x@ --verbose --flag=@n@ && echo ok || echo fail >&2
tail -f "$@a@" &
wait $!
diff <(sort a) <(sort b) > /dev/null
read -r @c@ <<< "$@a@"
"#,
    r#"
@a@=(@n@ 2 3 "$@b@")
@a@+=(four)
for ((i = 0; i < ${#@a@[@]}; i++)); do
  echo "${@a@[$i]}: $(( i * @n@ + 1 ))"
done
(( count++ ))
"#,
    r#"
until [ "$@a@" -ge @n@ ]; do @a@=$((@a@ + 1)); done
while :; do sleep 1; done & trap 'kill $!' EXIT
"#,
    r#"
function @f@ {
  declare -r @a@="@w@"
  export @b@=@n@
  { echo a; echo b; } | sort
  ( cd "$@a@" && pwd )
}
"#,
    r#"
if [[ -n "$@a@" && ! -d "$@b@" ]] || test -f "@c@.sh"; then
  mkdir -p "$@b@"
fi
[ "$@a@" = "@w@" ] && echo same
"#,
    r#"
@x@ --option-one "$@a@" \
    --option-two "$@b@" \
    --number @n@
"#,
    r#"
select @a@ in start stop @b@; do
  echo "chose $@a@"
  break
done
"#,
    r#"
time @x@ build --jobs=@n@
time { sleep 1; @x@ status; }
"#,
    r#"
case "$@a@" in
  a*) echo prefix ;;&
  *b) echo suffix ;&
  @w@) echo ext ;;
  *) echo other ;;
esac
"#,
    r#"
@x@ build |& tee "@b@.log"
@x@ test |& grep -c FAIL
"#,
    r#"
@x@ run &> "$@a@.@w@"
@x@ run &>> "@b@.log"
"#,
    r#"
echo "$@a@" | tee >(grep err > @b@.err) >(wc -l > @c@.count) > /dev/null
exec > >(tee -a "@b@.log") 2>&1
"#,
    r#"
ref=@a@
echo "${!ref}" "${!@b@[@]}" "${!@c@}"
"#,
    r#"
if [[ "$@a@" =~ ^[0-9]+$ ]]; then echo "@a@ is a number"; fi
[[ $@b@ =~ ^(@w@|log)$ ]] && echo "match ${BASH_REMATCH[1]}"
"#,
    r#"
readonly @u@=@n@
typeset -i @a@=@n@
typeset -a @b@=(1 2 3)
declare -A @c@=([one]=1 [two]=2)
@c@[three]=3
for @f@ in "${!@c@[@]}"; do echo "$@f@=${@c@[$@f@]}"; done
"#,
    r#"
exec 3< "@a@.@w@"
read -r @b@ <&3
exec 3<&-
exec 4>&-
echo ok >&2
exec 5<> "@c@.lock"
"#,
    r#"
echo $"hello @a@" {1..@n@} {a,b,c}.@w@
IFS=$'\n\t'
printf '%s\n' $'line1\nline2' "tab\there" 'single' "a\"b" \$literal
"#,
    r#"
while getopts "hv:o:" opt; do
  case $opt in
    h) usage; exit 0 ;;
    v) @a@=$OPTARG ;;
    o) @b@=$OPTARG ;;
    \?) exit 2 ;;
  esac
done
shift $((OPTIND - 1))
"#,
    r#"
set -- "$@a@" "@b@"
eval "$1=\"$2\""
trap 'rm -f "$@a@"; exit' INT TERM
trap - EXIT
"#,
    r#"
if ! command -v @x@ > /dev/null 2>&1; then
  echo "@x@ missing" >&2
  exit 127
fi
"#,
    r#"
(( @a@ += @n@ )); (( @b@ = @a@ * 2 ))
echo "$(( @a@ % 7 )) $(( (@a@ + @b@) / 2 )) $(( 1 << 3 ))"
let "@c@ = @a@ + 1"
"#,
    r#"
cat "$@a@" |
  grep -v '^#' |
  sort -u |
  awk '{ print $1 }' > "@b@.txt"
"#,
    r#"
@f@() {
  local -a @a@=("$@")
  local IFS=,
  echo "${@a@[*]}" "${#@a@[@]}" "${@a@[0]}"
  return $?
}
"#,
    r#"
echo "${@a@:1:3}" "${@a@: -2}" "${@a@,,}" "${@a@^}" "${@a@/#@w@/x}" "${@a@/%@w@/y}"
"#,
    r#"
@x@ one & pid1=$!
@x@ two & pid2=$!
wait $pid1 $pid2 || echo "failed: $?"
"#,
    r#"
while read -r @a@ @b@; do
  printf '%s -> %s\n' "$@a@" "$@b@"
done < <(@x@ list)
"#,
    r#"
@a@=`echo \`date +%s\``
@b@=`basename \`pwd\``
"#,
    r#"
[ "$@a@" = "@w@" ] || [ "$@a@" != "x" ] && echo ok
[ -n "$@b@" -a -r "@c@.@w@" ] && . "@c@.@w@"
"#,
    r#"
@x@ run 2>/dev/null 1>&2 < /dev/null
@x@ run >| "@a@.out"
@x@ run 2>&1 | tee -a "@b@.log"
"#,
    r#"
# --- @a@ ---
# @s@ @s@ @s@
@a@=@n@ # trailing comment about @s@
"#,
    r#"
export @u@=@p@ PATH="$PATH:@p@"
@u@=@n@ @x@ run
LC_ALL=C sort -u "$@a@" > "$@b@"
"#,
    r#"
@x@ check || { echo "check failed with $?" >&2; exit 1; }
! [ -d "$@a@" ] && mkdir -p "$@a@"
"#,
    r#"
echo "héllo wörld ✓ $@a@" # ünïcode comment
"#,
    r#"
@a@=$(( (@n@ + 1) * (${@b@:-2} - 1) ))
echo "$((@a@ / 2))" $((@a@ % 3)) $(( @a@ > @n@ ? 1 : 0 ))
"#,
    r#"
for @a@ in a b c; do
  for @b@ in 1 2 3; do
    [ "$@a@$@b@" = "b2" ] && break 2
    echo "$@a@$@b@"
  done
done
"#,
    r#"
@f@() { echo "$1" | tr a-z A-Z; }
@f@ "@s@" | head -n 1
"#,
    r#"
declare -x @u@="$(@x@ --version 2>&1 | head -n 1)"
unset @a@ @b@
"#,
    r#"
if [ "$(id -u)" -ne 0 ]; then
  exec sudo "$0" "$@"
fi
"#,
    r#"
read -r -p "continue (y/n)? " @a@
[[ $@a@ == [yY]* ]] || exit 1
"#,
    r#"
@a@=$(@x@ ls | while read -r l; do echo "${l%%.*}"; done | sort -u)
"#,
];

const FLAT: [&str; 12] = [
    r#"
cat <<EOF > "$@a@.@w@"
# generated for $USER on $(date +%F)
@b@=${@b@:-@n@}
path=$(dirname "$0")/@c@ and `hostname`
EOF
"#,
    r#"
cat <<'EOF' >> @c@.@w@
export PATH="$HOME/bin:$PATH"
alias ll='ls -la'   # kept as is: $(not run) `not run` ${not}
EOF
"#,
    r#"
@f@() {
@t@cat <<-EOF
@t@@t@name: $@a@
@t@@t@total: $(( @n@ + 1 ))
@t@@t@home: ${HOME:-/root}
@t@EOF
}
"#,
    r#"
echo 'single $quoted \n' "double $@a@ \"esc\" \$lit" $'tab\there'
@a@="multi
line string $@b@"
msg='it'\''s'
"#,
    r#"
@a@=$(cat <<EOF
inside $(echo @w@)
EOF
)
"#,
    r#"
cat <<ONE; cat <<'TWO'
first $@a@
ONE
second $@a@
TWO
"#,
    r#"
cat <<"EOF" > @c@.@w@
literal $@a@ and $(not run) and `not run` and ${not}
EOF
"#,
    r#"
cat <<\EOF | @x@ -
raw $@a@ stays raw
EOF
"#,
    r#"
cat <<EOF
total: $(
  wc -l < "@a@.@w@"
)
name: ${@b@:-
  default
}
EOF
echo after-heredoc
"#,
    r#"
cat <<-'EOF'
@t@literal $@a@ $(x)
@t@EOF
"#,
    r#"
@a@=`cat <<EOF
text $@b@
EOF
`
case $@a@ in
  @w@) echo seen ;;
esac
"#,
    r#"
@a@="line one
  line two with $(echo @n@)
line three"
echo "$@a@" | while read -r l; do echo ">> $l"; done
"#,
];

fn generate(target: usize) -> String {
    let mut g = Gen {
        rng: Rng(SEED),
        out: String::new(),
        lines: 0,
        bag: Vec::new(),
    };
    g.line(0, "#!/usr/bin/env bash");
    g.comment(0);
    g.line(0, "set -euo pipefail");
    while g.lines < target {
        g.block(0);
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
    use std::collections::HashSet;

    use rubrix_highlight::{Kind, spans};

    use super::*;

    const BASIC: &[&str] = &[
        "${", ":-", "$(", "`", " # ", "'", "\"", "if ", "case ",
        "while ", "for ", "done", "[[", "<<EOF",
    ];

    #[rustfmt::skip]
    const CONSTRUCTS: &[&str] = &[
        "<<EOF", "<<'EOF'", "<<-EOF", "<<\"EOF\"", "<<\\EOF",
        "<<-'EOF'", "<<ONE; cat <<'TWO'", "<<<", "$(\n", "`\n", ":-\n",
        "${", "${#", "${!", "[@]", "##", "%%", "//", "^^", ",,", ":-",
        "$(", "$((", "`", "\\`", "$'", "$\"", "a#b", " # ", "'", "\"",
        "if ", "elif ", "else", "fi", "case ", "esac", "for ",
        "select ", "while ", "until ", "do", "done", "function ",
        "() {", "time ", "local ", "declare ", "typeset ", "readonly ",
        "export ", "return", "break", "continue", "exit ", "exec ",
        "eval ", "trap ", "unset ", ";;&", ";&", ";;", "|&", "&>",
        "&>>", ">(", "<(", "=~", "[[", "]]", "[ ", "((", "+=", "=(",
        ">>", ">&", "<&", ">|", "<>", "2>", "&&", "||", " &\n", "\\\n",
        "$#", "$?", "$@", "$$", "$!", "$1", "{ ", " }",
    ];

    #[rustfmt::skip]
    const KINDS: &[Kind] = &[
        Kind::Comment, Kind::Function, Kind::FunctionBuiltin,
        Kind::FunctionDefinition, Kind::Keyword, Kind::KeywordControl,
        Kind::KeywordDeclaration, Kind::Number, Kind::Operator,
        Kind::PunctuationBracket, Kind::PunctuationDelimiter,
        Kind::PunctuationSpecial, Kind::String, Kind::StringEscape,
        Kind::StringSpecial, Kind::Variable, Kind::VariableSpecial,
    ];

    #[test]
    fn tiers_cover_every_construct() {
        for (tier, target) in TIERS {
            let text = generate(target);
            let wanted =
                if tier == "tiny" { BASIC } else { CONSTRUCTS };
            for &needle in wanted {
                assert!(
                    text.contains(needle),
                    "{tier} lacks {needle:?}"
                );
            }
        }
    }

    #[test]
    fn large_tier_lexes_to_every_shell_kind() {
        let text = generate(TIERS[2].1);
        let highlights = highlight(Lang::Shell, &text);
        let seen: HashSet<Kind> = (0..highlights.line_count())
            .flat_map(|line| spans(highlights.stored_runs(line)))
            .map(|(_, kind)| kind)
            .collect();
        for kind in KINDS {
            assert!(seen.contains(kind), "no {kind:?} run");
        }
    }

    #[test]
    fn large_tier_does_not_repeat_itself() {
        let text = generate(TIERS[2].1);
        let lines: Vec<&str> = text.lines().collect();
        let distinct: HashSet<&str> = lines.iter().copied().collect();
        assert!(
            distinct.len() * 100 >= lines.len() * 60,
            "only {} of {} lines are distinct",
            distinct.len(),
            lines.len()
        );
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
