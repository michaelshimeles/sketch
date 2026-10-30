//! Paints, blend modes, stroke and text styling, layout and constraint properties.

use super::node::{Color, NodeId};
use crate::geom::{V2, v2};
use serde::{Deserialize, Serialize};

fn one() -> f64 {
    1.0
}
fn is_one(x: &f64) -> bool {
    *x == 1.0
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Stop {
    pub pos: f64,
    pub color: Color,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ImageMode {
    /// Cover the box, cropping the overflow (Figma "Fill").
    #[default]
    Fill,
    /// Contain inside the box (Figma "Fit").
    Fit,
    /// Stretch to the box, ignoring the aspect ratio.
    Stretch,
    /// Repeat at the image's natural size.
    Tile,
}

/// One fill layer. Gradient geometry is in unit-box coordinates of the node (0..1).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "lowercase")]
pub enum Paint {
    Solid {
        color: Color,
    },
    Linear {
        stops: Vec<Stop>,
        from: V2,
        to: V2,
    },
    Radial {
        stops: Vec<Stop>,
        center: V2,
        /// Radii along x and y as fractions of the box.
        radius: V2,
    },
    Image {
        /// Content hash of an image registered in [`crate::assets`].
        asset: String,
        #[serde(default)]
        mode: ImageMode,
        #[serde(default = "one", skip_serializing_if = "is_one")]
        opacity: f64,
    },
}

impl Paint {
    pub fn solid(c: Color) -> Paint {
        Paint::Solid { color: c }
    }
    pub fn linear(a: Color, b: Color) -> Paint {
        Paint::Linear {
            stops: vec![Stop { pos: 0.0, color: a }, Stop { pos: 1.0, color: b }],
            from: v2(0.5, 0.0),
            to: v2(0.5, 1.0),
        }
    }
    pub fn radial(a: Color, b: Color) -> Paint {
        Paint::Radial {
            stops: vec![Stop { pos: 0.0, color: a }, Stop { pos: 1.0, color: b }],
            center: v2(0.5, 0.5),
            radius: v2(0.5, 0.5),
        }
    }
    pub fn kind_name(&self) -> &'static str {
        match self {
            Paint::Solid { .. } => "Solid",
            Paint::Linear { .. } => "Linear",
            Paint::Radial { .. } => "Radial",
            Paint::Image { .. } => "Image",
        }
    }
    /// A representative color (for swatches and simplified renderers).
    pub fn preview_color(&self) -> Color {
        match self {
            Paint::Solid { color } => *color,
            Paint::Linear { stops, .. } | Paint::Radial { stops, .. } => {
                stops.first().map_or(Color::BLACK, |s| s.color)
            }
            Paint::Image { .. } => Color::rgb(0xb0, 0xb0, 0xb8),
        }
    }
    pub fn stops(&self) -> Option<&[Stop]> {
        match self {
            Paint::Linear { stops, .. } | Paint::Radial { stops, .. } => Some(stops),
            _ => None,
        }
    }
}

/// Color of a gradient at `t` (stops sorted by position).
pub fn sample_stops(stops: &[Stop], t: f64) -> [f64; 4] {
    let f = |c: Color| {
        [
            c.r as f64 / 255.0,
            c.g as f64 / 255.0,
            c.b as f64 / 255.0,
            c.a as f64 / 255.0,
        ]
    };
    match stops {
        [] => [0.0; 4],
        [s] => f(s.color),
        _ => {
            if t <= stops[0].pos {
                return f(stops[0].color);
            }
            for w in stops.windows(2) {
                if t <= w[1].pos {
                    let span = (w[1].pos - w[0].pos).max(1e-9);
                    let k = ((t - w[0].pos) / span).clamp(0.0, 1.0);
                    let (a, b) = (f(w[0].color), f(w[1].color));
                    return [0, 1, 2, 3].map(|i| a[i] + (b[i] - a[i]) * k);
                }
            }
            f(stops.last().unwrap().color)
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize, Hash)]
#[serde(rename_all = "kebab-case")]
pub enum BlendMode {
    #[default]
    Normal,
    Multiply,
    Screen,
    Overlay,
    Darken,
    Lighten,
    ColorDodge,
    ColorBurn,
    HardLight,
    SoftLight,
    Difference,
    Exclusion,
    Hue,
    Saturation,
    Color,
    Luminosity,
}

impl BlendMode {
    pub const ALL: [BlendMode; 16] = [
        BlendMode::Normal,
        BlendMode::Multiply,
        BlendMode::Screen,
        BlendMode::Overlay,
        BlendMode::Darken,
        BlendMode::Lighten,
        BlendMode::ColorDodge,
        BlendMode::ColorBurn,
        BlendMode::HardLight,
        BlendMode::SoftLight,
        BlendMode::Difference,
        BlendMode::Exclusion,
        BlendMode::Hue,
        BlendMode::Saturation,
        BlendMode::Color,
        BlendMode::Luminosity,
    ];
    /// CSS / SVG `mix-blend-mode` name.
    pub fn css(self) -> &'static str {
        match self {
            BlendMode::Normal => "normal",
            BlendMode::Multiply => "multiply",
            BlendMode::Screen => "screen",
            BlendMode::Overlay => "overlay",
            BlendMode::Darken => "darken",
            BlendMode::Lighten => "lighten",
            BlendMode::ColorDodge => "color-dodge",
            BlendMode::ColorBurn => "color-burn",
            BlendMode::HardLight => "hard-light",
            BlendMode::SoftLight => "soft-light",
            BlendMode::Difference => "difference",
            BlendMode::Exclusion => "exclusion",
            BlendMode::Hue => "hue",
            BlendMode::Saturation => "saturation",
            BlendMode::Color => "color",
            BlendMode::Luminosity => "luminosity",
        }
    }
    pub fn label(self) -> &'static str {
        match self {
            BlendMode::Normal => "Normal",
            BlendMode::Multiply => "Multiply",
            BlendMode::Screen => "Screen",
            BlendMode::Overlay => "Overlay",
            BlendMode::Darken => "Darken",
            BlendMode::Lighten => "Lighten",
            BlendMode::ColorDodge => "Color dodge",
            BlendMode::ColorBurn => "Color burn",
            BlendMode::HardLight => "Hard light",
            BlendMode::SoftLight => "Soft light",
            BlendMode::Difference => "Difference",
            BlendMode::Exclusion => "Exclusion",
            BlendMode::Hue => "Hue",
            BlendMode::Saturation => "Saturation",
            BlendMode::Color => "Color",
            BlendMode::Luminosity => "Luminosity",
        }
    }
    /// Modes the GPU canvas composites exactly with fixed-function blending. The others are
    /// drawn as Normal on the canvas and exported exactly to SVG.
    pub fn gpu_exact(self) -> bool {
        matches!(
            self,
            BlendMode::Normal
                | BlendMode::Multiply
                | BlendMode::Screen
                | BlendMode::Darken
                | BlendMode::Lighten
        )
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum StrokeAlign {
    Inside,
    #[default]
    Center,
    Outside,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Cap {
    #[default]
    Butt,
    Round,
    Square,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Join {
    #[default]
    Miter,
    Round,
    Bevel,
}

// ---------------------------------------------------------------------------------- text

/// Style overrides for a range of characters `[start, end)` (char indices).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct TextSpan {
    pub start: usize,
    pub end: usize,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bold: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub italic: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub size: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub color: Option<Color>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub underline: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub strike: Option<bool>,
}

/// Node-wide text attributes beyond size and alignment.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct TextStyle {
    pub bold: bool,
    pub italic: bool,
    /// CSS `font-family` list (None = Inter). Families that aren't installed fall back to
    /// Inter.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub font_family: Option<String>,
    /// CSS weight, 1 to 1000. None = 400, or 700 when `bold`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub font_weight: Option<u16>,
    /// Line height as a multiple of the font size (None = font default).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub line_height: Option<f64>,
    /// Extra advance per character, in px.
    pub letter_spacing: f64,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub spans: Vec<TextSpan>,
}

impl TextStyle {
    pub fn is_default(&self) -> bool {
        *self == TextStyle::default()
    }

    /// The node's weight. `bold` wins when it disagrees with `font_weight`, so toggling
    /// bold on a 500-weight text switches it to 700 and back to 400.
    pub fn weight(&self) -> u16 {
        weight_for(self.bold, self.font_weight)
    }
}

/// `weight` when it agrees with `bold` (600 and up is bold), else 700 or 400.
fn weight_for(bold: bool, weight: Option<u16>) -> u16 {
    match weight {
        Some(w) if (w >= 600) == bold => w,
        _ if bold => 700,
        _ => 400,
    }
}

/// Resolved style of one character.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CharStyle {
    pub bold: bool,
    pub italic: bool,
    /// CSS weight; `bold` is `weight >= 600`.
    pub weight: u16,
    pub size: f64,
    pub color: Color,
    pub underline: bool,
    pub strike: bool,
}

impl TextStyle {
    pub fn char_style(&self, i: usize, size: f64, color: Color) -> CharStyle {
        let mut s = CharStyle {
            bold: self.bold,
            italic: self.italic,
            weight: self.weight(),
            size,
            color,
            underline: false,
            strike: false,
        };
        for sp in &self.spans {
            if i >= sp.start && i < sp.end {
                if let Some(b) = sp.bold {
                    s.bold = b;
                    s.weight = weight_for(b, Some(s.weight));
                }
                if let Some(b) = sp.italic {
                    s.italic = b;
                }
                if let Some(z) = sp.size {
                    s.size = z;
                }
                if let Some(c) = sp.color {
                    s.color = c;
                }
                if let Some(u) = sp.underline {
                    s.underline = u;
                }
                if let Some(u) = sp.strike {
                    s.strike = u;
                }
            }
        }
        s
    }

    /// Keep spans attached to their characters after inserting `n` chars at `at`.
    pub fn on_insert(&mut self, at: usize, n: usize) {
        for sp in &mut self.spans {
            // New chars take the style of the char before them, so typing at a span's start
            // (after unstyled text) pushes it right, and typing inside or at its end extends it.
            if at < sp.start || (at == sp.start && at > 0) {
                sp.start += n;
                sp.end += n;
            } else if at <= sp.end {
                sp.end += n;
            }
        }
    }

    /// Adjust spans after deleting chars `[a, b)`.
    pub fn on_delete(&mut self, a: usize, b: usize) {
        let n = b - a;
        let map = |x: usize| {
            if x <= a {
                x
            } else if x >= b {
                x - n
            } else {
                a
            }
        };
        for sp in &mut self.spans {
            sp.start = map(sp.start);
            sp.end = map(sp.end);
        }
        self.spans.retain(|s| s.end > s.start);
    }

    /// Apply `f` to the style of chars `[a, b)`, splitting spans as needed.
    pub fn apply_range(&mut self, a: usize, b: usize, f: impl Fn(&mut TextSpan)) {
        if b <= a {
            return;
        }
        let mut sp = TextSpan {
            start: a,
            end: b,
            ..Default::default()
        };
        f(&mut sp);
        self.spans.push(sp);
        self.normalize();
    }

    /// Merge overlapping spans into disjoint runs (later spans win), dropping empty ones.
    pub fn normalize(&mut self) {
        let mut cuts: Vec<usize> = self.spans.iter().flat_map(|s| [s.start, s.end]).collect();
        cuts.sort_unstable();
        cuts.dedup();
        let mut out: Vec<TextSpan> = Vec::new();
        for w in cuts.windows(2) {
            let (a, b) = (w[0], w[1]);
            let mut m = TextSpan {
                start: a,
                end: b,
                ..Default::default()
            };
            let mut any = false;
            for s in self.spans.iter().filter(|s| s.start <= a && s.end >= b) {
                any = true;
                m.bold = s.bold.or(m.bold);
                m.italic = s.italic.or(m.italic);
                m.size = s.size.or(m.size);
                m.color = s.color.or(m.color);
                m.underline = s.underline.or(m.underline);
                m.strike = s.strike.or(m.strike);
                // Later spans take precedence.
                if s.bold.is_some() {
                    m.bold = s.bold;
                }
                if s.italic.is_some() {
                    m.italic = s.italic;
                }
                if s.size.is_some() {
                    m.size = s.size;
                }
                if s.color.is_some() {
                    m.color = s.color;
                }
                if s.underline.is_some() {
                    m.underline = s.underline;
                }
                if s.strike.is_some() {
                    m.strike = s.strike;
                }
            }
            let empty = m.bold.is_none()
                && m.italic.is_none()
                && m.size.is_none()
                && m.color.is_none()
                && m.underline.is_none()
                && m.strike.is_none();
            if !any || empty {
                continue;
            }
            match out.last_mut() {
                Some(l)
                    if l.end == a
                        && (l.bold, l.italic, l.size, l.color, l.underline, l.strike)
                            == (m.bold, m.italic, m.size, m.color, m.underline, m.strike) =>
                {
                    l.end = b
                }
                _ => out.push(m),
            }
        }
        self.spans = out;
    }
}

// ---------------------------------------------------------------------------------- layout

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Direction {
    #[default]
    Horizontal,
    Vertical,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Align {
    #[default]
    Start,
    Center,
    End,
}

/// Figma-style auto layout (CSS flexbox): direction, gap, padding, alignment.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct AutoLayout {
    pub direction: Direction,
    pub wrap: bool,
    pub gap: f64,
    /// Distribute free space between children ("space between").
    pub gap_auto: bool,
    /// Gap between wrapped lines.
    pub cross_gap: f64,
    /// Top, right, bottom, left.
    pub padding: [f64; 4],
    pub align_main: Align,
    pub align_cross: Align,
}

impl Default for AutoLayout {
    fn default() -> Self {
        AutoLayout {
            direction: Direction::Horizontal,
            wrap: false,
            gap: 10.0,
            gap_auto: false,
            cross_gap: 10.0,
            padding: [10.0; 4],
            align_main: Align::Start,
            align_cross: Align::Start,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Sizing {
    #[default]
    Fixed,
    /// Shrink to fit the content (auto-layout frames and text).
    Hug,
    /// Fill the auto-layout parent.
    Fill,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Constraint {
    /// Left / top.
    #[default]
    Min,
    /// Right / bottom.
    Max,
    /// Left & right / top & bottom.
    Stretch,
    Center,
    Scale,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct Constraints {
    pub h: Constraint,
    pub v: Constraint,
}

/// Per-node layout participation (as a child of an auto-layout or constrained frame).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct LayoutChild {
    /// Horizontal and vertical sizing.
    pub sizing: [Sizing; 2],
    /// Ignore auto layout (positioned by constraints instead).
    pub absolute: bool,
    pub constraints: Constraints,
}

impl LayoutChild {
    pub fn is_default(&self) -> bool {
        *self == LayoutChild::default()
    }
}

/// Link from an instance (or a node inside one) to its main component counterpart.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct InstanceLink {
    /// The corresponding node in the main component.
    pub main: NodeId,
    /// Properties overridden on this node (not synced from the main component).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub overrides: Vec<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn spans_follow_edits() {
        let mut t = TextStyle::default();
        t.apply_range(2, 5, |s| s.bold = Some(true));
        assert_eq!((t.spans[0].start, t.spans[0].end), (2, 5));
        t.on_insert(0, 2); // before the span
        assert_eq!((t.spans[0].start, t.spans[0].end), (4, 7));
        t.on_insert(7, 1); // at its end: typing extends the bold run
        assert_eq!(t.spans[0].end, 8);
        t.on_insert(5, 3); // inside
        assert_eq!((t.spans[0].start, t.spans[0].end), (4, 11));
        t.on_delete(3, 6);
        assert_eq!((t.spans[0].start, t.spans[0].end), (3, 8));
        t.on_delete(0, 20);
        assert!(t.spans.is_empty());
    }

    #[test]
    fn apply_range_splits_and_merges() {
        let mut t = TextStyle::default();
        t.apply_range(0, 10, |s| s.bold = Some(true));
        t.apply_range(3, 6, |s| s.color = Some(Color::rgb(255, 0, 0)));
        assert_eq!(t.spans.len(), 3);
        let s = t.char_style(4, 12.0, Color::BLACK);
        assert!(s.bold && s.color == Color::rgb(255, 0, 0));
        t.apply_range(3, 6, |s| s.color = Some(Color::rgb(255, 0, 0)));
        assert_eq!(t.spans.len(), 3);
        t.apply_range(0, 10, |s| s.bold = Some(false));
        assert!(!t.char_style(1, 12.0, Color::BLACK).bold);
    }

    #[test]
    fn gradient_sampling() {
        let s = [
            Stop {
                pos: 0.0,
                color: Color::BLACK,
            },
            Stop {
                pos: 1.0,
                color: Color::WHITE,
            },
        ];
        assert_eq!(sample_stops(&s, 0.5)[0], 0.5);
        assert_eq!(sample_stops(&s, -1.0)[0], 0.0);
        assert_eq!(sample_stops(&s, 2.0)[0], 1.0);
    }
}
