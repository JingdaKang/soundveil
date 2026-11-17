//! Which installed font each caption typeface is drawn in.
//!
//! The Mac app asks the system for a rounded, serif, or monospaced design of its own font. Slint
//! takes one family name and nothing else, so each typeface lists the families to try on this
//! system, best first, and the first one installed is used.

use soundveil_core::CaptionTypeface;

/// A family and the weights that suit it. A font with no such weight draws its nearest.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Face {
    /// Empty for the interface's own font.
    pub family: &'static str,
    pub weight: i32,
    /// For sound words and slang.
    pub strong: i32,
}

const fn face(family: &'static str, weight: i32, strong: i32) -> Face {
    Face {
        family,
        weight,
        strong,
    }
}

/// The interface's own font, at the weights captions had before typefaces. It is what Classic
/// means, and what any typeface falls back to when none of its families are installed.
const SYSTEM: Face = face("", 600, 700);

// Windows has no rounded interface font. Calibri is the nearest that every copy has: its stems
// and corners are rounded, which shows at caption sizes. It has no weight between regular and
// bold, so plain words are regular.
#[cfg(windows)]
const ROUNDED: &[Face] = &[face("Calibri", 400, 700)];
#[cfg(windows)]
const SERIF: &[Face] = &[face("Georgia", 400, 700), face("Cambria", 400, 700)];
#[cfg(windows)]
const TYPEWRITER: &[Face] = &[
    face("Cascadia Mono", 500, 700),
    face("Consolas", 400, 700),
    face("Courier New", 400, 700),
];

// These only matter for looking at the app on a Mac, which is not where it ships.
#[cfg(target_os = "macos")]
const ROUNDED: &[Face] = &[
    face("SF Pro Rounded", 500, 800),
    face("Arial Rounded MT Bold", 400, 400),
];
#[cfg(target_os = "macos")]
const SERIF: &[Face] = &[face("New York", 500, 800), face("Georgia", 400, 700)];
#[cfg(target_os = "macos")]
const TYPEWRITER: &[Face] = &[face("SF Mono", 500, 800), face("Menlo", 400, 700)];

// No font is certain on Linux, so each list runs from the best match to the most common. The
// rounded ones are rarely there unless someone added them. Ubuntu's and GNOME's own fonts are
// soft enough to stand in.
#[cfg(not(any(windows, target_os = "macos")))]
const ROUNDED: &[Face] = &[
    face("Nunito", 600, 800),
    face("M PLUS Rounded 1c", 500, 800),
    face("Varela Round", 400, 400),
    face("Ubuntu", 500, 700),
    face("Ubuntu Sans", 500, 800),
    face("Cantarell", 500, 800),
];
#[cfg(not(any(windows, target_os = "macos")))]
const SERIF: &[Face] = &[
    face("Noto Serif", 500, 800),
    face("Source Serif 4", 500, 800),
    face("DejaVu Serif", 400, 700),
    face("Liberation Serif", 400, 700),
];
#[cfg(not(any(windows, target_os = "macos")))]
const TYPEWRITER: &[Face] = &[
    face("Noto Sans Mono", 500, 800),
    face("DejaVu Sans Mono", 400, 700),
    face("Liberation Mono", 400, 700),
    face("Ubuntu Mono", 400, 700),
];

fn wanted(typeface: CaptionTypeface) -> &'static [Face] {
    match typeface {
        CaptionTypeface::Rounded => ROUNDED,
        CaptionTypeface::Classic => &[],
        CaptionTypeface::Serif => SERIF,
        CaptionTypeface::Typewriter => TYPEWRITER,
    }
}

fn choose(typeface: CaptionTypeface, mut installed: impl FnMut(&str) -> bool) -> Face {
    wanted(typeface)
        .iter()
        .copied()
        .find(|face| installed(face.family))
        .unwrap_or(SYSTEM)
}

/// The face for every typeface, in the order of `CaptionTypeface::ALL`.
#[derive(Clone, Copy, Debug)]
pub struct Faces([Face; CaptionTypeface::ALL.len()]);

impl Faces {
    /// Asks the font list Slint itself draws from, so a family found here is one it can use.
    pub fn installed() -> Self {
        let mut fonts = slint::fontique_011::shared_collection();
        Self::from(|family| fonts.family_id(family).is_some())
    }

    fn from(mut installed: impl FnMut(&str) -> bool) -> Self {
        Self(CaptionTypeface::ALL.map(|typeface| choose(typeface, &mut installed)))
    }

    pub fn face(&self, typeface: CaptionTypeface) -> Face {
        self.0[index(typeface)]
    }
}

/// Where a typeface sits in `CaptionTypeface::ALL`, which is also its place in Settings.
pub fn index(typeface: CaptionTypeface) -> usize {
    CaptionTypeface::ALL
        .iter()
        .position(|other| *other == typeface)
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classic_is_the_interfaces_own_font() {
        let faces = Faces::from(|_| true);
        assert_eq!(faces.face(CaptionTypeface::Classic), SYSTEM);
    }

    #[test]
    fn the_first_family_installed_is_used() {
        let faces = Faces::from(|_| true);
        for typeface in [
            CaptionTypeface::Rounded,
            CaptionTypeface::Serif,
            CaptionTypeface::Typewriter,
        ] {
            assert_eq!(faces.face(typeface), wanted(typeface)[0], "{typeface:?}");
        }
        let last = SERIF[SERIF.len() - 1];
        let faces = Faces::from(|family| family == last.family);
        assert_eq!(faces.face(CaptionTypeface::Serif), last);
    }

    #[test]
    fn with_none_installed_every_typeface_still_draws() {
        let faces = Faces::from(|_| false);
        for typeface in CaptionTypeface::ALL {
            assert_eq!(faces.face(typeface), SYSTEM, "{typeface:?}");
        }
    }

    #[test]
    fn every_typeface_has_families_except_classic() {
        for typeface in CaptionTypeface::ALL {
            let classic = typeface == CaptionTypeface::Classic;
            assert_eq!(wanted(typeface).is_empty(), classic, "{typeface:?}");
            for face in wanted(typeface) {
                assert!(!face.family.is_empty());
                assert!(face.weight <= face.strong, "{}", face.family);
            }
        }
    }

    #[test]
    fn typefaces_keep_their_place_in_settings() {
        for (place, typeface) in CaptionTypeface::ALL.into_iter().enumerate() {
            assert_eq!(index(typeface), place);
        }
    }
}
