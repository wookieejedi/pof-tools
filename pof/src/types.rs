use std::cell::Cell;
use std::collections::BTreeSet;
use std::convert::TryFrom;
use std::fmt::{Debug, Display};
use std::io::{self, Write};
use std::ops::{Add, AddAssign, Deref, DerefMut, Div, DivAssign, Index, IndexMut, Mul, MulAssign, Neg, Sub, SubAssign};
use std::path::PathBuf;
use std::str::FromStr;

use itertools::Itertools;

use byteorder::{WriteBytesExt, LE};
pub use dae_parser::UpAxis;
use glm::{TMat3, TMat4, Vec3};
use nalgebra::Matrix3;
use nalgebra_glm::Mat4;
extern crate nalgebra_glm as glm;

use crate::write::Serialize;

macro_rules! id_type {
    ($name:ident, $type:ty) => {
        #[derive(Clone, Copy, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
        pub struct $name(pub $type);
        impl Debug for $name {
            fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                f.write_fmt(format_args!("{}", &self.0))
            }
        }
        impl Serialize for $name {
            fn write_to(&self, w: &mut impl Write) -> io::Result<()> {
                self.0.write_to(w)
            }
        }
    };
}

id_type! {ObjectId, u32}
id_type! {TextureId, u32}
id_type! {VertexId, u32}
id_type! {NormalId, u32}
id_type! {PolygonId, u32}
id_type! {PathId, u32}

impl TextureId {
    /// Used temporarily during parsing to denote untextured polygons
    /// before the "untextured" texture has been added
    pub(crate) const UNTEXTURED: Self = Self(u32::MAX);
}

// what, a global?? in rust?????
// this is how the current version is kept track of while writing pof to disk
// much easier than having to pass around a version to every Serialize implementation despite it mattering in like 1% of cases
thread_local! {
    pub(crate) static VERSION: Cell<Version> = Cell::new(Version::default());
}
pub(crate) fn get_version() -> Version {
    VERSION.with(|f| f.get())
}

// like a regular vector, but indexed with ObjectIds only, for some safety
#[derive(Debug)]
pub struct ObjVec<T>(pub Vec<T>);
impl<T> Index<ObjectId> for ObjVec<T> {
    type Output = T;

    fn index(&self, index: ObjectId) -> &Self::Output {
        &self.0[index.0 as usize]
    }
}
impl<T> IndexMut<ObjectId> for ObjVec<T> {
    fn index_mut(&mut self, index: ObjectId) -> &mut Self::Output {
        &mut self.0[index.0 as usize]
    }
}
impl<'a, T> IntoIterator for &'a ObjVec<T> {
    type Item = &'a T;

    type IntoIter = std::slice::Iter<'a, T>;

    fn into_iter(self) -> Self::IntoIter {
        self.iter()
    }
}
impl<T> Default for ObjVec<T> {
    fn default() -> Self {
        Self(Default::default())
    }
}
impl<T> ObjVec<T> {
    fn iter(&self) -> std::slice::Iter<'_, T> {
        self.0.iter()
    }
}
impl<T> Deref for ObjVec<T> {
    type Target = Vec<T>;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}
impl<T> DerefMut for ObjVec<T> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}

macro_rules! mk_struct {
    ($($(#[$meta:meta])* pub struct $tyname:ident { $(pub $name:ident: $ty:ty,)* })*) => {
        $(
            $(#[$meta])*
            pub struct $tyname {
                $(pub $name: $ty,)*
            }

            impl Serialize for $tyname {
                fn write_to(&self, w: &mut impl Write) -> io::Result<()> {
                    $(self.$name.write_to(w)?;)*
                    Ok(())
                }
            }
        )*
    };
}

#[derive(Clone, Copy)]
pub enum Axis {
    X,
    Y,
    Z,
}

const ALL_AXES: [Axis; 3] = [Axis::X, Axis::Y, Axis::Z];

mk_struct! {
    #[derive(Clone, Copy, Default)]
    pub struct Vec3d {
        pub x: f32,
        pub y: f32,
        pub z: f32,
    }
}
impl Eq for Vec3d {}
impl PartialEq for Vec3d {
    // NaN == NaN, fuck you
    fn eq(&self, other: &Self) -> bool {
        self.x.to_bits() == other.x.to_bits() && self.y.to_bits() == other.y.to_bits() && self.z.to_bits() == other.z.to_bits()
    }
}
impl std::hash::Hash for Vec3d {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.x.to_bits().hash(state);
        self.y.to_bits().hash(state);
        self.z.to_bits().hash(state);
    }
}
impl Debug for Vec3d {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "({}, {}, {})", &self.x, &self.y, &self.z)
    }
}
impl From<[f32; 3]> for Vec3d {
    fn from([x, y, z]: [f32; 3]) -> Self {
        Vec3d { x, y, z }
    }
}
impl From<Vec3d> for [f32; 3] {
    fn from(Vec3d { x, y, z }: Vec3d) -> Self {
        [x, y, z]
    }
}
impl From<(f32, f32, f32)> for Vec3d {
    fn from((x, y, z): (f32, f32, f32)) -> Self {
        Vec3d { x, y, z }
    }
}
impl From<Vec3d> for Vec3 {
    fn from(Vec3d { x, y, z }: Vec3d) -> Self {
        glm::vec3(x, y, z)
    }
}
impl From<Vec3> for Vec3d {
    fn from(vec: Vec3) -> Self {
        <[f32; 3]>::from(vec).into()
    }
}
impl From<Vec3d> for nalgebra::Point3<f32> {
    fn from(Vec3d { x, y, z }: Vec3d) -> Self {
        [x, y, z].into()
    }
}
impl From<nalgebra::Point3<f32>> for Vec3d {
    fn from(vec: nalgebra::Point3<f32>) -> Self {
        <[f32; 3]>::from(vec).into()
    }
}
impl FromStr for Vec3d {
    type Err = ();

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let mut iter = s.split(',');

        let vec = Vec3d {
            x: iter.next().ok_or(())?.trim().parse().map_err(|_| ())?,
            y: iter.next().ok_or(())?.trim().parse().map_err(|_| ())?,
            z: iter.next().ok_or(())?.trim().parse().map_err(|_| ())?,
        };

        if iter.next().is_some() {
            Err(())
        } else {
            Ok(vec)
        }
    }
}
impl Display for Vec3d {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}, {}, {}", &self.x, &self.y, &self.z)
    }
}
impl Vec3d {
    pub const ZERO: Vec3d = Vec3d { x: 0.0, y: 0.0, z: 0.0 };
    pub const INFINITY: Vec3d = Vec3d { x: f32::INFINITY, y: f32::INFINITY, z: f32::INFINITY };
    pub const NEG_INFINITY: Vec3d = Vec3d {
        x: f32::NEG_INFINITY,
        y: f32::NEG_INFINITY,
        z: f32::NEG_INFINITY,
    };
    pub const fn new(x: f32, y: f32, z: f32) -> Self {
        Vec3d { x, y, z }
    }
    pub fn to_tuple(self) -> (f32, f32, f32) {
        (self.x, self.y, self.z)
    }
    pub fn magnitude(self) -> f32 {
        f32::sqrt(self.magnitude_squared())
    }
    pub fn magnitude_squared(self) -> f32 {
        self.x * self.x + self.y * self.y + self.z * self.z
    }
    pub fn normalize(self) -> Vec3d {
        let mag = self.magnitude();
        self * (1.0 / mag)
    }
    pub fn dot(self, other: &Vec3d) -> f32 {
        (other.x * self.x) + (other.y * self.y) + (other.z * self.z)
    }
    pub fn cross(self, other: &Vec3d) -> Vec3d {
        Vec3d::new(self.y * other.z - self.z * other.y, self.z * other.x - self.x * other.z, self.x * other.y - self.y * other.x)
    }
    pub fn is_null(self) -> bool {
        self.x.abs() <= 0.000001 && self.y.abs() <= 0.000001 && self.z.abs() <= 0.000001
    }
    pub fn average(iter: impl Iterator<Item = Self>) -> Vec3d {
        let mut out = Vec3d::ZERO;
        let mut n = 0;

        for vec in iter {
            out += vec;
            n += 1;
        }

        out /= n as f32;
        out
    }

    /// Swizzle coordinates from POF (Right: `+x`, Up: `+y`, In: `-z`) to specified DAE convention
    pub fn from_coord(&self, up: UpAxis) -> Vec3d {
        match up {
            UpAxis::XUp => Vec3d { x: -self.z, y: self.x, z: self.y },
            UpAxis::YUp => Vec3d { x: self.x, y: self.y, z: -self.z },
            UpAxis::ZUp => Vec3d { x: self.x, y: self.z, z: self.y },
        }
    }

    /// Swizzle coordinates from specified DAE convention to POF (Right: `+x`, Up: `+y`, In: `-z`)
    pub fn to_coord(&self, up: UpAxis) -> Vec3d {
        match up {
            UpAxis::XUp => Vec3d { x: self.y, y: self.z, z: -self.x },
            UpAxis::YUp => Vec3d { x: self.x, y: self.y, z: -self.z },
            UpAxis::ZUp => Vec3d { x: self.x, y: self.z, z: self.y },
        }
    }

    /// Get a rotation matrix that will rotate an upwards-facing model to face the direction this vector is pointing.
    pub fn to_rotation_matrix(&self) -> nalgebra_glm::Mat4x4 {
        // https://gamedev.stackexchange.com/a/119017
        // find the planar angle
        let v = self.normalize();
        let planar_angle = v.x.atan2(v.z);
        // Rotation matrix around "ground" plane
        let planar_rot = glm::rotation(planar_angle, &glm::vec3(0., 1., 0.));
        // Find upwards angle
        let up_angle = v.y.acos();
        let up_rot = glm::rotation(up_angle, &glm::vec3(1., 0., 0.));
        planar_rot * up_rot
    }
}
impl Add for Vec3d {
    type Output = Vec3d;

    fn add(self, rhs: Self) -> Self::Output {
        Vec3d { x: self.x + rhs.x, y: self.y + rhs.y, z: self.z + rhs.z }
    }
}
impl AddAssign for Vec3d {
    fn add_assign(&mut self, rhs: Vec3d) {
        self.x += rhs.x;
        self.y += rhs.y;
        self.z += rhs.z;
    }
}
impl Sub for Vec3d {
    type Output = Vec3d;

    fn sub(self, rhs: Self) -> Self::Output {
        Vec3d { x: self.x - rhs.x, y: self.y - rhs.y, z: self.z - rhs.z }
    }
}
impl SubAssign for Vec3d {
    fn sub_assign(&mut self, rhs: Vec3d) {
        self.x -= rhs.x;
        self.y -= rhs.y;
        self.z -= rhs.z;
    }
}
impl MulAssign<f32> for Vec3d {
    fn mul_assign(&mut self, rhs: f32) {
        self.x *= rhs;
        self.y *= rhs;
        self.z *= rhs;
    }
}
impl Mul<f32> for Vec3d {
    type Output = Vec3d;

    fn mul(self, rhs: f32) -> Vec3d {
        Vec3d { x: self.x * rhs, y: self.y * rhs, z: self.z * rhs }
    }
}
impl Mul<Vec3d> for &Mat4 {
    type Output = Vec3d;

    fn mul(self, rhs: Vec3d) -> Self::Output {
        self.transform_point(&rhs.into()).into()
    }
}
impl DivAssign<f32> for Vec3d {
    fn div_assign(&mut self, rhs: f32) {
        self.x /= rhs;
        self.y /= rhs;
        self.z /= rhs;
    }
}
impl Div<f32> for Vec3d {
    type Output = Vec3d;

    fn div(self, rhs: f32) -> Vec3d {
        Vec3d { x: self.x / rhs, y: self.y / rhs, z: self.z / rhs }
    }
}
impl Neg for Vec3d {
    type Output = Vec3d;

    fn neg(self) -> Self::Output {
        Vec3d { x: -self.x, y: -self.y, z: -self.z }
    }
}
impl Index<Axis> for Vec3d {
    type Output = f32;

    fn index(&self, index: Axis) -> &Self::Output {
        match index {
            Axis::X => &self.x,
            Axis::Y => &self.y,
            Axis::Z => &self.z,
        }
    }
}
impl IndexMut<Axis> for Vec3d {
    fn index_mut(&mut self, index: Axis) -> &mut Self::Output {
        match index {
            Axis::X => &mut self.x,
            Axis::Y => &mut self.y,
            Axis::Z => &mut self.z,
        }
    }
}

