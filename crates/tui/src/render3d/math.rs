//! Minimal 3-vector / 3x3-matrix math. Hand-rolled: the renderer needs a
//! dozen operations, not a linear-algebra dependency.

use std::ops::{Add, Div, Mul, Neg, Sub};

#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Vec3 {
    pub x: f32,
    pub y: f32,
    pub z: f32,
}

pub const fn vec3(x: f32, y: f32, z: f32) -> Vec3 {
    Vec3 { x, y, z }
}

impl Vec3 {
    pub const ZERO: Vec3 = vec3(0.0, 0.0, 0.0);

    pub fn splat(v: f32) -> Vec3 {
        vec3(v, v, v)
    }

    pub fn dot(self, o: Vec3) -> f32 {
        self.x * o.x + self.y * o.y + self.z * o.z
    }

    pub fn length(self) -> f32 {
        self.dot(self).sqrt()
    }

    pub fn normalize(self) -> Vec3 {
        let len = self.length();
        if len == 0.0 { self } else { self / len }
    }

    /// Componentwise minimum.
    pub fn min(self, o: Vec3) -> Vec3 {
        vec3(self.x.min(o.x), self.y.min(o.y), self.z.min(o.z))
    }

    /// Componentwise maximum.
    pub fn max(self, o: Vec3) -> Vec3 {
        vec3(self.x.max(o.x), self.y.max(o.y), self.z.max(o.z))
    }

    pub fn max_element(self) -> f32 {
        self.x.max(self.y).max(self.z)
    }

    pub fn min_element(self) -> f32 {
        self.x.min(self.y).min(self.z)
    }

    /// Axis 0/1/2 -> x/y/z.
    pub fn axis(self, i: usize) -> f32 {
        match i {
            0 => self.x,
            1 => self.y,
            _ => self.z,
        }
    }
}

impl Add for Vec3 {
    type Output = Vec3;
    fn add(self, o: Vec3) -> Vec3 {
        vec3(self.x + o.x, self.y + o.y, self.z + o.z)
    }
}

impl Sub for Vec3 {
    type Output = Vec3;
    fn sub(self, o: Vec3) -> Vec3 {
        vec3(self.x - o.x, self.y - o.y, self.z - o.z)
    }
}

impl Neg for Vec3 {
    type Output = Vec3;
    fn neg(self) -> Vec3 {
        vec3(-self.x, -self.y, -self.z)
    }
}

/// Componentwise product (used for per-channel color scaling too).
impl Mul for Vec3 {
    type Output = Vec3;
    fn mul(self, o: Vec3) -> Vec3 {
        vec3(self.x * o.x, self.y * o.y, self.z * o.z)
    }
}

impl Mul<f32> for Vec3 {
    type Output = Vec3;
    fn mul(self, s: f32) -> Vec3 {
        vec3(self.x * s, self.y * s, self.z * s)
    }
}

/// Componentwise division. Deliberately allows division by zero: the slab
/// test relies on the resulting ±inf to handle axis-parallel rays.
impl Div for Vec3 {
    type Output = Vec3;
    fn div(self, o: Vec3) -> Vec3 {
        vec3(self.x / o.x, self.y / o.y, self.z / o.z)
    }
}

impl Div<f32> for Vec3 {
    type Output = Vec3;
    fn div(self, s: f32) -> Vec3 {
        vec3(self.x / s, self.y / s, self.z / s)
    }
}

/// Row-major 3x3 matrix.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Mat3 {
    pub rows: [Vec3; 3],
}

impl Mat3 {
    /// Right-handed rotation about +Y (yaw).
    pub fn rotation_y(a: f32) -> Mat3 {
        let (s, c) = a.sin_cos();
        Mat3 {
            rows: [vec3(c, 0.0, s), vec3(0.0, 1.0, 0.0), vec3(-s, 0.0, c)],
        }
    }

    /// Right-handed rotation about +X (pitch).
    pub fn rotation_x(a: f32) -> Mat3 {
        let (s, c) = a.sin_cos();
        Mat3 {
            rows: [vec3(1.0, 0.0, 0.0), vec3(0.0, c, -s), vec3(0.0, s, c)],
        }
    }

    /// Transpose — for a rotation matrix this is also its inverse, which is
    /// how world-space rays get pulled into the book's local space.
    pub fn transpose(self) -> Mat3 {
        let [a, b, c] = self.rows;
        Mat3 {
            rows: [vec3(a.x, b.x, c.x), vec3(a.y, b.y, c.y), vec3(a.z, b.z, c.z)],
        }
    }
}

impl Mul<Vec3> for Mat3 {
    type Output = Vec3;
    fn mul(self, v: Vec3) -> Vec3 {
        vec3(self.rows[0].dot(v), self.rows[1].dot(v), self.rows[2].dot(v))
    }
}

impl Mul<Mat3> for Mat3 {
    type Output = Mat3;
    fn mul(self, o: Mat3) -> Mat3 {
        let ot = o.transpose(); // columns of `o` as rows
        Mat3 {
            rows: [
                vec3(self.rows[0].dot(ot.rows[0]), self.rows[0].dot(ot.rows[1]), self.rows[0].dot(ot.rows[2])),
                vec3(self.rows[1].dot(ot.rows[0]), self.rows[1].dot(ot.rows[1]), self.rows[1].dot(ot.rows[2])),
                vec3(self.rows[2].dot(ot.rows[0]), self.rows[2].dot(ot.rows[1]), self.rows[2].dot(ot.rows[2])),
            ],
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn approx(a: Vec3, b: Vec3) -> bool {
        (a - b).length() < 1e-5
    }

    #[test]
    fn transpose_inverts_rotation() {
        let r = Mat3::rotation_y(0.7) * Mat3::rotation_x(-0.3);
        let id = r * r.transpose();
        let want = [vec3(1.0, 0.0, 0.0), vec3(0.0, 1.0, 0.0), vec3(0.0, 0.0, 1.0)];
        for (got, want) in id.rows.iter().zip(want.iter()) {
            assert!(approx(*got, *want), "{got:?} != {want:?}");
        }
    }

    #[test]
    fn yaw_quarter_turn_maps_z_to_x() {
        let r = Mat3::rotation_y(std::f32::consts::FRAC_PI_2);
        assert!(approx(r * vec3(0.0, 0.0, 1.0), vec3(1.0, 0.0, 0.0)));
    }

    #[test]
    fn pitch_quarter_turn_maps_z_to_y() {
        let r = Mat3::rotation_x(std::f32::consts::FRAC_PI_2);
        assert!(approx(r * vec3(0.0, 0.0, 1.0), vec3(0.0, -1.0, 0.0)));
    }

    #[test]
    fn normalize_leaves_zero_alone() {
        assert_eq!(Vec3::ZERO.normalize(), Vec3::ZERO);
    }
}
