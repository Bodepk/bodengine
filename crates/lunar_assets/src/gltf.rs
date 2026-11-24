//! Carga de documentos glTF 2.0 sin ventana, GPU ni reloj (H7).
//!
//! `gltf` e `image` quedan confinados en `lunar_assets` (ADR-012, ADR-013).
//! El resultado es una instantánea poseída: no quedan tipos de `gltf` ni de
//! `image` en la API pública, y las referencias son índices locales al
//! documento.

use std::collections::HashMap;
use std::error::Error;
use std::fmt;
use std::path::Path;

use crate::{ImageData, ImageLoadError, validate_dimensions};

/// Identificador opaco de un documento glTF dentro de un [`GltfStore`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct GltfHandle(u64);

impl GltfHandle {
    /// Devuelve el valor entero del handle. No es estable entre stores.
    #[must_use]
    pub const fn get(self) -> u64 {
        self.0
    }
}

impl fmt::Display for GltfHandle {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "GltfHandle({})", self.0)
    }
}

/// Errores de carga de glTF, sin exponer `gltf`, `image` ni `std::io` en la API.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GltfLoadError {
    /// El documento no es glTF 2.0 válido o no se pudo interpretar.
    Parse(String),
    /// No se pudo leer un archivo del disco.
    Io(String),
    /// El documento usa una característica no soportada por este cargador.
    Unsupported(String),
    /// Una imagen no se pudo convertir a RGBA8.
    InvalidImage(String),
    /// Una imagen excede los límites de ADR-012.
    TooLarge { width: u32, height: u32 },
    /// Se agotaron los identificadores de este store.
    HandleOverflow,
}

impl fmt::Display for GltfLoadError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Parse(message) => write!(f, "error parseando glTF: {message}"),
            Self::Io(message) => write!(f, "error leyendo glTF: {message}"),
            Self::Unsupported(message) => write!(f, "glTF no soportado: {message}"),
            Self::InvalidImage(message) => write!(f, "imagen glTF inválida: {message}"),
            Self::TooLarge { width, height } => {
                write!(f, "imagen glTF demasiado grande: {width}×{height}")
            }
            Self::HandleOverflow => write!(f, "se agotaron los handles de glTF"),
        }
    }
}

impl Error for GltfLoadError {}

/// Tipo de componente numérico de un accessor.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AccessorComponent {
    I8,
    U8,
    I16,
    U16,
    U32,
    F32,
}

/// Número de componentes por elemento de un accessor.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AccessorElements {
    Scalar,
    Vec2,
    Vec3,
    Vec4,
    Mat2,
    Mat3,
    Mat4,
}

/// Forma lógica de un accessor; los bytes siempre quedan contiguos.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AccessorShape {
    pub component: AccessorComponent,
    pub elements: AccessorElements,
    pub count: usize,
}

/// Datos de un accessor copiados a un buffer contiguo, sin `byte_stride`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AccessorData {
    pub shape: AccessorShape,
    pub bytes: Vec<u8>,
}

impl AccessorData {
    #[must_use]
    pub fn as_bytes(&self) -> &[u8] {
        &self.bytes
    }
}

/// Semántica de un atributo de vértice.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AttributeType {
    Positions,
    Normals,
    Tangents,
    Colors(u32),
    TexCoords(u32),
    Joints(u32),
    Weights(u32),
    Other(String),
}

/// Modo de topología de una primitiva.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PrimitiveMode {
    Points,
    Lines,
    LineLoop,
    LineStrip,
    Triangles,
    TriangleStrip,
    TriangleFan,
}

/// Una primitiva de malla; índices y atributos apuntan a [`GltfDocument::accessors`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PrimitiveData {
    pub attributes: Vec<(AttributeType, usize)>,
    pub indices: Option<usize>,
    pub material: Option<usize>,
    pub mode: PrimitiveMode,
}

/// Malla y sus primitivas.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MeshData {
    pub name: Option<String>,
    pub primitives: Vec<PrimitiveData>,
}

/// Nodo con transformación ya convertida a matriz 4×4 (columna mayor).
#[derive(Debug, Clone, PartialEq)]
pub struct NodeData {
    pub name: Option<String>,
    pub matrix: [[f32; 4]; 4],
    pub children: Vec<usize>,
    pub mesh: Option<usize>,
}