mk_struct! {
    #[derive(Debug, Default, Copy, Clone)]
    pub struct Mat3d {
        pub rvec: Vec3d,
        pub uvec: Vec3d,
        pub fvec: Vec3d,
    }
}
impl From<Mat3d> for glm::Mat3x3 {
    fn from(Mat3d { rvec, uvec, fvec }: Mat3d) -> Self {
        glm::Mat3x3::from_columns(&[rvec.into(), uvec.into(), fvec.into()])
    }
}
impl From<glm::Mat3x3> for Mat3d {
    fn from(mat: glm::Mat3x3) -> Self {
        Mat3d {
            rvec: Vec3::from(mat.column(0)).into(),
            uvec: Vec3::from(mat.column(1)).into(),
            fvec: Vec3::from(mat.column(2)).into(),
        }
    }
}
impl MulAssign<f32> for Mat3d {
    fn mul_assign(&mut self, rhs: f32) {
        self.rvec *= rhs;
        self.uvec *= rhs;
        self.fvec *= rhs;
    }
}

pub fn mat4_rotation_and_scaling_only(matrix: &TMat4<f32>) -> TMat4<f32> {
    let zero = Vec3d::ZERO.into();
    let translation = matrix.transform_point(&zero) - zero;
    matrix.append_translation(&(-translation))
}

pub fn mat4_rotation_only(matrix: &TMat4<f32>) -> TMat4<f32> {
    let matrix = mat4_rotation_and_scaling_only(matrix);
    let x = matrix.transform_vector(&Vec3::x());
    let y = matrix.transform_vector(&Vec3::y());
    let z = matrix.transform_vector(&Vec3::z());
    let mut arr = [x, y, z];
    Vec3::orthonormalize(&mut arr);
    TMat3::from_columns(&arr).to_homogeneous()
}

#[derive(Debug, Clone, Copy)]
pub struct NormalVec3(pub Vec3d);

impl Default for NormalVec3 {
    fn default() -> Self {
        Self(Vec3d { x: 0.0, y: 0.0, z: 1.0 })
    }
}
impl TryFrom<Vec3d> for NormalVec3 {
    type Error = ();

    fn try_from(value: Vec3d) -> Result<Self, Self::Error> {
        Ok(Self(Vec3::from(value).try_normalize(1e-6).ok_or(())?.into()))
    }
}
impl TryFrom<Vec3> for NormalVec3 {
    type Error = ();

    fn try_from(value: Vec3) -> Result<Self, Self::Error> {
        Vec3d::from(value).try_into()
    }
}
impl FromStr for NormalVec3 {
    type Err = ();
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        s.parse::<Vec3d>()?.try_into()
    }
}
impl Display for NormalVec3 {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        std::fmt::Display::fmt(&self.0, f)
    }
}

impl Mat3d {
    pub const IDENTITY: Mat3d = Mat3d {
        rvec: Vec3d::new(1., 0., 0.),
        uvec: Vec3d::new(0., 1., 0.),
        fvec: Vec3d::new(0., 0., 1.),
    };
}

#[derive(Default, Clone, Copy, PartialEq)]
pub struct BoundingBox {
    pub min: Vec3d,
    pub max: Vec3d,
}
impl Serialize for BoundingBox {
    fn write_to(&self, w: &mut impl Write) -> io::Result<()> {
        let bbox = self.sanitize();
        bbox.min.write_to(w)?;
        bbox.max.write_to(w)
    }
}
impl Debug for BoundingBox {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_fmt(format_args!("{:?}, {:?}", &self.min, &self.max))
    }
}
impl BoundingBox {
    pub const ZERO: Self = Self { min: Vec3d::ZERO, max: Vec3d::ZERO };
    pub const EMPTY: Self = Self { min: Vec3d::INFINITY, max: Vec3d::NEG_INFINITY };
    pub fn is_inverted(&self) -> bool {
        self.max.x < self.min.x || self.max.y < self.min.y || self.max.z < self.min.z
    }
    pub fn volume(&self) -> f32 {
        if self.is_inverted() {
            0.
        } else {
            (self.max.x - self.min.x) * (self.max.y - self.min.y) * (self.max.z - self.min.z)
        }
    }
    pub fn x_width(&self) -> f32 {
        self.max.x - self.min.x
    }
    pub fn y_height(&self) -> f32 {
        self.max.y - self.min.y
    }
    pub fn z_length(&self) -> f32 {
        self.max.z - self.min.z
    }
    pub fn size_on_axis(&self, axis: Axis) -> f32 {
        self.max[axis] - self.min[axis]
    }
    pub fn greatest_dimension(&self) -> Axis {
        ALL_AXES
            .into_iter()
            .max_by(|&axis1, &axis2| self.size_on_axis(axis1).partial_cmp(&self.size_on_axis(axis2)).unwrap())
            .unwrap()
    }
    pub fn expand_vec(&mut self, vec: Vec3d) {
        self.min.x = self.min.x.min(vec.x);
        self.min.y = self.min.y.min(vec.y);
        self.min.z = self.min.z.min(vec.z);
        self.max.x = self.max.x.max(vec.x);
        self.max.y = self.max.y.max(vec.y);
        self.max.z = self.max.z.max(vec.z);
    }
    pub fn expand_bbox(&mut self, bbox: &BoundingBox) {
        self.min.x = self.min.x.min(bbox.min.x);
        self.min.y = self.min.y.min(bbox.min.y);
        self.min.z = self.min.z.min(bbox.min.z);
        self.max.x = self.max.x.max(bbox.max.x);
        self.max.y = self.max.y.max(bbox.max.y);
        self.max.z = self.max.z.max(bbox.max.z);
    }
    pub fn from_vectors(iter: impl Iterator<Item = Vec3d>) -> BoundingBox {
        iter.fold(BoundingBox::EMPTY, |mut bbox, vec| {
            bbox.expand_vec(vec);
            bbox
        })
    }
    pub fn from_bboxes<'a>(iter: impl Iterator<Item = &'a Self>) -> BoundingBox {
        iter.fold(BoundingBox::EMPTY, |mut acc_bbox, bbox| {
            acc_bbox.expand_bbox(bbox);
            acc_bbox
        })
    }

    pub fn pad(mut self, pad: f32) -> BoundingBox {
        self.min.x -= pad;
        self.min.y -= pad;
        self.min.z -= pad;
        self.max.x += pad;
        self.max.y += pad;
        self.max.z += pad;
        self
    }

    pub fn contains(self, vec: Vec3d) -> bool {
        for axis in ALL_AXES {
            if vec[axis] < self.min[axis] || vec[axis] > self.max[axis] {
                return false;
            }
        }
        true
    }

    /// Replaces `Self::EMPTY` with `Self::ZERO`.
    pub fn sanitize(&self) -> &Self {
        if *self == Self::EMPTY {
            &Self::ZERO
        } else {
            self
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub enum BspLightKind {
    Muzzle = 1,
    Thruster = 2,
}
impl Serialize for BspLightKind {
    fn write_to(&self, w: &mut impl Write) -> io::Result<()> {
        w.write_u32::<LE>(*self as u32)
    }
}

pub const MAX_EYES: usize = 9;

mk_struct! {
    #[derive(Debug)]
    // this is pretty much unused by the engine
    pub struct BspLight {
        pub location: Vec3d,
        pub kind: BspLightKind,
    }

    #[derive(Debug, Clone, Default)]
    pub struct PathPoint {
        pub position: Vec3d,
        pub radius: f32,
        pub turrets: Vec<ObjectId>,
    }
}

#[derive(Default, Debug, Clone)]
pub struct EyePoint {
    pub attached_subobj: Option<ObjectId>,
    pub position: Vec3d,
    pub normal: NormalVec3,
}
impl EyePoint {
    pub fn apply_transform(&mut self, matrix: &TMat4<f32>) {
        self.position = matrix * self.position;

        let matrix = mat4_rotation_only(matrix);
        self.normal = (&matrix * self.normal.0).try_into().unwrap();
    }
}
impl Serialize for EyePoint {
    fn write_to(&self, w: &mut impl Write) -> io::Result<()> {
        self.attached_subobj.map_or(u32::MAX, |id| id.0).write_to(w)?;
        self.position.write_to(w)?;
        self.normal.write_to(w)
    }
}

#[derive(Clone, Default)]
pub struct Path {
    pub name: String,
    pub parent: String,
    pub points: Vec<PathPoint>,
}

impl Serialize for Path {
    fn write_to(&self, w: &mut impl Write) -> io::Result<()> {
        self.name.write_to(w)?;
        if get_version() >= Version::V20_02 {
            self.parent.write_to(w)?;
        }
        self.points.write_to(w)
    }
}

impl Debug for Path {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Path")
            .field("name", &self.name)
            .field("parent", &self.parent)
            .field("points", &self.points.len())
            .finish()
    }
}

impl Path {
    pub fn apply_transform(&mut self, matrix: &TMat4<f32>) {
        let scalar = matrix.determinant().abs().powf(1. / 3.);

        for point in &mut self.points {
            point.position = matrix * point.position;
            point.radius *= scalar;
        }
    }
}

#[derive(Debug, Clone)]
pub struct PolyVertex<T = NormalId> {
    pub vertex_id: VertexId,
    pub normal_id: T,
    pub uv: (f32, f32),
}
impl Serialize for PolyVertex<()> {
    fn write_to(&self, w: &mut impl Write) -> io::Result<()> {
        self.vertex_id.write_to(w)?;
        self.uv.write_to(w)
    }
}

#[derive(Clone, Copy, Debug, Default)]
pub struct Color {
    pub red: u8,
    pub green: u8,
    pub blue: u8,
}
impl Serialize for Color {
    fn write_to(&self, w: &mut impl Write) -> io::Result<()> {
        w.write_u8(self.red)?;
        w.write_u8(self.green)?;
        w.write_u8(self.blue)?;
        w.write_u8(0)
    }
}

#[derive(Clone)]
pub struct ShieldPolygon {
    pub normal: Vec3d,
    pub verts: (VertexId, VertexId, VertexId),
    pub neighbors: (PolygonId, PolygonId, PolygonId),
}
impl Debug for ShieldPolygon {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "ShieldPolygon: {:?}, verts: {:?}, {:?}, {:?}, neighbors: {:?}, {:?}, {:?}",
            self.normal, self.verts.0, self.verts.1, self.verts.2, self.neighbors.0, self.neighbors.1, self.neighbors.2
        )
    }
}
impl Serialize for ShieldPolygon {
    fn write_to(&self, w: &mut impl Write) -> io::Result<()> {
        let ShieldPolygon { normal, verts: (x, y, z), neighbors } = self;
        normal.write_to(w)?;
        (x.0 as u32, y.0 as u32, z.0 as u32).write_to(w)?;
        neighbors.write_to(w)
    }
}

#[derive(Debug)]
pub enum ShieldNode {
    Split {
        bbox: BoundingBox,
        front: Box<ShieldNode>,
        back: Box<ShieldNode>,
    },
    Leaf {
        bbox: BoundingBox,
        poly_list: Vec<PolygonId>,
    },
}
impl ShieldNode {
    pub(crate) const SPLIT: u32 = 0;
    pub(crate) const LEAF: u32 = 1;
}
impl Serialize for ShieldNode {
    fn write_to(&self, w: &mut impl Write) -> io::Result<()> {
        let mut buf = vec![];

        crate::write::write_shield_node(&mut buf, self, get_version() < Version::V22_00)?;

        w.write_u32::<LE>((buf.len()) as u32)?;
        w.write_all(&buf)
    }
}

#[derive(Debug, Clone)]
pub struct SpecialPoint {
    pub name: String,
    pub properties: String,
    pub position: Vec3d,
    pub radius: f32,
}
impl Default for SpecialPoint {
    fn default() -> Self {
        Self {
            name: Default::default(),
            properties: Default::default(),
            position: Default::default(),
            radius: 1.0,
        }
    }
}
impl Serialize for SpecialPoint {
    fn write_to(&self, w: &mut impl Write) -> io::Result<()> {
        if !self.name.starts_with('$') {
            format!("${}", self.name).write_to(w)?;
        } else {
            self.name.write_to(w)?;
        }
        self.properties.write_to(w)?;
        self.position.write_to(w)?;
        self.radius.write_to(w)
    }
}
impl SpecialPoint {
    pub fn apply_transform(&mut self, matrix: &TMat4<f32>) {
        let scalar = matrix.determinant().abs().powf(1. / 3.);

        self.position = matrix * self.position;
        self.radius *= scalar;
    }

