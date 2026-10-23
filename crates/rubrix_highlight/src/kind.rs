use core::fmt;

/// One list makes the enum, `ALL`, `COUNT`, the name table, and
/// `from_name`, so no second list of kinds can drift from it.
macro_rules! kinds {
    ($($(#[doc = $doc:literal])* $variant:ident = $name:literal,)*) => {
        /// What a run of text is, as a flat dotted name such as
        /// `comment.doc`, modelled on Pierre's token list.
        ///
        /// A theme styles a kind and falls back along [`Kind::parent`]
        /// to a root kind. Numeric values may change between
        /// versions, so store a kind by name.
        #[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
        #[derive(PartialOrd, Ord)]
        #[repr(u8)]
        pub enum Kind {
            #[default]
            $($(#[doc = $doc])* $variant,)*
        }

        impl Kind {
            /// Every kind, in declaration order.
            pub const ALL: [Kind; Kind::COUNT] = [$(Kind::$variant,)*];
            pub const COUNT: usize = [$($name,)*].len();
            const NAMES: [&'static str; Kind::COUNT] = [$($name,)*];

            pub fn from_name(name: &str) -> Option<Kind> {
                match name {
                    $($name => Some(Kind::$variant),)*
                    _ => None,
                }
            }
        }
    };
}

kinds! {
    /// Text with no finer kind: the plain lexer and whitespace-only
    /// lines. Pierre calls it `none`.
    Plain = "plain",
    /// Rust `#[..]` and `#![..]`, each one run.
    Attribute = "attribute",
    Boolean = "boolean",
    Comment = "comment",
    /// Rust `///`, `//!`, `/** */`, and `/*! */`. JSDoc bodies.
    CommentDoc = "comment.doc",
    /// `SCREAMING_CASE` names.
    Constant = "constant",
    /// `null`, `undefined`, `None`, `nil`, `iota`.
    ConstantBuiltin = "constant.builtin",
    /// A class name after `new`.
    Constructor = "constructor",
    /// The root of the diff kinds.
    Diff = "diff",
    /// `diff --git`, `index`, `---`, and `+++` lines.
    DiffHeader = "diff.header",
    /// `@@ -1,2 +1,3 @@` hunk headers.
    DiffHunk = "diff.hunk",
    /// Removed lines.
    DiffMinus = "diff.minus",
    /// Added lines.
    DiffPlus = "diff.plus",
    /// Calls, and shell command names.
    Function = "function",
    /// Built-in functions such as Python `print` or Go `len`.
    FunctionBuiltin = "function.builtin",
    /// TypeScript and Python decorators, each one run.
    FunctionDecorator = "function.decorator",
    /// The name a function definition binds.
    FunctionDefinition = "function.definition",
    /// Rust macro calls such as `println!`.
    FunctionMacro = "function.macro",
    /// Method calls such as `.len()`.
    FunctionMethod = "function.method",
    Keyword = "keyword",
    /// `if`, `else`, `match`, `for`, `while`, `loop`, `return`.
    KeywordControl = "keyword.control",
    /// Declaration keywords for lexers that set them apart.
    KeywordDeclaration = "keyword.declaration",
    /// `import`, `export`, `from`, `package`.
    KeywordImport = "keyword.import",
    /// JSDoc tags such as `@param`.
    KeywordJsdoc = "keyword.jsdoc",
    /// Word operators such as `typeof`, `in`, `and`, `not`.
    KeywordOperator = "keyword.operator",
    /// Rust lifetimes and loop labels, YAML anchors.
    Label = "label",
    /// YAML aliases such as `*base`.
    LabelAlias = "label.alias",
    /// Path segments before `::`, and package names.
    Namespace = "namespace",
    Number = "number",
    Operator = "operator",
    /// Fields, and TOML and YAML keys.
    Property = "property",
    /// JSON object keys.
    PropertyJsonKey = "property.json_key",
    Punctuation = "punctuation",
    PunctuationBracket = "punctuation.bracket",
    PunctuationDelimiter = "punctuation.delimiter",
    /// YAML `- ` list dashes.
    PunctuationListMarker = "punctuation.list_marker",
    /// Template `${ }`, f-string `{ }`, shell `$( )`, YAML `---`.
    PunctuationSpecial = "punctuation.special",
    String = "string",
    /// Rust chars and byte chars, Go runes.
    StringChar = "string.char",
    /// Python docstrings.
    StringDoc = "string.doc",
    StringEscape = "string.escape",
    StringRegex = "string.regex",
    /// TOML dates and times, shell heredoc delimiters.
    StringSpecial = "string.special",
    /// TOML `[table]` and `[[array]]` headers.
    Title = "title",
    Type = "type",
    TypeBuiltin = "type.builtin",
    /// The name a type definition binds, such as Go `type T`.
    TypeDefinition = "type.definition",
    /// JSDoc `{string}` types.
    TypeJsdoc = "type.jsdoc",
    Variable = "variable",
    /// JSDoc parameter names.
    VariableJsdoc = "variable.jsdoc",
    VariableParameter = "variable.parameter",
    /// `self`, `this`, `super`, and shell `$1`, `$@`.
    VariableSpecial = "variable.special",
}

impl Kind {
    pub const fn name(self) -> &'static str {
        Kind::NAMES[self as usize]
    }

    /// The kind named by dropping the last dotted segment, or `None`
    /// for a root kind.
    pub const fn parent(self) -> Option<Kind> {
        PARENTS[self as usize]
    }

    /// The end of the parent chain.
    ///
    /// `diff.plus` and `diff.minus` share the root `diff`, so a
    /// theme that styles only roots should also style those two.
    pub const fn root(self) -> Kind {
        let mut kind = self;
        while let Some(parent) = kind.parent() {
            kind = parent;
        }
        kind
    }
}

impl fmt::Display for Kind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.name())
    }
}

const PARENTS: [Option<Kind>; Kind::COUNT] = {
    let mut table = [None; Kind::COUNT];
    let mut i = 0;
    while i < Kind::COUNT {
        table[i] = parent_of(Kind::NAMES[i].as_bytes());
        i += 1;
    }
    table
};

/// Runs at compile time, so a dotted name whose prefix is not a kind
/// fails the build.
const fn parent_of(name: &[u8]) -> Option<Kind> {
    let mut dot = name.len();
    while dot > 0 {
        dot -= 1;
        if name[dot] == b'.' {
            let prefix = name.split_at(dot).0;
            let mut i = 0;
            while i < Kind::COUNT {
                if bytes_eq(Kind::NAMES[i].as_bytes(), prefix) {
                    return Some(Kind::ALL[i]);
                }
                i += 1;
            }
            panic!("a dotted kind's prefix must be a kind");
        }
    }
    None
}

const fn bytes_eq(a: &[u8], b: &[u8]) -> bool {
    if a.len() != b.len() {
        return false;
    }
    let mut i = 0;
    while i < a.len() {
        if a[i] != b[i] {
            return false;
        }
        i += 1;
    }
    true
}

#[cfg(test)]
mod tests {
    use super::Kind;

    #[test]
    fn names_round_trip() {
        for kind in Kind::ALL {
            assert_eq!(Kind::from_name(kind.name()), Some(kind));
        }
        assert_eq!(Kind::from_name("comment.doc.x"), None);
    }

    #[test]
    fn parent_chains_end_at_roots() {
        for kind in Kind::ALL {
            let root = kind.root();
            assert!(
                !root.name().contains('.'),
                "{kind} ends at {root}"
            );
            assert!(kind.name().starts_with(root.name()));
            if let Some(parent) = kind.parent() {
                let (prefix, _) =
                    kind.name().rsplit_once('.').unwrap();
                assert_eq!(parent.name(), prefix);
            }
        }
    }
}
