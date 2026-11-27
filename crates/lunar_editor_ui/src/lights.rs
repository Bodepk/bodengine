//! Luces del editor y su asignación por entidad (E49, ADR-050).
//!
//! Son datos del editor: un sidecar `lights.map.txt` junto a la escena. No hay
//! componentes nuevos en el esquema v2, tipos GPU ni cambios en la bridge C++.

use std::collections::BTreeMap;
use std::fs;
use std::path::Path;

use egui::{Color32, Ui, Vec2};

/// Nombre del fichero lateral de luces.
pub(crate) const MAP_FILE: &str = "lights.map.txt";
/// Suelo ambiental compartido con ADR-037.
pub(crate) const AMBIENT: f32 = 0.35;

/// Clases de luz que entiende el sombreado CPU del editor.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub(crate) enum LightKind {
    #[default]
    Directional,
    Point,
}

impl LightKind {
    pub(crate) const ALL: [Self; 2] = [Self::Directional, Self::Point];

    pub(crate) fn label(self) -> &'static str {
        match self {
            Self::Directional => "Direccional",
            Self::Point => "Puntual",
        }
    }

    fn token(self) -> &'static str {
        match self {
            Self::Directional => "d",
            Self::Point => "p",
        }
    }

    fn from_token(token: &str) -> Option<Self> {
        match token {
            "d" => Some(Self::Directional),
            "p" => Some(Self::Point),
            _ => None,
        }
    }
}

/// Parámetros de una luz de edición. Todo finito y acotado (ADR-050).
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct Light {
    pub(crate) kind: LightKind,
    /// RGB en `[0,1]`; el alfa no se usa en el sombreado.
    pub(crate) color: [f32; 3],
    /// Multiplicador en `[0,2]`.
    pub(crate) intensity: f32,
    /// Vector hacia la luz, en `[0,1]` por componente; se normaliza al sombrear.
    pub(crate) direction: [f32; 3],
    /// Alcance de una luz puntual, en `[0,50]`.
    pub(crate) range: f32,
}

impl Default for Light {
    fn default() -> Self {
        Self {
            kind: LightKind::Directional,
            color: [1.0; 3],
            intensity: 1.0,
            direction: [-0.4, 0.8, 0.6],
            range: 10.0,
        }
    }
}

fn sanitize(value: f32, default: f32, min: f32, max: f32) -> f32 {
    if value.is_finite() {
        value.clamp(min, max)
    } else {
        default
    }
}

fn numbers(text: &str, defaults: [f32; 3], min: f32, max: f32) -> [f32; 3] {
    let mut out = defaults;
    for (index, part) in text.split(',').take(3).enumerate() {
        if let Ok(value) = part.trim().parse::<f32>() {
            out[index] = sanitize(value, defaults[index], min, max);
        }
    }
    out
}

fn triplet(values: [f32; 3]) -> String {
    values
        .iter()
        .map(|value| format!("{value:.4}"))
        .collect::<Vec<_>>()
        .join(",")
}

impl Light {
    /// Codifica la luz como una línea `tipo:color:intensidad:dirección:alcance`.
    #[must_use]
    pub(crate) fn encode(&self) -> String {
        let color = sanitize_color(self.color);
        let intensity = sanitize(self.intensity, 1.0, 0.0, 2.0);
        let direction = self.direction.map(|value| sanitize(value, 0.0, -1.0, 1.0));
        let range = sanitize(self.range, 10.0, 0.0, 50.0);
        format!(
            "{}:{}:{intensity:.4}:{}:{range:.4}",
            self.kind.token(),
            triplet(color),
            triplet(direction)
        )
    }

    /// Decodifica tolerantly: cada campo inválido cae a su default.
    #[must_use]
    pub(crate) fn decode(text: &str) -> Option<Self> {
        let mut parts = text.trim().split(':');
        let kind = LightKind::from_token(parts.next()?.trim())?;
        let color = numbers(parts.next()?, [1.0; 3], 0.0, 1.0);
        let intensity = parts
            .next()?
            .trim()
            .parse::<f32>()
            .ok()
            .map_or(1.0, |value| sanitize(value, 1.0, 0.0, 2.0));
        let direction = numbers(parts.next()?, Light::default().direction, -1.0, 1.0);
        let range = parts
            .next()?
            .trim()
            .parse::<f32>()
            .ok()
            .map_or(10.0, |value| sanitize(value, 10.0, 0.0, 50.0));
        Some(Self {
            kind,
            color,
            intensity,
            direction,
            range,
        })
    }

    #[must_use]
    pub(crate) fn preview_color(&self) -> Color32 {
        let [r, g, b] = sanitize_color(self.color);
        let channel = |value: f32| (value.clamp(0.0, 1.0) * 255.0).round() as u8;
        Color32::from_rgb(channel(r), channel(g), channel(b))
    }

