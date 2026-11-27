//! Materiales PBR del editor y su asignación por entidad (E47, ADR-047).
//!
//! Son datos editor-owned: archivos `.material.json` con GUID de ADR-034 y un
//! sidecar `materials.map.txt` junto a la escena. No hay tipos GPU, ni cambios
//! en el esquema v2 ni en la bridge C++. Toda la lógica de este módulo es pura
//! salvo la lectura/escritura explícita de `load`/`save`.

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use egui::{Color32, Sense, Ui, Vec2};

/// Sufijo que distingue un asset de material de otros JSON.
pub(crate) const MATERIAL_SUFFIX: &str = ".material.json";
/// Nombre del fichero lateral de asignaciones.
pub(crate) const MAP_FILE: &str = "materials.map.txt";
/// Lado del cuadrado de previsualización, en píxeles fijos (ADR-047).
pub(crate) const PREVIEW_SIZE: f32 = 64.0;

/// Parámetros editables de un material (ADR-047).
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Material {
    pub(crate) name: String,
    pub(crate) albedo: [f32; 4],
    pub(crate) metallic: f32,
    pub(crate) roughness: f32,
    pub(crate) occlusion: f32,
    pub(crate) emissive: [f32; 3],
    /// Ruta opcional a una textura de normales; no se sube a GPU.
    pub(crate) normal: Option<String>,
    /// Referencia textual a un shader; no se compila (ADR-048).
    pub(crate) shader: Option<String>,
}

impl Default for Material {
    fn default() -> Self {
        Self {
            name: String::from("Material"),
            albedo: [0.8, 0.8, 0.8, 1.0],
            metallic: 0.0,
            roughness: 0.5,
            occlusion: 1.0,
            emissive: [0.0; 3],
            normal: None,
            shader: None,
        }
    }
}

fn sanitize(value: f32, default: f32) -> f32 {
    if value.is_finite() {
        value.clamp(0.0, 1.0)
    } else {
        default
    }
}

fn quote(value: &str) -> String {
    let mut out = String::with_capacity(value.len() + 2);
    out.push('"');
    for character in value.chars() {
        match character {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            other if other.is_control() => {
                out.push_str(&format!("\\u{:04x}", u32::from(other)));
            }
            other => out.push(other),
        }
    }
    out.push('"');
    out
}

fn numbers(values: &[f32]) -> String {
    let body = values
        .iter()
        .map(|value| format!("{:.6}", sanitize(*value, 0.0)))
        .collect::<Vec<_>>()
        .join(", ");
    format!("[{body}]")
}

impl Material {
    /// Codifica el material como JSON legible con una clave por campo.
    #[must_use]
    pub(crate) fn encode(&self) -> String {
        let mut out = format!(
            concat!(
                "{{\n",
                "  \"name\": {},\n",
                "  \"albedo\": {},\n",
                "  \"metallic\": {:.6},\n",
                "  \"roughness\": {:.6},\n",
                "  \"occlusion\": {:.6},\n",
                "  \"emissive\": {},\n",
            ),
            quote(&self.name),
            numbers(&self.albedo),
            sanitize(self.metallic, 0.0),
            sanitize(self.roughness, 0.5),
            sanitize(self.occlusion, 1.0),
            numbers(&self.emissive),
        );
        let normal = self
            .normal
            .as_deref()
            .map_or_else(|| "null".to_owned(), quote);
        let shader = self
            .shader
            .as_deref()
            .map_or_else(|| "null".to_owned(), quote);
        out.push_str(&format!(
            "  \"normal\": {normal},\n  \"shader\": {shader}\n}}\n"
        ));
        out
    }

