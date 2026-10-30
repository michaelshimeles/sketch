//! SVG import through usvg: every shape becomes an editable vector layer with its fill
//! (solid, linear or radial gradient), stroke and opacity; embedded raster images become
//! image-filled rectangles; text is converted to outlines (using the bundled Inter faces).

use crate::geom::{Rect, V2, v2};
use crate::model::{
    Cap, Color, Document, FillRule, ImageMode, Join, Node, NodeId, NodeKind, Paint, Stop,
};
use crate::vector::{self, VPath};
use std::sync::Arc;

fn tf(t: &usvg::Transform, x: f32, y: f32) -> V2 {
    v2(
        (t.sx * x + t.kx * y + t.tx) as f64,
        (t.ky * x + t.sy * y + t.ty) as f64,
    )
}

fn color(c: usvg::Color, a: f32) -> Color {
    Color::rgba(
        c.red,
        c.green,
        c.blue,
        (a.clamp(0.0, 1.0) * 255.0).round() as u8,
    )
}

fn stops(s: &[usvg::Stop]) -> Vec<Stop> {
    s.iter()
        .map(|st| Stop {
            pos: st.offset().get() as f64,
            color: color(st.color(), st.opacity().get()),
        })
        .collect()
}

/// A paint in the node's unit box (gradients are given in user space by usvg).
fn paint(p: &usvg::Paint, opacity: f32, t: &usvg::Transform, b: Rect) -> Paint {
    let unit = |w: V2| v2((w.x - b.x) / b.w.max(1e-9), (w.y - b.y) / b.h.max(1e-9));
    match p {
        usvg::Paint::Color(c) => Paint::solid(color(*c, opacity)),
        usvg::Paint::LinearGradient(g) => {
            let gt = t.pre_concat(g.transform());
            let from = unit(tf(&gt, g.x1(), g.y1()));
            let to = unit(tf(&gt, g.x2(), g.y2()));
            let mut st = stops(g.stops());
            for s in &mut st {
                s.color.a = (s.color.a as f32 * opacity) as u8;
            }
            Paint::Linear {
                stops: st,
                from,
                to,
            }
        }
        usvg::Paint::RadialGradient(g) => {
            let gt = t.pre_concat(g.transform());
            let c = tf(&gt, g.cx(), g.cy());
            let rx = tf(&gt, g.cx() + g.r().get(), g.cy()).dist(c);
            let ry = tf(&gt, g.cx(), g.cy() + g.r().get()).dist(c);
            let mut st = stops(g.stops());
            for s in &mut st {
                s.color.a = (s.color.a as f32 * opacity) as u8;
            }
            Paint::Radial {
                stops: st,
                center: unit(c),
                radius: v2(rx / b.w.max(1e-9), ry / b.h.max(1e-9)),
            }
        }
        usvg::Paint::Pattern(_) => {
            Paint::solid(Color::rgba(0x99, 0x99, 0xa0, (opacity * 255.0) as u8))
        }
    }
}

fn path_of(p: &usvg::Path) -> VPath {
    let t = p.abs_transform();
    let mut out = VPath::new();
    for seg in p.data().segments() {
        use usvg::tiny_skia_path::PathSegment as S;
        match seg {
            S::MoveTo(a) => {
                out.move_to(tf(&t, a.x, a.y));
            }
            S::LineTo(a) => {
                out.line_to(tf(&t, a.x, a.y));
            }
            S::QuadTo(c, a) => {
                out.quad_to(tf(&t, c.x, c.y), tf(&t, a.x, a.y));
            }
            S::CubicTo(c1, c2, a) => {
                out.cubic_to(tf(&t, c1.x, c1.y), tf(&t, c2.x, c2.y), tf(&t, a.x, a.y));
            }
            S::Close => {
                out.close();
            }
        }
    }
    out
}

