//! Superficie CPU de mallas y órdenes 3D (ADR-025).
//!
//! Este crate convierte documentos glTF en mallas validadas y produce órdenes
//! de dibujo deterministas. No depende de `wgpu`: la presentación GPU requiere
//! una decisión posterior. Ningún tipo de `gltf` ni de Rapier aparece en la API.

use std::collections::HashMap;
use std::error::Error;
use std::fmt;
#[cfg(feature = "gltf")]
use std::path::Path;

#[cfg(feature = "gltf")]
use lunar_assets::gltf::{
    AccessorComponent, AccessorData, AccessorElements, AttributeType, GltfDocument, GltfStore,
    PrimitiveMode,
};

/// Identificador opaco de una malla registrada en un [`Renderer3D`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct MeshId(u64);

impl MeshId {
    /// Crea un handle a partir del id público devuelto por el renderer.
    #[must_use]
    pub const fn from_u64(id: u64) -> Self {
        Self(id)
    }

    /// Devuelve el valor entero del handle. No es estable entre renderers.
    #[must_use]
    pub const fn get(self) -> u64 {
        self.0
    }
}

impl fmt::Display for MeshId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "MeshId({})", self.0)
    }
}

/// Fallo al definir una malla o al extraerla de un documento glTF.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MeshError {
    /// El índice de malla o primitiva no existe en el documento.
    UnknownPrimitive,
    /// El modo de la primitiva no es triangles.
    UnsupportedMode,
    /// Falta un accessor obligatorio o su forma no es la esperada.
    InvalidAccessor(String),
    /// El tipo de componente no se puede convertir a `u32`.
    UnsupportedIndexComponent,
    /// Los índices no son múltiplo de tres, están fuera de rango o no son finitos.
    InvalidIndices,
    /// Los vértices están vacíos, no tienen 8 componentes o contienen valores no finitos.
    InvalidVertices,
    /// El color base no es finito o está fuera de `[0, 1]`.
    InvalidColor,
    /// Se agotaron los identificadores de mallas.
    HandleOverflow,
}

impl fmt::Display for MeshError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UnknownPrimitive => f.write_str("la primitiva no existe en el documento"),
            Self::UnsupportedMode => f.write_str("solo se admiten primitivas triangles"),
            Self::InvalidAccessor(field) => write!(f, "accessor inválido: {field}"),
            Self::UnsupportedIndexComponent => {
                f.write_str("tipo de componente de índice no soportado")
            }
            Self::InvalidIndices => f.write_str("índices inválidos"),
            Self::InvalidVertices => f.write_str("vértices inválidos"),
            Self::InvalidColor => f.write_str("color inválido"),
            Self::HandleOverflow => f.write_str("se agotaron los handles de mallas"),
        }
    }
}

impl Error for MeshError {}

/// Malla poseída: vértices intercalados `(px, py, pz, nx, ny, nz, u, v)`.
#[derive(Debug, Clone, PartialEq)]
struct Mesh {
    vertices: Vec<f32>,
    indices: Vec<u32>,
    color: [f32; 4],
}

/// Una orden de dibujo 3D ya validada, en coordenadas de modelo.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct MeshDraw {
    pub mesh: u64,
    /// Matriz modelo en columna mayor, como array plano de 16 `f32`.
    pub model: [f32; 16],
    pub color: [f32; 4],
}

/// Registra mallas, mantiene la cámara y valida las órdenes de un frame.
#[derive(Debug)]
pub struct Renderer3D {
    meshes: HashMap<u64, Mesh>,
    draws: Vec<MeshDraw>,
    view_projection: [f32; 16],
    next_mesh: u64,
}

impl Default for Renderer3D {
    fn default() -> Self {
        Self::new()
    }
}

impl Renderer3D {
    /// Crea un renderer sin mallas, con cámara identidad y cola vacía.
    #[must_use]
    pub fn new() -> Self {
        Self {
            meshes: HashMap::new(),
            draws: Vec::new(),
            view_projection: identity(),
            next_mesh: 1,
        }
    }