    /// Decodifica de forma tolerante: cada campo inválido cae a su default.
    #[must_use]
    pub(crate) fn decode(text: &str) -> Self {
        let mut material = Self::default();
        let mut parser = JsonReader::new(text);
        if !parser.eat(b'{') {
            return material;
        }
        loop {
            if parser.eat(b'}') {
                break;
            }
            let Some(key) = parser.string() else {
                break;
            };
            if !parser.eat(b':') {
                break;
            }
            match key.as_str() {
                "name" => {
                    if let Some(name) = parser.string() {
                        if !name.is_empty() {
                            material.name = name;
                        }
                    } else {
                        parser.skip_value();
                    }
                }
                "albedo" => {
                    if let Some(values) = parser.array() {
                        let defaults = [0.8, 0.8, 0.8, 1.0];
                        for (index, slot) in material.albedo.iter_mut().enumerate() {
                            *slot = values
                                .get(index)
                                .map_or(defaults[index], |value| sanitize(*value, defaults[index]));
                        }
                    } else {
                        parser.skip_value();
                    }
                }
                "metallic" => {
                    if let Some(value) = parser.number() {
                        material.metallic = sanitize(value, 0.0);
                    } else {
                        parser.skip_value();
                    }
                }
                "roughness" => {
                    if let Some(value) = parser.number() {
                        material.roughness = sanitize(value, 0.5);
                    } else {
                        parser.skip_value();
                    }
                }
                "occlusion" => {
                    if let Some(value) = parser.number() {
                        material.occlusion = sanitize(value, 1.0);
                    } else {
                        parser.skip_value();
                    }
                }
                "emissive" => {
                    if let Some(values) = parser.array() {
                        for (index, slot) in material.emissive.iter_mut().enumerate() {
                            *slot = values.get(index).map_or(0.0, |value| sanitize(*value, 0.0));
                        }
                    } else {
                        parser.skip_value();
                    }
                }
                "normal" => {
                    if let Some(path) = parser.string() {
                        material.normal = (!path.is_empty()).then_some(path);
                    } else {
                        parser.skip_value();
                    }
                }
                "shader" => {
                    if let Some(path) = parser.string() {
                        material.shader = (!path.is_empty()).then_some(path);
                    } else {
                        parser.skip_value();
                    }
                }
                _ => {
                    parser.skip_value();
                }
            }
            if !parser.eat(b',') {
                break;
            }
        }
        material
    }

    /// Color determinista de la previsualización CPU (ADR-047). No es PBR.
    #[must_use]
    pub(crate) fn preview_rgba(&self) -> [u8; 4] {
        let factor = 0.55 + 0.45 * (1.0 - self.roughness);
        let mut rgba = [0u8; 4];
        for (index, channel) in self.albedo[..3].iter().enumerate() {
            let lit = channel * factor * self.occlusion + self.emissive[index];
            rgba[index] = (lit.clamp(0.0, 1.0) * 255.0).round() as u8;
        }
        rgba[3] = (self.albedo[3].clamp(0.0, 1.0) * 255.0).round() as u8;
        rgba
    }

    /// Mismo color en formato de egui para los paneles.
    #[must_use]
    pub(crate) fn preview_color(&self) -> Color32 {
        let [r, g, b, a] = self.preview_rgba();
        Color32::from_rgba_unmultiplied(r, g, b, a)
    }

    /// Dibuja el cuadrado fijo de previsualización; no consulta texturas ni GPU.
    pub(crate) fn preview_ui(&self, ui: &mut Ui) {
        let (rect, _) = ui.allocate_exact_size(Vec2::splat(PREVIEW_SIZE), Sense::hover());
        ui.painter().rect_filled(rect, 2.0, self.preview_color());
        ui.painter().rect_stroke(
            rect,
            2.0,
            (1.0, Color32::from_gray(110)),
            egui::StrokeKind::Middle,
        );
    }

    /// Controles editables. Devuelve `true` si algún valor cambió en este frame.
    pub(crate) fn editor_ui(&mut self, ui: &mut Ui) -> bool {
        let mut changed = false;
        ui.horizontal(|ui| {
            ui.label("Nombre");
            changed |= ui.text_edit_singleline(&mut self.name).changed();
        });
        ui.horizontal(|ui| {
            self.preview_ui(ui);
            ui.vertical(|ui| {
                changed |= ui
                    .color_edit_button_rgba_unmultiplied(&mut self.albedo)
                    .changed();
                ui.label(format!("Albedo {}", hex_rgba(self.preview_rgba())));
                changed |= slider(ui, "Metallic", &mut self.metallic);
                changed |= slider(ui, "Roughness", &mut self.roughness);
                changed |= slider(ui, "Occlusion", &mut self.occlusion);
            });
        });
        changed |= slider(ui, "Emisivo R", &mut self.emissive[0]);
        changed |= slider(ui, "Emisivo G", &mut self.emissive[1]);
        changed |= slider(ui, "Emisivo B", &mut self.emissive[2]);
        ui.horizontal(|ui| {
            ui.label("Normal");
            changed |= ui
                .text_edit_singleline(self.normal.get_or_insert_with(String::new))
                .changed();
        });
        if self.normal.as_deref() == Some("") {
            self.normal = None;
        }
        ui.horizontal(|ui| {
            ui.label("Shader");
            changed |= ui
                .text_edit_singleline(self.shader.get_or_insert_with(String::new))
                .changed();
        });
        if self.shader.as_deref() == Some("") {
            self.shader = None;
        }
        ui.colored_label(
            Color32::from_gray(150),
            "Referencias de editor: sin texturas ni compilación GPU",
        );
        changed
    }
}

