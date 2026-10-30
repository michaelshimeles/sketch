//! CSS ↔ node styles for the MCP tools.
//!
//! Agents describe styles the way Paper's MCP server expects them: React-style camelCase CSS
//! property maps (`{"backgroundColor": "#fff", "padding": "20px"}`) or inline `style="..."`
//! attributes. This module maps the subset that has a Sketch equivalent onto `Style`,
//! `TextStyle`, `AutoLayout` and `LayoutChild`, reports the rest as ignored, and turns nodes
//! back into CSS for `get_computed_styles` / `get_jsx`.

use crate::geom::v2;
use crate::model::rough::{FillStyle, Rough};
use crate::model::{
    Align, AutoLayout, BlendMode, Color, Direction, ImageMode, Node, NodeKind, Paint, Sizing, Stop,
    TextAlign,
};
use serde_json::{Map, Value};

/// An ordered list of `(camelCaseProperty, value)` pairs.
pub type Styles = Vec<(String, String)>;

/// `background-color` → `backgroundColor`; camelCase passes through.
pub fn camel(prop: &str) -> String {
    let mut out = String::new();
    let mut up = false;
    for c in prop.trim().chars() {
        if c == '-' {
            up = !out.is_empty();
        } else if up {
            out.extend(c.to_uppercase());
            up = false;
        } else {
            out.push(c);
        }
    }
    out
}

/// Parse an inline `style` attribute (`a: b; c: d`), respecting parentheses and quotes.
pub fn parse_inline(s: &str) -> Styles {
    let mut out = Vec::new();
    for decl in split_top(s, ';') {
        if let Some((k, v)) = decl.split_once(':') {
            let (k, v) = (k.trim(), v.trim().trim_end_matches("!important").trim());
            if !k.is_empty() && !v.is_empty() {
                out.push((camel(k), v.to_string()));
            }
        }
    }
    out
}

/// A JSON style object (`{"padding": 8, "color": "#000"}`) as styles. Numbers become px for
/// lengths and stay unitless for the few unitless properties.
pub fn from_json(v: &Value) -> Result<Styles, String> {
    let Some(m) = v.as_object() else {
        return Err("styles must be an object".into());
    };
    Ok(m.iter()
        .filter_map(|(k, v)| {
            let s = match v {
                Value::String(s) => s.clone(),
                Value::Number(n) => {
                    let unitless = matches!(
                        camel(k).as_str(),
                        "opacity"
                            | "fontWeight"
                            | "lineHeight"
                            | "flexGrow"
                            | "flexShrink"
                            | "zIndex"
                            | "roughness"
                            | "seed"
                            | "bowing"
                    );
                    if unitless {
                        n.to_string()
                    } else {
                        format!("{n}px")
                    }
                }
                Value::Bool(b) => b.to_string(),
                Value::Null => return None,
                other => other.to_string(),
            };
            Some((camel(k), s))
        })
        .collect())
}

/// Split on `sep` outside parentheses and quotes.
pub fn split_top(s: &str, sep: char) -> Vec<String> {
    let mut out = Vec::new();
    let (mut depth, mut quote, mut cur) = (0i32, None::<char>, String::new());
    for c in s.chars() {
        match (quote, c) {
            (Some(q), c) if c == q => quote = None,
            (Some(_), _) => {}
            (None, '"' | '\'') => quote = Some(c),
            (None, '(') => depth += 1,
            (None, ')') => depth -= 1,
            (None, c) if c == sep && depth == 0 => {
                out.push(std::mem::take(&mut cur));
                continue;
            }
            _ => {}
        }
        cur.push(c);
    }
    out.push(cur);
    out.into_iter()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .collect()
}

/// Split on whitespace outside parentheses (`1px solid rgb(0, 0, 0)` → 3 parts).
pub fn split_ws(s: &str) -> Vec<String> {
    let mut out = Vec::new();
    let (mut depth, mut cur) = (0i32, String::new());
    for c in s.chars() {
        match c {
            '(' => depth += 1,
            ')' => depth -= 1,
            c if c.is_whitespace() && depth == 0 => {
                if !cur.is_empty() {
                    out.push(std::mem::take(&mut cur));
                }
                continue;
            }
            _ => {}
        }
        cur.push(c);
    }
    if !cur.is_empty() {
        out.push(cur);
    }
    out
}

// ------------------------------------------------------------------------------ values

/// A CSS length in px. `em` is relative to `font_size`; percentages are returned as `None`
/// (use [`percent`]).
pub fn length(s: &str, font_size: f64) -> Option<f64> {
    let s = s.trim();
    let num = |t: &str| t.trim().parse::<f64>().ok().filter(|v| v.is_finite());
    if s == "0" {
        return Some(0.0);
    }
    if let Some(t) = s.strip_suffix("px") {
        return num(t);
    }
    if let Some(t) = s.strip_suffix("rem") {
        return num(t).map(|v| v * 16.0);
    }
    if let Some(t) = s.strip_suffix("em") {
        return num(t).map(|v| v * font_size);
    }
    if let Some(t) = s.strip_suffix("pt") {
        return num(t).map(|v| v * 4.0 / 3.0);
    }
    num(s)
}

pub fn percent(s: &str) -> Option<f64> {
    s.trim()
        .strip_suffix('%')
        .and_then(|t| t.trim().parse::<f64>().ok())
        .map(|v| v / 100.0)
}

/// 1–4 value shorthand (padding, border-radius): top/tl, right/tr, bottom/br, left/bl.
pub fn four(s: &str, font_size: f64) -> Option<[f64; 4]> {
    let v: Vec<f64> = split_ws(s)
        .iter()
        .map(|p| length(p, font_size))
        .collect::<Option<_>>()?;
    Some(match v.as_slice() {
        [a] => [*a; 4],
        [a, b] => [*a, *b, *a, *b],
        [a, b, c] => [*a, *b, *c, *b],
        [a, b, c, d, ..] => [*a, *b, *c, *d],
        [] => return None,
    })
}

