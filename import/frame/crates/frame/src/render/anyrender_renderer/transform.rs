/// A 2D affine transform in Figma row-major format.
///
/// Figma stores transforms as `[[a, b, tx], [c, d, ty]]` where:
///   - Row 0: `[a, b, tx]` → `x' = a*x + b*y + tx`
///   - Row 1: `[c, d, ty]` → `y' = c*x + d*y + ty`
///
/// Note: SVG's `matrix(a,b,c,d,e,f)` uses column-major convention — to emit
/// an SVG matrix string use [`AffineTransform2D::to_svg_matrix`] or the
/// [`std::fmt::Display`] impl, which reorders fields to `(a, c, b, d, tx, ty)`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct AffineTransform2D {
    /// Row 0, col 0: x-scale / cos(θ)
    pub a: f64,
    /// Row 0, col 1: -sin(θ) for pure rotation
    pub b: f64,
    /// Row 0, col 2: x-translation
    pub tx: f64,
    /// Row 1, col 0: sin(θ) for pure rotation
    pub c: f64,
    /// Row 1, col 1: y-scale / cos(θ)
    pub d: f64,
    /// Row 1, col 2: y-translation
    pub ty: f64,
}

impl AffineTransform2D {
    /// The identity transform.
    pub fn identity() -> Self {
        Self {
            a: 1.0,
            b: 0.0,
            tx: 0.0,
            c: 0.0,
            d: 1.0,
            ty: 0.0,
        }
    }

    /// A pure rotation by `degrees` (counter-clockwise).
    pub fn rotation(degrees: f64) -> Self {
        let r = degrees.to_radians();
        let (sin_r, cos_r) = r.sin_cos();
        Self {
            a: cos_r,
            b: -sin_r,
            tx: 0.0,
            c: sin_r,
            d: cos_r,
            ty: 0.0,
        }
    }

    /// A pure translation.
    pub fn translation(tx: f64, ty: f64) -> Self {
        Self {
            a: 1.0,
            b: 0.0,
            tx,
            c: 0.0,
            d: 1.0,
            ty,
        }
    }

    /// A non-uniform scale.
    pub fn scale(sx: f64, sy: f64) -> Self {
        Self {
            a: sx,
            b: 0.0,
            tx: 0.0,
            c: 0.0,
            d: sy,
            ty: 0.0,
        }
    }

    /// Apply this transform to point `(u, v)` and return the transformed point.
    ///
    /// `x' = a*u + b*v + tx`, `y' = c*u + d*v + ty`
    pub fn transform_point(&self, u: f64, v: f64) -> (f64, f64) {
        (
            self.a * u + self.b * v + self.tx,
            self.c * u + self.d * v + self.ty,
        )
    }

    /// Determinant of the 2×2 linear part: `a*d - b*c`.
    pub fn determinant(&self) -> f64 {
        self.a * self.d - self.b * self.c
    }

    /// Returns `true` if this is (approximately) the identity transform.
    pub fn is_identity(&self) -> bool {
        (self.a - 1.0).abs() <= f64::EPSILON
            && self.b.abs() <= f64::EPSILON
            && self.tx.abs() <= f64::EPSILON
            && self.c.abs() <= f64::EPSILON
            && (self.d - 1.0).abs() <= f64::EPSILON
            && self.ty.abs() <= f64::EPSILON
    }

    /// Extract the rotation angle in radians from the first column of the
    /// linear part: `atan2(c, a)`.
    ///
    /// For a pure rotation matrix `[[cos θ, -sin θ], [sin θ, cos θ]]` this
    /// recovers θ exactly. In the presence of scale the result is the rotation
    /// of the x-axis.
    pub fn decompose_rotation(&self) -> f64 {
        self.c.atan2(self.a)
    }

    /// Return the inverse transform, or `None` if the matrix is singular
    /// (determinant ≈ 0).
    pub fn inverse(&self) -> Option<Self> {
        let det = self.determinant();
        if det.abs() < 1e-12 {
            return None;
        }
        Some(Self {
            a: self.d / det,
            b: -self.b / det,
            tx: (self.b * self.ty - self.d * self.tx) / det,
            c: -self.c / det,
            d: self.a / det,
            ty: (self.c * self.tx - self.a * self.ty) / det,
        })
    }

    /// Return the SVG `matrix()` coefficients in column-major order:
    /// `(a, c, b, d, tx, ty)`.
    ///
    /// SVG `matrix(a,b,c,d,e,f)` encodes `x' = a*x + c*y + e` and
    /// `y' = b*x + d*y + f`, so the column-major reordering is required.
    pub fn to_svg_matrix_coeffs(&self) -> (f64, f64, f64, f64, f64, f64) {
        (self.a, self.c, self.b, self.d, self.tx, self.ty)
    }
}

/// Compose two transforms: `self` applied *after* `rhs`.
///
/// `(self * rhs)(v) = self(rhs(v))`
impl std::ops::Mul for AffineTransform2D {
    type Output = Self;

    fn mul(self, rhs: Self) -> Self {
        Self {
            a: self.a * rhs.a + self.b * rhs.c,
            b: self.a * rhs.b + self.b * rhs.d,
            tx: self.a * rhs.tx + self.b * rhs.ty + self.tx,
            c: self.c * rhs.a + self.d * rhs.c,
            d: self.c * rhs.b + self.d * rhs.d,
            ty: self.c * rhs.tx + self.d * rhs.ty + self.ty,
        }
    }
}

/// Format as an SVG `matrix(a,b,c,d,e,f)` string using column-major order.
impl std::fmt::Display for AffineTransform2D {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let (a, b, c, d, e, ff) = self.to_svg_matrix_coeffs();
        write!(f, "matrix({a} {b} {c} {d} {e} {ff})")
    }
}

/// Parse from Figma's row-major `[[a, b, tx], [c, d, ty]]` representation.
impl From<[[f64; 3]; 2]> for AffineTransform2D {
    fn from(t: [[f64; 3]; 2]) -> Self {
        Self {
            a: t[0][0],
            b: t[0][1],
            tx: t[0][2],
            c: t[1][0],
            d: t[1][1],
            ty: t[1][2],
        }
    }
}

impl From<AffineTransform2D> for [[f64; 3]; 2] {
    fn from(t: AffineTransform2D) -> Self {
        [[t.a, t.b, t.tx], [t.c, t.d, t.ty]]
    }
}