    pub fn is_subsystem(&self) -> bool {
        properties_get_field(&self.properties, "$special") == Some("subsystem")
    }
}

#[derive(Default, Debug, Clone)]
pub struct WeaponHardpoint {
    pub position: Vec3d,
    pub normal: NormalVec3,
    pub offset: f32,
}
impl Serialize for WeaponHardpoint {
    fn write_to(&self, w: &mut impl Write) -> io::Result<()> {
        self.position.write_to(w)?;
        self.normal.write_to(w)?;
        let version = get_version();
        if version >= Version::V21_18 && version != Version::V22_00 {
            self.offset.write_to(w)?;
        }
        Ok(())
    }
}
impl WeaponHardpoint {
    pub fn apply_transform(&mut self, matrix: &TMat4<f32>) {
        self.position = matrix * self.position;
        let matrix = mat4_rotation_only(&matrix);
        self.normal = (&matrix * self.normal.0).try_into().unwrap();
    }
}

#[derive(Debug, Clone)]
pub struct ThrusterGlow {
    pub position: Vec3d,
    pub normal: NormalVec3,
    pub radius: f32,
}
impl Default for ThrusterGlow {
    fn default() -> Self {
        Self {
            position: Default::default(),
            normal: NormalVec3(Vec3d { x: 0.0, y: 0.0, z: -1.0 }),
            radius: 1.0,
        }
    }
}
impl Serialize for ThrusterGlow {
    fn write_to(&self, w: &mut impl Write) -> io::Result<()> {
        self.position.write_to(w)?;
        self.normal.write_to(w)?;
        if get_version() > Version::V20_04 {
            self.radius.write_to(w)?;
        }
        Ok(())
    }
}
impl ThrusterGlow {
    pub fn apply_transform(&mut self, matrix: &TMat4<f32>) {
        self.position = matrix * self.position;

        let scalar = matrix.determinant().abs().powf(1. / 3.);
        self.radius *= scalar;

        let matrix = mat4_rotation_only(&matrix);
        self.normal = (&matrix * self.normal.0).try_into().unwrap();
    }
}

mk_struct! {
    #[derive(Debug, Clone, Copy, PartialEq)]
    pub struct DockingPoint {
        pub position: Vec3d,
        pub normal: Vec3d,
    }
}
impl Default for DockingPoint {
    fn default() -> Self {
        Self {
            position: Default::default(),
            normal: Vec3d { x: 0.0, y: 1.0, z: 0.0 },
        }
    }
}

mk_struct! {
    #[derive(Debug, Clone)]
    pub struct GlowPoint {
        pub position: Vec3d,
        pub normal: Vec3d,
        pub radius: f32,
    }
}
impl Default for GlowPoint {
    fn default() -> Self {
        Self {
            position: Default::default(),
            normal: Default::default(),
            radius: 1.0,
        }
    }
}
impl GlowPoint {
    pub fn apply_transform(&mut self, matrix: &TMat4<f32>) {
        self.position = matrix * self.position;

        let scalar = matrix.determinant().abs().powf(1. / 3.);
        self.radius *= scalar;

        let matrix = mat4_rotation_only(&matrix);
        self.normal = (&matrix * self.normal).normalize();
    }
}

#[derive(Debug, Default)]
pub struct ObjHeader {
    pub max_radius: f32,
    pub obj_flags: u32,
    pub num_subobjects: u32,
    pub bbox: BoundingBox,
    pub detail_levels: Vec<ObjectId>,
    pub mass: f32,
    pub center_of_mass: Vec3d,
    pub moment_of_inertia: Mat3d,
    pub cross_sections: Vec<(f32, f32)>, // depth, radius
    pub bsp_lights: Vec<BspLight>,
}

#[derive(Debug)]
pub struct ShieldData {
    pub verts: Vec<Vec3d>,
    pub polygons: Vec<ShieldPolygon>,
    pub collision_tree: Option<ShieldNode>,
}
impl ShieldData {
    pub fn recalculate_tree(verts: &[Vec3d], polygons: &[ShieldPolygon]) -> ShieldNode {
        // these structs make up the smallest bits of data we'll need for this
        // the regular data structure isn't well-optimized for this, so its easier to make something purpose built
        struct ShieldPolyInfo {
            id: PolygonId,
            bbox: BoundingBox,
            center: Vec3d,
        }

        // go over the polygons, making 'infos'
        let poly_infos = polygons
            .iter()
            .enumerate()
            .map(|(i, poly)| {
                // hacky way to turn a tuple into an iterator
                let verts = vec![
                    verts[poly.verts.0 .0 as usize],
                    verts[poly.verts.1 .0 as usize],
                    verts[poly.verts.2 .0 as usize],
                ];
                let vert_iter = verts.iter().copied();

                ShieldPolyInfo {
                    id: PolygonId(i as u32),
                    bbox: BoundingBox::from_vectors(vert_iter.clone()).pad(0.01),
                    center: Vec3d::average(vert_iter),
                }
            })
            .collect::<Vec<_>>();

        fn recalc_recurse(poly_infos: &mut [&ShieldPolyInfo]) -> ShieldNode {
            if let [poly_info] = *poly_infos {
                // if theres only one polygon we're at the base case
                ShieldNode::Leaf { bbox: poly_info.bbox, poly_list: vec![poly_info.id] }
            } else {
                let bbox = BoundingBox::from_bboxes(poly_infos.iter().map(|poly_info| &poly_info.bbox)).pad(0.01);
                let axis = bbox.greatest_dimension();
                poly_infos.sort_by(|a, b| a.center[axis].partial_cmp(&b.center[axis]).unwrap());

                let halfpoint = poly_infos.len() / 2;

                ShieldNode::Split {
                    front: Box::new(recalc_recurse(&mut poly_infos[..halfpoint])),
                    back: Box::new(recalc_recurse(&mut poly_infos[halfpoint..])),
                    bbox,
                }
            }
        }

        if poly_infos.is_empty() {
            ShieldNode::Leaf { bbox: BoundingBox::default(), poly_list: vec![] }
        } else {
            recalc_recurse(&mut poly_infos.iter().collect::<Vec<_>>())
        }
    }

    pub fn recalculate_bboxes(&mut self) {
        if let Some(tree) = &mut self.collision_tree {
            recalculate_bboxes_recurse(&self.verts, &self.polygons, tree);
        }

        fn recalculate_bboxes_recurse(verts: &[Vec3d], polygons: &[ShieldPolygon], node: &mut ShieldNode) {
            match node {
                ShieldNode::Split { bbox, front, back } => {
                    recalculate_bboxes_recurse(verts, polygons, front);
                    recalculate_bboxes_recurse(verts, polygons, back);

                    *bbox = BoundingBox::EMPTY;
                    let child_bbox = match **front {
                        ShieldNode::Split { ref bbox, .. } => bbox,
                        ShieldNode::Leaf { ref bbox, .. } => bbox,
                    };
                    bbox.expand_bbox(child_bbox);
                    let child_bbox = match **back {
                        ShieldNode::Split { ref bbox, .. } => bbox,
                        ShieldNode::Leaf { ref bbox, .. } => bbox,
                    };
                    bbox.expand_bbox(child_bbox);
                }
                ShieldNode::Leaf { bbox, poly_list } => {
                    *bbox = BoundingBox::EMPTY;
                    for id in poly_list {
                        bbox.expand_vec(verts[polygons[id.0 as usize].verts.0 .0 as usize]);
                        bbox.expand_vec(verts[polygons[id.0 as usize].verts.1 .0 as usize]);
                        bbox.expand_vec(verts[polygons[id.0 as usize].verts.2 .0 as usize]);
                    }
                }
            }
        }
    }

    pub fn apply_transform(&mut self, matrix: &TMat4<f32>) {
        for vert in &mut self.verts {
            *vert = matrix * *vert;
        }

        let matrix = mat4_rotation_only(&matrix);
        for poly in &mut self.polygons {
            poly.normal = &matrix * poly.normal;
        }

        self.recalculate_bboxes();
    }
}

#[derive(Clone, Debug)]
pub struct Polygon {
    pub normal: Vec3d,
    // this might be TextureId::UNTEXTURED during parsing which indicates untextured;
    // this is cleaned up in `post_parse_fill_untextured_slot`
    // with an explicit "untextured" texture id
    pub texture: TextureId,
    pub verts: Vec<PolyVertex>,
}

#[derive(Debug, Clone)]
pub enum BspNode {
    Split {
        bbox: BoundingBox,
        front: Box<BspNode>,
        back: Box<BspNode>,
    },
    Leaf {
        bbox: BoundingBox,
        poly: Polygon,
    },
    Empty,
}
impl Default for BspNode {
    fn default() -> Self {
        Self::Empty
    }
}
impl BspNode {
    pub fn leaves_mut(&mut self) -> BspNodeIterMut<'_> {
        BspNodeIterMut { stack: vec![self] }
    }

    pub fn bbox(&self) -> &BoundingBox {
        match self {
            BspNode::Split { bbox, .. } | BspNode::Leaf { bbox, .. } => bbox,
            BspNode::Empty => &BoundingBox::EMPTY,
        }
    }

    pub fn leaves(&self) -> BspNodeIter<'_> {
        BspNodeIter { stack: vec![self] }
    }

    pub fn into_leaves(self) -> BspNodeIntoIter {
        BspNodeIntoIter { stack: vec![Box::new(self)] }
    }

    pub fn sum_of_bboxes(&self) -> f32 {
        match self {
            BspNode::Split { bbox, front, back, .. } => bbox.volume() + front.sum_of_bboxes() + back.sum_of_bboxes(),
            BspNode::Leaf { bbox, .. } => bbox.volume(),
            BspNode::Empty => 0.,
        }
    }

    pub fn sum_depth_and_size(&self) -> (u32, u32) {
        match self {
            BspNode::Split { front, back, .. } => {
                let (depth1, sz1) = front.sum_depth_and_size();
                let (depth2, sz2) = back.sum_depth_and_size();
                (depth1 + depth2 + sz1 + sz2, sz1 + sz2)
            }
            BspNode::Leaf { .. } => (0, 1),
            BspNode::Empty => (0, 0),
        }
    }

    pub fn recalculate_bboxes(&mut self, verts: &[Vec3d]) {
        match self {
            BspNode::Split { bbox, front, back, .. } => {
                front.recalculate_bboxes(verts);
                back.recalculate_bboxes(verts);
                *bbox = *front.bbox();
                bbox.expand_bbox(back.bbox());
            }
            BspNode::Leaf { bbox, poly } => {
                *bbox = BoundingBox::from_vectors(poly.verts.iter().map(|vert| verts[vert.vertex_id.0 as usize]));
            }
            BspNode::Empty => {}
        }
    }
}

pub struct BspNodeIterMut<'a> {
    stack: Vec<&'a mut BspNode>,
}

impl<'a> Iterator for BspNodeIterMut<'a> {
    type Item = (&'a mut BoundingBox, &'a mut Polygon);

    fn next(&mut self) -> Option<Self::Item> {
        loop {
            match self.stack.pop()? {
                BspNode::Split { front, back, .. } => {
                    self.stack.push(back);
                    self.stack.push(front);
                }
                BspNode::Leaf { bbox, poly } => {
                    return Some((bbox, poly));
                }
                BspNode::Empty => {}
            }
        }
    }
}

pub struct BspNodeIter<'a> {
    stack: Vec<&'a BspNode>,
}

impl<'a> Iterator for BspNodeIter<'a> {
    type Item = (&'a BoundingBox, &'a Polygon);

    fn next(&mut self) -> Option<Self::Item> {
        loop {
            match self.stack.pop()? {
                BspNode::Split { front, back, .. } => {
                    self.stack.push(back);
                    self.stack.push(front);
                }
                BspNode::Leaf { bbox, poly } => {
                    return Some((bbox, poly));
                }
                BspNode::Empty => {}
            }
        }
    }
}

pub struct BspNodeIntoIter {
    stack: Vec<Box<BspNode>>,
}

impl Iterator for BspNodeIntoIter {
    type Item = (BoundingBox, Polygon);