const NAMED: &[(&str, u32)] = &[
    ("black", 0x000000),
    ("white", 0xffffff),
    ("red", 0xff0000),
    ("green", 0x008000),
    ("lime", 0x00ff00),
    ("blue", 0x0000ff),
    ("yellow", 0xffff00),
    ("orange", 0xffa500),
    ("purple", 0x800080),
    ("pink", 0xffc0cb),
    ("gray", 0x808080),
    ("grey", 0x808080),
    ("lightgray", 0xd3d3d3),
    ("lightgrey", 0xd3d3d3),
    ("darkgray", 0xa9a9a9),
    ("darkgrey", 0xa9a9a9),
    ("silver", 0xc0c0c0),
    ("navy", 0x000080),
    ("teal", 0x008080),
    ("aqua", 0x00ffff),
    ("cyan", 0x00ffff),
    ("magenta", 0xff00ff),
    ("fuchsia", 0xff00ff),
    ("maroon", 0x800000),
    ("olive", 0x808000),
    ("brown", 0xa52a2a),
    ("indigo", 0x4b0082),
    ("violet", 0xee82ee),
    ("gold", 0xffd700),
    ("coral", 0xff7f50),
    ("salmon", 0xfa8072),
    ("tomato", 0xff6347),
    ("crimson", 0xdc143c),
    ("skyblue", 0x87ceeb),
    ("steelblue", 0x4682b4),
    ("royalblue", 0x4169e1),
    ("dodgerblue", 0x1e90ff),
    ("tan", 0xd2b48c),
    ("beige", 0xf5f5dc),
    ("ivory", 0xfffff0),
    ("whitesmoke", 0xf5f5f5),
    ("gainsboro", 0xdcdcdc),
    ("slategray", 0x708090),
    ("darkslategray", 0x2f4f4f),
    ("forestgreen", 0x228b22),
    ("seagreen", 0x2e8b57),
    ("mintcream", 0xf5fffa),
    ("lavender", 0xe6e6fa),
    ("rebeccapurple", 0x663399),
];

fn channel(s: &str, scale: f64) -> Option<f64> {
    let s = s.trim();
    match percent(s) {
        Some(p) => Some(p * scale),
        None => s.parse::<f64>().ok(),
    }
}

fn alpha(s: Option<&String>) -> Option<f64> {
    match s {
        None => Some(1.0),
        Some(a) => channel(a, 1.0),
    }
}

fn to_u8(v: f64) -> u8 {
    (v.clamp(0.0, 1.0) * 255.0).round() as u8
}

fn func_args(s: &str) -> Option<(String, Vec<String>)> {
    let open = s.find('(')?;
    let name = s[..open].trim().to_ascii_lowercase();
    let inner = s[open + 1..].strip_suffix(')')?;
    // Both `rgb(1, 2, 3, .5)` and `rgb(1 2 3 / .5)`.
    let parts: Vec<String> = if inner.contains(',') {
        inner.split(',').map(|p| p.trim().to_string()).collect()
    } else {
        let (main, a) = match inner.split_once('/') {
            Some((m, a)) => (m, Some(a.trim().to_string())),
            None => (inner, None),
        };
        let mut v: Vec<String> = main.split_whitespace().map(String::from).collect();
        v.extend(a);
        v
    };
    Some((name, parts))
}

fn hsl(h: f64, s: f64, l: f64) -> [f64; 3] {
    let h = h.rem_euclid(360.0) / 360.0;
    let q = if l < 0.5 {
        l * (1.0 + s)
    } else {
        l + s - l * s
    };
    let p = 2.0 * l - q;
    let f = |t: f64| {
        let t = t.rem_euclid(1.0);
        if t < 1.0 / 6.0 {
            p + (q - p) * 6.0 * t
        } else if t < 0.5 {
            q
        } else if t < 2.0 / 3.0 {
            p + (q - p) * (2.0 / 3.0 - t) * 6.0
        } else {
            p
        }
    };
    [f(h + 1.0 / 3.0), f(h), f(h - 1.0 / 3.0)]
}

fn oklab_to_srgb(l: f64, a: f64, b: f64) -> [f64; 3] {
    let l_ = l + 0.396_337_777_4 * a + 0.215_803_757_3 * b;
    let m_ = l - 0.105_561_345_8 * a - 0.063_854_172_8 * b;
    let s_ = l - 0.089_484_177_5 * a - 1.291_485_548 * b;
    let (l3, m3, s3) = (l_.powi(3), m_.powi(3), s_.powi(3));
    let lin = [
        4.076_741_662_1 * l3 - 3.307_711_591_3 * m3 + 0.230_969_929_2 * s3,
        -1.268_438_004_6 * l3 + 2.609_757_401_1 * m3 - 0.341_319_396_5 * s3,
        -0.004_196_086_3 * l3 - 0.703_418_614_7 * m3 + 1.707_614_701 * s3,
    ];
    lin.map(|c| {
        if c <= 0.003_130_8 {
            12.92 * c
        } else {
            1.055 * c.powf(1.0 / 2.4) - 0.055
        }
    })
}

