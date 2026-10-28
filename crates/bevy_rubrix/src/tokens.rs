use std::sync::Arc;

use bevy::color::{Alpha, Color, Laba, Luminance, Mix};
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

    /// The colour a diff tints added lines and words with. Pierre's
    /// green by default, a lighter one when [`Tone::Body`] is light,
    /// as on a dark theme.
    fn addition(&self) -> Color {
        pick(self, 0x5ECC71, 0x0DBE4E)
    }

    /// The colour a diff tints removed lines and words with. Pierre's
    /// red by default, picked as [`addition`](Self::addition) is.
    fn deletion(&self) -> Color {
        pick(self, 0xFF6762, 0xFF2E3F)
    }
}

/// `dark` on a theme whose body text is light, else `light`.
fn pick<T: TextTokens + ?Sized>(
    theme: &T,
    dark: u32,
    light: u32,
) -> Color {
    let on_dark = theme.tone(Tone::Body).luminance() >= 0.5;
    let [_, r, g, b] =
        if on_dark { dark } else { light }.to_be_bytes();
    Color::srgb_u8(r, g, b)
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

/// The fill of a changed row and the stronger fill of its changed
/// words, as Pierre's `style.css` mixes them. The row mixes `base`
/// into `background` in Lab, 20% on a dark background and 12% on a
/// light one. The words take `base` at alpha 0.2 or 0.15, over the
/// row.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Tint {
    pub(crate) row: Color,
    pub(crate) word: Color,
}

impl Tint {
    pub(crate) fn new(background: Color, base: Color) -> Tint {
        let dark = background.luminance() < 0.5;
        let (share, alpha) =
            if dark { (0.2, 0.2) } else { (0.12, 0.15) };
        let row =
            Laba::from(background).mix(&Laba::from(base), share);
        Tint {
            row: row.into(),
            word: base.with_alpha(alpha),
        }
    }
}

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

    fn srgb(color: Color) -> [u8; 4] {
        let rgba = color.to_srgba();
        [rgba.red, rgba.green, rgba.blue, rgba.alpha]
            .map(|channel| (channel * 255.0).round() as u8)
    }

    /// The expected values come from a CIE Lab (D65) mix computed
    /// apart from Bevy's.
    #[test]
    fn add_and_delete_rows_mix_the_background_with_their_colour() {
        struct Body(Color);
        impl TextTokens for Body {
            fn tone(&self, _: Tone) -> Color {
                self.0
            }

            fn body_size(&self) -> f32 {
                14.0
            }

            fn small_size(&self) -> f32 {
                11.0
            }
        }
        impl SyntaxTokens for Body {}
        let (dark, light) = (Body(Color::WHITE), Body(Color::BLACK));

        let panel = Color::srgb_u8(0x1E, 0x1E, 0x22);
        let add = Tint::new(panel, dark.addition());
        let delete = Tint::new(panel, dark.deletion());
        assert_eq!(srgb(add.row), [45, 61, 49, 255]);
        assert_eq!(srgb(delete.row), [72, 45, 46, 255]);
        assert_eq!(srgb(add.word), [0x5E, 0xCC, 0x71, 51]);
        assert_eq!(srgb(delete.word), [0xFF, 0x67, 0x62, 51]);

        let panel = Color::srgb_u8(0xF5, 0xF5, 0xF7);
        let add = Tint::new(panel, light.addition());
        let delete = Tint::new(panel, light.deletion());
        assert_eq!(srgb(add.row), [224, 239, 226, 255]);
        assert_eq!(srgb(delete.row), [253, 225, 223, 255]);
        assert_eq!(srgb(add.word), [0x0D, 0xBE, 0x4E, 38]);
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
