//! How a theme colours code: a style per [`Kind`], and how a kind
//! with none falls back to its parents.

use std::sync::Arc;

use bevy::color::Color;
use bevy::text::{FontSource, FontStyle, FontWeight};
use bevy_fynix::tokens::{TextTokens, Tone};
use bevy_fynix::views::Look;
use rubrix_highlight::Kind;

use crate::CODE_FONT;

/// How a theme draws one kind. A `None` field falls back along
/// [`Kind::parent`], as [`resolve`] describes.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct SyntaxStyle {
    pub color: Option<Color>,
    pub face: Option<Face>,
}

impl SyntaxStyle {
    pub fn color(color: Color) -> SyntaxStyle {
        SyntaxStyle {
            color: Some(color),
            face: None,
        }
    }

    pub fn italic(self) -> SyntaxStyle {
        self.with_face(|face| face.style = FontStyle::Italic)
    }

    pub fn bold(self) -> SyntaxStyle {
        self.with_face(|face| face.weight = FontWeight::BOLD)
    }

    fn with_face(
        mut self,
        set: impl FnOnce(&mut Face),
    ) -> SyntaxStyle {
        let mut face = self.face.unwrap_or_default();
        set(&mut face);
        self.face = Some(face);
        self
    }
}

/// A weight and a slant, which one style sets together.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Face {
    pub weight: FontWeight,
    pub style: FontStyle,
}

/// A theme's colours for code. Every method has a default, so
/// `impl SyntaxTokens for MyTheme {}` colours code by the theme's
/// tones.
pub trait SyntaxTokens: TextTokens {
    /// The style of `kind` itself, before any fallback. A theme
    /// overrides the kinds it cares about and ends with
    /// `_ => default_style(self, kind)`.
    fn style(&self, kind: Kind) -> SyntaxStyle {
        default_style(self, kind)
    }

    /// The font of code and its line numbers.
    fn code_font(&self) -> FontSource {
        FontSource::Handle(CODE_FONT)
    }
}

/// [`SyntaxTokens::style`]'s default. Root kinds take a tone:
/// keyword, attribute, label, title and `diff.plus` take
/// [`Tone::Accent`]; string, number, boolean, constant and
/// `diff.minus` take [`Tone::Critical`]; comment takes
/// [`Tone::Faint`]. Every other kind has no style, so a dotted kind
/// takes its parent's and the rest take [`Tone::Body`].
pub fn default_style<T: TextTokens + ?Sized>(
    theme: &T,
    kind: Kind,
) -> SyntaxStyle {
    let tone = match kind {
        Kind::Keyword
        | Kind::Attribute
        | Kind::Label
        | Kind::Title
        | Kind::DiffPlus => Tone::Accent,
        Kind::String
        | Kind::Number
        | Kind::Boolean
        | Kind::Constant
        | Kind::DiffMinus => Tone::Critical,
        Kind::Comment => Tone::Faint,
        _ => return SyntaxStyle::default(),
    };
    SyntaxStyle::color(theme.tone(tone))
}

/// The colour and face `kind` is drawn in, as Pierre's
/// `resolveThemeSyntax` finds them: walk `kind` and then its parents.
/// The face is the first one met. The walk stops at the first
/// colour, so a face set only above it does not apply. With no colour
/// on the way, the colour is [`Tone::Body`]'s.
pub fn resolve<T: SyntaxTokens + ?Sized>(
    theme: &T,
    kind: Kind,
) -> (Color, Face) {
    let mut face = None;
    let mut at = Some(kind);
    while let Some(key) = at {
        let style = theme.style(key);
        face = face.or(style.face);
        if let Some(color) = style.color {
            return (color, face.unwrap_or_default());
        }
        at = key.parent();
    }
    (theme.tone(Tone::Body), face.unwrap_or_default())
}

/// Every kind's look under one theme, resolved once when a view is
/// built: a view reads its theme only then.
#[derive(Clone)]
pub(crate) struct Styles(Arc<[Look; Kind::COUNT]>);

impl Styles {
    pub(crate) fn new<T: SyntaxTokens + ?Sized>(theme: &T) -> Styles {
        Styles(Arc::new(Kind::ALL.map(|kind| {
            let (color, face) = resolve(theme, kind);
            Look {
                color: Some(color),
                weight: Some(face.weight),
                style: Some(face.style),
                ..Look::default()
            }
        })))
    }