/// Parse a CSS color: hex, `rgb[a]()`, `hsl[a]()`, `oklch()`, `oklab()`, named colors and
/// `transparent`.
pub fn color(s: &str) -> Option<Color> {
    let s = s.trim();
    let lower = s.to_ascii_lowercase();
    if lower == "transparent" || lower == "none" {
        return Some(Color::TRANSPARENT);
    }
    if let Some(h) = s.strip_prefix('#') {
        let h: String = match h.len() {
            3 | 4 => h.chars().flat_map(|c| [c, c]).collect(),
            6 | 8 => h.to_string(),
            _ => return None,
        };
        return Color::from_hex(&format!("#{h}"));
    }
    if let Some((_, rgb)) = NAMED.iter().find(|(n, _)| *n == lower) {
        return Some(Color::rgb((rgb >> 16) as u8, (rgb >> 8) as u8, *rgb as u8));
    }
    let (name, a) = func_args(&lower)?;
    let rgb = match name.as_str() {
        "rgb" | "rgba" if a.len() >= 3 => [
            channel(&a[0], 255.0)? / 255.0,
            channel(&a[1], 255.0)? / 255.0,
            channel(&a[2], 255.0)? / 255.0,
        ],
        "hsl" | "hsla" if a.len() >= 3 => {
            let h = a[0].trim_end_matches("deg").parse::<f64>().ok()?;
            let sat = percent(&a[1]).or_else(|| a[1].parse::<f64>().ok().map(|v| v / 100.0))?;
            let l = percent(&a[2]).or_else(|| a[2].parse::<f64>().ok().map(|v| v / 100.0))?;
            hsl(h, sat, l)
        }
        "oklch" if a.len() >= 3 => {
            let l = channel(&a[0], 1.0)?;
            let c = channel(&a[1], 0.4)?;
            let h = a[2]
                .trim_end_matches("deg")
                .parse::<f64>()
                .ok()?
                .to_radians();
            oklab_to_srgb(l, c * h.cos(), c * h.sin())
        }
        "oklab" if a.len() >= 3 => oklab_to_srgb(
            channel(&a[0], 1.0)?,
            channel(&a[1], 0.4)?,
            channel(&a[2], 0.4)?,
        ),
        _ => return None,
    };
    let al = alpha(a.get(3))?;
    Some(Color::rgba(
        to_u8(rgb[0]),
        to_u8(rgb[1]),
        to_u8(rgb[2]),
        to_u8(al),
    ))
}

/// `rgba()` text for a color (hex when opaque).
pub fn color_css(c: Color) -> String {
    if c.a == 255 {
        c.to_hex()
    } else {
        format!(
            "rgba({}, {}, {}, {})",
            c.r,
            c.g,
            c.b,
            (c.a as f64 / 255.0 * 1000.0).round() / 1000.0
        )
    }
}

/// `linear-gradient(...)` / `radial-gradient(...)` as a paint.
pub fn gradient(s: &str) -> Option<Paint> {
    let (name, _) = func_args(s)?;
    let open = s.find('(')?;
    let inner = s[open + 1..].trim().strip_suffix(')')?;
    let args = split_top(inner, ',');
    let mut stops_src: &[String] = &args;
    let mut angle = 180.0f64; // to bottom
    if name == "linear-gradient" {
        let first = args.first()?.to_ascii_lowercase();
        if let Some(d) = first.strip_suffix("deg") {
            angle = d.trim().parse().ok()?;
            stops_src = &args[1..];
        } else if let Some(dir) = first.strip_prefix("to ") {
            let (mut x, mut y) = (0.0f64, 0.0f64);
            for w in dir.split_whitespace() {
                match w {
                    "top" => y = -1.0,
                    "bottom" => y = 1.0,
                    "left" => x = -1.0,
                    "right" => x = 1.0,
                    _ => {}
                }
            }
            angle = x.atan2(-y).to_degrees();
            stops_src = &args[1..];
        }
    } else if name == "radial-gradient" {
        if args
            .first()
            .is_some_and(|a| color(&split_ws(a)[0]).is_none())
        {
            stops_src = &args[1..];
        }
    } else {
        return None;
    }
    let n = stops_src.len();
    if n == 0 {
        return None;
    }
    let mut stops = Vec::new();
    for (i, st) in stops_src.iter().enumerate() {
        let parts = split_ws(st);
        let c = color(parts.first()?)?;
        let pos = parts.get(1).and_then(|p| percent(p)).unwrap_or(if n == 1 {
            0.0
        } else {
            i as f64 / (n - 1) as f64
        });
        stops.push(Stop { pos, color: c });
    }
    if stops.len() == 1 {
        stops.push(stops[0]);
        stops[1].pos = 1.0;
    }
    Some(if name == "linear-gradient" {
        let r = angle.to_radians();
        let d = v2(r.sin() * 0.5, -r.cos() * 0.5);
        Paint::Linear {
            stops,
            from: v2(0.5 - d.x, 0.5 - d.y),
            to: v2(0.5 + d.x, 0.5 + d.y),
        }
    } else {
        Paint::Radial {
            stops,
            center: v2(0.5, 0.5),
            radius: v2(0.5, 0.5),
        }
    })
}

fn gradient_css(p: &Paint) -> Option<String> {
    let stops = |s: &[Stop]| {
        s.iter()
            .map(|s| format!("{} {}%", color_css(s.color), (s.pos * 100.0).round()))
            .collect::<Vec<_>>()
            .join(", ")
    };
    match p {
        Paint::Linear { stops: s, from, to } => {
            let a = (to.x - from.x)
                .atan2(-(to.y - from.y))
                .to_degrees()
                .rem_euclid(360.0);
            Some(format!("linear-gradient({}deg, {})", a.round(), stops(s)))
        }
        Paint::Radial { stops: s, .. } => Some(format!("radial-gradient({})", stops(s))),
        _ => None,
    }
}

/// `url(...)` target.
pub fn url(s: &str) -> Option<String> {
    let inner = s.trim().strip_prefix("url(")?.strip_suffix(')')?;
    Some(inner.trim().trim_matches(['"', '\'']).to_string())
}

// ------------------------------------------------------------------------------ apply

/// Where the node sits, for properties whose meaning depends on the parent.
#[derive(Debug, Clone, Copy, Default)]
pub struct Context {
    /// The parent's auto layout (None = free positioning or page root).
    pub parent_layout: Option<AutoLayout>,
    /// The parent's size (for percentages and right/bottom).
    pub parent_size: Option<(f64, f64)>,
}

/// What `apply` did besides changing the node.
#[derive(Debug, Default)]
pub struct Applied {
    /// Properties with no effect in Sketch (reported back to the agent).
    pub ignored: Vec<String>,
    /// Image URL to resolve and use as the top fill (resolved by the caller).
    pub image: Option<(String, ImageMode)>,
    /// `right` / `bottom` offsets to resolve once the size is known.
    pub right: Option<f64>,
    pub bottom: Option<f64>,
}

fn axis_set(n: &mut Node, axis: usize, s: Sizing) {
    n.layout_child.sizing[axis] = s;
}

fn layout_mut(n: &mut Node) -> &mut AutoLayout {
    n.layout.get_or_insert(AutoLayout {
        direction: Direction::Horizontal,
        gap: 0.0,
        cross_gap: 0.0,
        padding: [0.0; 4],
        ..AutoLayout::default()
    })
}

