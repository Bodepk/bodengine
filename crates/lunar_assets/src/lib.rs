//! Carga y almacenamiento de imágenes sin ventana ni GPU (H6).
//!
//! `image` queda confinado en este crate (ADR-012). Por ahora solo se
//! decodifica PNG; el formato de referencias en escenas se decide en ADR-008.

use std::collections::HashMap;
use std::error::Error;
use std::fmt;
use std::path::Path;

/// Lado máximo permitido para una imagen decodificada.
pub const MAX_DIMENSION: u32 = 16_384;

/// Número máximo de píxeles por imagen; equivale a 4096 × 4096.
pub const MAX_PIXELS: usize = 16_777_216;

/// Identificador opaco de una imagen dentro de un [`ImageStore`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ImageHandle(u64);

impl ImageHandle {
    /// Devuelve el valor entero del handle. No es estable entre stores.
    #[must_use]
    pub const fn get(self) -> u64 {
        self.0
    }
}

impl fmt::Display for ImageHandle {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "ImageHandle({})", self.0)
    }
}

/// Errores de carga de imágenes, sin exponer `image` ni `std::io` en la API.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ImageLoadError {
    /// Los bytes no contienen una imagen PNG válida.
    Decode(String),
    /// No se pudo leer el archivo.
    Io(String),
    /// La imagen está vacía o no tiene píxeles.
    EmptyImage,
    /// La imagen excede [`MAX_DIMENSION`] o [`MAX_PIXELS`].
    TooLarge { width: u32, height: u32 },
    /// Se agotaron los identificadores de este store.
    HandleOverflow,
}

impl fmt::Display for ImageLoadError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Decode(message) => write!(f, "error decodificando imagen: {message}"),
            Self::Io(message) => write!(f, "error leyendo imagen: {message}"),
            Self::EmptyImage => write!(f, "la imagen no contiene píxeles"),
            Self::TooLarge { width, height } => {
                write!(f, "imagen demasiado grande: {width}×{height}")
            }
            Self::HandleOverflow => write!(f, "se agotaron los handles de imagen"),
        }
    }
}

impl Error for ImageLoadError {}

/// Imagen decodificada en RGBA8, en orden de filas de arriba abajo.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ImageData {
    pub width: u32,
    pub height: u32,
    pub pixels: Vec<u8>,
}

impl ImageData {
    /// Devuelve los bytes RGBA8 como una sola rebanada.
    #[must_use]
    pub fn as_rgba(&self) -> &[u8] {
        &self.pixels
    }
}

/// Almacena imágenes decodificadas y entrega handles opacos locales.
///
/// No hay registro global ni caché por ruta: esa política depende del
/// formato de escenas que se fijará en ADR-008 antes de H9.
#[derive(Debug, Default)]
pub struct ImageStore {
    next_handle: u64,
    images: HashMap<ImageHandle, ImageData>,
}

impl ImageStore {
    /// Crea un store vacío.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Decodifica bytes PNG y guarda la imagen.
    pub fn load_from_bytes(&mut self, bytes: &[u8]) -> Result<ImageHandle, ImageLoadError> {
        if bytes.is_empty() {
            return Err(ImageLoadError::EmptyImage);
        }

        let decoded = image::load_from_memory(bytes)
            .map_err(|error| ImageLoadError::Decode(error.to_string()))?;
        let width = decoded.width();
        let height = decoded.height();
        validate_dimensions(width, height)?;

        let rgba = decoded.into_rgba8();
        let data = ImageData {
            width,
            height,
            pixels: rgba.into_raw(),
        };
        self.insert(data)
    }

    /// Lee un archivo PNG del disco y guarda la imagen decodificada.
    pub fn load_from_path<P: AsRef<Path>>(
        &mut self,
        path: P,
    ) -> Result<ImageHandle, ImageLoadError> {
        let bytes = std::fs::read(path).map_err(|error| ImageLoadError::Io(error.to_string()))?;
        self.load_from_bytes(&bytes)
    }

    /// Devuelve una imagen previamente cargada, si el handle pertenece a este store.
    #[must_use]
    pub fn get(&self, handle: ImageHandle) -> Option<&ImageData> {
        self.images.get(&handle)
    }

    /// Elimina una imagen. El handle nunca se reutiliza.
    pub fn remove(&mut self, handle: ImageHandle) -> Option<ImageData> {
        self.images.remove(&handle)
    }

    /// Número de imágenes almacenadas.
    #[must_use]
    pub fn len(&self) -> usize {
        self.images.len()
    }

    /// Indica si el store no contiene imágenes.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.images.is_empty()
    }

    fn insert(&mut self, data: ImageData) -> Result<ImageHandle, ImageLoadError> {
        let next = self
            .next_handle
            .checked_add(1)
            .ok_or(ImageLoadError::HandleOverflow)?;
        let handle = ImageHandle(next);
        self.next_handle = next;
        self.images.insert(handle, data);
        Ok(handle)
    }
}