    fn next(&mut self) -> Option<Self::Item> {
        loop {
            match *self.stack.pop()? {
                BspNode::Split { front, back, .. } => {
                    self.stack.push(back);
                    self.stack.push(front);
                }
                BspNode::Leaf { bbox, poly } => {
                    return Some((bbox, poly));
                }
                BspNode::Empty => {}
            }
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct BspData {
    pub verts: Vec<Vec3d>,
    pub norms: Vec<Vec3d>,
    pub collision_tree: BspNode,
}
impl BspData {
    pub(crate) const ENDOFBRANCH: u32 = 0;
    pub(crate) const DEFFPOINTS: u32 = 1;
    pub(crate) const FLATPOLY: u32 = 2;
    pub(crate) const TMAPPOLY: u32 = 3;
    pub(crate) const SORTNORM: u32 = 4;
    pub(crate) const BOUNDBOX: u32 = 5;
    pub(crate) const TMAPPOLY2: u32 = 6;
    pub(crate) const SORTNORM2: u32 = 7;
}
impl BspData {
    pub fn recalculate(verts: &[Vec3d], polygons: impl Iterator<Item = Polygon>) -> BspNode {
        // first go over the polygons, filling some data, and exporting their bboxes and centers, which is important for the actual BSP generation
        let polygons = polygons
            .map(|mut poly| {
                let vert_iter = poly.verts.iter().map(|polyvert| verts[polyvert.vertex_id.0 as usize]);

                // generate the normal by averaging the cross products of adjacent edges
                let mut glm_verts = vert_iter.clone().map(Vec3::from); // first convert to glm vectors
                poly.normal = if poly.verts.len() == 3 {
                    // optimize a bit for for triangles, which we'll have a lot of
                    if let [Some(a), Some(b), Some(c)] = [glm_verts.next(), glm_verts.next(), glm_verts.next()] {
                        (a - b).cross(&(b - c)).into()
                    } else {
                        unreachable!()
                    }
                } else {
                    Vec3d::average(
                        glm_verts
                            .cycle() // cycle through the verts indefinitely...
                            .take(poly.verts.len() + 2) // ..but stop 2 after the end...
                            .collect::<Vec<_>>()
                            .windows(3) //...so with windows(3) we get 1,2,3 -> 2,3,1 -> 3,1,2 (for an example with 3 verts)
                            .map(|verts| (verts[0] - verts[1]).cross(&(verts[1] - verts[2])).into()), // ...and then get the cross from each triple, and average them
                    )
                }
                .normalize(); // and then normalize

                (Vec3d::average(vert_iter.clone()), BoundingBox::from_vectors(vert_iter).pad(0.01), poly)
            })
            .collect::<Vec<_>>();

        fn recalc_recurse(polygons: &mut [&(Vec3d, BoundingBox, Polygon)]) -> BspNode {
            if let [&(_, bbox, ref polygon)] = *polygons {
                // if there's only one polygon we're at the base case
                BspNode::Leaf { bbox, poly: polygon.clone() }
            } else {
                let bbox = BoundingBox::from_bboxes(polygons.iter().map(|(_, bbox, _)| bbox)).pad(0.01);
                let axis = bbox.greatest_dimension();
                polygons.sort_by(|a, b| a.0[axis].partial_cmp(&b.0[axis]).unwrap());

                let halfpoint = polygons.len() / 2;

                BspNode::Split {
                    front: Box::new(recalc_recurse(&mut polygons[..halfpoint])),
                    back: Box::new(recalc_recurse(&mut polygons[halfpoint..])),
                    bbox,
                }
            }
        }

        if polygons.is_empty() {
            BspNode::Empty
        } else {
            recalc_recurse(&mut polygons.iter().collect::<Vec<_>>())
        }
    }
}
impl Serialize for BspData {
    fn write_to(&self, w: &mut impl Write) -> io::Result<()> {
        w.write_u32::<LE>(0)?;

        let mut buf = vec![];

        crate::write::write_bsp_data(&mut buf, get_version(), self)?;

        w.write_u32::<LE>(buf.len() as u32)?;
        w.write_all(&buf)
    }
}

#[derive(Debug, Clone, Default)]
pub struct ThrusterBank {
    pub properties: String,
    pub glows: Vec<ThrusterGlow>,
}
impl Serialize for ThrusterBank {
    fn write_to(&self, w: &mut impl Write) -> io::Result<()> {
        (self.glows.len() as u32).write_to(w)?;
        if get_version() >= Version::V21_17 {
            self.properties.write_to(w)?;
        }
        for glow in &self.glows {
            glow.write_to(w)?;
        }
        Ok(())
    }
}

impl ThrusterBank {
    pub fn get_engine_subsys(&self) -> Option<&str> {
        properties_get_field(&self.properties, "$engine_subsystem")
    }
}

macro_rules! mk_enumeration {
    ($($(#[$meta:meta])* pub enum $tyname:ident($base:ty) {
        $($(#[$doc:meta])* $name:ident = $n:literal,)*
    })*) => {
        $(
            $(#[$meta])*
            pub enum $tyname {
                $($(#[$doc])* $name = $n,)*
            }

            impl Serialize for $tyname {
                fn write_to(&self, w: &mut impl Write) -> io::Result<()> {
                    (*self as $base).write_to(w)
                }
            }

            impl TryFrom<$base> for $tyname {
                type Error = ();

                fn try_from(value: $base) -> Result<Self, Self::Error> {
                    match value {
                        $($n => Ok(Self::$name),)*
                        _ => Err(()),
                    }
                }
            }
            impl From<$tyname> for $base {
                fn from(value: $tyname) -> Self {
                    match value {
                        $($tyname::$name => $n,)*
                    }
                }
            }
        )*
    };
}

mk_enumeration! {
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub enum SubsysTranslationType(i32) {
        None = -1,
        Unused = 0, // previously MOVEMENT_TYPE_POS
        Regular = 1, // previously MOVEMENT_TYPE_ROT
        Turret = 2, // for turrets only
        Triggered = 3,
        Intrinsic = 4, // intrinsic (non-subsystem-based)
    }
}
impl Default for SubsysTranslationType {
    fn default() -> Self {
        Self::None
    }
}

mk_enumeration! {
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub enum SubsysTranslationAxis(i32) {
        None = -1,
        X = 0,
        Z = 1,
        Y = 2,
        Other = 3,
    }
}
impl Default for SubsysTranslationAxis {
    fn default() -> Self {
        Self::None
    }
}

mk_enumeration! {
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub enum SubsysRotationType(i32) {
        None = -1,
        Unused = 0, // previously MOVEMENT_TYPE_POS
        Regular = 1, // previously MOVEMENT_TYPE_ROT
        Turret = 2, // for turrets only
        Triggered = 3,
        Intrinsic = 4, // intrinsic (non-subsystem-based)
    }
}
impl Default for SubsysRotationType {
    fn default() -> Self {
        Self::None
    }
}

mk_enumeration! {
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub enum SubsysRotationAxis(i32) {
        None = -1,
        X = 0,
        Z = 1,
        Y = 2,
        Other = 3,
    }
}
impl Default for SubsysRotationAxis {
    fn default() -> Self {
        Self::None
    }
}

pub const MAX_DEBRIS_OBJECTS: u32 = 32;

/// "semantic name links", fields derived specifically from their names
/// recalculated by recalc_semantic_name_links
#[derive(Debug, Copy, Clone)]
pub enum NameLink {
    /// points from a turret to its destroyed version
    DestroyedVersion(ObjectId),
    /// back-link for [`DestroyedVersion`]
    DestroyedVersionOf(ObjectId),
    /// points to the debris version of this object
    LiveDebris(ObjectId),
    /// back-link for [`LiveDebris`]
    LiveDebrisOf(ObjectId),
    /// Points from the highest detail level object to one of the lower detail levels.
    /// Repeats for each lower detail version; they are in arbitrary order
    DetailLevel(ObjectId, u8),
    /// back-link for [`DetailLevel`]: points from lower detail to highest detail version
    DetailLevelOf(ObjectId, u8),
}

#[derive(Debug, Clone, Default)]
pub struct SubObject {
    pub obj_id: ObjectId,
    pub radius: f32,
    pub parent: Option<ObjectId>,
    pub offset: Vec3d,
    pub geo_center: Vec3d,
    pub bbox: BoundingBox,
    pub name: String,
    pub properties: String,
    pub rotation_type: SubsysRotationType,
    pub rotation_axis: SubsysRotationAxis,
    pub translation_type: SubsysTranslationType,
    pub translation_axis: SubsysTranslationAxis,
    pub bsp_data: BspData,

    // the following fields are derived information
    pub(crate) children: Vec<ObjectId>,
    pub is_debris_model: bool,

    // "semantic name links", fields derived specifically from their names
    // recalculated by recalc_semantic_name_links
    pub name_links: Vec<NameLink>,
}
impl SubObject {
    pub fn parent(&self) -> Option<ObjectId> {
        self.parent
    }

    pub fn children(&self) -> std::slice::Iter<'_, ObjectId> {
        self.children.iter()
    }

    pub fn is_destroyed_model(&self) -> bool {
        for link in &self.name_links {
            if matches!(link, NameLink::DestroyedVersionOf(_)) {
                return true;
            }
        }
        false
    }

    pub fn recalc_radius(&mut self) {
        self.radius = 0.00001;

        for vert in &self.bsp_data.verts {
            if vert.magnitude() > self.radius {
                self.radius = vert.magnitude();
            }
        }
    }

    pub fn recalc_bbox(&mut self) {
        self.bbox.min = self.bsp_data.verts[0];
        self.bbox.max = self.bsp_data.verts[0];

        for vert in &self.bsp_data.verts {
            if vert.x < self.bbox.min.x {
                self.bbox.min.x = vert.x
            }
            if vert.y < self.bbox.min.y {
                self.bbox.min.y = vert.y
            }
            if vert.z < self.bbox.min.z {
                self.bbox.min.z = vert.z
            }
            if vert.x > self.bbox.max.x {
                self.bbox.max.x = vert.x
            }
            if vert.y > self.bbox.max.y {
                self.bbox.max.y = vert.y
            }
            if vert.z > self.bbox.max.z {
                self.bbox.max.z = vert.z
            }
        }
        // self.bbox = match self.bsp_data.collision_tree {
        //     BspNode::Split { bbox, .. } => bbox,
        //     BspNode::Leaf { bbox, .. } => bbox,
        // };
    }

    pub fn uvec_fvec(&self) -> Option<(Vec3d, Vec3d)> {
        parse_uvec_fvec(&self.properties)
    }

    pub fn is_subsystem(&self) -> bool {
        properties_get_field(&self.properties, "$special") == Some("subsystem")
    }

    /// returns the surface area of the subobject, and the average surface area position
    pub fn surface_area_average_pos(&self) -> (f32, Vec3d) {
        let mut surface_area = 0.0;
        let mut weighted_sum = Vec3d::ZERO;
        for (_, poly) in self.bsp_data.collision_tree.leaves() {
            let v1 = self.bsp_data.verts[poly.verts[0].vertex_id.0 as usize];
            let v2 = self.bsp_data.verts[poly.verts[1].vertex_id.0 as usize];
            let v3 = self.bsp_data.verts[poly.verts[2].vertex_id.0 as usize];

            let v12 = v2 - v1;
            let v13 = v3 - v1;
            let this_area = v12.cross(&v13).magnitude();
            weighted_sum += Vec3d::average([v1, v2, v3].into_iter()) * this_area;
            surface_area += this_area;
        }
        (surface_area, weighted_sum / surface_area)
    }
}

fn parse_uvec_fvec(props: &str) -> Option<(Vec3d, Vec3d)> {
    let uvec = Vec3d::from_str(properties_get_field(props, "$uvec")?).ok()?;
    let fvec = Vec3d::from_str(properties_get_field(props, "$fvec")?).ok()?;
    Some((uvec, fvec))
}

impl Serialize for SubObject {
    fn write_to(&self, w: &mut impl Write) -> io::Result<()> {
        let version = get_version();
        self.obj_id.write_to(w)?;
        if version >= Version::V21_16 {
            self.radius.write_to(w)?;
            self.parent.unwrap_or(ObjectId(u32::MAX)).write_to(w)?;
            self.offset.write_to(w)?;
        } else {
            self.parent.unwrap_or(ObjectId(u32::MAX)).write_to(w)?;
            self.offset.write_to(w)?;
            self.radius.write_to(w)?;
        }
        self.geo_center.write_to(w)?;
        self.bbox.write_to(w)?;
        self.name.write_to(w)?;
        self.properties.write_to(w)?;
        self.rotation_type.write_to(w)?;
        self.rotation_axis.write_to(w)?;
        if version >= Version::V23_01 {
            self.translation_type.write_to(w)?;
            self.translation_axis.write_to(w)?;
        }
        self.bsp_data.write_to(w)
    }
}

#[derive(Debug, Clone)]
pub struct Dock {
    pub properties: String,
    pub path: Option<PathId>,
    pub position: Vec3d,
    pub fvec: NormalVec3,
    pub uvec: NormalVec3,
}

impl Default for Dock {
    fn default() -> Self {
        Self {
            properties: Default::default(),
            path: Default::default(),
            position: Default::default(),
            fvec: NormalVec3(Vec3d::new(0., 1., 0.)),
            uvec: Default::default(),
        }
    }
}

impl Serialize for Dock {
    fn write_to(&self, w: &mut impl Write) -> io::Result<()> {
        self.properties.write_to(w)?;
        match self.path {
            None => 0_u32.write_to(w)?,
            Some(x) => [x].write_to(w)?,
        }
        let points = vec![
            DockingPoint { position: self.position - self.uvec.0, normal: self.fvec.0 },
            DockingPoint { position: self.position + self.uvec.0, normal: self.fvec.0 },
        ];
        points.write_to(w)
    }
}
impl Dock {
    #[must_use]
    pub fn orthonormalize(&(mut xvec): &Vec3, fvec: &Vec3) -> NormalVec3 {
        xvec -= *fvec * xvec.dot(fvec);
        match xvec.try_into() {
            Ok(xvec) => xvec,
            Err(()) => {
                xvec = glm::vec3(0.0, 1.0, 0.0);
                (xvec - fvec * xvec.dot(fvec)).try_into().unwrap_or_default()
            }
        }
    }

    pub fn get_uvec_angle(&self) -> f32 {
        let mut fvec: Vec3 = self.fvec.0.into();
        if fvec.try_normalize_mut(1e-6).is_none() {
            fvec = glm::vec3(1.0, 0.0, 0.0);
        }
        let uvec = self.uvec.0.into();
        let xvec = glm::vec3(0.0, 0.0, 1.0);
        let xvec = Self::orthonormalize(&xvec, &fvec).0.into();
        f32::atan2(fvec.cross(&xvec).dot(&uvec), xvec.dot(&uvec))
    }

    pub fn set_uvec_angle(&mut self, ang: f32) {
        let mut fvec: Vec3 = self.fvec.0.into();
        if fvec.try_normalize_mut(1e-6).is_none() {
            fvec = glm::vec3(1.0, 0.0, 0.0);
        }
        let mut uvec = glm::vec3(0.0, 0.0, 1.0);
        let uvec = Self::orthonormalize(&mut uvec, &fvec).0.into();

        self.uvec = glm::rotate_vec3(&uvec, ang, &fvec).try_into().unwrap_or_default();
    }

    pub fn get_name(&self) -> Option<&str> {
        properties_get_field(&self.properties, "$name")
    }

    pub fn get_parent_obj(&self) -> Option<&str> {
        properties_get_field(&self.properties, "$parent_submodel")
    }

    pub fn apply_transform(&mut self, matrix: &TMat4<f32>) {
        self.position = matrix * self.position;

        let matrix = mat4_rotation_only(&matrix);
        self.fvec = (&matrix * self.fvec.0).try_into().unwrap();
        self.uvec = (&matrix * self.uvec.0).try_into().unwrap();
    }
}

pub const MAX_TURRET_POINTS: usize = 10;

mk_struct! {
    #[derive(Clone)]
    pub struct Turret {
        pub base_obj: ObjectId,
        pub gun_obj: ObjectId,
        pub normal: NormalVec3,
        pub fire_points: Vec<Vec3d>,
    }
}

impl Default for Turret {
    fn default() -> Self {
        Self {
            base_obj: Default::default(),
            gun_obj: Default::default(),
            normal: Default::default(),
            fire_points: vec![],
        }
    }
}

impl Debug for Turret {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Turret")
            .field("base_obj", &self.base_obj)
            .field("gun_obj", &self.gun_obj)
            .field("normal", &self.normal)
            .field("fire_points", &self.fire_points.len())
            .finish()
    }
}

impl Turret {
    pub fn apply_transform(&mut self, matrix: &TMat4<f32>) {
        let matrix = mat4_rotation_and_scaling_only(matrix);
        for point in &mut self.fire_points {
            *point = &matrix * *point;
        }

        let matrix = mat4_rotation_only(&matrix);
        self.normal = (&matrix * self.normal.0).try_into().unwrap();
    }
}

#[derive(Debug, Clone, Default)]
pub struct Insignia {
    pub detail_level: u32,
    pub vertices: Vec<Vec3d>,
    pub offset: Vec3d,
    pub faces: Vec<(PolyVertex<()>, PolyVertex<()>, PolyVertex<()>)>, // any number of 3-tuples of polyvertices (without normals hence the <()>)
}

impl Serialize for Insignia {
    fn write_to(&self, w: &mut impl Write) -> io::Result<()> {
        self.detail_level.write_to(w)?;
        (self.faces.len() as u32).write_to(w)?;
        self.vertices.write_to(w)?;
        self.offset.write_to(w)?;
        for face in &self.faces {
            face.write_to(w)?;
        }
        Ok(())
    }
}

impl Insignia {
    pub fn apply_transform(&mut self, matrix: &TMat4<f32>) {
        for point in &mut self.vertices {
            *point = matrix * *point;
        }

        self.offset = matrix * self.offset;
    }
}

#[derive(Debug, Default, Clone)]
pub struct GlowPointBank {
    pub disp_time: i32,
    pub on_time: u32,
    pub off_time: u32,
    pub obj_parent: ObjectId,
    pub lod: u32,
    pub glow_type: u32,
    pub properties: String,
    pub glow_points: Vec<GlowPoint>,
}
impl Serialize for GlowPointBank {
    fn write_to(&self, w: &mut impl Write) -> io::Result<()> {
        self.disp_time.write_to(w)?;
        self.on_time.write_to(w)?;
        self.off_time.write_to(w)?;
        self.obj_parent.write_to(w)?;
        self.lod.write_to(w)?;
        self.glow_type.write_to(w)?;
        (self.glow_points.len() as u32).write_to(w)?;
        self.properties.write_to(w)?;
        for glow in &self.glow_points {
            glow.write_to(w)?;
        }
        Ok(())
    }
}

pub const MAX_TEXTURES: usize = 64;

pub const MAX_NAME_LEN: usize = 31;

pub const MAX_PROPERTIES_LEN: usize = 255;

macro_rules! mk_versions {
    (@latest $last:ident) => { Self::$last };
    (@latest $first:ident $($rest:ident)*) => { mk_versions!(@latest $($rest)*) };
    ($($(#[doc=$doc:expr])* $name:ident($num:literal, $str:literal),)*) => {
        mk_enumeration! {
            #[derive(PartialOrd, Ord, PartialEq, Eq, Debug, Clone, Copy)]
            pub enum Version(i32) {
                $($(#[doc=$doc])* $name = $num,)*
            }
        }
        impl Version {
            pub const LATEST: Version = mk_versions!(@latest $($name)*);
            pub fn to_str(self) -> &'static str {
                match self {
                    $(Version::$name => $str,)*
                }
            }
            pub fn documentation(self) -> &'static str {
                match self {
                    $(Version::$name => concat!($($doc, "\n"),*).trim()),*
                }
            }
            pub fn for_each(mut f: impl FnMut(Self)) {
                $(f(Version::$name);)*
            }
        }
        impl Default for Version {
            fn default() -> Self {
                Self::LATEST
            }
        }
        impl Display for Version {
            fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                write!(f, "{}", self.to_str())
            }
        }
    };
}

mk_versions! {
    /// Prehistoric - First version
    V19_00(1900, "19.00"),
    /// Prehistoric - Mass / MOI introduced
    V19_03(1903, "19.03"),
    /// Prehistoric - Bounding box added to BSP
    V20_00(2000, "20.00"),
    /// Prehistoric - Added path parent names
    V20_02(2002, "20.02"),
    /// Prehistoric - normal_point became center_point in TMAPPOLY
    V20_03(2003, "20.03"),
    /// Prehistoric - Glow point radius introduced after this
    V20_04(2004, "20.04"),
    /// Prehistoric - Muzzle flash introduced
    V20_07(2007, "20.07"),
    /// Prehistoric - Area mass conversion
    V20_09(2009, "20.09"),
    /// Retail FS1 - Cross sections introduced
    V20_14(2014, "20.14"),
    /// FS1 POFs produced by COB2FS v0.05
    V20_16(2016, "20.16"),
    /// Retail FS2 - PCS2 Compatible
    V21_16(2116, "21.16"),
    /// Retail FS2 - PCS2 Compatible - Thruster properties added
    V21_17(2117, "21.17"),
    /// External weapon angle offset added
    V21_18(2118, "21.18"),
    /// SLC2 replaces SLDC (no weapon offset compatibility)
    V22_00(2200, "22.00"),
    /// External weapon angle offset compatible
    V22_01(2201, "22.01"),
    /// Extended vertex and normal limits per subobject and file size optimizations
    V23_00(2300, "23.00"),
    /// Added submodel translation support
    V23_01(2301, "23.01"),
}

#[derive(Debug, Default)]
pub struct Model {
    pub version: Version,
    pub header: ObjHeader,
    pub sub_objects: ObjVec<SubObject>,
    pub textures: Vec<String>,
    pub paths: Vec<Path>,
    pub special_points: Vec<SpecialPoint>,
    pub eye_points: Vec<EyePoint>,
    pub primary_weps: Vec<Vec<WeaponHardpoint>>,
    pub secondary_weps: Vec<Vec<WeaponHardpoint>>,
    pub turrets: Vec<Turret>,
    pub thruster_banks: Vec<ThrusterBank>,
    pub glow_banks: Vec<GlowPointBank>,
    pub visual_center: Vec3d,
    pub comments: String,
    pub docking_bays: Vec<Dock>,
    pub insignias: Vec<Insignia>,
    pub shield_data: Option<ShieldData>,

    pub path_to_file: PathBuf,
    pub untextured_idx: Option<TextureId>,
    pub warnings: BTreeSet<Warning>,
    pub errors: BTreeSet<Error>,
}
impl Model {
    // rechecks just one or all of the errors on the model
    pub fn recheck_errors(&mut self, error_to_check: Set<Error>) {
        if let Set::One(error) = error_to_check {
            let failed_check = match &error {
                Error::InvalidTurretGunSubobject(turret) => self.turret_gun_subobj_not_valid(*turret),
                Error::TooManyDebrisObjects => self.num_debris_objects() > MAX_DEBRIS_OBJECTS,
                Error::DetailAndDebrisObj(id) => self.header.detail_levels.contains(&id) && self.sub_objects[*id].is_debris_model,
                Error::DetailObjWithParent(id) => self.header.detail_levels.contains(&id) && self.sub_objects[*id].parent().is_some(),
                Error::TooManyVerts(id) => self.sub_objects[*id].bsp_data.verts.len() > self.max_verts_norms_per_subobj(),
                Error::TooManyNorms(id) => self.sub_objects[*id].bsp_data.norms.len() > self.max_verts_norms_per_subobj(),
                Error::DuplicateSubobjectName(name) => self.sub_objects.iter().filter(|subobj| subobj.name == *name).count() > 1,
                Error::UnnamedSubObject(id) => self.sub_objects[*id].name.is_empty(),
            };

            let existing_warning = self.errors.contains(&error);
            if existing_warning && !failed_check {
                self.errors.remove(&error);
            } else if !existing_warning && failed_check {
                self.errors.insert(error);
            }
        } else {
            self.errors.clear();

            for i in 0..self.turrets.len() {
                if self.turret_gun_subobj_not_valid(i) {
                    self.errors.insert(Error::InvalidTurretGunSubobject(i));
                }
            }

            if self.num_debris_objects() > MAX_DEBRIS_OBJECTS {
                self.errors.insert(Error::TooManyDebrisObjects);
            }

            for &id in &self.header.detail_levels {
                let subobj = &self.sub_objects[id];
                if subobj.parent().is_some() {
                    self.errors.insert(Error::DetailObjWithParent(id));
                }
                if subobj.is_debris_model {
                    self.errors.insert(Error::DetailAndDebrisObj(id));
                }
            }

            for subobj in &self.sub_objects {
                if subobj.name.is_empty() {
                    self.errors.insert(Error::UnnamedSubObject(subobj.obj_id));
                }

                if subobj.bsp_data.verts.len() > self.max_verts_norms_per_subobj() {
                    self.errors.insert(Error::TooManyVerts(subobj.obj_id));
                }

                if subobj.bsp_data.norms.len() > self.max_verts_norms_per_subobj() {
                    self.errors.insert(Error::TooManyNorms(subobj.obj_id));
                }
            }

            for duped_name in self.sub_objects.iter().map(|subobj| &subobj.name).duplicates() {
                self.errors.insert(Error::DuplicateSubobjectName(duped_name.clone()));
            }
        }
    }

    fn turret_gun_subobj_not_valid(&self, turret_num: usize) -> bool {
        let turret = &self.turrets[turret_num];
        if turret.base_obj == turret.gun_obj {
            return false;
        }

        for &child_id in self.sub_objects[turret.base_obj].children() {
            if child_id == turret.gun_obj {
                return false;
            }
        }

        true
    }

    // rechecks just one or all of the warnings on the model
    pub fn recheck_warnings(&mut self, warning_to_check: Set<Warning>) {
        if let Set::One(warning) = warning_to_check {
            let failed_check = match &warning {
                Warning::RadiusTooSmall(subobj_opt) => self.radius_test_failed(*subobj_opt),
                Warning::BBoxTooSmall(subobj_opt) => self.bbox_test_failed(*subobj_opt),
                Warning::DockingBayWithoutPath(bay_num) => self.docking_bays.get(*bay_num).map_or(false, |bay| bay.path.is_none()),
                Warning::ThrusterPropertiesInvalidVersion(bank_idx) => {
                    self.version <= Version::V21_16 && self.thruster_banks.get(*bank_idx).map_or(false, |bank| !bank.properties.is_empty())
                }
                Warning::WeaponOffsetInvalidVersion { primary, bank, point } => {
                    (self.version <= Version::V21_17 || self.version == Version::V22_00) && {
                        if *primary {
                            *bank < self.primary_weps.len() && self.primary_weps[*bank].get(*point).map_or(false, |point| point.offset != 0.0)
                        } else {
                            *bank < self.secondary_weps.len() && self.secondary_weps[*bank].get(*point).map_or(false, |point| point.offset != 0.0)
                        }
                    }
                }
                Warning::SubObjectTranslationInvalidVersion(id) => {
                    self.version < Version::V23_01 && self.sub_objects[*id].translation_axis != SubsysTranslationAxis::None
                }
                Warning::InvertedBBox(id_opt) => {
                    if let Some(id) = id_opt {
                        self.sub_objects[*id].bbox.is_inverted()
                    } else {
                        self.header.bbox.is_inverted()
                    }
                }
                Warning::UntexturedPolygons => self.untextured_idx.is_some(),
                Warning::TooManyEyePoints => self.eye_points.len() > MAX_EYES,
                Warning::TooManyTextures => self.textures.len() > MAX_TEXTURES,
                Warning::TooFewTurretFirePoints(idx) => self.turrets.get(*idx).map_or(false, |turret| turret.fire_points.is_empty()),
                Warning::TooManyTurretFirePoints(idx) => self
                    .turrets
                    .get(*idx)
                    .map_or(false, |turret| turret.fire_points.len() > MAX_TURRET_POINTS),
                Warning::DuplicatePathName(name) => self.paths.iter().filter(|path| path.name == *name).count() > 1,
                Warning::DuplicateDetailLevel(duped_id) => self.header.detail_levels.iter().filter(|id| duped_id == *id).count() > 1,

                Warning::PathNameTooLong(idx) => self.paths.get(*idx).map_or(false, |path| path.name.len() > MAX_NAME_LEN),
                Warning::SubObjectNameTooLong(id) => self.sub_objects[*id].name.len() > MAX_NAME_LEN,
                Warning::SpecialPointNameTooLong(idx) => self
                    .special_points
                    .get(*idx)
                    .map_or(false, |spec_point| spec_point.name.len() > MAX_NAME_LEN),
                Warning::DockingBayNameTooLong(idx) => self
                    .docking_bays
                    .get(*idx)
                    .map_or(false, |dock| properties_get_field(&dock.properties, "$name").unwrap_or_default().len() > MAX_NAME_LEN),

                Warning::GlowBankPropertiesTooLong(idx) => self.glow_banks.get(*idx).map_or(false, |bank| bank.properties.len() > MAX_PROPERTIES_LEN),
                Warning::ThrusterPropertiesTooLong(idx) => self
                    .thruster_banks
                    .get(*idx)
                    .map_or(false, |bank| bank.properties.len() > MAX_PROPERTIES_LEN),
                Warning::SubObjectPropertiesTooLong(id) => self.sub_objects[*id].properties.len() > MAX_PROPERTIES_LEN,
                Warning::DockingBayPropertiesTooLong(idx) => self
                    .docking_bays
                    .get(*idx)
                    .map_or(false, |bank| bank.properties.len() > MAX_PROPERTIES_LEN),
                Warning::SpecialPointPropertiesTooLong(idx) => self
                    .special_points
                    .get(*idx)
                    .map_or(false, |spec_point| spec_point.properties.len() > MAX_PROPERTIES_LEN),
                Warning::InvalidDockParentSubmodel(idx) => self.docking_bays.get(*idx).map_or(false, |dock| {
                    properties_get_field(&dock.properties, "$parent_submodel").map_or(false, |name| self.get_obj_id_by_name(name).is_none())
                }),
                Warning::Detail0NonZeroOffset => self
                    .header
                    .detail_levels
                    .get(0)
                    .map_or(false, |id| !self.sub_objects[*id].offset.is_null()),
            };

            let existing_warning = self.warnings.contains(&warning);
            if existing_warning && !failed_check {
                self.warnings.remove(&warning);
            } else if !existing_warning && failed_check {
                self.warnings.insert(warning);
            }
        } else {
            self.warnings.clear();

            if self.radius_test_failed(None) {
                self.warnings.insert(Warning::RadiusTooSmall(None));
            }

            if self.bbox_test_failed(None) {
                self.warnings.insert(Warning::BBoxTooSmall(None));
            }

            if self.header.bbox.is_inverted() && self.header.bbox != BoundingBox::EMPTY {
                self.warnings.insert(Warning::InvertedBBox(None));
            }

            for subobj in &self.sub_objects {
                if self.bbox_test_failed(Some(subobj.obj_id)) {
                    self.warnings.insert(Warning::BBoxTooSmall(Some(subobj.obj_id)));
                }

                if self.radius_test_failed(Some(subobj.obj_id)) {
                    self.warnings.insert(Warning::RadiusTooSmall(Some(subobj.obj_id)));
                }

                if subobj.bbox.is_inverted() && subobj.bbox != BoundingBox::EMPTY {
                    self.warnings.insert(Warning::InvertedBBox(Some(subobj.obj_id)));
                }

                if subobj.name.len() > MAX_NAME_LEN {
                    self.warnings.insert(Warning::SubObjectNameTooLong(subobj.obj_id));
                }

                if subobj.properties.len() > MAX_PROPERTIES_LEN {
                    self.warnings.insert(Warning::SubObjectPropertiesTooLong(subobj.obj_id));
                }

                if self.version < Version::V23_01 && subobj.translation_axis != SubsysTranslationAxis::None {
                    self.warnings.insert(Warning::SubObjectTranslationInvalidVersion(subobj.obj_id));
                }
            }

            for (i, dock) in self.docking_bays.iter().enumerate() {
                if dock.path.is_none() {
                    self.warnings.insert(Warning::DockingBayWithoutPath(i));
                }

                if dock.properties.len() > MAX_PROPERTIES_LEN {
                    self.warnings.insert(Warning::DockingBayPropertiesTooLong(i));
                }

                if properties_get_field(&dock.properties, "$name").unwrap_or_default().len() > MAX_NAME_LEN {
                    self.warnings.insert(Warning::DockingBayNameTooLong(i));
                }

                if properties_get_field(&dock.properties, "$parent_submodel").map_or(false, |name| self.get_obj_id_by_name(name).is_none()) {
                    self.warnings.insert(Warning::InvalidDockParentSubmodel(i));
                }
            }

            for (i, bank) in self.thruster_banks.iter().enumerate() {
                if !bank.properties.is_empty() {
                    if self.version <= Version::V21_16 {
                        self.warnings.insert(Warning::ThrusterPropertiesInvalidVersion(i));
                    }

                    if bank.properties.len() > MAX_PROPERTIES_LEN {
                        self.warnings.insert(Warning::ThrusterPropertiesTooLong(i));
                    }
                }
            }

            if self.version <= Version::V21_17 || self.version == Version::V22_00 {
                for (i, bank) in self.primary_weps.iter().enumerate() {
                    for (j, point) in bank.iter().enumerate() {
                        if point.offset != 0.0 {
                            self.warnings
                                .insert(Warning::WeaponOffsetInvalidVersion { primary: true, bank: i, point: j });
                        }
                    }
                }
                for (i, bank) in self.secondary_weps.iter().enumerate() {
                    for (j, point) in bank.iter().enumerate() {
                        if point.offset != 0.0 {
                            self.warnings
                                .insert(Warning::WeaponOffsetInvalidVersion { primary: false, bank: i, point: j });
                        }
                    }
                }
            }

            for (i, turret) in self.turrets.iter().enumerate() {
                if turret.fire_points.is_empty() {
                    self.warnings.insert(Warning::TooFewTurretFirePoints(i));
                } else if turret.fire_points.len() > MAX_TURRET_POINTS {
                    self.warnings.insert(Warning::TooManyTurretFirePoints(i));
                }
            }

            for (i, glow_bank) in self.glow_banks.iter().enumerate() {
                if glow_bank.properties.len() > MAX_PROPERTIES_LEN {
                    self.warnings.insert(Warning::GlowBankPropertiesTooLong(i));
                }
            }

            for (i, special_point) in self.special_points.iter().enumerate() {
                if special_point.name.len() > MAX_NAME_LEN {
                    self.warnings.insert(Warning::SpecialPointNameTooLong(i));
                }

                if special_point.properties.len() > MAX_PROPERTIES_LEN {
                    self.warnings.insert(Warning::SpecialPointPropertiesTooLong(i));
                }
            }

            for (i, path) in self.paths.iter().enumerate() {
                if path.name.len() > MAX_NAME_LEN {
                    self.warnings.insert(Warning::PathNameTooLong(i));
                }
            }

            for duped_name in self.paths.iter().map(|path| &path.name).duplicates() {
                self.warnings.insert(Warning::DuplicatePathName(duped_name.clone()));
            }

            if let Some(id) = self.header.detail_levels.get(0) {
                if !self.sub_objects[*id].offset.is_null() {
                    self.warnings.insert(Warning::Detail0NonZeroOffset);
                }
            }

            for duped_id in self.header.detail_levels.iter().duplicates() {
                self.warnings.insert(Warning::DuplicateDetailLevel(*duped_id));
            }

            if self.untextured_idx.is_some() {
                self.warnings.insert(Warning::UntexturedPolygons);
            }

            if self.eye_points.len() > MAX_EYES {
                self.warnings.insert(Warning::TooManyEyePoints);
            }

            if self.textures.len() > MAX_TEXTURES {
                self.warnings.insert(Warning::TooManyTextures);
            }
        }
    }

    // tests if the radius for a subobject or the header is too small for its geometry
    // None means the header/entire model's radius
    fn radius_test_failed(&self, subobj_opt: Option<ObjectId>) -> bool {
        if let Some(subobj) = subobj_opt {
            let subobj = &self.sub_objects[subobj];
            let radius_with_margin = (1.0 + f32::EPSILON) * subobj.radius;
            for vert in &subobj.bsp_data.verts {
                if vert.magnitude() > radius_with_margin {
                    return true;
                }
            }
        } else {
            let radius_with_margin = (1.0 + f32::EPSILON) * self.header.max_radius;
            if let Some(&detail_0) = self.header.detail_levels.first() {
                for subobj in &self.sub_objects {
                    // we dont care about subobjects which aren't part of the detail0 hierarchy
                    if !self.is_obj_id_ancestor(subobj.obj_id, detail_0) {
                        continue;
                    }

                    let offset = self.get_total_subobj_offset(subobj.obj_id);
                    for vert in &subobj.bsp_data.verts {
                        if (*vert + offset).magnitude() > radius_with_margin {
                            return true;
                        }
                    }
                }
            }
        }

        false
    }

    // tests if the bbox for a subobject or the header is too small for its geometry
    // None means the header/entire model's radius
    fn bbox_test_failed(&self, subobj_opt: Option<ObjectId>) -> bool {
        if let Some(subobj) = subobj_opt {
            let subobj = &self.sub_objects[subobj];
            for vert in &subobj.bsp_data.verts {
                if !subobj.bbox.contains(*vert) {
                    return true;
                }
            }
        } else if let Some(&detail_0) = self.header.detail_levels.first() {
            for subobj in &self.sub_objects {
                // we dont care about subobjects which aren't part of the detail0 hierarchy
                if !self.is_obj_id_ancestor(subobj.obj_id, detail_0) {
                    continue;
                }

                let offset = self.get_total_subobj_offset(subobj.obj_id);
                for vert in &subobj.bsp_data.verts {
                    if !self.header.bbox.contains(offset + *vert) {
                        return true;
                    }
                }
            }
        }

        false
    }

    pub fn get_total_subobj_offset(&self, id: ObjectId) -> Vec3d {
        let mut subobj = &self.sub_objects[id];
        let mut out = subobj.offset;
        while let Some(parent) = subobj.parent {
            subobj = &self.sub_objects[parent];
            out += subobj.offset;
        }
        out
    }

    // see if maybe_ancestor is actually an ancestor of obj_id in the subobject hierarchy
    pub fn is_obj_id_ancestor(&self, obj_id: ObjectId, maybe_ancestor: ObjectId) -> bool {
        if obj_id == maybe_ancestor {
            return true;
        }

        let mut sub_obj_parent = self.sub_objects[obj_id].parent;
        loop {
            if sub_obj_parent == Some(maybe_ancestor) {
                return true;
            } else if sub_obj_parent.is_none() {
                return false;
            }
            assert!(sub_obj_parent != self.sub_objects[sub_obj_parent.unwrap()].parent, "cycle detected!! {:?} {:?}", obj_id, sub_obj_parent);
            sub_obj_parent = self.sub_objects[sub_obj_parent.unwrap()].parent;
        }
    }

    pub fn get_sobj_detail_level(&self, obj_id: ObjectId) -> Option<u32> {
        for (i, id) in self.header.detail_levels.iter().enumerate() {
            if self.is_obj_id_ancestor(obj_id, *id) {
                return Some(i as u32);
            }
        }
        None
    }

    pub fn get_subobj_names(&self) -> Vec<String> {
        let mut ret = vec![];
        for subobj in &self.sub_objects {
            ret.push(subobj.name.clone());
        }
        ret
    }

    pub fn get_obj_id_by_name(&self, name: &str) -> Option<ObjectId> {
        for subobj in &self.sub_objects {
            if subobj.name == name {
                return Some(subobj.obj_id);
            }
        }
        None
    }

    pub fn path_removal_fixup(&mut self, removed_idx: PathId) {
        for bay in &mut self.docking_bays {
            if let Some(path_num) = bay.path {
                if removed_idx == path_num {
                    bay.path = None;
                } else if path_num > removed_idx {
                    bay.path = Some(PathId(path_num.0 - 1));
                }
            }
        }
    }

    pub fn get_valid_gun_subobjects_for_turret(&self, existing_obj: ObjectId, turret_obj: ObjectId) -> (Vec<ObjectId>, usize) {
        let mut out_vec = vec![];
        let mut out_idx = 0;
        let mut found_existing_obj = false;

        // check the turret base object itself first
        if existing_obj == turret_obj {
            out_idx = out_vec.len();
            found_existing_obj = true;
        }
        out_vec.push(turret_obj);

        // then iterate through immediate base object children, which are also valid
        for &child_id in &self.sub_objects[turret_obj].children {
            if existing_obj == child_id {
                out_idx = out_vec.len();
                found_existing_obj = true;
            }
            out_vec.push(child_id);
        }

        // if none of the above are the current selection, that means its invalid!
        // invalidity is handled separately, but we have to add this invalid object either way, so append it at the end
        if !found_existing_obj {
            out_idx = out_vec.len();
            out_vec.push(existing_obj);
        }

        (out_vec, out_idx)
    }

    pub fn do_for_recursive_subobj_children<'a>(&'a self, id: ObjectId, f: &mut impl FnMut(&'a SubObject)) {
        f(&self.sub_objects[id]);

        for &child_id in self.sub_objects[id].children() {
            f(&self.sub_objects[child_id]);
            self.do_for_recursive_subobj_children(child_id, f);
        }
    }

    pub fn num_debris_objects(&self) -> u32 {
        let mut num_debris = 0;
        for sobj in &self.sub_objects {
            if sobj.is_debris_model {
                num_debris += 1;
            }
        }
        num_debris
    }

    pub fn apply_transform(&mut self, matrix: &TMat4<f32>) {
        for i in 0..self.sub_objects.len() {
            // only apply to top-level subobjects (no parent), apply_transform() will
            // recursively apply the proper transform to its children
            if self.sub_objects[ObjectId(i as u32)].parent().is_none() {
                self.apply_subobj_transform(ObjectId(i as u32), &matrix, true);
            }
        }

        self.recalc_bbox();
        self.recalc_radius();

        for path in &mut self.paths {
            path.apply_transform(&matrix);
        }

        for point in &mut self.special_points {
            point.apply_transform(&matrix);
        }

        for bank in &mut self.primary_weps {
            for point in bank {
                point.apply_transform(&matrix);
            }
        }
        for bank in &mut self.secondary_weps {
            for point in bank {
                point.apply_transform(&matrix);
            }
        }

        for bank in &mut self.thruster_banks {
            for point in &mut bank.glows {
                point.apply_transform(&matrix);
            }
        }

        for bank in &mut self.glow_banks {
            for point in &mut bank.glow_points {
                point.apply_transform(&matrix);
            }
        }

        for dock in &mut self.docking_bays {
            dock.apply_transform(&matrix);
        }

        for eye in &mut self.eye_points {
            eye.apply_transform(&matrix)
        }

        for insignia in &mut self.insignias {
            insignia.apply_transform(&matrix);
        }

        for turret in &mut self.turrets {
            turret.apply_transform(&matrix);
        }

        if let Some(shield) = &mut self.shield_data {
            shield.apply_transform(&matrix);
        }
    }

    pub fn apply_subobj_transform(&mut self, id: ObjectId, matrix: &TMat4<f32>, transform_offset: bool) {
        let zero = Vec3d::ZERO.into();
        let translation = matrix.transform_point(&zero) - zero;
        let no_trans_matrix = &matrix.append_translation(&(-translation));

        let subobj = &mut self.sub_objects[id];
        subobj.radius = 0.0;
        for vert in &mut subobj.bsp_data.verts {
            *vert = no_trans_matrix * *vert;
            if !transform_offset {
                *vert += translation.into();
            }
            if vert.magnitude() > subobj.radius {
                subobj.radius = vert.magnitude();
            }
        }

        // this preserves rotations, but inverts scales, which is the proper transformation for normals
        let norm_matrix = no_trans_matrix.try_inverse().unwrap().transpose();

        for norm in &mut subobj.bsp_data.norms {
            *norm = (&norm_matrix * *norm).normalize();
        }

        subobj.bsp_data.collision_tree =
            BspData::recalculate(&subobj.bsp_data.verts, std::mem::take(&mut subobj.bsp_data.collision_tree).into_leaves().map(|(_, poly)| poly));

        subobj.bbox = *subobj.bsp_data.collision_tree.bbox();

        if transform_offset {
            subobj.offset = matrix * subobj.offset;
        }

        let children = subobj.children.clone();

        for child_id in children {
            self.apply_subobj_transform(child_id, no_trans_matrix, true)
        }
    }

    pub fn recalc_subobj_offset(&mut self, id: ObjectId) {
        let subobj = &mut self.sub_objects[id];
        let new_offset = Vec3d::average(subobj.bsp_data.verts.iter().map(|vert| *vert + subobj.offset));
        self.subobj_move_only_offset(id, new_offset)
    }

    pub fn subobj_move_only_offset(&mut self, id: ObjectId, new_offset: Vec3d) {
        let subobj = &mut self.sub_objects[id];
        let diff = new_offset - subobj.offset;

        let children = subobj.children.clone();
        for id in &children {
            self.sub_objects[*id].offset -= diff;
        }

        let subobj = &mut self.sub_objects[id];
        subobj.bbox.max -= diff;
        subobj.bbox.min -= diff;
        subobj.offset = new_offset;
        self.apply_subobj_transform(id, &glm::translation(&(-diff).into()), false);
        self.sub_objects[id].recalc_radius();
    }

    pub fn recalc_radius(&mut self) {
        self.header.max_radius = 0.00001;
        if let Some(&detail_0) = self.header.detail_levels.first() {
            for subobj in &self.sub_objects {
                if !self.is_obj_id_ancestor(subobj.obj_id, detail_0) {
                    continue;
                }

                let offset = self.get_total_subobj_offset(subobj.obj_id);
                for vert in &subobj.bsp_data.verts {
                    if (*vert + offset).magnitude() > self.header.max_radius {
                        self.header.max_radius = (*vert + offset).magnitude();
                    }
                }
            }
        }
        // Also include shield mesh in radius calculation, but only if a shield exists
        if let Some(shield) = &self.shield_data {
            for vert in &shield.verts {
                let mag = vert.magnitude();
                if mag > self.header.max_radius {
                    self.header.max_radius = mag;
                }
            }
        }
    }

    pub fn recalc_bbox(&mut self) {
        let mut new_bbox = self.header.bbox;
        new_bbox.min = Vec3d { x: -0.00001, y: -0.00001, z: -0.00001 };
        new_bbox.max = Vec3d { x: 0.00001, y: 0.00001, z: 0.00001 };

        if let Some(&detail_0) = self.header.detail_levels.first() {
            for subobj in &self.sub_objects {
                if !self.is_obj_id_ancestor(subobj.obj_id, detail_0) {
                    continue;
                }

                let offset = self.get_total_subobj_offset(subobj.obj_id);
                let min = offset + subobj.bbox.min;
                let max = offset + subobj.bbox.max;
                new_bbox.min = Vec3d {
                    x: f32::min(new_bbox.min.x, min.x),
                    y: f32::min(new_bbox.min.y, min.y),
                    z: f32::min(new_bbox.min.z, min.z),
                };

                new_bbox.max = Vec3d {
                    x: f32::max(new_bbox.max.x, max.x),
                    y: f32::max(new_bbox.max.y, max.y),
                    z: f32::max(new_bbox.max.z, max.z),
                };
            }
        }

        self.header.bbox = new_bbox;
    }

    pub fn recalc_mass(&mut self) {
        self.header.mass = 4.65 * (self.header.bbox.volume().powf(2.0 / 3.0));
    }

    pub fn recalc_moi(&mut self) {
        self.header.moment_of_inertia = Mat3d::default();

        fn sum_verts_recurse(subobjects: &ObjVec<SubObject>, id: ObjectId) -> usize {
            subobjects[id].bsp_data.verts.len() + subobjects[id].children.iter().map(|id| sum_verts_recurse(subobjects, *id)).sum::<usize>()
        }

        if let Some(&detail_0) = self.header.detail_levels.first() {
            let num_verts = sum_verts_recurse(&self.sub_objects, detail_0);

            fn add_point_mass_moi(moi: &mut Matrix3<f64>, pos: Vec3d) {
                moi.column_mut(0).x += (pos.y * pos.y + pos.z * pos.z) as f64;
                moi.column_mut(0).y -= (pos.x * pos.y) as f64;
                moi.column_mut(0).z -= (pos.x * pos.z) as f64;
                moi.column_mut(1).x -= (pos.x * pos.y) as f64;
                moi.column_mut(1).y += (pos.x * pos.x + pos.z * pos.z) as f64;
                moi.column_mut(1).z -= (pos.y * pos.z) as f64;
                moi.column_mut(2).x -= (pos.x * pos.z) as f64;
                moi.column_mut(2).y -= (pos.y * pos.z) as f64;
                moi.column_mut(2).z += (pos.x * pos.x + pos.y * pos.y) as f64;
            }

            fn accumulate_moi_recurse(subobjects: &ObjVec<SubObject>, id: ObjectId, moi: &mut Matrix3<f64>) {
                subobjects[id].bsp_data.verts.iter().for_each(|vert| add_point_mass_moi(moi, *vert));
                subobjects[id].children.iter().for_each(|id| accumulate_moi_recurse(subobjects, *id, moi));
            }

            let mut new_moi: Matrix3<f64> = Matrix3::zeros();

            accumulate_moi_recurse(&self.sub_objects, detail_0, &mut new_moi);

            let point_mass = self.header.mass as f64 / num_verts as f64;
            new_moi *= point_mass;
            new_moi = new_moi.try_inverse().unwrap();
            self.header.moment_of_inertia = new_moi.cast::<f32>().into();
        }
    }

    /// returns the surface area of detail0 and its children, and the average surface area position
    pub fn surface_area_average_pos(&self) -> (f32, Vec3d) {
        let mut surface_area = 0.0;
        let mut weighted_avg = Vec3d::ZERO;
        let detail0 = if let Some(id) = self.header.detail_levels.first() {
            id
        } else {
            return (0.0, Vec3d::ZERO);
        };

        self.do_for_recursive_subobj_children(*detail0, &mut |subobj| {
            let (this_area, this_avg) = subobj.surface_area_average_pos();
            weighted_avg += this_avg * this_area;
            surface_area += this_area;
        });

        (surface_area, weighted_avg / surface_area)
    }

    pub fn recalc_all_children_ids(&mut self) {
        for subobj in self.sub_objects.iter_mut() {
            subobj.children.clear();
        }

        for i in 0..self.sub_objects.len() {
            if let Some(parent) = self.sub_objects.0[i].parent {
                let id = self.sub_objects.0[i].obj_id;
                self.sub_objects[parent].children.push(id);
            }
        }
    }

    pub fn recalc_semantic_name_links(&mut self) {
        // clear everything first
        for subobj in self.sub_objects.iter_mut() {
            subobj.name_links.clear();
        }

        for i in (0..self.sub_objects.len()).map(|i| ObjectId(i as u32)) {
            let mut name1 = &self.sub_objects[i].name;
            if let Some((_, debris_of)) = name1.split_once("debris-") {
                if let Some(obj) = self.sub_objects.iter().find(|obj| debris_of.starts_with(&obj.name)) {
                    let j = obj.obj_id;
                    self.sub_objects[j].name_links.push(NameLink::LiveDebris(i));
                    self.sub_objects[i].name_links.push(NameLink::LiveDebrisOf(j));
                    name1 = &self.sub_objects[i].name;
                }
            }
            if let Some(destroyed_of) = name1.strip_suffix("-destroyed") {
                if let Some(obj) = self.sub_objects.iter().find(|obj| obj.name == destroyed_of) {
                    let j = obj.obj_id;
                    self.sub_objects[j].name_links.push(NameLink::DestroyedVersion(i));
                    self.sub_objects[i].name_links.push(NameLink::DestroyedVersionOf(j));
                    name1 = &self.sub_objects[i].name;
                }
            }
            for j in (0..self.sub_objects.len()).map(|i| ObjectId(i as u32)) {
                let name2 = &self.sub_objects[j].name;
                if name1.len() == name2.len() && self.sub_objects[j].parent.is_some() && self.sub_objects[i].parent.is_some() {
                    // zip them together and filter for equal characters, leaving only the remaining, differing characters
                    let mut iter = name1.chars().zip(name2.chars()).filter(|(c1, c2)| c1 != c2);
                    // grab the characters that differ and don't continue if there's more than one,
                    // and check that they're 'a' and 'b'..='h' respectively
                    if let (Some(('a', ch @ 'b'..='h')), None) = (iter.next(), iter.next()) {
                        let level = ch as u8 - 'a' as u8;
                        self.sub_objects[j].name_links.push(NameLink::DetailLevelOf(i, level));
                        self.sub_objects[i].name_links.push(NameLink::DetailLevel(j, level));
                        name1 = &self.sub_objects[i].name;
                    }
                }
            }
        }
    }

    pub fn clean_up(&mut self) {
        if let Some(shield) = &mut self.shield_data {
            if shield.collision_tree.is_none() {
                shield.collision_tree = Some(ShieldData::recalculate_tree(&shield.verts, &shield.polygons));
            }
        }
    }

    pub fn make_orphan(&mut self, would_be_orphan: ObjectId) {
        if let Some(parent_id) = self.sub_objects[would_be_orphan].parent {
            // maintain it's current relative position to the whole model
            self.sub_objects[would_be_orphan].offset = self.get_total_subobj_offset(would_be_orphan);

            let parent_children = &mut self.sub_objects[parent_id].children;
            parent_children.remove(parent_children.iter().position(|child_id| *child_id == would_be_orphan).unwrap());
        }
        self.sub_objects[would_be_orphan].parent = None;
    }

    pub fn make_parent(&mut self, new_parent: ObjectId, new_child: ObjectId) -> Option<()> {
        if !self.is_obj_id_ancestor(new_parent, new_child) {
            self.sub_objects[new_parent].children.push(new_child);
            self.sub_objects[new_child].parent = Some(new_parent);

            // maintain it's current relative position to the whole model
            let offset_from_parents = self.get_total_subobj_offset(new_child) - self.sub_objects[new_child].offset;
            self.sub_objects[new_child].offset -= offset_from_parents;

            Some(())
        } else {
            None
        }
    }

    pub fn max_verts_norms_per_subobj(&self) -> usize {
        if self.version >= Version::V23_00 {
            u32::MAX as usize
        } else {
            u16::MAX as usize
        }
    }

    pub fn global_import(&mut self, mut import_model: Box<Model>) {
        self.header.mass = import_model.header.mass;
        self.header.moment_of_inertia = import_model.header.moment_of_inertia;
        self.primary_weps = import_model.primary_weps;
        self.secondary_weps = import_model.secondary_weps;
        self.docking_bays = import_model.docking_bays;
        self.thruster_banks = import_model.thruster_banks;
        self.glow_banks = import_model.glow_banks;
        self.special_points = import_model.special_points;
        self.paths = import_model.paths;
        self.eye_points = import_model.eye_points;
        self.insignias = import_model.insignias;

        // turrets are more complicated, exact base + arm object name matches only
        import_model.turrets.retain_mut(|turret| {
            for subobj in &self.sub_objects {
                if import_model.sub_objects[turret.base_obj].name == subobj.name
                    && self.get_obj_id_by_name(&import_model.sub_objects[turret.gun_obj].name).is_some()
                {
                    turret.base_obj = subobj.obj_id;
                    turret.gun_obj = self.get_obj_id_by_name(&import_model.sub_objects[turret.gun_obj].name).unwrap();
                    return true;
                }
            }
            false
        });
        self.turrets = import_model.turrets;

        self.recheck_warnings(Set::All);
        self.recheck_errors(Set::All);
    }

    pub fn turret_matrix(&self, turret_idx: usize) -> TMat4<f32> {
        let turret = &self.turrets[turret_idx];
        let mut arr = if let Some((uvec, fvec)) = self.sub_objects[turret.base_obj].uvec_fvec() {
            [uvec.into(), fvec.into()]
        } else {
            [turret.normal.0.into(), Vec3d::new(0.0, 0.0, 1.0).into()]
        };

        if Vec3::orthonormalize(&mut arr) == 1 {
            // mustve chosen +z as the fvec...
            arr = [turret.normal.0.into(), Vec3d::new(0.0, 1.0, 0.0).into()];
            Vec3::orthonormalize(&mut arr);
        }
        let [uvec, fvec] = arr;
        let rvec = uvec.cross(&fvec);
        let mat = TMat3::from_columns(&[rvec, uvec, fvec]);
        mat.to_homogeneous()
    }
}

pub enum Set<T> {
    All,
    One(T),
}

#[derive(PartialEq, Eq, PartialOrd, Ord, Debug, Clone)]
pub enum Error {
    InvalidTurretGunSubobject(usize), // turret index
    TooManyDebrisObjects,
    DetailObjWithParent(ObjectId),
    DetailAndDebrisObj(ObjectId),
    TooManyVerts(ObjectId),
    TooManyNorms(ObjectId),
    UnnamedSubObject(ObjectId),
    DuplicateSubobjectName(String),
    // all turret base/gun objects must be disjoint!
}

#[derive(PartialEq, Eq, PartialOrd, Ord, Clone, Debug)]
pub enum Warning {
    RadiusTooSmall(Option<ObjectId>),
    BBoxTooSmall(Option<ObjectId>),
    InvertedBBox(Option<ObjectId>),
    UntexturedPolygons,
    DockingBayWithoutPath(usize),
    ThrusterPropertiesInvalidVersion(usize),
    WeaponOffsetInvalidVersion { primary: bool, bank: usize, point: usize },
    SubObjectTranslationInvalidVersion(ObjectId),
    TooFewTurretFirePoints(usize),
    TooManyTurretFirePoints(usize),
    DuplicatePathName(String),
    DuplicateDetailLevel(ObjectId),
    TooManyEyePoints,
    TooManyTextures,
    InvalidDockParentSubmodel(usize),
    Detail0NonZeroOffset,

    PathNameTooLong(usize),
    SpecialPointNameTooLong(usize),
    SubObjectNameTooLong(ObjectId),
    DockingBayNameTooLong(usize),

    SubObjectPropertiesTooLong(ObjectId),
    ThrusterPropertiesTooLong(usize),
    DockingBayPropertiesTooLong(usize),
    GlowBankPropertiesTooLong(usize),
    SpecialPointPropertiesTooLong(usize),
    // path with no parent
    // thruster with no engine subsys (and an engine subsys exists)
    // turret uvec != turret normal
    // turret subobject properties not set up for a turret
}

pub fn post_parse_fill_untextured_slot(sub_objects: &mut Vec<SubObject>, textures: &mut Vec<String>) -> Option<TextureId> {
    let max_texture = TextureId(textures.len().try_into().unwrap());
    let untextured_id = match textures.iter().position(|tex| tex == "Untextured") {
        Some(index) => TextureId(index.try_into().unwrap()),
        None => max_texture,
    };
    let mut has_untextured = false;
    for subobj in sub_objects.iter_mut() {
        for (_, poly) in subobj.bsp_data.collision_tree.leaves_mut() {
            if poly.texture >= max_texture {
                has_untextured = true;
                poly.texture = untextured_id;
            }
        }
    }
    if untextured_id < max_texture {
        Some(untextured_id)
    } else if has_untextured {
        textures.push(format!("Untextured"));
        Some(untextured_id)
    } else {
        None
    }
}

pub fn properties_delete_field(properties: &mut String, field: &str) {
    if let Some(start_idx) = properties.find(field) {
        let mut end_idx = if let Some(idx) = properties[start_idx..].chars().position(|d| d.is_ascii_control()) {
            start_idx + idx
        } else {
            start_idx + properties[start_idx..].len()
        };

        let mut chars = properties[start_idx..].chars();
        while end_idx < properties.len() && chars.next().unwrap().is_ascii_control() {
            end_idx += 1;
        }

        *properties = format!("{}{}", &properties[..start_idx], &properties[end_idx..]).trim().to_string();
    }
}

fn properties_find_field(properties: &str, field: &str) -> Option<(usize, usize)> {
    if let Some(mut start_idx) = properties.find(field) {
        let end_idx = if let Some(idx) = properties[start_idx..].chars().position(|d| d.is_ascii_control()) {
            start_idx + idx
        } else {
            properties.len()
        };

        start_idx += field.len();

        let mut chars = properties[start_idx..].chars();
        while chars.next().map_or(false, |c| c == '=' || c == ':' || (c.is_whitespace() && c != '\n')) {
            start_idx += 1;
        }

        Some((start_idx, end_idx))
    } else {
        None
    }
}

pub fn properties_update_field(properties: &mut String, field: &str, val: &str) {
    if val == "" {
        properties_delete_field(properties, field);
    } else {
        if properties.is_empty() {
            *properties = format!("{}={}", field, val);
        } else if let Some((start_idx, end_idx)) = properties_find_field(properties, field) {
            *properties = format!("{}{}{}", &properties[..start_idx], val, &properties[end_idx..]);
        } else {
            *properties = format!("{}\n{}={}", properties, field, val);
        }
    }
}

pub fn properties_get_field<'a>(properties: &'a str, field: &str) -> Option<&'a str> {
    if let Some((start_idx, end_idx)) = properties_find_field(properties, field) {
        Some(&properties[start_idx..end_idx])
    } else {
        None
    }
}

pub fn properties_set_flag(properties: &mut String, flag: &str) {
    if properties_find_field(properties, flag).is_none() {
        *properties = format!("{}\n{}", properties, flag);
    }
}

pub fn properties_remove_flag(properties: &mut String, flag: &str) {
    properties_delete_field(properties, flag);
}