fn set_text_flag(n: &mut Node, f: impl Fn(&mut crate::model::TextSpan)) {
    let len = n.kind.text().map_or(0, |t| t.chars().count());
    if len > 0 {
        n.text_style.apply_range(0, len, f);
    }
}

fn set_size(n: &mut Node, axis: usize, v: &str, ctx: &Context, a: &mut Applied, key: &str) {
    let fs = n.kind.font_size().unwrap_or(16.0);
    let v = v.trim();
    let is_text = matches!(n.kind, NodeKind::Text { .. });
    match v {
        "fit-content" | "auto" | "max-content" | "min-content" => {
            if is_text {
                if axis == 0
                    && let NodeKind::Text { auto_width, .. } = &mut n.kind
                {
                    *auto_width = true;
                }
                axis_set(n, axis, Sizing::Fixed);
            } else {
                axis_set(n, axis, Sizing::Hug);
            }
        }
        _ => {
            if let Some(p) = percent(v) {
                if ctx.parent_layout.is_some() && p >= 0.999 {
                    axis_set(n, axis, Sizing::Fill);
                    if axis == 0
                        && let NodeKind::Text { auto_width, .. } = &mut n.kind
                    {
                        *auto_width = false;
                    }
                } else if let Some(ps) = ctx.parent_size {
                    let size = if axis == 0 { ps.0 } else { ps.1 } * p;
                    resize_axis(n, axis, size);
                    axis_set(n, axis, Sizing::Fixed);
                } else {
                    a.ignored.push(key.into());
                }
            } else if let Some(px) = length(v, fs) {
                resize_axis(n, axis, px.max(0.0));
                axis_set(n, axis, Sizing::Fixed);
            } else {
                a.ignored.push(key.into());
            }
        }
    }
}

fn resize_axis(n: &mut Node, axis: usize, size: f64) {
    let mut r = n.rect;
    if axis == 0 {
        r.w = size;
        if let NodeKind::Text { auto_width, .. } = &mut n.kind {
            *auto_width = false;
        }
    } else {
        r.h = size;
    }
    if matches!(n.kind, NodeKind::Text { .. }) {
        n.rect = r;
    } else {
        n.set_rect_scaling(r);
    }
}

fn align_of(v: &str) -> Option<Align> {
    Some(match v.trim() {
        "flex-start" | "start" | "left" | "top" | "normal" | "stretch" | "baseline" => Align::Start,
        "center" => Align::Center,
        "flex-end" | "end" | "right" | "bottom" => Align::End,
        _ => return None,
    })
}

fn rough_mut(n: &mut Node) -> &mut Rough {
    let seed = (n.id.0 as u32).wrapping_mul(2_654_435_761).max(1);
    n.style.rough.get_or_insert(Rough {
        seed,
        ..Rough::default()
    })
}