fn hex_rgba([r, g, b, a]: [u8; 4]) -> String {
    format!("#{r:02x}{g:02x}{b:02x} · α {a}")
}

fn slider(ui: &mut Ui, label: &str, value: &mut f32) -> bool {
    ui.horizontal(|ui| {
        ui.label(label);
        ui.add(egui::DragValue::new(value).speed(0.01).range(0.0..=1.0))
            .changed()
    })
    .inner
}

/// Lector JSON mínimo para un objeto plano; no usa `serde` ni dependencias.
struct JsonReader<'a> {
    text: &'a [u8],
    pos: usize,
}

impl<'a> JsonReader<'a> {
    fn new(text: &'a str) -> Self {
        Self {
            text: text.as_bytes(),
            pos: 0,
        }
    }

    fn whitespace(&mut self) {
        while self.text.get(self.pos).is_some_and(u8::is_ascii_whitespace) {
            self.pos += 1;
        }
    }

    fn eat(&mut self, expected: u8) -> bool {
        self.whitespace();
        if self.text.get(self.pos) == Some(&expected) {
            self.pos += 1;
            true
        } else {
            false
        }
    }

    fn string(&mut self) -> Option<String> {
        self.whitespace();
        if !self.eat(b'"') {
            return None;
        }
        let mut out = String::new();
        while let Some(byte) = self.text.get(self.pos).copied() {
            self.pos += 1;
            match byte {
                b'"' => return Some(out),
                b'\\' => {
                    let escape = *self.text.get(self.pos)?;
                    self.pos += 1;
                    match escape {
                        b'"' => out.push('"'),
                        b'\\' => out.push('\\'),
                        b'/' => out.push('/'),
                        b'n' => out.push('\n'),
                        b'r' => out.push('\r'),
                        b't' => out.push('\t'),
                        b'b' => out.push('\u{0008}'),
                        b'f' => out.push('\u{000c}'),
                        b'u' => {
                            let digits = self.text.get(self.pos..self.pos + 4)?;
                            let code = std::str::from_utf8(digits).ok()?.parse::<u32>().ok()?;
                            self.pos += 4;
                            out.push(char::from_u32(code)?);
                        }
                        _ => return None,
                    }
                }
                _ if byte.is_ascii() => out.push(byte as char),
                _ => return None,
            }
        }
        None
    }

    fn number(&mut self) -> Option<f32> {
        self.whitespace();
        let start = self.pos;
        while self.text.get(self.pos).is_some_and(|byte| {
            byte.is_ascii_digit() || matches!(byte, b'-' | b'+' | b'.' | b'e' | b'E')
        }) {
            self.pos += 1;
        }
        std::str::from_utf8(self.text.get(start..self.pos)?)
            .ok()?
            .parse::<f32>()
            .ok()
    }

    fn array(&mut self) -> Option<Vec<f32>> {
        if !self.eat(b'[') {
            return None;
        }
        let mut values = Vec::new();
        if self.eat(b']') {
            return Some(values);
        }
        loop {
            values.push(self.number()?);
            if self.eat(b']') {
                return Some(values);
            }
            if !self.eat(b',') {
                return None;
            }
        }
    }

    fn skip_value(&mut self) {
        self.whitespace();
        match self.text.get(self.pos).copied() {
            Some(b'"') => {
                self.string();
            }
            Some(b'{') | Some(b'[') => {
                let open = self.text[self.pos];
                let close = if open == b'{' { b'}' } else { b']' };
                self.pos += 1;
                let mut depth = 1usize;
                while let Some(byte) = self.text.get(self.pos).copied() {
                    self.pos += 1;
                    if byte == b'"' {
                        self.pos -= 1;
                        self.string();
                    } else if byte == open {
                        depth += 1;
                    } else if byte == close {
                        depth -= 1;
                        if depth == 0 {
                            break;
                        }
                    }
                }
            }
            _ => {
                while self
                    .text
                    .get(self.pos)
                    .is_some_and(|byte| !matches!(byte, b',' | b'}' | b']'))
                {
                    self.pos += 1;
                }
            }
        }
    }
}

/// Biblioteca de materiales leídos de la raíz de assets.
#[derive(Debug, Clone, Default, PartialEq)]
pub(crate) struct MaterialLibrary {
    entries: BTreeMap<String, Material>,
}