    pub(crate) fn look(&self, kind: Kind) -> &Look {
        &self.0[kind as usize]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const BODY: Color = Color::WHITE;
    const BLUE: Color = Color::srgb(0.0, 0.0, 1.0);
    const GREEN: Color = Color::srgb(0.0, 1.0, 0.0);

    /// A theme that styles only the kinds `styles` names.
    struct Only(Vec<(Kind, SyntaxStyle)>);

    impl TextTokens for Only {
        fn tone(&self, tone: Tone) -> Color {
            match tone {
                Tone::Body => BODY,
                _ => Color::BLACK,
            }
        }

        fn body_size(&self) -> f32 {
            14.0
        }

        fn small_size(&self) -> f32 {
            11.0
        }
    }

    impl SyntaxTokens for Only {
        fn style(&self, kind: Kind) -> SyntaxStyle {
            self.0
                .iter()
                .find(|(styled, _)| *styled == kind)
                .map_or_else(SyntaxStyle::default, |(_, style)| {
                    *style
                })
        }
    }

    fn italic() -> Face {
        Face {
            style: FontStyle::Italic,
            ..Face::default()
        }
    }

    #[test]
    fn a_theme_styling_only_keyword_colours_keyword_control() {
        let theme =
            Only(vec![(Kind::Keyword, SyntaxStyle::color(BLUE))]);
        assert_eq!(
            resolve(&theme, Kind::KeywordControl),
            (BLUE, Face::default())
        );
        assert_eq!(resolve(&theme, Kind::String).0, BODY);
    }

    #[test]
    fn colour_and_face_resolve_from_different_keys() {
        let theme = Only(vec![
            (Kind::Keyword, SyntaxStyle::color(BLUE)),
            (Kind::KeywordControl, SyntaxStyle::default().italic()),
        ]);
        assert_eq!(
            resolve(&theme, Kind::KeywordControl),
            (BLUE, italic())
        );
        assert_eq!(
            resolve(&theme, Kind::Keyword),
            (BLUE, Face::default())
        );
    }

    #[test]
    fn a_face_above_the_nearest_colour_does_not_apply() {
        let theme = Only(vec![
            (Kind::Keyword, SyntaxStyle::color(BLUE).bold()),
            (Kind::KeywordControl, SyntaxStyle::color(GREEN)),
        ]);
        assert_eq!(
            resolve(&theme, Kind::KeywordControl),
            (GREEN, Face::default())
        );
    }

    #[test]
    fn a_face_beside_the_nearest_colour_applies() {
        let theme = Only(vec![(
            Kind::Comment,
            SyntaxStyle::color(GREEN).italic(),
        )]);
        assert_eq!(
            resolve(&theme, Kind::CommentDoc),
            (GREEN, italic())
        );
    }

    #[test]
    fn an_unstyled_root_is_body() {
        let theme = Only(vec![(
            Kind::Variable,
            SyntaxStyle::default().italic(),
        )]);
        assert_eq!(
            resolve(&theme, Kind::Operator),
            (BODY, Face::default())
        );
        assert_eq!(
            resolve(&theme, Kind::VariableParameter),
            (BODY, italic())
        );
    }

    #[test]
    fn the_default_styles_root_kinds_by_tone_and_dotted_kinds_inherit()
     {
        struct Toned;
        impl TextTokens for Toned {
            fn tone(&self, tone: Tone) -> Color {
                match tone {
                    Tone::Body => BODY,
                    Tone::Accent => BLUE,
                    Tone::Critical => GREEN,
                    Tone::Faint => Color::BLACK,
                    _ => Color::NONE,
                }
            }

            fn body_size(&self) -> f32 {
                14.0
            }

            fn small_size(&self) -> f32 {
                11.0
            }
        }
        impl SyntaxTokens for Toned {}

        let colour = |kind| resolve(&Toned, kind).0;
        assert_eq!(colour(Kind::KeywordControl), BLUE);
        assert_eq!(colour(Kind::Attribute), BLUE);
        assert_eq!(colour(Kind::DiffPlus), BLUE);
        assert_eq!(colour(Kind::StringEscape), GREEN);
        assert_eq!(colour(Kind::DiffMinus), GREEN);
        assert_eq!(colour(Kind::CommentDoc), Color::BLACK);
        assert_eq!(colour(Kind::Function), BODY);
        assert_eq!(colour(Kind::Plain), BODY);
        assert_eq!(
            default_style(&Toned, Kind::KeywordControl),
            SyntaxStyle::default()
        );
    }
}