/// Apply styles to a node. Layout-dependent properties use `ctx`; text properties apply to
/// text and sticky nodes; box properties to everything else.
pub fn apply(n: &mut Node, styles: &[(String, String)], ctx: &Context) -> Applied {
    let mut a = Applied::default();
    // Font size first: `em` values elsewhere depend on it.
    if let Some((_, v)) = styles.iter().rev().find(|(k, _)| k == "fontSize") {
        let cur = n.kind.font_size().unwrap_or(16.0);
        if let Some(px) = length(v, cur).filter(|p| *p > 0.0) {
            match &mut n.kind {
                NodeKind::Text { font_size, .. } | NodeKind::Sticky { font_size, .. } => {
                    *font_size = px
                }
                _ => {}
            }
        }
    }
    let is_text = n.kind.text().is_some();
    let fs = n.kind.font_size().unwrap_or(16.0);
    for (k, v) in styles {
        let v = v.trim();
        let lower = v.to_ascii_lowercase();
        let ok = match k.as_str() {
            "fontSize" => true, // applied above; inherited by text children
            "width" => {
                set_size(n, 0, v, ctx, &mut a, k);
                true
            }
            "height" => {
                set_size(n, 1, v, ctx, &mut a, k);
                true
            }
            "left" | "x" => match length(v, fs) {
                Some(x) => {
                    n.rect.x = x;
                    true
                }
                None => false,
            },
            "top" | "y" => match length(v, fs) {
                Some(y) => {
                    n.rect.y = y;
                    true
                }
                None => false,
            },
            "right" => {
                a.right = length(v, fs);
                a.right.is_some()
            }
            "bottom" => {
                a.bottom = length(v, fs);
                a.bottom.is_some()
            }
            "position" => {
                match lower.as_str() {
                    "absolute" | "fixed" => n.layout_child.absolute = true,
                    _ => n.layout_child.absolute = false,
                }
                true
            }
            "display" => match lower.as_str() {
                "none" => {
                    n.visible = false;
                    true
                }
                "flex" | "inline-flex" if n.is_frame() => {
                    n.visible = true;
                    layout_mut(n);
                    true
                }
                "block" | "inline-block" | "flex" | "inline-flex" => {
                    n.visible = true;
                    true
                }
                _ => false,
            },
            "visibility" => {
                n.visible = lower != "hidden";
                true
            }
            "flexDirection" if n.is_frame() => {
                layout_mut(n).direction = if lower.starts_with("column") {
                    Direction::Vertical
                } else {
                    Direction::Horizontal
                };
                true
            }
            "flexWrap" if n.is_frame() => {
                layout_mut(n).wrap = lower.starts_with("wrap");
                true
            }
            "gap" if n.is_frame() => {
                let parts = split_ws(v);
                match (
                    parts.first().and_then(|p| length(p, fs)),
                    parts.get(1).and_then(|p| length(p, fs)),
                ) {
                    (Some(r), c) => {
                        let l = layout_mut(n);
                        // CSS: `gap: row column`. Main-axis gap is the column gap in a row.
                        let c = c.unwrap_or(r);
                        if l.direction == Direction::Horizontal {
                            l.gap = c;
                            l.cross_gap = r;
                        } else {
                            l.gap = r;
                            l.cross_gap = c;
                        }
                        true
                    }
                    _ => false,
                }
            }
            "rowGap" | "columnGap" if n.is_frame() => match length(v, fs) {
                Some(g) => {
                    let l = layout_mut(n);
                    let main_is_row = l.direction == Direction::Horizontal;
                    if (k == "columnGap") == main_is_row {
                        l.gap = g;
                    } else {
                        l.cross_gap = g;
                    }
                    true
                }
                None => false,
            },
            "padding" if n.is_frame() => match four(v, fs) {
                Some(p) => {
                    layout_mut(n).padding = p;
                    true
                }
                None => false,
            },
            "paddingTop" | "paddingRight" | "paddingBottom" | "paddingLeft" if n.is_frame() => {
                match length(v, fs) {
                    Some(p) => {
                        let i = ["paddingTop", "paddingRight", "paddingBottom", "paddingLeft"]
                            .iter()
                            .position(|x| x == k)
                            .unwrap();
                        layout_mut(n).padding[i] = p;
                        true
                    }
                    None => false,
                }
            }
            "paddingInline" | "paddingBlock" if n.is_frame() => {
                let parts: Vec<f64> = split_ws(v).iter().filter_map(|p| length(p, fs)).collect();
                match parts.as_slice() {
                    [a0, rest @ ..] => {
                        let b0 = rest.first().copied().unwrap_or(*a0);
                        let l = layout_mut(n);
                        if k == "paddingInline" {
                            l.padding[3] = *a0;
                            l.padding[1] = b0;
                        } else {
                            l.padding[0] = *a0;
                            l.padding[2] = b0;
                        }
                        true
                    }
                    _ => false,
                }
            }
            "justifyContent" if n.is_frame() => {
                let l = layout_mut(n);
                if matches!(
                    lower.as_str(),
                    "space-between" | "space-around" | "space-evenly"
                ) {
                    l.gap_auto = true;
                    true
                } else if let Some(al) = align_of(&lower) {
                    l.gap_auto = false;
                    l.align_main = al;
                    true
                } else {
                    false
                }
            }
            "alignItems" if n.is_frame() => match align_of(&lower) {
                Some(al) => {
                    layout_mut(n).align_cross = al;
                    true
                }
                None => false,
            },
            "flexGrow" | "flex" => {
                let grow = split_ws(v)
                    .first()
                    .and_then(|g| g.parse::<f64>().ok())
                    .unwrap_or(0.0);
                match ctx.parent_layout {
                    Some(pl) => {
                        let ma = if pl.direction == Direction::Horizontal {
                            0
                        } else {
                            1
                        };
                        if grow > 0.0 {
                            axis_set(n, ma, Sizing::Fill);
                            if ma == 0
                                && let NodeKind::Text { auto_width, .. } = &mut n.kind
                            {
                                *auto_width = false;
                            }
                        }
                        true
                    }
                    None => false,
                }
            }
            "alignSelf" => match ctx.parent_layout {
                Some(pl) if lower == "stretch" => {
                    let ca = if pl.direction == Direction::Horizontal {
                        1
                    } else {
                        0
                    };
                    axis_set(n, ca, Sizing::Fill);
                    if ca == 0
                        && let NodeKind::Text { auto_width, .. } = &mut n.kind
                    {
                        *auto_width = false;
                    }
                    true
                }
                _ => false,
            },
            "fontFamily" if is_text => {
                n.text_style.font_family = match lower.trim() {
                    "" | "inherit" | "initial" | "unset" => None,
                    _ => Some(v.trim().to_string()),
                };
                true
            }
            "flexShrink" | "flexBasis" | "boxSizing" | "minWidth" | "minHeight" | "maxWidth"
            | "maxHeight" | "zIndex" | "cursor" | "whiteSpace" | "fontFamily" | "wordBreak"
            | "overflowWrap" => {
                // Accepted without an equivalent; harmless. fontFamily is inherited by text
                // children.
                !matches!(
                    k.as_str(),
                    "minWidth" | "minHeight" | "maxWidth" | "maxHeight"
                )
            }
            "overflow" if n.is_frame() => {
                if let NodeKind::Frame { clip } = &mut n.kind {
                    *clip = matches!(lower.as_str(), "hidden" | "clip" | "scroll" | "auto");
                }
                true
            }
            "backgroundColor" | "background" | "backgroundImage" | "fill" if !is_text => {
                apply_background(n, k, v, &mut a)
            }
            "backgroundColor" | "background"
                if is_text && matches!(n.kind, NodeKind::Sticky { .. }) =>
            {
                apply_background(n, k, v, &mut a)
            }
            "backgroundSize" | "objectFit" => {
                let mode = match lower.as_str() {
                    "contain" => ImageMode::Fit,
                    "100% 100%" | "fill" => ImageMode::Stretch,
                    "auto" | "none" => ImageMode::Tile,
                    _ => ImageMode::Fill,
                };
                if let Some((_, m)) = &mut a.image {
                    *m = mode;
                }
                for p in n.style.fills.iter_mut() {
                    if let Paint::Image { mode: m, .. } = p {
                        *m = mode;
                    }
                }
                true
            }
            "backgroundPosition" | "backgroundRepeat" | "objectPosition" => true,
            "color" if is_text => match color(v) {
                Some(c) => {
                    if matches!(n.kind, NodeKind::Sticky { .. }) {
                        // Sticky text color lives in the spans; the fill is the paper.
                        set_text_flag(n, |s| s.color = Some(c));
                    } else {
                        n.style.set_fill(Some(c));
                    }
                    true
                }
                None => false,
            },
            "color" => true, // inherited by text children; meaningless on boxes
            "fontWeight" if is_text => {
                let w: f64 = match lower.as_str() {
                    "bold" | "bolder" => 700.0,
                    "normal" | "lighter" => 400.0,
                    _ => v.parse().unwrap_or(400.0),
                };
                n.text_style.bold = w >= 600.0;
                n.text_style.font_weight = Some(w.clamp(1.0, 1000.0) as u16);
                true
            }
            "fontStyle" if is_text => {
                n.text_style.italic = lower == "italic" || lower == "oblique";
                true
            }
            "lineHeight" if is_text => {
                let lh = if lower == "normal" {
                    None
                } else if let Some(p) = percent(v) {
                    Some(p)
                } else if v.ends_with("px") || v.ends_with("em") || v.ends_with("rem") {
                    length(v, fs).map(|px| px / fs)
                } else {
                    v.parse::<f64>().ok()
                };
                n.text_style.line_height = lh.filter(|x| *x > 0.0);
                lh.is_some() || lower == "normal"
            }
            "letterSpacing" if is_text => {
                match length(v, fs).or((lower == "normal").then_some(0.0)) {
                    Some(ls) => {
                        n.text_style.letter_spacing = ls;
                        true
                    }
                    None => false,
                }
            }
            "textAlign" if is_text => {
                let al = match lower.as_str() {
                    "center" => TextAlign::Center,
                    "right" | "end" => TextAlign::Right,
                    _ => TextAlign::Left,
                };
                if let NodeKind::Text { align, .. } = &mut n.kind {
                    *align = al;
                }
                true
            }
            "textDecoration" | "textDecorationLine" if is_text => {
                let u = lower.contains("underline");
                let s = lower.contains("line-through");
                set_text_flag(n, |sp| {
                    sp.underline = Some(u);
                    sp.strike = Some(s);
                });
                true
            }
            "fontWeight" | "fontStyle" | "lineHeight" | "letterSpacing" | "textAlign"
            | "textDecoration" => true, // inherited by text children
            "border" | "outline" | "borderTop" | "borderRight" | "borderBottom" | "borderLeft" => {
                apply_border(n, v, fs)
            }
            "borderColor" | "stroke" => match color(v) {
                Some(c) => {
                    n.style.stroke = (c.a > 0).then_some(c);
                    if n.style.stroke_width <= 0.0 {
                        n.style.stroke_width = 1.0;
                    }
                    true
                }
                None => false,
            },
            "borderWidth" | "strokeWidth" => match length(v, fs) {
                Some(w) => {
                    n.style.stroke_width = w;
                    if k == "borderWidth" {
                        n.style.stroke_align = crate::model::StrokeAlign::Inside;
                    }
                    if w > 0.0 && n.style.stroke.is_none() {
                        n.style.stroke = Some(Color::INK);
                    }
                    true
                }
                None => false,
            },
            "borderStyle" | "strokeStyle" => {
                set_dash(n, &lower);
                true
            }
            "strokeDasharray" => {
                n.style.dash = split_ws(&v.replace(',', " "))
                    .iter()
                    .filter_map(|p| length(p, fs))
                    .collect();
                true
            }
            "borderRadius" => match four(v, fs) {
                Some(r) if r.iter().all(|x| *x == r[0]) => {
                    n.style.corner_radius = r[0];
                    n.style.radii = None;
                    true
                }
                Some(r) => {
                    n.style.radii = Some(r);
                    true
                }
                None => match percent(v) {
                    Some(p) => {
                        n.style.corner_radius = n.rect.w.min(n.rect.h) * p;
                        true
                    }
                    None => false,
                },
            },
            "borderTopLeftRadius"
            | "borderTopRightRadius"
            | "borderBottomRightRadius"
            | "borderBottomLeftRadius" => match length(v, fs) {
                Some(r) => {
                    let i = [
                        "borderTopLeftRadius",
                        "borderTopRightRadius",
                        "borderBottomRightRadius",
                        "borderBottomLeftRadius",
                    ]
                    .iter()
                    .position(|x| x == k)
                    .unwrap();
                    let mut rr = n.style.corner_radii();
                    rr[i] = r;
                    n.style.radii = Some(rr);
                    true
                }
                None => false,
            },
            "opacity" => match v.parse::<f64>().ok().or(percent(v)) {
                Some(o) => {
                    n.style.opacity = o.clamp(0.0, 1.0);
                    true
                }
                None => false,
            },
            "mixBlendMode" => match BlendMode::ALL.iter().find(|b| b.css() == lower) {
                Some(b) => {
                    n.style.blend = *b;
                    true
                }
                None => false,
            },
            // Excalidraw-style hand-drawn rendering (Sketch extensions).
            "sketch" | "handDrawn" => {
                if matches!(lower.as_str(), "true" | "1" | "on" | "yes") {
                    rough_mut(n);
                } else {
                    n.style.rough = None;
                }
                true
            }
            "roughness" | "sloppiness" => {
                let r = match lower.as_str() {
                    "architect" => Some(0.0),
                    "artist" => Some(1.0),
                    "cartoonist" => Some(2.0),
                    _ => v.parse::<f64>().ok(),
                };
                match r {
                    Some(r) => {
                        rough_mut(n).roughness = r.clamp(0.0, 4.0);
                        true
                    }
                    None => false,
                }
            }
            "fillStyle" => {
                let fsty = match lower.as_str() {
                    "hachure" => Some(FillStyle::Hachure),
                    "cross-hatch" | "crosshatch" => Some(FillStyle::CrossHatch),
                    "solid" => Some(FillStyle::Solid),
                    "zigzag" => Some(FillStyle::Zigzag),
                    "dots" => Some(FillStyle::Dots),
                    _ => None,
                };
                match fsty {
                    Some(f) => {
                        rough_mut(n).fill_style = f;
                        true
                    }
                    None => false,
                }
            }
            "bowing" => match v.parse::<f64>() {
                Ok(b) => {
                    rough_mut(n).bowing = b;
                    true
                }
                Err(_) => false,
            },
            "seed" => match v.parse::<u32>() {
                Ok(s) => {
                    rough_mut(n).seed = s.max(1);
                    true
                }
                Err(_) => false,
            },
            _ => false,
        };
        if !ok && !a.ignored.contains(k) {
            a.ignored.push(k.clone());
        }
    }
    a
}

