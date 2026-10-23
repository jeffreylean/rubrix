use core::fmt;

macro_rules! kinds {
    ($($(#[doc = $doc:literal])* $variant:ident = $name:literal,)*) => {
        /// What a run of text is, as a flat dotted name such as
        /// `comment.doc`, modelled on Pierre's token list.
        ///
        /// A theme styles a kind and falls back along [`Kind::parent`]
        /// to a root kind. Numeric values may change between
        /// versions, so store a kind by name.
        #[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
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
    /// Removed diff lines. A root kind, as in Pierre, so a theme
    /// that styles only roots still tells it from `diff.plus`.
    DiffMinus = "diff.minus",
    /// Added diff lines. A root kind.
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
    /// for a root kind. The roots are the names with no dot, and
    /// `diff.plus` and `diff.minus`.
    pub const fn parent(self) -> Option<Kind> {
        PARENTS[self as usize]
    }

    /// The end of the parent chain.
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

/// Dotted kinds with no parent. Pierre's themes style these two and
/// have no `diff` key.
const DOTTED_ROOTS: [Kind; 2] = [Kind::DiffMinus, Kind::DiffPlus];

const PARENTS: [Option<Kind>; Kind::COUNT] = {
    let mut table = [None; Kind::COUNT];
    let mut i = 0;
    while i < Kind::COUNT {
        table[i] = parent_of(Kind::ALL[i]);
        i += 1;
    }
    table
};

const fn parent_of(kind: Kind) -> Option<Kind> {
    let mut i = 0;
    while i < DOTTED_ROOTS.len() {
        if DOTTED_ROOTS[i] as u8 == kind as u8 {
            return None;
        }
        i += 1;
    }
    let name = kind.name().as_bytes();
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
    use super::{DOTTED_ROOTS, Kind};

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
            assert_eq!(root.parent(), None);
            assert!(kind.name().starts_with(root.name()));
            assert!(
                !root.name().contains('.')
                    || DOTTED_ROOTS.contains(&root),
                "{kind} ends at {root}"
            );
            if let Some(parent) = kind.parent() {
                let (prefix, _) =
                    kind.name().rsplit_once('.').unwrap();
                assert_eq!(parent.name(), prefix);
            }
        }
    }
}
