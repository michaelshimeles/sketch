//! Draw ops → GPU triangle meshes (world space), plus the gradient ramp atlas.

use super::draw::{DrawOp, PaintSpec};
use super::mesh::{
    Mesh, PAINT_IMAGE, PAINT_IMAGE_TILE, PAINT_LINEAR, PAINT_RADIAL, PAINT_SOLID, Vertex,
};
use crate::geom::{Rect, V2, v2};
use crate::model::{Cap, Join, Stop, sample_stops};
use crate::text;
use crate::vector::{VPath, dash_polyline};
use lyon::tessellation::{
    BuffersBuilder, FillOptions, FillRule, FillTessellator, FillVertex, LineCap, LineJoin,
    StrokeOptions, StrokeTessellator, StrokeVertex, VertexBuffers,
};
use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};

/// Triangles that share one image texture (None = no texture).
#[derive(Debug, Clone, Default)]
pub struct Piece {
    pub image: Option<String>,
    pub mesh: Mesh,
}

// ---------------------------------------------------------------------------------- atlas

pub const ATLAS_W: u32 = 256;
pub const ATLAS_H: u32 = 256;

/// Gradient ramps baked into rows of a 256×256 texture, shared by every render.
pub struct GradientAtlas {
    rows: HashMap<String, u32>,
    pub pixels: Vec<u8>,
    /// Bumps when pixels change, so the GPU copy can be refreshed.
    pub version: u64,
}

pub fn atlas() -> &'static Mutex<GradientAtlas> {
    static A: OnceLock<Mutex<GradientAtlas>> = OnceLock::new();
    A.get_or_init(|| {
        Mutex::new(GradientAtlas {
            rows: HashMap::new(),
            pixels: vec![255; (ATLAS_W * ATLAS_H * 4) as usize],
            version: 1,
        })
    })
}

/// Atlas row for a set of stops (allocated on first use; wraps when full).
pub fn gradient_row(stops: &[Stop]) -> u32 {
    let key = format!("{stops:?}");
    let mut a = atlas().lock().unwrap();
    if let Some(r) = a.rows.get(&key) {
        return *r;
    }
    let row = (a.rows.len() as u32 % (ATLAS_H - 1)) + 1;
    for x in 0..ATLAS_W {
        let c = sample_stops(stops, x as f64 / (ATLAS_W - 1) as f64);
        let i = ((row * ATLAS_W + x) * 4) as usize;
        for (k, v) in c.iter().enumerate() {
            a.pixels[i + k] = (v * 255.0).round().clamp(0.0, 255.0) as u8;
        }
    }
    a.rows.insert(key, row);
    a.version += 1;
    row
}

// ---------------------------------------------------------------------------------- tessellation

pub struct Tess {
    fill: FillTessellator,
    stroke: StrokeTessellator,
    /// World-space tolerance.
    pub tol: f64,
    /// Zoom the mesh is built for (for greeking tiny text).
    pub zoom: f64,
}

impl Tess {
    pub fn new(zoom: f64) -> Tess {
        Tess {
            fill: FillTessellator::new(),
            stroke: StrokeTessellator::new(),
            tol: 0.15 / zoom,
            zoom,
        }
    }

    pub fn ops(&mut self, ops: &[DrawOp]) -> Vec<Piece> {
        let mut out: Vec<Piece> = Vec::new();
        for op in ops {
            let (image, mesh) = self.op(op);
            if mesh.indices.is_empty() {
                continue;
            }
            match out.last_mut() {
                Some(p) if p.image == image => p.mesh.append(&mesh),
                _ => out.push(Piece { image, mesh }),
            }
        }
        out
    }

    fn op(&mut self, op: &DrawOp) -> (Option<String>, Mesh) {
        match op {
            DrawOp::Fill {
                path,
                paint,
                even_odd,
            } => {
                let rule = if *even_odd {
                    FillRule::EvenOdd
                } else {
                    FillRule::NonZero
                };
                let (image, f) = paint_fn(paint);
                (image, self.fill_path(path, rule, f))
            }
            DrawOp::Stroke {
                path,
                color,
                width,
                cap,
                join,
                dash,
            } => (None, self.stroke(path, *color, *width, *cap, *join, dash)),
            DrawOp::Glyphs {
                face,
                size,
                color,
                glyphs,
            } => (None, self.glyphs(*face, *size, *color, glyphs)),
        }
    }

    fn fill_path(&mut self, path: &VPath, rule: FillRule, f: impl Fn(V2) -> Vertex) -> Mesh {
        let mut vb: VertexBuffers<Vertex, u32> = VertexBuffers::new();
        let _ = self.fill.tessellate_path(
            &path.to_lyon(),
            &FillOptions::tolerance(self.tol as f32).with_fill_rule(rule),
            &mut BuffersBuilder::new(&mut vb, |v: FillVertex| {
                let p = v.position();
                f(v2(p.x as f64, p.y as f64))
            }),
        );
        Mesh {
            vertices: vb.vertices,
            indices: vb.indices,
        }
    }