fn set_dash(n: &mut Node, style: &str) {
    let w = n.style.stroke_width.max(1.0);
    n.style.dash = match style {
        "dashed" => vec![w * 4.0, w * 3.0],
        "dotted" => vec![w, w * 2.0],
        _ => Vec::new(),
    };
}

fn apply_border(n: &mut Node, v: &str, fs: f64) -> bool {
    let lower = v.to_ascii_lowercase();
    if lower == "none" || lower == "0" {
        n.style.stroke = None;
        return true;
    }
    let mut any = false;
    let mut style = "solid".to_string();
    for p in split_ws(v) {
        if let Some(w) = length(&p, fs) {
            n.style.stroke_width = w;
            any = true;
        } else if let Some(c) = color(&p) {
            n.style.stroke = (c.a > 0).then_some(c);
            any = true;
        } else if matches!(p.as_str(), "solid" | "dashed" | "dotted" | "double") {
            style = p;
            any = true;
        }
    }
    if any {
        if n.style.stroke.is_none() && !lower.contains("transparent") {
            n.style.stroke = Some(Color::INK);
        }
        n.style.stroke_align = crate::model::StrokeAlign::Inside;
        set_dash(n, &style);
    }
    any
}

fn apply_background(n: &mut Node, k: &str, v: &str, a: &mut Applied) -> bool {
    let lower = v.to_ascii_lowercase();
    if lower == "none" || lower == "transparent" {
        if k != "backgroundImage" {
            n.style.fills.clear();
        } else {
            n.style.fills.retain(|p| matches!(p, Paint::Solid { .. }));
        }
        return true;
    }
    // Layers: the first CSS layer is on top; ours are bottom to top.
    let mut fills = Vec::new();
    for layer in split_top(v, ',').iter().rev() {
        if let Some(c) = color(layer) {
            fills.push(Paint::solid(c));
        } else if let Some(g) = gradient(layer) {
            fills.push(g);
        } else if let Some(u) = url(layer) {
            a.image = Some((u, ImageMode::Fill));
        } else if let Some(c) = split_ws(layer).iter().find_map(|p| color(p)) {
            // `background: #fff no-repeat` and similar shorthands.
            fills.push(Paint::solid(c));
        } else {
            return false;
        }
    }
    if k == "backgroundImage" {
        // Keep the solid background color below the image/gradient layers.
        n.style.fills.retain(|p| matches!(p, Paint::Solid { .. }));
        n.style.fills.extend(fills);
    } else {
        n.style.fills = fills;
    }
    true
}