fn walk(g: &usvg::Group, opacity: f32, out: &mut Vec<Node>) {
    let opacity = opacity * g.opacity().get();
    for child in g.children() {
        match child {
            usvg::Node::Group(sub) => walk(sub, opacity, out),
            usvg::Node::Path(p) => {
                if !p.is_visible() {
                    continue;
                }
                let path = path_of(p);
                let Some(b) = path
                    .flatten(0.25)
                    .iter()
                    .flat_map(|poly| poly.pts.clone())
                    .map(|q| Rect::new(q.x, q.y, 0.0, 0.0))
                    .reduce(|a, r| a.union(&r))
                else {
                    continue;
                };
                let t = p.abs_transform();
                let mut n = Node::new(
                    NodeId(0),
                    NodeKind::Path {
                        subpaths: vector::to_subpaths(&path, b.min()),
                        fill_rule: match p.fill().map(|f| f.rule()) {
                            Some(usvg::FillRule::EvenOdd) => FillRule::EvenOdd,
                            _ => FillRule::NonZero,
                        },
                    },
                    b,
                );
                n.name = if p.id().is_empty() {
                    "Path".into()
                } else {
                    p.id().into()
                };
                n.style.fills = p
                    .fill()
                    .map(|f| vec![paint(f.paint(), f.opacity().get(), &t, b)])
                    .unwrap_or_default();
                n.style.stroke = None;
                if let Some(s) = p.stroke() {
                    let scale = ((t.sx * t.sx + t.ky * t.ky).sqrt() as f64
                        + (t.kx * t.kx + t.sy * t.sy).sqrt() as f64)
                        * 0.5;
                    n.style.stroke = Some(match s.paint() {
                        usvg::Paint::Color(c) => color(*c, s.opacity().get()),
                        other => paint(other, s.opacity().get(), &t, b).preview_color(),
                    });
                    n.style.stroke_width = s.width().get() as f64 * scale;
                    n.style.cap = match s.linecap() {
                        usvg::LineCap::Butt => Cap::Butt,
                        usvg::LineCap::Round => Cap::Round,
                        usvg::LineCap::Square => Cap::Square,
                    };
                    n.style.join = match s.linejoin() {
                        usvg::LineJoin::Round => Join::Round,
                        usvg::LineJoin::Bevel => Join::Bevel,
                        _ => Join::Miter,
                    };
                    n.style.dash = s
                        .dasharray()
                        .map(|d| d.iter().map(|x| *x as f64 * scale).collect())
                        .unwrap_or_default();
                }
                n.style.opacity = opacity as f64;
                out.push(n);
            }
            usvg::Node::Image(img) => {
                if !img.is_visible() {
                    continue;
                }
                let data: Option<Arc<Vec<u8>>> = match img.kind() {
                    usvg::ImageKind::PNG(d)
                    | usvg::ImageKind::JPEG(d)
                    | usvg::ImageKind::GIF(d)
                    | usvg::ImageKind::WEBP(d) => Some(d.clone()),
                    usvg::ImageKind::SVG(tree) => {
                        // Nested SVG: import its shapes too.
                        walk(tree.root(), opacity, out);
                        None
                    }
                };
                let Some(data) = data else { continue };
                let Ok(asset) = crate::assets::register(&data) else {
                    continue;
                };
                let bb = img.abs_bounding_box();
                let r = Rect::new(
                    bb.x() as f64,
                    bb.y() as f64,
                    bb.width() as f64,
                    bb.height() as f64,
                );
                let mut n = Node::new(NodeId(0), NodeKind::Rect, r);
                n.name = "Image".into();
                n.style.fills = vec![Paint::Image {
                    asset,
                    mode: ImageMode::Stretch,
                    opacity: 1.0,
                }];
                n.style.opacity = opacity as f64;
                out.push(n);
            }
            usvg::Node::Text(text) => walk(text.flattened(), opacity, out),
        }
    }
}

fn options() -> usvg::Options<'static> {
    let mut o = usvg::Options::default();
    let db = o.fontdb_mut();
    for i in 0..4 {
        db.load_font_data(crate::text::face_bytes(i).to_vec());
    }
    db.load_font_data(crate::theme::INTER_MEDIUM.to_vec());
    db.load_font_data(crate::theme::INTER_SEMIBOLD.to_vec());
    db.set_sans_serif_family("Inter");
    db.set_serif_family("Inter");
    db.set_monospace_family("Inter");
    o.font_family = "Inter".into();
    o
}

