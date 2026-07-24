//! The book as a single ray-traced cuboid.
//!
//! One box means no rasterizer, no z-buffer and no clipping: for every
//! subpixel we build a camera ray, pull it into the book's local space with
//! the transposed rotation, and run a slab intersection. The axis that wins
//! the slab test names the face, and the two remaining local coordinates are
//! the UV — exact perspective, for free.

use super::math::{Mat3, Vec3, vec3};
use super::texture::Cover;

/// Every book is 1.5 units tall; width and thickness come from the edition
/// (see [`super::Model`]), which is what keeps covers from being stretched.
pub const HALF_HEIGHT: f32 = 0.75;

/// Reference proportions for the tests: a 6x9in trade paperback.
#[cfg(test)]
pub const DEFAULT_HALF_EXTENTS: Vec3 = vec3(0.50, HALF_HEIGHT, 0.09);

/// Vertical field of view, radians.
const FOV: f32 = 0.45;
/// How much of the frame the book fills, by the height of the region in cells.
///
/// A small pane has no room to spare, but on a full-size terminal a book scaled
/// to the frame is simply too big — the object is the centrepiece, not the
/// whole pane. So the fraction slides down as the region grows, which caps the
/// object's absolute size instead of letting it track the window.
pub fn fill_for(rows: u16) -> f32 {
    const SMALL: f32 = 18.0;
    const LARGE: f32 = 44.0;
    let t = ((rows as f32 - SMALL) / (LARGE - SMALL)).clamp(0.0, 1.0);
    // Scaled 20% down from the old 0.88→0.60: on a full terminal the book was
    // still reading as too large for a centrepiece object.
    0.70 + t * (0.48 - 0.70)
}

/// Two-point rig plus ambient. Without the fill, faces turned away from the
/// key go black — which is what the back cover does every time the book spins
/// past halfway.
const AMBIENT: f32 = 0.42;
const KEY: f32 = 0.45;
const FILL_LIGHT: f32 = 0.22;
const CREAM: Vec3 = vec3(0.949, 0.925, 0.867);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Face {
    Front,
    Back,
    Spine,
    ForeEdge,
    Top,
    Bottom,
}

