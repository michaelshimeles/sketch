//! Font lookup: resolves CSS `font-family` lists to the bundled Inter faces or to fonts
//! installed on the system.
//!
//! Face ids below [`FIRST_SYSTEM_FACE`] are the bundled Inter faces. System faces get the
//! next ids the first time text uses them. The system font index is built on the first
//! lookup of a family other than Inter, so documents that only use Inter never scan.

use crate::text::{self, FaceId};
use std::collections::HashMap;
use std::sync::{Mutex, OnceLock, RwLock};

/// Bundled Inter Medium (500).
pub const INTER_MEDIUM: FaceId = 4;
/// Bundled Inter SemiBold (600).
pub const INTER_SEMIBOLD: FaceId = 5;
/// The first id handed to a face loaded from the system.
pub const FIRST_SYSTEM_FACE: FaceId = 6;

struct Loaded {
    face: &'static ttf_parser::Face<'static>,
    family: String,
}

/// System faces in id order (index + FIRST_SYSTEM_FACE). Faces live for the whole process.
static LOADED: RwLock<Vec<Loaded>> = RwLock::new(Vec::new());

struct Index {
    db: fontdb::Database,
    by_db_id: HashMap<fontdb::ID, FaceId>,
    /// (lowercase family, weight, italic) -> face, including misses.
    resolved: HashMap<(String, u16, bool), Option<FaceId>>,
}

fn index() -> &'static Mutex<Index> {
    static INDEX: OnceLock<Mutex<Index>> = OnceLock::new();
    INDEX.get_or_init(|| {
        let mut db = fontdb::Database::new();
        db.load_system_fonts();
        Mutex::new(Index {
            db,
            by_db_id: HashMap::new(),
            resolved: HashMap::new(),
        })
    })
}

/// A face loaded from the system, by id.
pub(crate) fn system_face(id: FaceId) -> Option<&'static ttf_parser::Face<'static>> {
    let i = id.checked_sub(FIRST_SYSTEM_FACE)? as usize;
    LOADED.read().unwrap().get(i).map(|l| l.face)
}

/// The family name of a face, as CSS would name it.
pub fn family_name(id: FaceId) -> String {
    match id.checked_sub(FIRST_SYSTEM_FACE) {
        Some(i) => LOADED.read().unwrap()[i as usize].family.clone(),
        None => "Inter".into(),
    }
}

/// The names in a CSS `font-family` list, unquoted, in order.
pub fn families(css: &str) -> impl Iterator<Item = &str> {
    css.split(',')
        .map(|f| f.trim().trim_matches(['"', '\'']).trim())
        .filter(|f| !f.is_empty())
}

/// Inter and the generic CSS families render with the bundled Inter faces.
fn is_bundled(name: &str) -> bool {
    matches!(
        name.to_ascii_lowercase().as_str(),
        "inter"
            | "sans-serif"
            | "serif"
            | "monospace"
            | "cursive"
            | "fantasy"
            | "system-ui"
            | "ui-sans-serif"
            | "ui-serif"
            | "ui-monospace"
            | "ui-rounded"
            | "-apple-system"
            | "blinkmacsystemfont"
            | "emoji"
            | "math"
            | "fangsong"
    )
}

/// The bundled Inter face nearest to a weight and slant.
pub fn inter(weight: u16, italic: bool) -> FaceId {
    if italic {
        return text::face_id(weight >= 600, true);
    }
    match weight {
        0..450 => 0,
        450..550 => INTER_MEDIUM,
        550..650 => INTER_SEMIBOLD,
        _ => 1,
    }
}

/// The face for a CSS `font-family` list at a weight (1 to 1000) and slant: the first
/// family in the list that is installed, else Inter. Generic families stop the search at
/// Inter.
pub fn resolve(family: Option<&str>, weight: u16, italic: bool) -> FaceId {
    if let Some(css) = family {
        for name in families(css) {
            if is_bundled(name) {
                break;
            }
            if let Some(id) = system(name, weight, italic) {
                return id;
            }
        }
    }
    inter(weight, italic)
}

/// Whether a family renders as itself (Inter, or installed on the system).
pub fn is_available(name: &str) -> bool {
    name.eq_ignore_ascii_case("inter") || !styles(name).is_empty()
}

/// The (weight, italic) pairs installed for a family, sorted. Empty when it isn't installed.
pub fn styles(name: &str) -> Vec<(u16, bool)> {
    let mut out: Vec<(u16, bool)> = if name.eq_ignore_ascii_case("inter") {
        vec![
            (400, false),
            (500, false),
            (600, false),
            (700, false),
            (400, true),
            (700, true),
        ]
    } else {
        let ix = index().lock().unwrap();
        ix.db
            .faces()
            .filter(|f| matches_name(f, name))
            .map(|f| (f.weight.0, f.style != fontdb::Style::Normal))
            .collect()
    };
    out.sort_unstable_by_key(|(w, i)| (*i, *w));
    out.dedup();
    out
}