    /// Controles editables. Devuelve `true` si algún valor cambió en este frame.
    pub(crate) fn editor_ui(&mut self, ui: &mut Ui) -> bool {
        let mut changed = false;
        egui::ComboBox::from_label("Tipo")
            .selected_text(self.kind.label())
            .show_ui(ui, |ui| {
                for kind in LightKind::ALL {
                    if ui
                        .selectable_label(self.kind == kind, kind.label())
                        .clicked()
                    {
                        self.kind = kind;
                        changed = true;
                    }
                }
            });
        ui.horizontal(|ui| {
            ui.label("Color");
            let mut rgba = [self.color[0], self.color[1], self.color[2], 1.0];
            changed |= ui.color_edit_button_rgba_unmultiplied(&mut rgba).changed();
            self.color = [rgba[0], rgba[1], rgba[2]];
        });
        ui.horizontal(|ui| {
            ui.label("Intensidad");
            changed |= ui
                .add(
                    egui::DragValue::new(&mut self.intensity)
                        .speed(0.01)
                        .range(0.0..=2.0),
                )
                .changed();
        });
        match self.kind {
            LightKind::Directional => {
                ui.label("Dirección hacia la luz");
                for (index, label) in ["X", "Y", "Z"].into_iter().enumerate() {
                    ui.horizontal(|ui| {
                        ui.label(label);
                        changed |= ui
                            .add(
                                egui::DragValue::new(&mut self.direction[index])
                                    .speed(0.01)
                                    .range(-1.0..=1.0),
                            )
                            .changed();
                    });
                }
            }
            LightKind::Point => {
                ui.horizontal(|ui| {
                    ui.label("Alcance");
                    changed |= ui
                        .add(
                            egui::DragValue::new(&mut self.range)
                                .speed(0.1)
                                .range(0.0..=50.0),
                        )
                        .changed();
                });
                ui.colored_label(
                    Color32::from_gray(150),
                    "La posición es la transformación de la entidad",
                );
            }
        }
        ui.horizontal(|ui| {
            let (rect, _) = ui.allocate_exact_size(Vec2::splat(16.0), egui::Sense::hover());
            ui.painter().rect_filled(rect, 2.0, self.preview_color());
            ui.colored_label(
                Color32::from_gray(150),
                "Aproximación CPU: sin sombras ni texturas",
            );
        });
        changed
    }
}

fn sanitize_color(color: [f32; 3]) -> [f32; 3] {
    color.map(|value| sanitize(value, 1.0, 0.0, 1.0))
}

/// Luces por entidad, ordenadas para escrituras estables.
#[derive(Debug, Clone, Default, PartialEq)]
pub(crate) struct LightMap {
    entries: BTreeMap<u64, Light>,
}

impl LightMap {
    #[must_use]
    pub(crate) fn get(&self, id: u64) -> Option<&Light> {
        self.entries.get(&id)
    }

    #[must_use]
    pub(crate) fn len(&self) -> usize {
        self.entries.len()
    }

    /// Iteración estable por id para resolver las luces en el sombreado (E49).
    pub(crate) fn iter(&self) -> impl Iterator<Item = (u64, &Light)> {
        self.entries.iter().map(|(id, light)| (*id, light))
    }

    pub(crate) fn set(&mut self, id: u64, light: Light) {
        self.entries.insert(id, light);
    }

    pub(crate) fn remove(&mut self, id: u64) {
        self.entries.remove(&id);
    }

    /// Descarta luces de entidades que ya no existen.
    pub(crate) fn retain(&mut self, ids: &[u64]) {
        self.entries.retain(|id, _| ids.contains(id));
    }

    #[must_use]
    pub(crate) fn encode(&self) -> String {
        let mut out = String::from("# lunar lights.map.txt (ADR-050)\n");
        for (id, light) in &self.entries {
            out.push_str(&format!("{id}={}\n", light.encode()));
        }
        out
    }

    /// Decodifica líneas `id=tipo:color:...`, ignorando comentarios y entradas rotas.
    #[must_use]
    pub(crate) fn decode(text: &str) -> Self {
        let mut entries = BTreeMap::new();
        for line in text.lines() {
            let line = line.trim();
            if line.is_empty() || line.starts_with('#') {
                continue;
            }
            let Some((id, value)) = line.split_once('=') else {
                continue;
            };
            if let (Ok(id), Some(light)) = (id.trim().parse::<u64>(), Light::decode(value)) {
                entries.insert(id, light);
            }
        }
        Self { entries }
    }
}

/// Carga el sidecar desde el directorio de la escena.
#[must_use]
pub(crate) fn load(scene_dir: &Path) -> LightMap {
    let text = fs::read_to_string(scene_dir.join(MAP_FILE)).unwrap_or_default();
    LightMap::decode(&text)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn light_round_trip_preserves_every_field() {
        let light = Light {
            kind: LightKind::Point,
            color: [0.2, 0.4, 0.6],
            intensity: 1.75,
            direction: [-0.5, 0.25, 1.0],
            range: 12.5,
        };
        assert_eq!(Light::decode(&light.encode()), Some(light));
    }

    #[test]
    fn invalid_fields_fall_back_independently() {
        let light = Light::decode("d:abc,0.5,9:abc:0,NaN,2:1000").unwrap();
        assert_eq!(light.kind, LightKind::Directional);
        assert_eq!(light.color, [1.0, 0.5, 1.0]);
        assert_eq!(light.intensity, 1.0);
        assert_eq!(light.direction, [0.0, 0.8, 1.0]);
        assert_eq!(light.range, 50.0);
        assert_eq!(Light::decode("xx:1,1,1:1:0,0,1:1"), None);
    }

    #[test]
    fn map_round_trip_and_retain_are_stable() {
        let mut map = LightMap::default();
        map.set(7, Light::default());
        map.set(2, Light::default());
        let decoded = LightMap::decode(&map.encode());
        assert_eq!(decoded, map);
        assert_eq!(decoded.len(), 2);
        let mut pruned = decoded;
        pruned.retain(&[2]);
        assert_eq!(pruned.get(2), Some(&Light::default()));
        assert_eq!(pruned.get(7), None);
        pruned.remove(2);
        assert_eq!(pruned.len(), 0);
    }
}