    /// Registra una malla a partir de vértices intercalados e índices `u32`.
    pub fn define_mesh(
        &mut self,
        vertices: &[f32],
        indices: &[u32],
        color: [f32; 4],
    ) -> Result<MeshId, MeshError> {
        if vertices.is_empty()
            || !vertices.len().is_multiple_of(8)
            || !vertices.iter().all(|v| v.is_finite())
        {
            return Err(MeshError::InvalidVertices);
        }
        let vertex_count = vertices.len() / 8;
        if indices.is_empty()
            || !indices.len().is_multiple_of(3)
            || !indices.iter().all(|index| (*index as usize) < vertex_count)
        {
            return Err(MeshError::InvalidIndices);
        }
        if !color
            .iter()
            .all(|channel| channel.is_finite() && (0.0..=1.0).contains(channel))
        {
            return Err(MeshError::InvalidColor);
        }
        if self.next_mesh == u64::MAX {
            return Err(MeshError::HandleOverflow);
        }
        let public_id = self.next_mesh;
        self.next_mesh += 1;
        self.meshes.insert(
            public_id,
            Mesh {
                vertices: vertices.to_vec(),
                indices: indices.to_vec(),
                color,
            },
        );
        Ok(MeshId(public_id))
    }

    /// Extrae una primitiva de un documento glTF y la registra como malla.
    #[cfg(feature = "gltf")]
    pub fn from_gltf(
        &mut self,
        document: &GltfDocument,
        mesh_index: usize,
        primitive_index: usize,
    ) -> Result<MeshId, MeshError> {
        let mesh = document
            .meshes
            .get(mesh_index)
            .ok_or(MeshError::UnknownPrimitive)?;
        let primitive = mesh
            .primitives
            .get(primitive_index)
            .ok_or(MeshError::UnknownPrimitive)?;
        if primitive.mode != PrimitiveMode::Triangles {
            return Err(MeshError::UnsupportedMode);
        }

        let positions = self.accessor(document, &primitive.attributes, AttributeType::Positions)?;
        if positions.shape.elements != AccessorElements::Vec3 || positions.shape.count == 0 {
            return Err(MeshError::InvalidAccessor("POSITION".to_owned()));
        }
        let count = positions.shape.count;
        let normals = optional_accessor(document, &primitive.attributes, AttributeType::Normals);
        if let Some(normals) = normals
            && (normals.shape.elements != AccessorElements::Vec3 || normals.shape.count != count)
        {
            return Err(MeshError::InvalidAccessor("NORMAL".to_owned()));
        }
        let texcoords =
            optional_accessor(document, &primitive.attributes, AttributeType::TexCoords(0));
        if let Some(texcoords) = texcoords
            && (texcoords.shape.elements != AccessorElements::Vec2
                || texcoords.shape.count != count)
        {
            return Err(MeshError::InvalidAccessor("TEXCOORD_0".to_owned()));
        }

        let mut vertices = Vec::with_capacity(count * 8);
        for index in 0..count {
            let position = read_vec(positions, index, 3)?;
            let normal = match normals {
                Some(accessor) => read_vec(accessor, index, 3)?,
                None => [0.0, 0.0, 1.0],
            };
            let uv = match texcoords {
                Some(accessor) => read_vec(accessor, index, 2)?,
                None => [0.0, 0.0, 0.0],
            };
            vertices.extend_from_slice(&[
                position[0],
                position[1],
                position[2],
                normal[0],
                normal[1],
                normal[2],
                uv[0],
                uv[1],
            ]);
        }

        let indices = match primitive.indices {
            Some(accessor_index) => {
                let accessor = document
                    .accessors
                    .get(accessor_index)
                    .ok_or_else(|| MeshError::InvalidAccessor("índices".to_owned()))?;
                if accessor.shape.elements != AccessorElements::Scalar {
                    return Err(MeshError::InvalidAccessor("índices".to_owned()));
                }
                (0..accessor.shape.count)
                    .map(|index| read_index(accessor, index))
                    .collect::<Result<Vec<_>, _>>()?
            }
            None => (0..count as u32).collect(),
        };

        let color = primitive
            .material
            .and_then(|index| document.materials.get(index))
            .map_or([1.0, 1.0, 1.0, 1.0], |material| material.base_color_factor);

        self.define_mesh(&vertices, &indices, color)
    }

    /// Número de vértices de una malla registrada.
    #[must_use]
    pub fn vertex_count(&self, mesh: MeshId) -> Option<usize> {
        self.meshes.get(&mesh.0).map(|data| data.vertices.len() / 8)
    }