fn validate_dimensions(width: u32, height: u32) -> Result<(), ImageLoadError> {
    if width == 0 || height == 0 {
        return Err(ImageLoadError::EmptyImage);
    }
    if width > MAX_DIMENSION || height > MAX_DIMENSION {
        return Err(ImageLoadError::TooLarge { width, height });
    }

    let pixels = usize::try_from(width)
        .ok()
        .and_then(|w| usize::try_from(height).ok().and_then(|h| w.checked_mul(h)));
    match pixels {
        Some(count) if count <= MAX_PIXELS => Ok(()),
        _ => Err(ImageLoadError::TooLarge { width, height }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;
    use std::path::PathBuf;
    use std::sync::atomic::{AtomicU64, Ordering};

    static TEMP_COUNTER: AtomicU64 = AtomicU64::new(0);

    fn sample_png(width: u32, height: u32) -> Vec<u8> {
        let mut pixels = Vec::with_capacity(usize::try_from(width * height * 4).unwrap());
        for y in 0..height {
            for x in 0..width {
                pixels.extend_from_slice(&[(x * 17) as u8, (y * 31) as u8, 128, 255]);
            }
        }

        let rgba = image::RgbaImage::from_raw(width, height, pixels).expect("valid buffer");
        let mut cursor = Cursor::new(Vec::new());
        image::DynamicImage::ImageRgba8(rgba)
            .write_to(&mut cursor, image::ImageFormat::Png)
            .expect("encode PNG");
        cursor.into_inner()
    }

    fn temp_png_path() -> PathBuf {
        let counter = TEMP_COUNTER.fetch_add(1, Ordering::Relaxed);
        std::env::temp_dir().join(format!("lunar_assets_{}_{counter}.png", std::process::id()))
    }

    #[test]
    fn load_from_bytes_decodes_rgba8() {
        let png = sample_png(2, 3);
        let mut store = ImageStore::new();

        let handle = store.load_from_bytes(&png).expect("decode PNG");

        assert_eq!(handle.get(), 1);
        assert_eq!(store.len(), 1);
        let image = store.get(handle).expect("stored image");
        assert_eq!(image.width, 2);
        assert_eq!(image.height, 3);
        assert_eq!(image.as_rgba().len(), 24);
        assert_eq!(image.as_rgba()[0..4], [0, 0, 128, 255]);
        assert_eq!(image.as_rgba()[20..24], [17, 62, 128, 255]);
    }

    #[test]
    fn handles_are_monotonic_and_failures_do_not_consume_one() {
        let png = sample_png(1, 1);
        let mut store = ImageStore::new();

        let error = store
            .load_from_bytes(b"not a png")
            .expect_err("invalid PNG");
        assert!(matches!(error, ImageLoadError::Decode(_)));
        assert!(store.is_empty());

        let first = store.load_from_bytes(&png).expect("first image");
        let second = store.load_from_bytes(&png).expect("second image");
        assert_eq!(first.get(), 1);
        assert_eq!(second.get(), 2);
        assert_ne!(first, second);
    }

    #[test]
    fn load_from_path_reads_file() {
        let path = temp_png_path();
        std::fs::write(&path, sample_png(3, 1)).expect("write PNG");
        let mut store = ImageStore::new();

        let handle = store.load_from_path(&path).expect("load PNG from path");
        let image = store.get(handle).expect("stored image");
        assert_eq!((image.width, image.height), (3, 1));
        assert_eq!(image.as_rgba().len(), 12);

        std::fs::remove_file(&path).expect("remove PNG");
    }

    #[test]
    fn missing_path_reports_io_error() {
        let path = temp_png_path();
        let mut store = ImageStore::new();

        let error = store.load_from_path(&path).expect_err("missing file");
        assert!(matches!(error, ImageLoadError::Io(_)));

        let handle = store.load_from_bytes(&sample_png(1, 1)).expect("valid PNG");
        assert_eq!(handle.get(), 1);
    }

    #[test]
    fn removal_drops_image_but_does_not_reuse_handle() {
        let png = sample_png(1, 1);
        let mut store = ImageStore::new();
        let first = store.load_from_bytes(&png).expect("first image");

        let removed = store.remove(first).expect("remove image");
        assert_eq!(removed.width, 1);
        assert!(store.get(first).is_none());

        let second = store.load_from_bytes(&png).expect("second image");
        assert_eq!(second.get(), 2);
    }

    #[test]
    fn validate_dimensions_rejects_empty_and_oversized_images() {
        assert_eq!(validate_dimensions(1, 1), Ok(()));
        assert_eq!(validate_dimensions(4096, 4096), Ok(()));
        assert_eq!(validate_dimensions(0, 1), Err(ImageLoadError::EmptyImage));
        assert_eq!(validate_dimensions(1, 0), Err(ImageLoadError::EmptyImage));
        assert_eq!(
            validate_dimensions(MAX_DIMENSION + 1, 1),
            Err(ImageLoadError::TooLarge {
                width: MAX_DIMENSION + 1,
                height: 1
            })
        );
        assert_eq!(
            validate_dimensions(4096, 4097),
            Err(ImageLoadError::TooLarge {
                width: 4096,
                height: 4097
            })
        );
    }
}
