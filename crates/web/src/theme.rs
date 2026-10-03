//! Fonts and the colour scheme (DESIGN.md, ADR 0005).
//!
//! The fonts are Fontsource's variable WOFF2 files for Atkinson Hyperlegible
//! Next and Mono, vendored in `assets/fonts` (pinned to 5.3.0, OFL) and
//! bundled as assets, so release builds embed them and pages make no
//! third-party requests.
//!
//! The colour scheme follows the system unless the reader picks one with the
//! theme form, which posts to `/-/theme` and keeps the choice in a cookie
//! (NFR-UI-005). It works without JavaScript. It is a display preference of
//! the browser, not account data, so it has no API endpoint (ADR 0005).

use serde::Deserialize;
use topcoat::Result;
use topcoat::asset::{Asset, asset};
use topcoat::context::Cx;
use topcoat::cookie::time::Duration;
use topcoat::cookie::{Cookie, Cookies, SameSite, cookies};
use topcoat::font::{Font, UnicodeRange, UnicodeRanges, font};
use topcoat::router::content::Form;
use topcoat::router::error::{SeeOther, see_other};
use topcoat::router::route;

use crate::session::{core, safe_return_to};

/// Fontsource's `latin` and `latin-ext` subsets. The browser fetches a
/// subset's file only when the page uses a character from it.
const LATIN: UnicodeRanges = UnicodeRanges::new(&[
    UnicodeRange::from_u32(0x0000, 0x00FF),
    UnicodeRange::from_u32(0x0131, 0x0131),
    UnicodeRange::from_u32(0x0152, 0x0153),
    UnicodeRange::from_u32(0x02BB, 0x02BC),
    UnicodeRange::from_u32(0x02C6, 0x02C6),
    UnicodeRange::from_u32(0x02DA, 0x02DA),
    UnicodeRange::from_u32(0x02DC, 0x02DC),
    UnicodeRange::from_u32(0x0304, 0x0304),
    UnicodeRange::from_u32(0x0308, 0x0308),
    UnicodeRange::from_u32(0x0329, 0x0329),
    UnicodeRange::from_u32(0x2000, 0x206F),
    UnicodeRange::from_u32(0x20AC, 0x20AC),
    UnicodeRange::from_u32(0x2122, 0x2122),
    UnicodeRange::from_u32(0x2191, 0x2191),
    UnicodeRange::from_u32(0x2193, 0x2193),
    UnicodeRange::from_u32(0x2212, 0x2212),
    UnicodeRange::from_u32(0x2215, 0x2215),
    UnicodeRange::from_u32(0xFEFF, 0xFEFF),
    UnicodeRange::from_u32(0xFFFD, 0xFFFD),
]);
const LATIN_EXT: UnicodeRanges = UnicodeRanges::new(&[
    UnicodeRange::from_u32(0x0100, 0x02BA),
    UnicodeRange::from_u32(0x02BD, 0x02C5),
    UnicodeRange::from_u32(0x02C7, 0x02CC),
    UnicodeRange::from_u32(0x02CE, 0x02D7),
    UnicodeRange::from_u32(0x02DD, 0x02FF),
    UnicodeRange::from_u32(0x0304, 0x0304),
    UnicodeRange::from_u32(0x0308, 0x0308),
    UnicodeRange::from_u32(0x0329, 0x0329),
    UnicodeRange::from_u32(0x1D00, 0x1DBF),
    UnicodeRange::from_u32(0x1E00, 0x1E9F),
    UnicodeRange::from_u32(0x1EF2, 0x1EFF),
    UnicodeRange::from_u32(0x2020, 0x2020),
    UnicodeRange::from_u32(0x20A0, 0x20AB),
    UnicodeRange::from_u32(0x20AD, 0x20C0),
    UnicodeRange::from_u32(0x2113, 0x2113),
    UnicodeRange::from_u32(0x2C60, 0x2C7F),
    UnicodeRange::from_u32(0xA720, 0xA7FF),
]);

/// The sans file for Latin text, preloaded on every page.
pub const SANS_LATIN: Asset = asset!("assets/fonts/atkinson-hyperlegible-next-latin-wght-normal.woff2");

pub const SANS: Font = font! {
    "Atkinson Hyperlegible Next",
    @font-face {
        src: url(SANS_LATIN) format("woff2") tech("variations");
        font-weight: 200 800;
        font-style: normal;
        font-display: swap;
        unicode-range: LATIN;
    }
    @font-face {
        src: url(asset!(
                "assets/fonts/atkinson-hyperlegible-next-latin-ext-wght-normal.woff2",
            )) format("woff2") tech("variations");
            font-weight: 200 800;
            font-style: normal;
            font-display: swap;
            unicode-range: LATIN_EXT;
    }
};

pub const MONO: Font = font! {
    "Atkinson Hyperlegible Mono",
    @font-face {
        src: url(asset!(
                "assets/fonts/atkinson-hyperlegible-mono-latin-wght-normal.woff2",
            )) format("woff2") tech("variations");
            font-weight: 200 800;
            font-style: normal;
            font-display: swap;
            unicode-range: LATIN;
    }
    @font-face {
        src: url(asset!(
                "assets/fonts/atkinson-hyperlegible-mono-latin-ext-wght-normal.woff2",
            )) format("woff2") tech("variations");
            font-weight: 200 800;
            font-style: normal;
            font-display: swap;
            unicode-range: LATIN_EXT;
    }
};

const COOKIE: &str = "klotho_theme";

/// The reader's colour scheme choice.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Theme {
    /// Follow `prefers-color-scheme`.
    #[default]
    System,
    Light,
    Dark,
}

impl Theme {
    pub const ALL: [Theme; 3] = [Theme::System, Theme::Light, Theme::Dark];

    pub fn as_str(self) -> &'static str {
        match self {
            Theme::System => "system",
            Theme::Light => "light",
            Theme::Dark => "dark",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Theme::System => "System",
            Theme::Light => "Light",
            Theme::Dark => "Dark",
        }
    }

    /// The class for `<html>`: none lets the stylesheet follow the system.
    pub fn html_class(self) -> Option<&'static str> {
        match self {
            Theme::System => None,
            Theme::Light => Some("light"),
            Theme::Dark => Some("dark"),
        }
    }

    fn parse(value: &str) -> Theme {
        match value {
            "light" => Theme::Light,
            "dark" => Theme::Dark,
            _ => Theme::System,
        }
    }
}

/// The theme from the request's cookie.
pub fn current_theme(cx: &Cx) -> Theme {
    cookies(cx).get(COOKIE).map_or(Theme::System, |cookie| Theme::parse(cookie.value_trimmed()))
}

#[derive(Deserialize)]
pub struct ThemeForm {
    theme: Theme,
    return_to: Option<String>,
}

/// Stores the choice and goes back to the page the form was on.
#[route(POST "/-/theme")]
pub async fn set_theme(cx: &Cx, Form(form): Form<ThemeForm>) -> Result<SeeOther> {
    let jar = cookies(cx);
    let base = Cookie::build((COOKIE, form.theme.as_str()))
        .path("/")
        .http_only(true)
        .same_site(SameSite::Lax)
        .secure(core(cx).urls().is_https());
    if form.theme == Theme::System {
        jar.remove(base.build());
    } else {
        jar.add(base.max_age(Duration::days(365)).build());
    }
    Ok(see_other(safe_return_to(form.return_to.as_deref())))
}