    /// Número de índices de una malla registrada.
    #[must_use]
    pub fn index_count(&self, mesh: MeshId) -> Option<usize> {
        self.meshes.get(&mesh.0).map(|data| data.indices.len())
    }

    /// Color base de una malla registrada.
    #[must_use]
    pub fn color(&self, mesh: MeshId) -> Option<[f32; 4]> {
        self.meshes.get(&mesh.0).map(|data| data.color)
    }

    /// Elimina una malla y devuelve si existía. Las órdenes previas se conservan.
    pub fn remove_mesh(&mut self, mesh: MeshId) -> bool {
        self.meshes.remove(&mesh.0).is_some()
    }

    /// Calcula la matriz vista×proyección en columna mayor y devuelve si es válida.
    #[allow(clippy::too_many_arguments)] // vectoriales explícitos: evita una struct de configuración
    pub fn set_camera(
        &mut self,
        eye: [f32; 3],
        target: [f32; 3],
        up: [f32; 3],
        fov: f32,
        aspect: f32,
        near: f32,
        far: f32,
    ) -> bool {
        let valid = [eye, target, up].iter().flatten().all(|v| v.is_finite())
            && [fov, aspect, near, far].iter().all(|v| v.is_finite())
            && fov > 0.0
            && fov < std::f32::consts::PI
            && aspect > 0.0
            && near > 0.0
            && far > near;
        if !valid {
            return false;
        }
        let view = look_at(eye, target, up);
        let projection = perspective(fov, aspect, near, far);
        self.view_projection = multiply(&projection, &view);
        true
    }

    /// Devuelve la matriz vista×proyección actual, en columna mayor.
    #[must_use]
    pub const fn view_projection(&self) -> [f32; 16] {
        self.view_projection
    }

    /// Valida todas las órdenes antes de encolarlas; si alguna falla, no encola ninguna.
    pub fn submit(&mut self, draws: &[MeshDraw]) -> bool {
        if !draws.iter().all(|draw| {
            self.meshes.contains_key(&draw.mesh)
                && draw.model.iter().all(|value| value.is_finite())
                && draw
                    .color
                    .iter()
                    .all(|channel| channel.is_finite() && (0.0..=1.0).contains(channel))
        }) {
            return false;
        }
        self.draws.extend_from_slice(draws);
        true
    }

    /// Descarta las órdenes del frame anterior.
    pub fn begin_frame(&mut self) {
        self.draws.clear();
    }

    /// Órdenes acumuladas en el frame actual.
    #[must_use]
    pub fn draws(&self) -> &[MeshDraw] {
        &self.draws
    }

    #[cfg(feature = "gltf")]
    fn accessor<'a>(
        &self,
        document: &'a GltfDocument,
        attributes: &[(AttributeType, usize)],
        expected: AttributeType,
    ) -> Result<&'a AccessorData, MeshError> {
        let name = format!("{expected:?}");
        optional_accessor(document, attributes, expected).ok_or(MeshError::InvalidAccessor(name))
    }
}

#[cfg(feature = "gltf")]
fn optional_accessor<'a>(
    document: &'a GltfDocument,
    attributes: &[(AttributeType, usize)],
    expected: AttributeType,
) -> Option<&'a AccessorData> {
    attributes
        .iter()
        .find(|(kind, _)| *kind == expected)
        .and_then(|(_, index)| document.accessors.get(*index))
}

#[cfg(feature = "gltf")]
fn component_size(component: AccessorComponent) -> usize {
    match component {
        AccessorComponent::I8 | AccessorComponent::U8 => 1,
        AccessorComponent::I16 | AccessorComponent::U16 => 2,
        AccessorComponent::U32 | AccessorComponent::F32 => 4,
    }
}

#[cfg(feature = "gltf")]
fn elements_count(elements: AccessorElements) -> usize {
    match elements {
        AccessorElements::Scalar => 1,
        AccessorElements::Vec2 => 2,
        AccessorElements::Vec3 => 3,
        AccessorElements::Vec4 => 4,
        AccessorElements::Mat2 => 4,
        AccessorElements::Mat3 => 9,
        AccessorElements::Mat4 => 16,
    }
}