// ------------------------------------------------------------------------------ read back

fn px(v: f64) -> String {
    let r = (v * 100.0).round() / 100.0;
    if r == r.trunc() {
        format!("{}px", r as i64)
    } else {
        format!("{r}px")
    }
}

fn align_css(a: Align) -> &'static str {
    match a {
        Align::Start => "flex-start",
        Align::Center => "center",
        Align::End => "flex-end",
    }
}

/// The node's styles as camelCase CSS (what `get_computed_styles` returns).
pub fn computed(n: &Node, parent_layout: Option<&AutoLayout>, is_root: bool) -> Map<String, Value> {
    let mut m = Map::new();
    let mut put = |k: &str, v: String| {
        m.insert(k.into(), Value::String(v));
    };
    let in_flow = parent_layout.is_some() && !n.layout_child.absolute;
    if !in_flow && !is_root {
        put("position", "absolute".into());
    }
    if !in_flow {
        put("left", px(n.rect.x));
        put("top", px(n.rect.y));
    }
    let size_css = |axis: usize, v: f64| match n.layout_child.sizing[axis] {
        Sizing::Hug => "fit-content".to_string(),
        Sizing::Fill if parent_layout.is_some() => "100%".to_string(),
        _ => px(v),
    };
    if let NodeKind::Text { auto_width, .. } = &n.kind {
        if *auto_width {
            put("width", "fit-content".into());
        } else {
            put("width", size_css(0, n.rect.w));
        }
        put("height", "fit-content".into());
    } else {
        put("width", size_css(0, n.rect.w));
        put("height", size_css(1, n.rect.h));
    }
    if let Some(pl) = parent_layout
        && in_flow
    {
        let ma = if pl.direction == Direction::Horizontal {
            0
        } else {
            1
        };
        if n.layout_child.sizing[ma] == Sizing::Fill {
            put("flexGrow", "1".into());
        }
        if n.layout_child.sizing[1 - ma] == Sizing::Fill {
            put("alignSelf", "stretch".into());
        }
        put("flexShrink", "0".into());
    }
    if let Some(l) = &n.layout {
        put("display", "flex".into());
        put(
            "flexDirection",
            if l.direction == Direction::Vertical {
                "column"
            } else {
                "row"
            }
            .into(),
        );
        if l.wrap {
            put("flexWrap", "wrap".into());
        }
        if l.gap_auto {
            put("justifyContent", "space-between".into());
        } else {
            put("justifyContent", align_css(l.align_main).into());
            put("gap", px(l.gap));
        }
        put("alignItems", align_css(l.align_cross).into());
        let p = l.padding;
        if p.iter().any(|x| *x != 0.0) {
            put(
                "padding",
                if p.iter().all(|x| *x == p[0]) {
                    px(p[0])
                } else {
                    format!("{} {} {} {}", px(p[0]), px(p[1]), px(p[2]), px(p[3]))
                },
            );
        }
    }
    if let NodeKind::Frame { clip: true } = n.kind {
        put("overflow", "hidden".into());
    }
    if !n.visible {
        put("display", "none".into());
    }
    match &n.kind {
        NodeKind::Text {
            font_size, align, ..
        } => {
            let ts = &n.text_style;
            let face = crate::fonts::resolve(ts.font_family.as_deref(), ts.weight(), ts.italic);
            put("fontFamily", crate::fonts::family_name(face));
            put("fontSize", px(*font_size));
            put("fontWeight", ts.weight().to_string());
            if n.text_style.italic {
                put("fontStyle", "italic".into());
            }
            if let Some(lh) = n.text_style.line_height {
                put("lineHeight", px(lh * font_size));
            }
            if n.text_style.letter_spacing != 0.0 {
                put(
                    "letterSpacing",
                    format!(
                        "{}em",
                        (n.text_style.letter_spacing / font_size * 1000.0).round() / 1000.0
                    ),
                );
            }
            put(
                "textAlign",
                match align {
                    TextAlign::Left => "left",
                    TextAlign::Center => "center",
                    TextAlign::Right => "right",
                }
                .into(),
            );
            if let Some(c) = n.style.fill() {
                put("color", color_css(c));
            }
        }
        _ => {
            let layers: Vec<String> = n
                .style
                .fills
                .iter()
                .rev()
                .filter_map(|p| match p {
                    Paint::Solid { color } => Some(color_css(*color)),
                    Paint::Image { asset, .. } => Some(format!("url(paper-asset://{asset})")),
                    g => gradient_css(g),
                })
                .collect();
            if layers.len() == 1 && matches!(n.style.fills[0], Paint::Solid { .. }) {
                put("backgroundColor", layers[0].clone());
            } else if !layers.is_empty() {
                put("background", layers.join(", "));
            }
        }
    }
    if let Some(s) = n.style.stroke
        && n.style.stroke_width > 0.0
    {
        let kind = if n.style.dash.is_empty() {
            "solid"
        } else if n.style.dash[0] <= n.style.stroke_width * 1.5 {
            "dotted"
        } else {
            "dashed"
        };
        put(
            "border",
            format!("{} {kind} {}", px(n.style.stroke_width), color_css(s)),
        );
    }
    if let Some(r) = n.style.radii {
        put(
            "borderRadius",
            format!("{} {} {} {}", px(r[0]), px(r[1]), px(r[2]), px(r[3])),
        );
    } else if n.style.corner_radius > 0.0 {
        put("borderRadius", px(n.style.corner_radius));
    }
    if n.style.opacity < 1.0 {
        put(
            "opacity",
            format!("{}", (n.style.opacity * 1000.0).round() / 1000.0),
        );
    }
    if n.style.blend != BlendMode::Normal {
        put("mixBlendMode", n.style.blend.css().into());
    }
    if let Some(r) = n.style.rough {
        put("sketch", "true".into());
        put("roughness", format!("{}", r.roughness));
        put(
            "fillStyle",
            match r.fill_style {
                FillStyle::Hachure => "hachure",
                FillStyle::CrossHatch => "cross-hatch",
                FillStyle::Solid => "solid",
                FillStyle::Zigzag => "zigzag",
                FillStyle::Dots => "dots",
            }
            .into(),
        );
    }
    m
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::geom::Rect;
    use crate::model::NodeId;

    #[test]
    fn colors() {
        assert_eq!(color("#fff"), Some(Color::WHITE));
        assert_eq!(
            color("#11223380"),
            Some(Color::rgba(0x11, 0x22, 0x33, 0x80))
        );
        assert_eq!(color("rgb(255, 0, 0)"), Some(Color::rgb(255, 0, 0)));
        assert_eq!(
            color("rgba(0 0 255 / 50%)"),
            Some(Color::rgba(0, 0, 255, 128))
        );
        assert_eq!(color("hsl(120, 100%, 25%)"), Some(Color::rgb(0, 128, 0)));
        assert_eq!(color("RebeccaPurple"), Some(Color::rgb(0x66, 0x33, 0x99)));
        let w = color("oklch(1 0 0)").unwrap();
        assert!(w.r >= 254 && w.g >= 254 && w.b >= 254, "{w:?}");
        assert_eq!(color("nonsense"), None);
    }

    #[test]
    fn inline_and_json_styles() {
        let s = parse_inline("background-color: rgb(1, 2, 3); padding:4px 8px ;font-family:'A; B'");
        assert_eq!(s[0], ("backgroundColor".into(), "rgb(1, 2, 3)".into()));
        assert_eq!(s[1], ("padding".into(), "4px 8px".into()));
        assert_eq!(s[2].1, "'A; B'");
        let j = from_json(&serde_json::json!({"width": 10, "opacity": 0.5})).unwrap();
        assert!(j.contains(&("width".into(), "10px".into())));
        assert!(j.contains(&("opacity".into(), "0.5".into())));
    }

    #[test]
    fn frame_styles_map_to_auto_layout() {
        let mut n = Node::new(
            NodeId(1),
            NodeKind::Frame { clip: false },
            Rect::new(0.0, 0.0, 10.0, 10.0),
        );
        let a = apply(
            &mut n,
            &parse_inline(
                "display:flex; flex-direction:column; gap:12px; padding:8px 16px; \
                 justify-content:center; align-items:flex-end; width:320px; height:fit-content; \
                 background:#ff0000; border:2px dashed #000; border-radius:12px; box-shadow: 0 0 1px red",
            ),
            &Context::default(),
        );
        let l = n.layout.unwrap();
        assert_eq!(l.direction, Direction::Vertical);
        assert_eq!(l.gap, 12.0);
        assert_eq!(l.padding, [8.0, 16.0, 8.0, 16.0]);
        assert_eq!((l.align_main, l.align_cross), (Align::Center, Align::End));
        assert_eq!(n.rect.w, 320.0);
        assert_eq!(n.layout_child.sizing[1], Sizing::Hug);
        assert_eq!(n.style.fill(), Some(Color::rgb(255, 0, 0)));
        assert_eq!(n.style.stroke_width, 2.0);
        assert!(!n.style.dash.is_empty());
        assert_eq!(n.style.corner_radius, 12.0);
        assert_eq!(a.ignored, vec!["boxShadow".to_string()]);
        let back = computed(&n, None, true);
        assert_eq!(back["flexDirection"], "column");
        assert_eq!(back["borderRadius"], "12px");
    }

    #[test]
    fn gradients_and_sketch() {
        let Some(Paint::Linear { stops, from, to }) = gradient("linear-gradient(90deg, red, blue)")
        else {
            panic!()
        };
        assert_eq!(stops.len(), 2);
        assert!((from.x - 0.0).abs() < 1e-9 && (to.x - 1.0).abs() < 1e-9);
        let mut n = Node::new(NodeId(3), NodeKind::Rect, Rect::new(0.0, 0.0, 10.0, 10.0));
        apply(
            &mut n,
            &parse_inline("roughness: 2; fill-style: cross-hatch"),
            &Context::default(),
        );
        let r = n.style.rough.unwrap();
        assert_eq!((r.roughness, r.fill_style), (2.0, FillStyle::CrossHatch));
    }
}
