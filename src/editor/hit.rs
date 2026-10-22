//! Hit testing in world space.

use crate::geom::{Rect, V2, v2};
use crate::ink;
use crate::model::{Document, NodeId, NodeKind};
use crate::render::draw;

/// Tolerance for thin geometry, in screen pixels.
const HIT_SLOP_PX: f64 = 4.0;

/// Whether `p` (world) is inside the clip regions of all of the node's ancestors.
pub fn inside_ancestor_clips(doc: &Document, id: NodeId, p: V2) -> bool {
    doc.ancestors(id).into_iter().all(|a| {
        let n = doc.get(a).unwrap();
        !matches!(n.kind, NodeKind::Frame { clip: true })
            || doc.world_rect(a).is_some_and(|r| r.contains(p))
    })
}

/// Shape test for a single node, ignoring children and clipping.
pub fn node_contains(doc: &Document, id: NodeId, p: V2, zoom: f64) -> bool {
    let Some(n) = doc.get(id) else { return false };
    let Some(r) = doc.world_rect(id) else {
        return false;
    };
    let slop = HIT_SLOP_PX / zoom;
    match &n.kind {
        NodeKind::Frame { .. } | NodeKind::Rect | NodeKind::Text { .. } => r.contains(p),
        NodeKind::Group => false,
        NodeKind::Stroke {
            points,
            width,
            thinning,
        } => {
            if !r.inflate(slop).contains(p) {
                return false;
            }
            let local = p - r.min();
            let max_r = ink::radius(*width, *thinning, 1.0);
            ink::distance_to_polyline(points, local) <= max_r + slop
        }
        _ => {
            if !draw::draw_bounds(n, r).inflate(slop).contains(p) {
                return false;
            }
            let Some(path) = draw::shape_path(n, r) else {
                return false;
            };
            let tol = 0.25 / zoom;
            let even_odd = matches!(
                n.kind,
                NodeKind::Path {
                    fill_rule: crate::model::FillRule::EvenOdd,
                    ..
                }
            );
            // Tiny shapes are easier to grab by their box.
            if r.w.min(r.h) * zoom < 8.0 && draw::is_area(n) && r.inflate(slop).contains(p) {
                return true;
            }
            let filled =
                draw::is_area(n) && (!n.style.fills.is_empty() || n.style.stroke.is_none());
            (filled && path.contains(p, even_odd, tol))
                || path.distance(p, tol) <= n.style.stroke_width * 0.5 + slop
        }
    }
}

/// Topmost visible, unlocked node under `p`, deepest first.
pub fn hit_test(doc: &Document, p: V2, zoom: f64) -> Option<NodeId> {
    doc.paint_order().into_iter().rev().find(|id| {
        doc.is_effectively_visible(*id)
            && !doc.is_effectively_locked(*id)
            && node_contains(doc, *id, p, zoom)
            && inside_ancestor_clips(doc, *id, p)
    })
}

/// Screen-space rectangle of a top-level frame's name label.
pub fn frame_label_rect(frame_screen: &Rect, name_width_px: f64) -> Rect {
    Rect::new(
        frame_screen.x,
        frame_screen.y - 20.0,
        name_width_px.max(24.0) + 4.0,
        18.0,
    )
}

/// Which of the 8 resize handles (fractions of the bounds) is under the screen point.
pub fn handle_at(bounds_screen: &Rect, p: V2) -> Option<(f64, f64)> {
    const R: f64 = 7.0;
    let mut best: Option<((f64, f64), f64)> = None;
    for (hx, hy) in HANDLES {
        let hp = v2(
            bounds_screen.x + bounds_screen.w * hx,
            bounds_screen.y + bounds_screen.h * hy,
        );
        let d = (hp.x - p.x).abs().max((hp.y - p.y).abs());
        // Edge handles are hidden on tiny selections; corners win ties.
        if (hx == 0.5 || hy == 0.5) && bounds_screen.w.min(bounds_screen.h) < 24.0 {
            continue;
        }
        if d <= R && best.is_none_or(|(_, bd)| d < bd) {
            best = Some(((hx, hy), d));
        }
    }
    best.map(|b| b.0)
}

pub const HANDLES: [(f64, f64); 8] = [
    (0.0, 0.0),
    (0.5, 0.0),
    (1.0, 0.0),
    (1.0, 0.5),
    (1.0, 1.0),
    (0.5, 1.0),
    (0.0, 1.0),
    (0.0, 0.5),
];

/// Comments may reference locked elements without unlocking or selecting them.
pub fn comment_hit_test(doc: &Document, p: V2, zoom: f64) -> Option<NodeId> {
    doc.paint_order().into_iter().rev().find(|id| {
        doc.is_effectively_visible(*id)
            && node_contains(doc, *id, p, zoom)
            && inside_ancestor_clips(doc, *id, p)
    })
}