/// Escena raíz; `roots` apunta a [`GltfDocument::nodes`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SceneData {
    pub name: Option<String>,
    pub roots: Vec<usize>,
}

/// Modo de mezcla de un material.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AlphaMode {
    Opaque,
    Mask,
    Blend,
}

/// Textura; `source` apunta a [`GltfDocument::images`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TextureData {
    pub name: Option<String>,
    pub source: usize,
}

/// Material PBR metálico-rugoso; las texturas son índices locales.
#[derive(Debug, Clone, PartialEq)]
pub struct MaterialData {
    pub name: Option<String>,
    pub base_color_factor: [f32; 4],
    pub metallic_factor: f32,
    pub roughness_factor: f32,
    pub emissive_factor: [f32; 3],
    pub alpha_mode: AlphaMode,
    pub alpha_cutoff: Option<f32>,
    pub double_sided: bool,
    pub base_color_texture: Option<usize>,
    pub metallic_roughness_texture: Option<usize>,
    pub normal_texture: Option<usize>,
    pub occlusion_texture: Option<usize>,
    pub emissive_texture: Option<usize>,
}

/// Instantánea poseída de un documento glTF.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct GltfDocument {
    pub scenes: Vec<SceneData>,
    pub nodes: Vec<NodeData>,
    pub meshes: Vec<MeshData>,
    pub materials: Vec<MaterialData>,
    pub textures: Vec<TextureData>,
    pub images: Vec<ImageData>,
    pub accessors: Vec<AccessorData>,
}

/// Almacena documentos glTF y entrega handles opacos locales.
///
/// No hay registro global ni caché ruta→handle: esa política depende del
/// formato de escenas que se fijará en ADR-008 antes de H9.
#[derive(Debug, Default)]
pub struct GltfStore {
    next_handle: u64,
    documents: HashMap<GltfHandle, GltfDocument>,
}

impl GltfStore {
    /// Crea un store vacío.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Importa un GLB autocontenido desde memoria.
    ///
    /// Se rechazan referencias externas para evitar depender del directorio
    /// de trabajo; los datos `data:` embebidos sí se admiten.
    pub fn load_from_slice(&mut self, bytes: &[u8]) -> Result<GltfHandle, GltfLoadError> {
        let gltf = gltf::Gltf::from_slice(bytes).map_err(map_import_error)?;
        ensure_self_contained(&gltf)?;
        let (document, buffers, images) = gltf::import_slice(bytes).map_err(map_import_error)?;
        self.insert(build_document(&document, &buffers, &images)?)
    }

    /// Importa un `.gltf`/`.glb` y resuelve recursos externos relativos a él.
    pub fn load_from_path<P: AsRef<Path>>(&mut self, path: P) -> Result<GltfHandle, GltfLoadError> {
        let (document, buffers, images) = gltf::import(path).map_err(map_import_error)?;
        self.insert(build_document(&document, &buffers, &images)?)
    }

    /// Devuelve un documento previamente cargado, si el handle pertenece a este store.
    #[must_use]
    pub fn get(&self, handle: GltfHandle) -> Option<&GltfDocument> {
        self.documents.get(&handle)
    }

    /// Elimina un documento. El handle nunca se reutiliza.
    pub fn remove(&mut self, handle: GltfHandle) -> Option<GltfDocument> {
        self.documents.remove(&handle)
    }

    /// Número de documentos almacenados.
    #[must_use]
    pub fn len(&self) -> usize {
        self.documents.len()
    }

    /// Indica si el store no contiene documentos.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.documents.is_empty()
    }

    fn insert(&mut self, document: GltfDocument) -> Result<GltfHandle, GltfLoadError> {
        let next = self
            .next_handle
            .checked_add(1)
            .ok_or(GltfLoadError::HandleOverflow)?;
        let handle = GltfHandle(next);
        self.next_handle = next;
        self.documents.insert(handle, document);
        Ok(handle)
    }
}

fn map_import_error(error: gltf::Error) -> GltfLoadError {
    if matches!(error, gltf::Error::Io(_)) {
        GltfLoadError::Io(error.to_string())
    } else {
        GltfLoadError::Parse(error.to_string())
    }
}