/// Parse SVG markup into layers (world coordinates = SVG user units, top-left at 0,0).
pub fn parse(svg: &str) -> Result<Vec<Node>, String> {
    let tree = usvg::Tree::from_str(svg, &options()).map_err(|e| format!("invalid SVG: {e}"))?;
    let mut out = Vec::new();
    walk(tree.root(), 1.0, &mut out);
    if out.is_empty() {
        return Err("the SVG has no visible shapes".into());
    }
    Ok(out)
}

/// Import an SVG as a page document with one group holding its shapes.
pub fn import_page(svg: &str, name: &str) -> Result<Document, String> {
    let nodes = parse(svg)?;
    let mut doc = Document {
        name: name.into(),
        ..Default::default()
    };
    let mut rec = crate::history::Recording::new("import", &doc);
    let mut tx = crate::history::Tx::new(&mut doc, &mut rec);
    insert_group(&mut tx, nodes, name, V2::ZERO);
    Ok(doc)
}

/// Insert imported shapes as a group (offset by `delta`). Returns the group id.
pub fn insert_group(
    tx: &mut crate::history::Tx,
    nodes: Vec<Node>,
    name: &str,
    delta: V2,
) -> NodeId {
    let gid = tx.alloc_id();
    let b = Rect::union_all(&nodes.iter().map(|n| n.rect).collect::<Vec<_>>()).unwrap_or_default();
    let mut g = Node::new(gid, NodeKind::Group, b.translate(delta));
    g.name = name.into();
    tx.insert(g, None, None);
    for mut n in nodes {
        n.id = tx.alloc_id();
        n.rect = n.rect.translate(delta - b.min());
        tx.insert(n, Some(gid), None);
    }
    crate::layout::normalize_group(tx, gid);
    gid
}

#[cfg(test)]
mod tests {
    use super::*;

    const SVG: &str = r##"<svg xmlns="http://www.w3.org/2000/svg" width="200" height="120" viewBox="0 0 200 120">
      <defs><linearGradient id="g" x1="0" y1="0" x2="1" y2="0">
        <stop offset="0" stop-color="#ff0000"/><stop offset="1" stop-color="#0000ff"/></linearGradient></defs>
      <rect x="10" y="10" width="80" height="50" rx="8" fill="url(#g)"/>
      <g transform="translate(100 0)" opacity="0.5">
        <circle cx="50" cy="40" r="30" fill="#00aa00" stroke="#000" stroke-width="4" stroke-dasharray="6 3"/>
      </g>
      <text x="20" y="100" font-size="20" fill="#333">Hello</text>
    </svg>"##;

    #[test]
    fn imports_shapes_gradients_strokes_and_text() {
        let nodes = parse(SVG).unwrap();
        assert!(nodes.len() >= 3, "{}", nodes.len());
        let rect = &nodes[0];
        assert_eq!(rect.rect.min(), v2(10.0, 10.0));
        assert!(matches!(rect.style.fills[0], Paint::Linear { .. }));
        let circle = &nodes[1];
        assert!((circle.rect.x - 120.0).abs() < 0.5, "{:?}", circle.rect);
        assert_eq!(circle.style.opacity, 0.5);
        assert_eq!(circle.style.stroke_width, 4.0);
        assert_eq!(circle.style.dash, vec![6.0, 3.0]);
        // Text came through as outlines.
        assert!(
            nodes.len() >= 3
                && nodes[2..]
                    .iter()
                    .all(|n| matches!(n.kind, NodeKind::Path { .. }))
        );
        let doc = import_page(SVG, "logo").unwrap();
        doc.validate().unwrap();
        assert_eq!(doc.root.len(), 1);
    }

    #[test]
    fn roundtrips_our_own_svg_export() {
        let ed = crate::demo::showcase();
        let svg = crate::io::svg::export(&ed.doc, None, None).unwrap();
        let nodes = parse(&svg).unwrap();
        assert!(nodes.len() > 20);
    }
}