#[cfg(feature = "gltf")]
fn raw_value(accessor: &AccessorData, component: usize) -> Option<f32> {
    let stride = component_size(accessor.shape.component);
    let offset = component.checked_mul(stride)?;
    let bytes = accessor.bytes.get(offset..offset.checked_add(stride)?)?;
    Some(match accessor.shape.component {
        AccessorComponent::F32 => f32::from_le_bytes(bytes.try_into().ok()?),
        AccessorComponent::I8 => i8::from_le_bytes(bytes.try_into().ok()?) as f32,
        AccessorComponent::U8 => bytes[0] as f32,
        AccessorComponent::I16 => i16::from_le_bytes(bytes.try_into().ok()?) as f32,
        AccessorComponent::U16 => u16::from_le_bytes(bytes.try_into().ok()?) as f32,
        AccessorComponent::U32 => u32::from_le_bytes(bytes.try_into().ok()?) as f32,
    })
}

#[cfg(feature = "gltf")]
fn read_vec(
    accessor: &AccessorData,
    index: usize,
    components: usize,
) -> Result<[f32; 3], MeshError> {
    let stride = elements_count(accessor.shape.elements);
    let mut result = [0.0; 3];
    for (component, slot) in result.iter_mut().enumerate().take(components) {
        let value = raw_value(accessor, index * stride + component)
            .ok_or_else(|| MeshError::InvalidAccessor("fuera de rango".to_owned()))?;
        if !value.is_finite() {
            return Err(MeshError::InvalidAccessor("valor no finito".to_owned()));
        }
        *slot = value;
    }
    Ok(result)
}

#[cfg(feature = "gltf")]
fn read_index(accessor: &AccessorData, index: usize) -> Result<u32, MeshError> {
    let value = raw_value(accessor, index)
        .ok_or_else(|| MeshError::InvalidAccessor("índice fuera de rango".to_owned()))?;
    if !value.is_finite() || value < 0.0 || value.fract() != 0.0 {
        return Err(MeshError::InvalidIndices);
    }
    Ok(value as u32)
}

fn identity() -> [f32; 16] {
    [
        1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0,
    ]
}

fn normalize(vector: [f32; 3]) -> [f32; 3] {
    let length = (vector[0] * vector[0] + vector[1] * vector[1] + vector[2] * vector[2]).sqrt();
    if length == 0.0 {
        return [0.0, 0.0, 0.0];
    }
    [vector[0] / length, vector[1] / length, vector[2] / length]
}

fn cross(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
    [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ]
}

fn dot(a: [f32; 3], b: [f32; 3]) -> f32 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}

fn look_at(eye: [f32; 3], target: [f32; 3], up: [f32; 3]) -> [f32; 16] {
    let forward = normalize([target[0] - eye[0], target[1] - eye[1], target[2] - eye[2]]);
    let side = normalize(cross(forward, up));
    let camera_up = cross(side, forward);
    [
        side[0],
        camera_up[0],
        -forward[0],
        0.0,
        side[1],
        camera_up[1],
        -forward[1],
        0.0,
        side[2],
        camera_up[2],
        -forward[2],
        0.0,
        -dot(side, eye),
        -dot(camera_up, eye),
        dot(forward, eye),
        1.0,
    ]
}

fn perspective(fov: f32, aspect: f32, near: f32, far: f32) -> [f32; 16] {
    let focal = 1.0 / (fov * 0.5).tan();
    let depth = (far + near) / (near - far);
    [
        focal / aspect,
        0.0,
        0.0,
        0.0,
        0.0,
        focal,
        0.0,
        0.0,
        0.0,
        0.0,
        depth,
        -1.0,
        0.0,
        0.0,
        2.0 * far * near / (near - far),
        0.0,
    ]
}

/// Multiplica dos matrices en columna mayor: `left * right`.
fn multiply(left: &[f32; 16], right: &[f32; 16]) -> [f32; 16] {
    let mut result = [0.0; 16];
    for column in 0..4 {
        for row in 0..4 {
            let mut value = 0.0;
            for k in 0..4 {
                value += left[k * 4 + row] * right[column * 4 + k];
            }
            result[column * 4 + row] = value;
        }
    }
    result
}