fn ensure_self_contained(gltf: &gltf::Gltf) -> Result<(), GltfLoadError> {
    let document = &gltf.document;
    for buffer in document.buffers() {
        if let gltf::buffer::Source::Uri(uri) = buffer.source()
            && !uri.starts_with("data:")
        {
            return Err(GltfLoadError::Unsupported(format!(
                "buffer externo en carga por bytes: {uri}"
            )));
        }
    }
    for image in document.images() {
        if let gltf::image::Source::Uri { uri, .. } = image.source()
            && !uri.starts_with("data:")
        {
            return Err(GltfLoadError::Unsupported(format!(
                "imagen externa en carga por bytes: {uri}"
            )));
        }
    }
    Ok(())
}

fn build_document(
    document: &gltf::Document,
    buffers: &[gltf::buffer::Data],
    imported_images: &[gltf::image::Data],
) -> Result<GltfDocument, GltfLoadError> {
    let accessors = document
        .accessors()
        .map(|accessor| copy_accessor(&accessor, buffers))
        .collect::<Result<Vec<_>, _>>()?;
    let images = imported_images
        .iter()
        .map(to_rgba8)
        .collect::<Result<Vec<_>, _>>()?;
    let textures = document
        .textures()
        .map(|texture| TextureData {
            name: texture.name().map(str::to_owned),
            source: texture.source().index(),
        })
        .collect();
    let materials = document
        .materials()
        .map(|material| read_material(&material))
        .collect::<Result<Vec<_>, _>>()?;
    let meshes = document
        .meshes()
        .map(|mesh| {
            Ok(MeshData {
                name: mesh.name().map(str::to_owned),
                primitives: mesh
                    .primitives()
                    .map(|primitive| {
                        Ok(PrimitiveData {
                            attributes: primitive
                                .attributes()
                                .map(|(semantic, accessor)| {
                                    (map_semantic(semantic), accessor.index())
                                })
                                .collect(),
                            indices: primitive.indices().map(|accessor| accessor.index()),
                            material: primitive.material().index(),
                            mode: map_mode(primitive.mode())?,
                        })
                    })
                    .collect::<Result<Vec<_>, _>>()?,
            })
        })
        .collect::<Result<Vec<_>, _>>()?;
    let nodes = document
        .nodes()
        .map(|node| NodeData {
            name: node.name().map(str::to_owned),
            matrix: node.transform().matrix(),
            children: node.children().map(|child| child.index()).collect(),
            mesh: node.mesh().map(|mesh| mesh.index()),
        })
        .collect();
    let scenes = document
        .scenes()
        .map(|scene| SceneData {
            name: scene.name().map(str::to_owned),
            roots: scene.nodes().map(|node| node.index()).collect(),
        })
        .collect();

    Ok(GltfDocument {
        scenes,
        nodes,
        meshes,
        materials,
        textures,
        images,
        accessors,
    })
}

fn map_component(component: gltf::accessor::DataType) -> AccessorComponent {
    match component {
        gltf::accessor::DataType::I8 => AccessorComponent::I8,
        gltf::accessor::DataType::U8 => AccessorComponent::U8,
        gltf::accessor::DataType::I16 => AccessorComponent::I16,
        gltf::accessor::DataType::U16 => AccessorComponent::U16,
        gltf::accessor::DataType::U32 => AccessorComponent::U32,
        gltf::accessor::DataType::F32 => AccessorComponent::F32,
    }
}

fn map_elements(dimensions: gltf::accessor::Dimensions) -> AccessorElements {
    match dimensions {
        gltf::accessor::Dimensions::Scalar => AccessorElements::Scalar,
        gltf::accessor::Dimensions::Vec2 => AccessorElements::Vec2,
        gltf::accessor::Dimensions::Vec3 => AccessorElements::Vec3,
        gltf::accessor::Dimensions::Vec4 => AccessorElements::Vec4,
        gltf::accessor::Dimensions::Mat2 => AccessorElements::Mat2,
        gltf::accessor::Dimensions::Mat3 => AccessorElements::Mat3,
        gltf::accessor::Dimensions::Mat4 => AccessorElements::Mat4,
    }
}