fn matches_name(f: &fontdb::FaceInfo, name: &str) -> bool {
    f.families.iter().any(|(n, _)| n.eq_ignore_ascii_case(name))
        || f.post_script_name.eq_ignore_ascii_case(name)
}

fn system(name: &str, weight: u16, italic: bool) -> Option<FaceId> {
    let key = (name.to_lowercase(), weight, italic);
    let mut ix = index().lock().unwrap();
    if let Some(r) = ix.resolved.get(&key) {
        return *r;
    }
    let r = ix.find(name, weight, italic).and_then(|id| ix.load(id));
    ix.resolved.insert(key, r);
    r
}

impl Index {
    /// CSS font matching within one family: slant first, then normal width, then weight.
    fn find(&self, name: &str, weight: u16, italic: bool) -> Option<fontdb::ID> {
        let all: Vec<&fontdb::FaceInfo> =
            self.db.faces().filter(|f| matches_name(f, name)).collect();
        use fontdb::Style::{Italic, Normal, Oblique};
        let order = if italic {
            [Italic, Oblique, Normal]
        } else {
            [Normal, Oblique, Italic]
        };
        let styled: Vec<&fontdb::FaceInfo> = order
            .iter()
            .map(|s| {
                all.iter()
                    .copied()
                    .filter(|f| f.style == *s)
                    .collect::<Vec<_>>()
            })
            .find(|v| !v.is_empty())?;
        let normal: Vec<&fontdb::FaceInfo> = styled
            .iter()
            .copied()
            .filter(|f| f.stretch == fontdb::Stretch::Normal)
            .collect();
        let pool = if normal.is_empty() { styled } else { normal };
        let weights: Vec<u16> = pool.iter().map(|f| f.weight.0).collect();
        let w = closest_weight(weight, &weights)?;
        pool.iter().find(|f| f.weight.0 == w).map(|f| f.id)
    }

    fn load(&mut self, id: fontdb::ID) -> Option<FaceId> {
        if let Some(f) = self.by_db_id.get(&id) {
            return Some(*f);
        }
        let family = self.db.face(id)?.families.first()?.0.clone();
        let (data, index) = self.db.with_face_data(id, |d, i| (d.to_vec(), i))?;
        // Leaked on purpose: glyph meshes and layouts hold `&'static` faces for the process.
        let data: &'static [u8] = Box::leak(data.into_boxed_slice());
        let face: &'static ttf_parser::Face<'static> =
            Box::leak(Box::new(ttf_parser::Face::parse(data, index).ok()?));
        let mut loaded = LOADED.write().unwrap();
        let fid = FIRST_SYSTEM_FACE + loaded.len() as FaceId;
        loaded.push(Loaded { face, family });
        self.by_db_id.insert(id, fid);
        Some(fid)
    }
}

/// The installed weight CSS picks for `want` (CSS Fonts 4, font-weight matching).
fn closest_weight(want: u16, have: &[u16]) -> Option<u16> {
    if have.contains(&want) {
        return Some(want);
    }
    let below = || have.iter().copied().filter(|w| *w < want).max();
    let above = || have.iter().copied().filter(|w| *w > want).min();
    if (400..=500).contains(&want) {
        have.iter()
            .copied()
            .filter(|w| *w > want && *w <= 500)
            .min()
            .or_else(below)
            .or_else(above)
    } else if want < 400 {
        below().or_else(above)
    } else {
        above().or_else(below)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn weight_matching_follows_css() {
        let have = [300, 400, 600, 900];
        assert_eq!(closest_weight(400, &have), Some(400));
        // 400–500 look up to 500 first, then down, then up.
        assert_eq!(closest_weight(500, &have), Some(400));
        assert_eq!(closest_weight(450, &[300, 500, 600]), Some(500));
        // Below 400 looks down first; above 500 looks up first.
        assert_eq!(closest_weight(350, &have), Some(300));
        assert_eq!(closest_weight(700, &have), Some(900));
        assert_eq!(closest_weight(950, &have), Some(900));
        assert_eq!(closest_weight(400, &[]), None);
    }

    #[test]
    fn font_stacks_unquote_and_fall_back_to_inter() {
        let names: Vec<&str> = families(" 'Plus Jakarta Sans', \"Recoleta\",serif ").collect();
        assert_eq!(names, ["Plus Jakarta Sans", "Recoleta", "serif"]);
        assert_eq!(resolve(Some("No Such Family 3f9a"), 400, false), 0);
        assert_eq!(resolve(Some("sans-serif"), 500, false), INTER_MEDIUM);
        assert_eq!(resolve(None, 600, false), INTER_SEMIBOLD);
        assert_eq!(resolve(None, 700, true), text::face_id(true, true));
        assert_eq!(family_name(INTER_MEDIUM), "Inter");
    }
}