/// Carga un documento glTF desde una ruta, para pruebas y consumo de la bridge.
#[cfg(feature = "gltf")]
pub fn load_gltf(path: &Path) -> Result<GltfDocument, lunar_assets::gltf::GltfLoadError> {
    let mut store = GltfStore::new();
    let handle = store.load_from_path(path)?;
    store
        .get(handle)
        .cloned()
        .ok_or(lunar_assets::gltf::GltfLoadError::HandleOverflow)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn triangle() -> ([f32; 24], Vec<u32>) {
        (
            [
                0.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0,
                0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0, 0.0,
            ],
            vec![0, 1, 2],
        )
    }

    #[test]
    fn meshes_are_validated_and_handles_are_opaque() {
        let mut renderer = Renderer3D::new();
        let (vertices, indices) = triangle();
        let mesh = renderer
            .define_mesh(&vertices, &indices, [1.0, 0.5, 0.0, 1.0])
            .expect("malla válida");
        assert_eq!(mesh.get(), 1);
        assert_eq!(renderer.vertex_count(mesh), Some(3));
        assert_eq!(renderer.index_count(mesh), Some(3));
        assert_eq!(renderer.color(mesh), Some([1.0, 0.5, 0.0, 1.0]));

        assert_eq!(
            renderer.define_mesh(&vertices[..7], &indices, [1.0; 4]),
            Err(MeshError::InvalidVertices)
        );
        assert_eq!(
            renderer.define_mesh(&vertices, &[0, 1], [1.0; 4]),
            Err(MeshError::InvalidIndices)
        );
        assert_eq!(
            renderer.define_mesh(&vertices, &[0, 1, 9], [1.0; 4]),
            Err(MeshError::InvalidIndices)
        );
        assert_eq!(
            renderer.define_mesh(&vertices, &indices, [f32::NAN; 4]),
            Err(MeshError::InvalidColor)
        );
        assert!(renderer.remove_mesh(mesh));
        assert!(!renderer.remove_mesh(mesh));
    }

    #[test]
    fn draws_are_atomic_and_frame_scoped() {
        let mut renderer = Renderer3D::new();
        let (vertices, indices) = triangle();
        let mesh = renderer.define_mesh(&vertices, &indices, [1.0; 4]).unwrap();
        let good = MeshDraw {
            mesh: mesh.get(),
            model: identity(),
            color: [1.0; 4],
        };
        assert!(renderer.submit(&[good]));
        assert_eq!(renderer.draws().len(), 1);

        let bad = MeshDraw { mesh: 999, ..good };
        assert!(!renderer.submit(&[good, bad]));
        assert_eq!(renderer.draws().len(), 1);

        let non_finite = MeshDraw {
            model: [f32::NAN; 16],
            ..good
        };
        assert!(!renderer.submit(&[non_finite]));
        renderer.begin_frame();
        assert!(renderer.draws().is_empty());
    }

    #[test]
    fn camera_rejects_invalid_inputs_and_builds_a_finite_matrix() {
        let mut renderer = Renderer3D::new();
        assert!(!renderer.set_camera(
            [0.0; 3],
            [0.0, 0.0, -1.0],
            [0.0, 1.0, 0.0],
            1.0,
            0.0,
            0.1,
            10.0
        ));
        assert_eq!(renderer.view_projection(), identity());
        assert!(renderer.set_camera(
            [0.0, 0.0, 3.0],
            [0.0; 3],
            [0.0, 1.0, 0.0],
            1.0,
            1.5,
            0.1,
            10.0
        ));
        assert!(
            renderer
                .view_projection()
                .iter()
                .all(|value| value.is_finite())
        );
        assert_ne!(renderer.view_projection(), identity());
    }

    #[cfg(feature = "gltf")]
    #[test]
    fn gltf_triangle_becomes_a_valid_mesh() {
        let path = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../examples/scene_demo/assets/triangle.glb");
        let document = load_gltf(&path).expect("glTF de ejemplo");
        let mut renderer = Renderer3D::new();
        let mesh = renderer
            .from_gltf(&document, 0, 0)
            .expect("primitiva del triángulo");
        assert!(renderer.vertex_count(mesh).unwrap_or_default() >= 3);
        assert_eq!(renderer.index_count(mesh).unwrap_or_default() % 3, 0);
        assert!(renderer.from_gltf(&document, 99, 0).is_err());
    }
}