fn copy_accessor(
    accessor: &gltf::Accessor<'_>,
    buffers: &[gltf::buffer::Data],
) -> Result<AccessorData, GltfLoadError> {
    if accessor.sparse().is_some() {
        return Err(GltfLoadError::Unsupported(format!(
            "accessor {} usa almacenamiento sparse",
            accessor.index()
        )));
    }

    let view = accessor.view().ok_or_else(|| {
        GltfLoadError::Unsupported(format!("accessor {} sin buffer view", accessor.index()))
    })?;
    let buffer = buffers
        .get(view.buffer().index())
        .ok_or_else(|| GltfLoadError::Parse(format!("buffer {} ausente", view.buffer().index())))?;

    let element_size = accessor.size();
    let count = accessor.count();
    let stride = view.stride().unwrap_or(element_size);
    if stride < element_size {
        return Err(GltfLoadError::Unsupported(format!(
            "stride {} menor que el tamaño de elemento {element_size} en accessor {}",
            stride,
            accessor.index()
        )));
    }

    let start = view
        .offset()
        .checked_add(accessor.offset())
        .ok_or_else(|| GltfLoadError::Parse("desbordamiento de offset".to_owned()))?;
    let length = count.checked_mul(stride).ok_or_else(|| {
        GltfLoadError::Parse(format!("accessor {} demasiado grande", accessor.index()))
    })?;
    let end = start.checked_add(length).ok_or_else(|| {
        GltfLoadError::Parse(format!("accessor {} fuera de rango", accessor.index()))
    })?;
    if end > buffer.0.len() {
        return Err(GltfLoadError::Parse(format!(
            "accessor {} fuera de los {} bytes del buffer",
            accessor.index(),
            buffer.0.len()
        )));
    }

    let mut bytes = Vec::with_capacity(count.saturating_mul(element_size));
    for index in 0..count {
        let offset = start + index * stride;
        bytes.extend_from_slice(&buffer.0[offset..offset + element_size]);
    }

    Ok(AccessorData {
        shape: AccessorShape {
            component: map_component(accessor.data_type()),
            elements: map_elements(accessor.dimensions()),
            count,
        },
        bytes,
    })
}

fn to_rgba8(image: &gltf::image::Data) -> Result<ImageData, GltfLoadError> {
    validate_dimensions(image.width, image.height).map_err(|error| match error {
        ImageLoadError::TooLarge { width, height } => GltfLoadError::TooLarge { width, height },
        other => GltfLoadError::InvalidImage(other.to_string()),
    })?;

    let channels = match image.format {
        gltf::image::Format::R8 => 1,
        gltf::image::Format::R8G8 => 2,
        gltf::image::Format::R8G8B8 => 3,
        gltf::image::Format::R8G8B8A8 => 4,
        other => {
            return Err(GltfLoadError::Unsupported(format!(
                "formato de imagen {other:?} no es RGBA8 de 8 bits"
            )));
        }
    };

    let pixels = usize::try_from(image.width)
        .ok()
        .and_then(|width| {
            usize::try_from(image.height)
                .ok()
                .and_then(|height| width.checked_mul(height))
        })
        .and_then(|count| count.checked_mul(channels))
        .ok_or(GltfLoadError::TooLarge {
            width: image.width,
            height: image.height,
        })?;
    if image.pixels.len() < pixels {
        return Err(GltfLoadError::InvalidImage(format!(
            "se esperaban {pixels} bytes y hay {}",
            image.pixels.len()
        )));
    }

    let mut rgba = Vec::with_capacity(pixels / channels * 4);
    for pixel in image.pixels[..pixels].chunks_exact(channels) {
        match channels {
            1 => rgba.extend_from_slice(&[pixel[0], pixel[0], pixel[0], 255]),
            2 => rgba.extend_from_slice(&[pixel[0], pixel[0], pixel[0], pixel[1]]),
            3 => rgba.extend_from_slice(&[pixel[0], pixel[1], pixel[2], 255]),
            _ => rgba.extend_from_slice(&[pixel[0], pixel[1], pixel[2], pixel[3]]),
        }
    }

    Ok(ImageData {
        width: image.width,
        height: image.height,
        pixels: rgba,
    })
}

