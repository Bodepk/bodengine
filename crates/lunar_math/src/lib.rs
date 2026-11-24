//! Matemáticas POD del motor. Sin dependencias de GPU ni de física (ADR-003).
//!
//! Todos los tipos son `#[repr(C)]` para poder cruzar la frontera FFI por valor.

use std::ops::{Add, Mul, Sub};

/// Vector 2D en representación C.
#[repr(C)]
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct Vec2 {
    pub x: f32,
    pub y: f32,
}

impl Vec2 {
    /// Crea un vector 2D.
    #[must_use]
    pub const fn new(x: f32, y: f32) -> Self {
        Self { x, y }
    }

    /// Producto escalar.
    #[must_use]
    pub const fn dot(self, rhs: Self) -> f32 {
        self.x * rhs.x + self.y * rhs.y
    }

    /// Longitud al cuadrado, sin raíz cuadrada.
    #[must_use]
    pub const fn length_squared(self) -> f32 {
        self.dot(self)
    }
}

impl Add for Vec2 {
    type Output = Self;

    fn add(self, rhs: Self) -> Self {
        Self::new(self.x + rhs.x, self.y + rhs.y)
    }
}

impl Sub for Vec2 {
    type Output = Self;

    fn sub(self, rhs: Self) -> Self {
        Self::new(self.x - rhs.x, self.y - rhs.y)
    }
}

impl Mul<f32> for Vec2 {
    type Output = Self;

    fn mul(self, rhs: f32) -> Self {
        Self::new(self.x * rhs, self.y * rhs)
    }
}

/// Vector 3D en representación C.
#[repr(C)]
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct Vec3 {
    pub x: f32,
    pub y: f32,
    pub z: f32,
}

impl Vec3 {
    /// Crea un vector 3D.
    #[must_use]
    pub const fn new(x: f32, y: f32, z: f32) -> Self {
        Self { x, y, z }
    }

    /// Producto escalar.
    #[must_use]
    pub const fn dot(self, rhs: Self) -> f32 {
        self.x * rhs.x + self.y * rhs.y + self.z * rhs.z
    }

    /// Producto vectorial.
    #[must_use]
    pub const fn cross(self, rhs: Self) -> Self {
        Self::new(
            self.y * rhs.z - self.z * rhs.y,
            self.z * rhs.x - self.x * rhs.z,
            self.x * rhs.y - self.y * rhs.x,
        )
    }

    /// Longitud al cuadrado, sin raíz cuadrada.
    #[must_use]
    pub const fn length_squared(self) -> f32 {
        self.dot(self)
    }
}

impl Add for Vec3 {
    type Output = Self;

    fn add(self, rhs: Self) -> Self {
        Self::new(self.x + rhs.x, self.y + rhs.y, self.z + rhs.z)
    }
}

impl Sub for Vec3 {
    type Output = Self;

    fn sub(self, rhs: Self) -> Self {
        Self::new(self.x - rhs.x, self.y - rhs.y, self.z - rhs.z)
    }
}

impl Mul<f32> for Vec3 {
    type Output = Self;

    fn mul(self, rhs: f32) -> Self {
        Self::new(self.x * rhs, self.y * rhs, self.z * rhs)
    }
}

/// Vector 4D en representación C (también usado como columna de matriz).
#[repr(C)]
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct Vec4 {
    pub x: f32,
    pub y: f32,
    pub z: f32,
    pub w: f32,
}

impl Vec4 {
    /// Crea un vector 4D.
    #[must_use]
    pub const fn new(x: f32, y: f32, z: f32, w: f32) -> Self {
        Self { x, y, z, w }
    }
}

/// Cuaternión en representación C, en orden `(x, y, z, w)`.
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Quat {
    pub x: f32,
    pub y: f32,
    pub z: f32,
    pub w: f32,
}

impl Quat {
    /// Identidad.
    pub const IDENTITY: Self = Self::new(0.0, 0.0, 0.0, 1.0);

    /// Crea un cuaternión.
    #[must_use]
    pub const fn new(x: f32, y: f32, z: f32, w: f32) -> Self {
        Self { x, y, z, w }
    }
}

impl Default for Quat {
    fn default() -> Self {
        Self::IDENTITY
    }
}

/// Matriz 4x4 en representación C, almacenada por columnas.
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Mat4 {
    pub columns: [Vec4; 4],
}

impl Mat4 {
    /// Matriz identidad.
    pub const IDENTITY: Self = Self {
        columns: [
            Vec4::new(1.0, 0.0, 0.0, 0.0),
            Vec4::new(0.0, 1.0, 0.0, 0.0),
            Vec4::new(0.0, 0.0, 1.0, 0.0),
            Vec4::new(0.0, 0.0, 0.0, 1.0),
        ],
    };

    /// Crea una matriz a partir de sus cuatro columnas.
    #[must_use]
    pub const fn new(columns: [Vec4; 4]) -> Self {
        Self { columns }
    }

    /// Devuelve la identidad.
    #[must_use]
    pub const fn identity() -> Self {
        Self::IDENTITY
    }
}

impl Default for Mat4 {
    fn default() -> Self {
        Self::IDENTITY
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::mem::size_of;

    #[test]
    fn vec2_operations() {
        let a = Vec2::new(1.0, 2.0);
        let b = Vec2::new(3.0, -4.0);
        assert_eq!(a + b, Vec2::new(4.0, -2.0));
        assert_eq!(b - a, Vec2::new(2.0, -6.0));
        assert_eq!(a * 2.0, Vec2::new(2.0, 4.0));
        assert_eq!(a.dot(b), -5.0);
        assert_eq!(a.length_squared(), 5.0);
    }

    #[test]
    fn vec3_cross_product_is_orthogonal() {
        let x = Vec3::new(1.0, 0.0, 0.0);
        let y = Vec3::new(0.0, 1.0, 0.0);
        let z = x.cross(y);
        assert_eq!(z, Vec3::new(0.0, 0.0, 1.0));
        assert_eq!(z.dot(x), 0.0);
        assert_eq!(z.dot(y), 0.0);
    }

    #[test]
    fn mat4_default_is_identity() {
        let m = Mat4::default();
        assert_eq!(m, Mat4::identity());
        assert_eq!(m.columns[0], Vec4::new(1.0, 0.0, 0.0, 0.0));
        assert_eq!(m.columns[3], Vec4::new(0.0, 0.0, 0.0, 1.0));
        assert_eq!(Quat::default(), Quat::IDENTITY);
    }

    #[test]
    fn types_are_pod_sized() {
        assert_eq!(size_of::<Vec2>(), 8);
        assert_eq!(size_of::<Vec3>(), 12);
        assert_eq!(size_of::<Vec4>(), 16);
        assert_eq!(size_of::<Quat>(), 16);
        assert_eq!(size_of::<Mat4>(), 64);
    }
}