impl Face {
    pub fn normal(self) -> Vec3 {
        match self {
            Face::Front => vec3(0.0, 0.0, 1.0),
            Face::Back => vec3(0.0, 0.0, -1.0),
            Face::Spine => vec3(-1.0, 0.0, 0.0),
            Face::ForeEdge => vec3(1.0, 0.0, 0.0),
            Face::Top => vec3(0.0, 1.0, 0.0),
            Face::Bottom => vec3(0.0, -1.0, 0.0),
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub struct Hit {
    pub face: Face,
    /// Local-space hit point.
    pub point: Vec3,
    pub u: f32,
    pub v: f32,
}

/// Orientation of the book, radians.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Pose {
    pub yaw: f32,
    pub pitch: f32,
}

impl Default for Pose {
    /// A three-quarter view with the book tipped back so its cover faces
    /// upward — spine and bottom page edges visible, cover still square enough
    /// to the camera to read.
    ///
    /// Positive yaw turns the spine toward the camera. Pitch sign decides which
    /// way the cover looks: positive tips the top edge forward so the cover
    /// faces *down*, negative tips it back so the cover faces *up*. Past about
    /// -1.2 the book goes flat and the cover foreshortens into nothing.
    fn default() -> Self {
        Pose {
            yaw: 0.44,
            pitch: -0.40,
        }
    }
}

impl Pose {
    pub fn rotation(self) -> Mat3 {
        Mat3::rotation_y(self.yaw) * Mat3::rotation_x(self.pitch)
    }
}

/// Half-angle tangents for the two screen axes. The field of view applies to
/// the *shorter* axis, so a tall narrow pane never clips the book sideways.
fn half_angles(aspect: f32) -> (f32, f32) {
    let t = (FOV * 0.5).tan();
    (t * aspect.max(1.0), t * (1.0 / aspect).max(1.0))
}

/// The eight corners of the box.
fn corners(h: Vec3) -> impl Iterator<Item = Vec3> {
    [-1.0f32, 1.0].into_iter().flat_map(move |sx| {
        [-1.0f32, 1.0].into_iter().flat_map(move |sy| {
            [-1.0f32, 1.0]
                .into_iter()
                .map(move |sz| vec3(h.x * sx, h.y * sy, h.z * sz))
        })
    })
}

/// Distance that fits the book to the frame.
///
/// Each corner imposes `dist >= q.z + |q.x| / (tx * fill)` (and the same for
/// y) — an exact perspective fit, not a bounding-sphere guess. The fit runs
/// over a full yaw sweep rather than the current angle, so a spinning book
/// holds a constant scale instead of breathing in and out.
pub fn camera_distance(aspect: f32, pitch: f32, h: Vec3, fill: f32) -> f32 {
    const STEPS: u32 = 32;
    let (tx, ty) = half_angles(aspect);
    let mut dist: f32 = 0.0;
    for step in 0..STEPS {
        let yaw = step as f32 * std::f32::consts::TAU / STEPS as f32;
        let rot = Mat3::rotation_y(yaw) * Mat3::rotation_x(pitch);
        for corner in corners(h) {
            let q = rot * corner;
            dist = dist
                .max(q.z + q.x.abs() / (tx * fill))
                .max(q.z + q.y.abs() / (ty * fill));
        }
    }
    dist
}

/// Ray direction through the point `(u, v)` of the image, both in 0..1 with v
/// running downward. `aspect` is the image's *physical* width/height — not the
/// sample grid's, since terminal subpixels aren't square.
pub fn primary_ray(u: f32, v: f32, aspect: f32) -> Vec3 {
    let (tx, ty) = half_angles(aspect);
    let x = (2.0 * u - 1.0) * tx;
    let y = (1.0 - 2.0 * v) * ty;
    vec3(x, y, -1.0).normalize()
}

/// Slab intersection against the axis-aligned box `±half` in local space.
pub fn intersect_box(ro: Vec3, rd: Vec3, half: Vec3) -> Option<Hit> {
    // Division by a zero component yields ±inf, which the min/max below
    // handle correctly for axis-parallel rays.
    let inv = Vec3::splat(1.0) / rd;
    let t1 = (-half - ro) * inv;
    let t2 = (half - ro) * inv;
    let lo = t1.min(t2);
    let hi = t1.max(t2);
    let tmin = lo.max_element();
    let tmax = hi.min_element();
    if tmin.is_nan() || tmax.is_nan() || tmin > tmax || tmax <= 0.0 {
        return None;
    }
    // Entering the box; if the camera were inside we would want tmax instead,
    // but the camera is always outside.
    let t = tmin;
    let axis = if lo.x >= lo.y && lo.x >= lo.z {
        0
    } else if lo.y >= lo.z {
        1
    } else {
        2
    };
    let point = ro + rd * t;
    let positive = rd.axis(axis) < 0.0;
    let face = match (axis, positive) {
        (0, true) => Face::ForeEdge,
        (0, false) => Face::Spine,
        (1, true) => Face::Top,
        (1, false) => Face::Bottom,
        (_, true) => Face::Front,
        (_, false) => Face::Back,
    };
    let (u, v) = face_uv(face, point, half);
    Some(Hit { face, point, u, v })
}

/// Map a local hit point to the face's UV, oriented so the cover reads the
/// right way round: u left→right with the spine at u=0, v top→bottom.
fn face_uv(face: Face, p: Vec3, half: Vec3) -> (f32, f32) {
    let nx = (p.x + half.x) / (2.0 * half.x);
    let ny = (half.y - p.y) / (2.0 * half.y);
    let nz = (p.z + half.z) / (2.0 * half.z);
    match face {
        Face::Front => (nx, ny),
        Face::Back => (1.0 - nx, ny),
        Face::Spine => (1.0 - nz, ny),
        Face::ForeEdge => (nz, ny),
        Face::Top => (nx, nz),
        Face::Bottom => (nx, 1.0 - nz),
    }
}

/// Unlit color of a face at a hit point.
fn albedo(hit: &Hit, half: Vec3, cover: &Cover) -> Vec3 {
    match hit.face {
        Face::Front => cover.texture.sample(hit.u, hit.v),
        Face::Back => back_board(hit, cover),
        Face::Spine => cover.accent,
        Face::ForeEdge | Face::Top | Face::Bottom => page_edge(hit, half),
    }
}

/// The back board: the accent color, darkened, with an inset panel so it reads
/// as the back of a jacket rather than a blank slab when the book turns round.
fn back_board(hit: &Hit, cover: &Cover) -> Vec3 {
    let inset = (0.10..0.90).contains(&hit.u) && (0.08..0.72).contains(&hit.v);
    // The additive floor keeps a black jacket from turning into a hole in the
    // pane when the spin brings the back round.
    let base = cover.accent * 0.80 + Vec3::splat(0.07);
    base * if inset { 0.84 } else { 1.0 }
}

/// Page edges: cream with fine ruling. Pages stack along local Z, so the
/// stripes are a function of the hit point's Z and stay parallel to the spine
/// on the top and bottom faces.
fn page_edge(hit: &Hit, half: Vec3) -> Vec3 {
    let stripe = (hit.point.z * 190.0).sin() * 0.5 + 0.5;
    let shade = 1.0 - 0.13 * stripe;
    // Slight dip toward the covers so the block reads as a stack, not a slab.
    let edge = (hit.point.z.abs() / half.z).powi(6);
    CREAM * shade * (1.0 - 0.18 * edge)
}

/// Shade one ray. Returns None when the ray misses the book.
pub fn shade(ro: Vec3, rd: Vec3, rot: Mat3, half: Vec3, cover: &Cover) -> Option<Vec3> {
    let inv = rot.transpose();
    let hit = intersect_box(inv * ro, inv * rd, half)?;
    let base = albedo(&hit, half, cover);
    let normal = rot * hit.face.normal();
    // Key up and to the left but well forward: a cover shaded into mud is a
    // cover you can't read at 40 columns. Fill comes from behind and below.
    let key = vec3(-0.38, 0.58, 1.0).normalize();
    let fill = vec3(0.55, -0.30, -0.85).normalize();
    let level = AMBIENT
        + KEY * normal.dot(key).max(0.0)
        + FILL_LIGHT * normal.dot(fill).max(0.0);
    let lit = base * level;
    // A touch of rim light so the silhouette separates from a dark pane.
    let rim = (1.0 - normal.dot(-rd).abs()).powi(3) * 0.09;
    Some(lit + Vec3::splat(rim))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn axial_ray_hits_the_front_cover_dead_center() {
        let ro = vec3(0.0, 0.0, 3.0);
        let rd = vec3(0.0, 0.0, -1.0);
        let hit = intersect_box(ro, rd, DEFAULT_HALF_EXTENTS).expect("should hit");
        assert_eq!(hit.face, Face::Front);
        let dist = (hit.point - ro).length();
        assert!((dist - (3.0 - DEFAULT_HALF_EXTENTS.z)).abs() < 1e-4, "distance = {dist}");
        assert!((hit.u - 0.5).abs() < 1e-4 && (hit.v - 0.5).abs() < 1e-4);
    }

    #[test]
    fn ray_past_the_corner_misses() {
        let ro = vec3(2.0, 2.0, 3.0);
        let rd = vec3(0.0, 0.0, -1.0);
        assert!(intersect_box(ro, rd, DEFAULT_HALF_EXTENTS).is_none());
    }

    #[test]
    fn ray_behind_the_camera_misses() {
        let ro = vec3(0.0, 0.0, 3.0);
        let rd = vec3(0.0, 0.0, 1.0);
        assert!(intersect_box(ro, rd, DEFAULT_HALF_EXTENTS).is_none());
    }

    #[test]
    fn ray_from_the_left_hits_the_spine() {
        let hit = intersect_box(vec3(-3.0, 0.0, 0.0), vec3(1.0, 0.0, 0.0), DEFAULT_HALF_EXTENTS)
            .expect("should hit");
        assert_eq!(hit.face, Face::Spine);
        let hit = intersect_box(vec3(0.0, 3.0, 0.0), vec3(0.0, -1.0, 0.0), DEFAULT_HALF_EXTENTS)
            .expect("should hit");
        assert_eq!(hit.face, Face::Top);
    }

    /// Every face the book presents to the camera across a full frame.
    fn visible_faces(pose: Pose) -> std::collections::HashSet<Face> {
        let rot = pose.rotation();
        let inv = rot.transpose();
        let ro = vec3(0.0, 0.0, camera_distance(100.0 / 60.0, pose.pitch, DEFAULT_HALF_EXTENTS, 0.8));
        let mut faces = std::collections::HashSet::new();
        for py in 0..60 {
            for px in 0..100 {
                let rd = primary_ray((px as f32 + 0.5) / 100.0, (py as f32 + 0.5) / 60.0, 100.0 / 60.0);
                if let Some(h) = intersect_box(inv * ro, inv * rd, DEFAULT_HALF_EXTENTS) {
                    faces.insert(h.face);
                }
            }
        }
        faces
    }

    #[test]
    fn the_fill_fraction_shrinks_as_the_pane_grows() {
        assert!(fill_for(10) > fill_for(30));
        assert!(fill_for(30) > fill_for(60));
        // Clamped at both ends, so it never inverts or runs away.
        assert_eq!(fill_for(4), fill_for(18));
        assert_eq!(fill_for(60), fill_for(200));
    }

    #[test]
    fn yaw_sign_decides_spine_or_fore_edge() {
        let left = visible_faces(Pose { yaw: 0.9, pitch: 0.0 });
        assert!(left.contains(&Face::Spine) && !left.contains(&Face::ForeEdge), "{left:?}");
        let right = visible_faces(Pose { yaw: -0.9, pitch: 0.0 });
        assert!(right.contains(&Face::ForeEdge) && !right.contains(&Face::Spine), "{right:?}");
    }

    #[test]
    fn uv_orientation_puts_the_spine_at_u_zero_and_the_top_at_v_zero() {
        let top_left = vec3(-DEFAULT_HALF_EXTENTS.x, DEFAULT_HALF_EXTENTS.y, DEFAULT_HALF_EXTENTS.z);
        let (u, v) = face_uv(Face::Front, top_left, DEFAULT_HALF_EXTENTS);
        assert!(u.abs() < 1e-6 && v.abs() < 1e-6, "({u}, {v})");
    }

    #[test]
    fn the_default_pose_faces_the_cover_upward() {
        let faces = visible_faces(Pose::default());
        assert!(faces.contains(&Face::Front), "{faces:?}");
        assert!(faces.contains(&Face::Spine), "{faces:?}");
        // Tipped back, so the bottom edge shows and the top does not.
        assert!(faces.contains(&Face::Bottom), "{faces:?}");
        assert!(!faces.contains(&Face::Top), "{faces:?}");
    }

    #[test]
    fn pitch_sign_decides_which_way_the_cover_looks() {
        let up = visible_faces(Pose { yaw: 0.0, pitch: -0.6 });
        assert!(up.contains(&Face::Bottom) && !up.contains(&Face::Top), "{up:?}");
        let down = visible_faces(Pose { yaw: 0.0, pitch: 0.6 });
        assert!(down.contains(&Face::Top) && !down.contains(&Face::Bottom), "{down:?}");
    }

    #[test]
    fn shading_hits_the_book_and_misses_the_corner() {
        let cover = super::super::texture::procedural_cover("test");
        let pose = Pose::default();
        let rot = pose.rotation();
        let h = DEFAULT_HALF_EXTENTS;
        let ro = vec3(0.0, 0.0, camera_distance(100.0 / 60.0, pose.pitch, h, 0.8));
        let ray = |u, v| primary_ray(u, v, 100.0 / 60.0);
        assert!(shade(ro, ray(0.5, 0.5), rot, h, &cover).is_some());
        assert!(shade(ro, ray(0.005, 0.005), rot, h, &cover).is_none());
    }
}