fn read_material(material: &gltf::Material<'_>) -> Result<MaterialData, GltfLoadError> {
    let pbr = material.pbr_metallic_roughness();
    let texture_index = |texture: Option<gltf::Texture<'_>>| texture.map(|texture| texture.index());
    Ok(MaterialData {
        name: material.name().map(str::to_owned),
        base_color_factor: pbr.base_color_factor(),
        metallic_factor: pbr.metallic_factor(),
        roughness_factor: pbr.roughness_factor(),
        emissive_factor: material.emissive_factor(),
        alpha_mode: match material.alpha_mode() {
            gltf::material::AlphaMode::Opaque => AlphaMode::Opaque,
            gltf::material::AlphaMode::Mask => AlphaMode::Mask,
            gltf::material::AlphaMode::Blend => AlphaMode::Blend,
        },
        alpha_cutoff: material.alpha_cutoff(),
        double_sided: material.double_sided(),
        base_color_texture: texture_index(pbr.base_color_texture().map(|info| info.texture())),
        metallic_roughness_texture: texture_index(
            pbr.metallic_roughness_texture().map(|info| info.texture()),
        ),
        normal_texture: texture_index(material.normal_texture().map(|info| info.texture())),
        occlusion_texture: texture_index(material.occlusion_texture().map(|info| info.texture())),
        emissive_texture: texture_index(material.emissive_texture().map(|info| info.texture())),
    })
}

#[allow(unreachable_patterns)] // `extras` puede añadir semánticas con otras features.
fn map_semantic(semantic: gltf::mesh::Semantic) -> AttributeType {
    match semantic {
        gltf::mesh::Semantic::Positions => AttributeType::Positions,
        gltf::mesh::Semantic::Normals => AttributeType::Normals,
        gltf::mesh::Semantic::Tangents => AttributeType::Tangents,
        gltf::mesh::Semantic::Colors(index) => AttributeType::Colors(index),
        gltf::mesh::Semantic::TexCoords(index) => AttributeType::TexCoords(index),
        gltf::mesh::Semantic::Joints(index) => AttributeType::Joints(index),
        gltf::mesh::Semantic::Weights(index) => AttributeType::Weights(index),
        other => AttributeType::Other(format!("{other:?}")),
    }
}