impl MaterialLibrary {
    #[must_use]
    pub(crate) fn get(&self, path: &str) -> Option<&Material> {
        self.entries.get(path)
    }

    #[must_use]
    pub(crate) fn paths(&self) -> Vec<String> {
        self.entries.keys().cloned().collect()
    }

    pub(crate) fn insert(&mut self, path: String, material: Material) {
        self.entries.insert(path, material);
    }

    /// Escanea la raíz en busca de `*.material.json` sin seguir enlaces.
    #[must_use]
    pub(crate) fn load(root: &Path) -> Self {
        let mut entries = BTreeMap::new();
        let mut files = Vec::new();
        collect_materials(root, root, &mut files);
        files.sort();
        for file in files {
            let Ok(relative) = file.strip_prefix(root) else {
                continue;
            };
            let path = relative.to_string_lossy().replace('\\', "/");
            if let Ok(text) = fs::read_to_string(&file) {
                entries.insert(path, Material::decode(&text));
            }
        }
        Self { entries }
    }

    /// Escribe un material con creación de directorios.
    pub(crate) fn save(&self, root: &Path, path: &str) -> Result<(), String> {
        let material = self
            .entries
            .get(path)
            .ok_or_else(|| format!("Material desconocido: {path}"))?;
        let full = root.join(path);
        if let Some(parent) = full.parent() {
            fs::create_dir_all(parent).map_err(|error| error.to_string())?;
        }
        fs::write(&full, material.encode()).map_err(|error| error.to_string())
    }

    /// Alinea el disco con este estado, usado al restaurar un undo/redo.
    pub(crate) fn sync_to_disk(&self, root: &Path, previous: &Self) -> Result<(), String> {
        for path in previous.paths() {
            if self.get(&path).is_none() {
                let _ = fs::remove_file(root.join(&path));
                let _ = fs::remove_file(root.join(format!("{path}.meta")));
            }
        }
        for path in self.paths() {
            self.save(root, &path)?;
        }
        Ok(())
    }
}

fn collect_materials(root: &Path, directory: &Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = fs::read_dir(directory) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            if path != root {
                collect_materials(root, &path, out);
            }
        } else if path
            .file_name()
            .and_then(|name| name.to_str())
            .is_some_and(|name| name.ends_with(MATERIAL_SUFFIX))
        {
            out.push(path);
        }
    }
}

/// Asignación de material por entidad, ordenada para escrituras estables.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct MaterialMap {
    bindings: BTreeMap<u64, String>,
}

impl MaterialMap {
    #[must_use]
    pub(crate) fn get(&self, id: u64) -> Option<&str> {
        self.bindings.get(&id).map(String::as_str)
    }

    pub(crate) fn set(&mut self, id: u64, path: Option<String>) {
        match path {
            Some(path) => {
                self.bindings.insert(id, path);
            }
            None => {
                self.bindings.remove(&id);
            }
        }
    }

    /// Descarta asignaciones de entidades que ya no existen.
    pub(crate) fn retain(&mut self, ids: &[u64]) {
        self.bindings.retain(|id, _| ids.contains(id));
    }

    #[must_use]
    pub(crate) fn encode(&self) -> String {
        let mut out = String::from("# lunar materials.map.txt (ADR-047)\n");
        for (id, path) in &self.bindings {
            out.push_str(&format!("{id}={path}\n"));
        }
        out
    }