    pub fn stroke(
        &mut self,
        path: &VPath,
        color: [f32; 4],
        width: f64,
        cap: Cap,
        join: Join,
        dash: &[f64],
    ) -> Mesh {
        if color[3] <= 0.0 || width <= 0.0 {
            return Mesh::default();
        }
        let lcap = match cap {
            Cap::Butt => LineCap::Butt,
            Cap::Round => LineCap::Round,
            Cap::Square => LineCap::Square,
        };
        let ljoin = match join {
            Join::Miter => LineJoin::Miter,
            Join::Round => LineJoin::Round,
            Join::Bevel => LineJoin::Bevel,
        };
        let opts = StrokeOptions::tolerance(self.tol as f32)
            .with_line_width(width as f32)
            .with_line_cap(lcap)
            .with_line_join(ljoin);
        let dashed;
        let path = if dash.iter().any(|d| *d > 0.0) {
            let mut p = VPath::new();
            for poly in path.flatten(self.tol) {
                let scaled: Vec<f64> = dash.to_vec();
                for d in dash_polyline(&poly.pts, poly.closed, &scaled) {
                    p.append(&VPath::polyline(&d, false));
                }
            }
            dashed = p;
            &dashed
        } else {
            path
        };
        let mut vb: VertexBuffers<Vertex, u32> = VertexBuffers::new();
        let _ = self.stroke.tessellate_path(
            &path.to_lyon(),
            &opts,
            &mut BuffersBuilder::new(&mut vb, |v: StrokeVertex| {
                Vertex::solid(v.position().to_array(), color)
            }),
        );
        Mesh {
            vertices: vb.vertices,
            indices: vb.indices,
        }
    }

    fn glyphs(
        &mut self,
        face: text::FaceId,
        size: f64,
        color: [f32; 4],
        glyphs: &[(u16, V2)],
    ) -> Mesh {
        let mut m = Mesh::default();
        if size * self.zoom < 2.5 {
            // Too small to read: a faint bar per glyph run ("greeking").
            if let (Some(first), Some(last)) = (glyphs.first(), glyphs.last()) {
                let h = size * 0.4;
                let r = Rect::new(
                    first.1.x,
                    first.1.y - size * 0.55,
                    last.1.x - first.1.x + size * 0.5,
                    h,
                );
                let c = [color[0], color[1], color[2], color[3] * 0.35];
                push_quad(&mut m, r, c);
            }
            return m;
        }
        let upm = text::face_of(face).units_per_em() as f64;
        let s = (size / upm) as f32;
        for (gid, o) in glyphs {
            let gm = text::glyph_mesh(face, *gid);
            if gm.indices.is_empty() {
                continue;
            }
            let base = m.vertices.len() as u32;
            let (ox, oy) = (o.x as f32, o.y as f32);
            m.vertices.extend(
                gm.vertices
                    .iter()
                    .map(|v| Vertex::solid([ox + v[0] * s, oy - v[1] * s], color)),
            );
            m.indices.extend(gm.indices.iter().map(|i| i + base));
        }
        m
    }
}

pub fn push_quad(m: &mut Mesh, r: Rect, color: [f32; 4]) {
    let base = m.vertices.len() as u32;
    let (x0, y0, x1, y1) = (r.x as f32, r.y as f32, r.right() as f32, r.bottom() as f32);
    for pos in [[x0, y0], [x1, y0], [x1, y1], [x0, y1]] {
        m.vertices.push(Vertex::solid(pos, color));
    }
    m.indices
        .extend([base, base + 1, base + 2, base, base + 2, base + 3]);
}

/// Per-vertex paint attributes for a paint spec.
fn paint_fn(paint: &PaintSpec) -> (Option<String>, Box<dyn Fn(V2) -> Vertex>) {
    let pos = |p: V2| [p.x as f32, p.y as f32];
    match paint.clone() {
        PaintSpec::Solid(c) => (None, Box::new(move |p| Vertex::solid(pos(p), c))),
        PaintSpec::Linear {
            p0,
            p1,
            stops,
            opacity,
        } => {
            let row = gradient_row(&stops);
            let d = p1 - p0;
            let l2 = d.dot(d).max(1e-12);
            (
                None,
                Box::new(move |p| Vertex {
                    pos: pos(p),
                    color: [1.0, 1.0, 1.0, opacity],
                    uv: [((p - p0).dot(d) / l2) as f32, 0.0],
                    kind: PAINT_LINEAR | row << 8,
                    _pad: 0,
                }),
            )
        }
        PaintSpec::Radial {
            center,
            rx,
            ry,
            stops,
            opacity,
        } => {
            let row = gradient_row(&stops);
            // Solve p - center = u*rx + v*ry.
            let det = rx.x * ry.y - rx.y * ry.x;
            let det = if det.abs() < 1e-12 { 1e-12 } else { det };
            (
                None,
                Box::new(move |p| {
                    let q = p - center;
                    let u = (q.x * ry.y - q.y * ry.x) / det;
                    let v = (rx.x * q.y - rx.y * q.x) / det;
                    Vertex {
                        pos: pos(p),
                        color: [1.0, 1.0, 1.0, opacity],
                        uv: [u as f32, v as f32],
                        kind: PAINT_RADIAL | row << 8,
                        _pad: 0,
                    }
                }),
            )
        }
        PaintSpec::Image {
            asset,
            place,
            tile,
            opacity,
        } => (
            Some(asset),
            Box::new(move |p| Vertex {
                pos: pos(p),
                color: [1.0, 1.0, 1.0, opacity],
                uv: [
                    ((p.x - place.x) / place.w.max(1e-9)) as f32,
                    ((p.y - place.y) / place.h.max(1e-9)) as f32,
                ],
                kind: if tile { PAINT_IMAGE_TILE } else { PAINT_IMAGE },
                _pad: 0,
            }),
        ),
    }
}

#[allow(dead_code)]
const _: u32 = PAINT_SOLID;