#[allow(unreachable_patterns)] // Modos futuros se rechazan explícitamente.
fn map_mode(mode: gltf::mesh::Mode) -> Result<PrimitiveMode, GltfLoadError> {
    match mode {
        gltf::mesh::Mode::Points => Ok(PrimitiveMode::Points),
        gltf::mesh::Mode::Lines => Ok(PrimitiveMode::Lines),
        gltf::mesh::Mode::LineLoop => Ok(PrimitiveMode::LineLoop),
        gltf::mesh::Mode::LineStrip => Ok(PrimitiveMode::LineStrip),
        gltf::mesh::Mode::Triangles => Ok(PrimitiveMode::Triangles),
        gltf::mesh::Mode::TriangleStrip => Ok(PrimitiveMode::TriangleStrip),
        gltf::mesh::Mode::TriangleFan => Ok(PrimitiveMode::TriangleFan),
        other => Err(GltfLoadError::Unsupported(format!(
            "modo de primitiva desconocido: {other:?}"
        ))),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;
    use std::path::PathBuf;
    use std::sync::atomic::{AtomicU64, Ordering};

    static TEMP_COUNTER: AtomicU64 = AtomicU64::new(0);

    fn glb(json: &str, bin: &[u8]) -> Vec<u8> {
        let mut json_chunk = json.as_bytes().to_vec();
        while !json_chunk.len().is_multiple_of(4) {
            json_chunk.push(b' ');
        }
        let mut bin_chunk = bin.to_vec();
        while !bin_chunk.len().is_multiple_of(4) {
            bin_chunk.push(0);
        }

        let total = 12 + 8 + json_chunk.len() + 8 + bin_chunk.len();
        let mut bytes = Vec::with_capacity(total);
        bytes.extend_from_slice(&0x46546c67u32.to_le_bytes());
        bytes.extend_from_slice(&2u32.to_le_bytes());
        bytes.extend_from_slice(&(total as u32).to_le_bytes());
        bytes.extend_from_slice(&(json_chunk.len() as u32).to_le_bytes());
        bytes.extend_from_slice(&0x4e4f534au32.to_le_bytes());
        bytes.extend_from_slice(&json_chunk);
        bytes.extend_from_slice(&(bin_chunk.len() as u32).to_le_bytes());
        bytes.extend_from_slice(&0x004e4942u32.to_le_bytes());
        bytes.extend_from_slice(&bin_chunk);
        bytes
    }

    fn triangle_bin() -> Vec<u8> {
        let mut bytes = Vec::new();
        for (x, y) in [(0.0_f32, 0.0_f32), (1.0, 0.0), (0.0, 1.0)] {
            bytes.extend_from_slice(&x.to_le_bytes());
            bytes.extend_from_slice(&y.to_le_bytes());
            bytes.extend_from_slice(&0.0_f32.to_le_bytes());
        }
        bytes.extend_from_slice(&0u16.to_le_bytes());
        bytes.extend_from_slice(&1u16.to_le_bytes());
        bytes.extend_from_slice(&2u16.to_le_bytes());
        bytes
    }

    fn triangle_glb() -> Vec<u8> {
        glb(
            r#"{
                "asset": {"version": "2.0"},
                "scene": 0,
                "scenes": [{"name": "Root", "nodes": [0]}],
                "nodes": [{"name": "Tri", "mesh": 0, "translation": [1, 2, 3]}],
                "meshes": [{"name": "Mesh", "primitives": [{
                    "attributes": {"POSITION": 0}, "indices": 1, "material": 0, "mode": 4
                }]}],
                "materials": [{"name": "Mat", "alphaMode": "OPAQUE",
                    "pbrMetallicRoughness": {"baseColorFactor": [1, 0, 0, 1]}
                }],
                "accessors": [
                    {"bufferView": 0, "componentType": 5126, "count": 3, "type": "VEC3",
                     "min": [0, 0, 0], "max": [1, 1, 0]},
                    {"bufferView": 1, "componentType": 5123, "count": 3, "type": "SCALAR"}
                ],
                "bufferViews": [
                    {"buffer": 0, "byteOffset": 0, "byteLength": 36},
                    {"buffer": 0, "byteOffset": 36, "byteLength": 6}
                ],
                "buffers": [{"byteLength": 42}]
            }"#,
            &triangle_bin(),
        )
    }

    fn sample_png() -> Vec<u8> {
        let image = image::RgbaImage::from_raw(1, 1, vec![255, 0, 0, 255]).expect("valid image");
        let mut cursor = Cursor::new(Vec::new());
        image::DynamicImage::ImageRgba8(image)
            .write_to(&mut cursor, image::ImageFormat::Png)
            .expect("encode PNG");
        cursor.into_inner()
    }

    fn temp_glb_path() -> PathBuf {
        let counter = TEMP_COUNTER.fetch_add(1, Ordering::Relaxed);
        std::env::temp_dir().join(format!("lunar_gltf_{}_{counter}.glb", std::process::id()))
    }

    #[test]
    fn load_from_slice_reads_scene_node_mesh_and_accessors() {
        let mut store = GltfStore::new();
        let handle = store
            .load_from_slice(&triangle_glb())
            .expect("import triangle GLB");

        assert_eq!(handle.get(), 1);
        assert_eq!(store.len(), 1);
        let document = store.get(handle).expect("stored document");
        assert_eq!(document.scenes.len(), 1);
        assert_eq!(document.scenes[0].name.as_deref(), Some("Root"));
        assert_eq!(document.scenes[0].roots, vec![0]);
        assert_eq!(document.nodes[0].name.as_deref(), Some("Tri"));
        assert_eq!(document.nodes[0].matrix[3][0..3], [1.0, 2.0, 3.0]);
        assert_eq!(document.nodes[0].mesh, Some(0));
        assert_eq!(
            document.meshes[0].primitives[0].mode,
            PrimitiveMode::Triangles
        );

        let primitive = &document.meshes[0].primitives[0];
        assert_eq!(primitive.attributes, vec![(AttributeType::Positions, 0)]);
        assert_eq!(primitive.indices, Some(1));
        assert_eq!(primitive.material, Some(0));
        assert_eq!(document.accessors[0].shape.count, 3);
        assert_eq!(
            document.accessors[0].shape.component,
            AccessorComponent::F32
        );
        assert_eq!(document.accessors[0].shape.elements, AccessorElements::Vec3);
        assert_eq!(document.accessors[1].as_bytes(), [0, 0, 1, 0, 2, 0]);
        assert_eq!(document.materials[0].alpha_mode, AlphaMode::Opaque);
    }

    #[test]
    fn load_from_path_reads_file() {
        let path = temp_glb_path();
        std::fs::write(&path, triangle_glb()).expect("write GLB");
        let mut store = GltfStore::new();

        let handle = store.load_from_path(&path).expect("load GLB from path");
        assert_eq!(store.get(handle).expect("stored document").meshes.len(), 1);

        std::fs::remove_file(&path).expect("remove GLB");
    }

    #[test]
    fn failures_do_not_consume_handles() {
        let mut store = GltfStore::new();
        let error = store
            .load_from_slice(b"not a glb")
            .expect_err("invalid GLB");
        assert!(matches!(error, GltfLoadError::Parse(_)));
        assert!(store.is_empty());

        let handle = store
            .load_from_slice(&triangle_glb())
            .expect("valid GLB after failure");
        assert_eq!(handle.get(), 1);
    }

    #[test]
    fn sparse_accessors_are_rejected() {
        let bytes = glb(
            r#"{
                "asset": {"version": "2.0"},
                "accessors": [{
                    "bufferView": 0, "componentType": 5126, "count": 1, "type": "VEC3",
                    "sparse": {
                        "count": 1,
                        "indices": {"bufferView": 1, "componentType": 5123},
                        "values": {"bufferView": 1, "byteOffset": 2}
                    }
                }],
                "bufferViews": [
                    {"buffer": 0, "byteOffset": 0, "byteLength": 12},
                    {"buffer": 0, "byteOffset": 12, "byteLength": 8}
                ],
                "buffers": [{"byteLength": 20}]
            }"#,
            &[0u8; 20],
        );
        let mut store = GltfStore::new();
        let error = store.load_from_slice(&bytes).expect_err("sparse accessor");
        assert!(matches!(error, GltfLoadError::Unsupported(_)));
        assert!(store.is_empty());
    }

    #[test]
    fn external_references_are_rejected_in_slice_imports() {
        let json = r#"{
            "asset": {"version": "2.0"},
            "buffers": [{"uri": "external.bin", "byteLength": 4}]
        }"#;
        let mut store = GltfStore::new();
        let error = store
            .load_from_slice(json.as_bytes())
            .expect_err("external reference");
        assert!(matches!(error, GltfLoadError::Unsupported(_)));
    }

    #[test]
    fn embedded_png_becomes_rgba8_image() {
        let png = sample_png();
        let bin_len = 36 + png.len();
        let json = format!(
            r#"{{
                "asset": {{"version": "2.0"}},
                "meshes": [{{"primitives": [{{"attributes": {{"POSITION": 0}}, "material": 0}}]}}],
                "materials": [{{"pbrMetallicRoughness": {{"baseColorTexture": {{"index": 0}}}}}}],
                "textures": [{{"source": 0}}],
                "images": [{{"bufferView": 1, "mimeType": "image/png"}}],
                "accessors": [
                    {{"bufferView": 0, "componentType": 5126, "count": 3, "type": "VEC3",
                      "min": [0, 0, 0], "max": [1, 1, 0]}}
                ],
                "bufferViews": [
                    {{"buffer": 0, "byteOffset": 0, "byteLength": 36}},
                    {{"buffer": 0, "byteOffset": 36, "byteLength": {}}}
                ],
                "buffers": [{{"byteLength": {}}}]
            }}"#,
            png.len(),
            bin_len
        );
        let mut bin = vec![0u8; 36];
        bin.extend_from_slice(&png);
        let mut store = GltfStore::new();

        let handle = store
            .load_from_slice(&glb(&json, &bin))
            .expect("import PNG in GLB");
        let document = store.get(handle).expect("stored document");
        assert_eq!(document.images.len(), 1);
        assert_eq!(
            (document.images[0].width, document.images[0].height),
            (1, 1)
        );
        assert_eq!(document.images[0].as_rgba(), [255, 0, 0, 255]);
        assert_eq!(document.textures[0].source, 0);
        assert_eq!(document.materials[0].base_color_texture, Some(0));
    }

    #[test]
    fn removal_drops_document_but_does_not_reuse_handle() {
        let mut store = GltfStore::new();
        let first = store
            .load_from_slice(&triangle_glb())
            .expect("first document");
        let removed = store.remove(first).expect("remove document");
        assert_eq!(removed.meshes.len(), 1);
        assert!(store.get(first).is_none());

        let second = store
            .load_from_slice(&triangle_glb())
            .expect("second document");
        assert_eq!(second.get(), 2);
    }
}