    /// Decodifica líneas `id=ruta`, ignorando comentarios y entradas rotas.
    #[must_use]
    pub(crate) fn decode(text: &str) -> Self {
        let mut bindings = BTreeMap::new();
        for line in text.lines() {
            let line = line.trim();
            if line.is_empty() || line.starts_with('#') {
                continue;
            }
            let Some((id, path)) = line.split_once('=') else {
                continue;
            };
            let (Ok(id), path) = (id.trim().parse::<u64>(), path.trim().to_owned()) else {
                continue;
            };
            if !path.is_empty() && !path.contains('\\') && !path.starts_with('/') {
                bindings.insert(id, path);
            }
        }
        Self { bindings }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn material_round_trip_preserves_every_field() {
        let material = Material {
            name: "Ladrillo \"rojo\"".to_owned(),
            albedo: [0.1, 0.2, 0.3, 0.4],
            metallic: 0.25,
            roughness: 0.75,
            occlusion: 0.5,
            emissive: [0.2, 0.0, 0.1],
            normal: Some("texturas/normal.png".to_owned()),
            shader: Some("shaders/custom.wgsl".to_owned()),
        };

        let encoded = material.encode();
        assert!(encoded.trim_start().starts_with('{'));
        assert_eq!(Material::decode(&encoded), material);
    }

    #[test]
    fn invalid_values_fall_back_independently() {
        let text = r#"{
            "name": "Prueba",
            "albedo": [1e400, 0.5, 2, 7],
            "metallic": "abc",
            "roughness": null,
            "occlusion": -1,
            "emissive": [0.25, 0.75],
            "normal": null,
            "shader": 7,
            "future": {"nested": [1, 2]}
        }"#;
        let material = Material::decode(text);

        assert_eq!(material.name, "Prueba");
        assert_eq!(material.albedo[0], 0.8);
        assert!((material.albedo[1] - 0.5).abs() < 1e-6);
        assert!((material.albedo[2] - 1.0).abs() < 1e-6);
        assert!((material.albedo[3] - 1.0).abs() < 1e-6);
        assert_eq!(material.metallic, 0.0);
        assert_eq!(material.roughness, 0.5);
        assert_eq!(material.occlusion, 0.0);
        assert_eq!(material.emissive[0], 0.25);
        assert_eq!(material.emissive[1], 0.75);
        assert_eq!(material.emissive[2], 0.0);
        assert_eq!(material.normal, None);
        assert_eq!(material.shader, None);
    }

    #[test]
    fn preview_color_is_deterministic_and_bounded() {
        let material = Material::default();
        assert_eq!(material.preview_rgba(), material.preview_rgba());
        assert_eq!(material.preview_rgba(), [158, 158, 158, 255]);

        let dark = Material {
            albedo: [0.0; 4],
            roughness: 1.0,
            ..Material::default()
        };
        assert_eq!(dark.preview_rgba(), [0, 0, 0, 0]);
    }

    #[test]
    fn preview_is_a_fixed_64_by_64_rectangle() {
        let ctx = egui::Context::default();
        let input = egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::Vec2::new(300.0, 200.0),
            )),
            ..Default::default()
        };
        let mut first = ctx.run_pass(input.clone(), |ctx| {
            egui::Window::new("material").show(ctx, |ui| Material::default().preview_ui(ui));
        });
        first.textures_delta.clear();
        let output = ctx.run_pass(input, |ctx| {
            egui::Window::new("material").show(ctx, |ui| Material::default().preview_ui(ui));
        });
        let rects: Vec<egui::Rect> = output
            .shapes
            .iter()
            .filter_map(|clipped| match &clipped.shape {
                egui::Shape::Rect(rect) => Some(rect.rect),
                _ => None,
            })
            .collect();
        assert!(
            rects.iter().any(|rect| {
                (rect.width() - PREVIEW_SIZE).abs() < 0.01
                    && (rect.height() - PREVIEW_SIZE).abs() < 0.01
            }),
            "{rects:?}"
        );
    }

    #[test]
    fn material_map_round_trip_and_retain() {
        let mut map = MaterialMap::default();
        map.set(2, Some("materials/a.material.json".to_owned()));
        map.set(1, Some("materials/b.material.json".to_owned()));
        map.set(3, None);
        let decoded = MaterialMap::decode(&map.encode());
        assert_eq!(decoded, map);
        assert_eq!(decoded.get(1), Some("materials/b.material.json"));

        let mut map = decoded;
        map.retain(&[2]);
        assert_eq!(map.get(1), None);
        assert_eq!(map.get(2), Some("materials/a.material.json"));
    }

    #[test]
    fn library_loads_materials_recursively_and_syncs_to_disk() {
        let root = std::env::temp_dir().join(format!("lunar_materials_{}", std::process::id()));
        let dir = root.join("materials");
        let _ = fs::create_dir_all(&dir);
        let path = "materials/piedra.material.json";
        fs::write(root.join(path), "{\"name\":\"Piedra\",\"roughness\":0.8}").expect("write");
        fs::write(root.join("nota.json"), "{}").expect("write");

        let library = MaterialLibrary::load(&root);
        assert_eq!(library.paths(), vec![path.to_owned()]);
        assert_eq!(
            library.get(path).map(|material| material.name.as_str()),
            Some("Piedra")
        );
        library.save(&root, path).expect("save");
        let text = fs::read_to_string(root.join(path)).expect("read");
        assert_eq!(Material::decode(&text).name, "Piedra");

        let empty = MaterialLibrary::default();
        empty.sync_to_disk(&root, &library).expect("sync");
        assert!(!root.join(path).exists());

        let _ = fs::remove_dir_all(&root);
    }
}
