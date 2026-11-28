//! Entorno y cielo CPU del viewport 3D (E50, ADR-051).
//!
//! Vive en un sidecar `environment.txt` junto a la escena. No hay HDRI,
//! cubemaps, texturas ni GPU: el fondo se pinta con bandas deterministas y la
//! niebla mezcla colores por cara.

use std::fs;
use std::path::Path;

use egui::{Color32, Pos2, Rect, Sense, Ui, Vec2};

use crate::app::EditorApp;

/// Nombre del fichero lateral del entorno.
pub(crate) const FILE: &str = "environment.txt";
/// Bandas horizontales del fondo (ADR-051).
pub(crate) const SKY_BANDS: usize = 48;

fn rgb(red: u8, green: u8, blue: u8) -> [f32; 3] {
    [
        red as f32 / 255.0,
        green as f32 / 255.0,
        blue as f32 / 255.0,
    ]
}

fn default_top() -> [f32; 3] {
    rgb(16, 20, 30)
}

fn default_middle() -> [f32; 3] {
    rgb(28, 33, 46)
}

fn default_bottom() -> [f32; 3] {
    rgb(10, 12, 18)
}

/// Modo de fondo del viewport.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub(crate) enum SkyMode {
    #[default]
    Solid,
    Gradient,
}

impl SkyMode {
    pub(crate) const ALL: [Self; 2] = [Self::Solid, Self::Gradient];

    pub(crate) fn label(self) -> &'static str {
        match self {
            Self::Solid => "Sólido",
            Self::Gradient => "Degradado",
        }
    }

    fn token(self) -> &'static str {
        match self {
            Self::Solid => "s",
            Self::Gradient => "g",
        }
    }

    fn from_token(token: &str) -> Option<Self> {
        match token {
            "s" => Some(Self::Solid),
            "g" => Some(Self::Gradient),
            _ => None,
        }
    }
}

/// Parámetros del entorno editor-owned (ADR-051).
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct Environment {
    pub(crate) mode: SkyMode,
    /// Color de la cima del degradado; con `Sólido` no se usa.
    pub(crate) top: [f32; 3],
    /// Color del horizonte y único color del modo `Sólido`.
    pub(crate) middle: [f32; 3],
    pub(crate) bottom: [f32; 3],
    pub(crate) fog: bool,
    pub(crate) fog_color: [f32; 3],
    /// Distancias de cámara en `[0,100]`.
    pub(crate) fog_start: f32,
    pub(crate) fog_end: f32,
}

impl Default for Environment {
    fn default() -> Self {
        Self {
            mode: SkyMode::Gradient,
            top: default_top(),
            middle: default_middle(),
            bottom: default_bottom(),
            fog: false,
            fog_color: rgb(158, 189, 220),
            fog_start: 15.0,
            fog_end: 60.0,
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

fn triplet(values: [f32; 3]) -> String {
    values
        .iter()
        .map(|value| format!("{:.4}", value.clamp(0.0, 1.0)))
        .collect::<Vec<_>>()
        .join(",")
}

fn parse_triplet(text: &str, default: [f32; 3]) -> [f32; 3] {
    let mut out = default;
    for (index, part) in text.split(',').take(3).enumerate() {
        if let Ok(value) = part.trim().parse::<f32>() {
            out[index] = sanitize(value, default[index], 0.0, 1.0);
        }
    }
    out
}

impl Environment {
    /// Codifica el entorno como líneas `clave=valor` legibles.
    #[must_use]
    pub(crate) fn encode(&self) -> String {
        format!(
            concat!(
                "# lunar environment.txt (ADR-051)\n",
                "mode={}\n",
                "top={}\n",
                "middle={}\n",
                "bottom={}\n",
                "fog={}\n",
                "fog_color={}\n",
                "fog_start={:.4}\n",
                "fog_end={:.4}\n",
            ),
            self.mode.token(),
            triplet(self.top),
            triplet(self.middle),
            triplet(self.bottom),
            u8::from(self.fog),
            triplet(self.fog_color),
            self.fog_start.clamp(0.0, 100.0),
            self.fog_end.clamp(0.0, 100.0),
        )
    }

    /// Decodifica tolerantly: cada clave inválida cae a su valor anterior.
    #[must_use]
    pub(crate) fn decode(text: &str) -> Self {
        let mut env = Self::default();
        for line in text.lines() {
            let line = line.trim();
            if line.is_empty() || line.starts_with('#') {
                continue;
            }
            let Some((key, value)) = line.split_once('=') else {
                continue;
            };
            let value = value.trim();
            match key.trim() {
                "mode" => {
                    if let Some(mode) = SkyMode::from_token(value) {
                        env.mode = mode;
                    }
                }
                "top" => env.top = parse_triplet(value, env.top),
                "middle" => env.middle = parse_triplet(value, env.middle),
                "bottom" => env.bottom = parse_triplet(value, env.bottom),
                "fog" => env.fog = value == "1" || value.eq_ignore_ascii_case("true"),
                "fog_color" => env.fog_color = parse_triplet(value, env.fog_color),
                "fog_start" => {
                    env.fog_start = value.parse::<f32>().map_or(env.fog_start, |parsed| {
                        sanitize(parsed, env.fog_start, 0.0, 100.0)
                    });
                }
                "fog_end" => {
                    env.fog_end = value.parse::<f32>().map_or(env.fog_end, |parsed| {
                        sanitize(parsed, env.fog_end, 0.0, 100.0)
                    });
                }
                _ => {}
            }
        }
        env.normalize_fog();
        env
    }

    /// Mantiene un intervalo de niebla válido incluso con datos corruptos.
    fn normalize_fog(&mut self) {
        if self.fog_end <= self.fog_start {
            if self.fog_start >= 100.0 {
                self.fog_start = 99.0;
            }
            self.fog_end = (self.fog_start + 1.0).min(100.0);
        }
    }
}

fn color(values: [f32; 3]) -> Color32 {
    let channel = |value: f32| (value.clamp(0.0, 1.0) * 255.0).round() as u8;
    Color32::from_rgb(channel(values[0]), channel(values[1]), channel(values[2]))
}

fn mix(a: [f32; 3], b: [f32; 3], t: f32) -> [f32; 3] {
    let t = t.clamp(0.0, 1.0);
    [
        a[0] + (b[0] - a[0]) * t,
        a[1] + (b[1] - a[1]) * t,
        a[2] + (b[2] - a[2]) * t,
    ]
}

/// Color del cielo para `t` entre 0 (cima) y 1 (base).
#[must_use]
pub(crate) fn sky_color(env: &Environment, t: f32) -> Color32 {
    match env.mode {
        SkyMode::Solid => color(env.middle),
        SkyMode::Gradient => {
            let t = t.clamp(0.0, 1.0);
            if t <= 0.5 {
                color(mix(env.top, env.middle, t * 2.0))
            } else {
                color(mix(env.middle, env.bottom, (t - 0.5) * 2.0))
            }
        }
    }
}

/// Factor de niebla para la profundidad de una cara.
#[must_use]
pub(crate) fn fog_factor(env: &Environment, depth: f32) -> f32 {
    if !env.fog || !depth.is_finite() || env.fog_end <= env.fog_start {
        return 0.0;
    }
    ((depth - env.fog_start) / (env.fog_end - env.fog_start)).clamp(0.0, 1.0)
}

/// Mezcla un color con el de la niebla, respetando el alfa original.
#[must_use]
pub(crate) fn apply_fog(base: Color32, env: &Environment, factor: f32) -> Color32 {
    let factor = factor.clamp(0.0, 1.0);
    let fog = color(env.fog_color);
    let channel = |value: u8, fog: u8| {
        (f32::from(value) * (1.0 - factor) + f32::from(fog) * factor).round() as u8
    };
    Color32::from_rgba_premultiplied(
        channel(base.r(), fog.r()),
        channel(base.g(), fog.g()),
        channel(base.b(), fog.b()),
        base.a(),
    )
}

/// Presets de cielo: solo datos, sin texturas.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum SkyPreset {
    Clear,
    Sunset,
    Night,
}

impl SkyPreset {
    pub(crate) const ALL: [Self; 3] = [Self::Clear, Self::Sunset, Self::Night];

    pub(crate) fn label(self) -> &'static str {
        match self {
            Self::Clear => "Despejado",
            Self::Sunset => "Atardecer",
            Self::Night => "Noche",
        }
    }

    fn apply(self, env: &mut Environment) {
        env.mode = SkyMode::Gradient;
        match self {
            Self::Clear => {
                env.top = rgb(41, 87, 158);
                env.middle = rgb(173, 204, 230);
                env.bottom = rgb(46, 51, 61);
            }
            Self::Sunset => {
                env.top = rgb(51, 46, 97);
                env.middle = rgb(242, 140, 71);
                env.bottom = rgb(51, 31, 36);
            }
            Self::Night => {
                env.top = rgb(8, 10, 26);
                env.middle = rgb(26, 33, 56);
                env.bottom = rgb(5, 8, 13);
            }
        }
    }
}

/// Carga el entorno del directorio de la escena.
#[must_use]
pub(crate) fn load(scene_dir: &Path) -> Environment {
    let text = fs::read_to_string(scene_dir.join(FILE)).unwrap_or_default();
    Environment::decode(&text)
}

/// Panel flotante de entorno, con undo/redo en cada cambio (ADR-051).
pub(crate) fn environment_ui(app: &mut EditorApp, ui: &mut Ui) {
    let mut next = app.environment;
    egui::ComboBox::from_label("Cielo")
        .selected_text(next.mode.label())
        .show_ui(ui, |ui| {
            for mode in SkyMode::ALL {
                if ui
                    .selectable_label(next.mode == mode, mode.label())
                    .clicked()
                {
                    next.mode = mode;
                }
            }
        });
    ui.horizontal(|ui| {
        ui.label("Presets");
        for preset in SkyPreset::ALL {
            if ui.button(preset.label()).clicked() {
                preset.apply(&mut next);
            }
        }
    });
    color_row(ui, "Cima", &mut next.top);
    color_row(ui, "Medio", &mut next.middle);
    color_row(ui, "Base", &mut next.bottom);
    sky_preview(ui, &next);
    ui.separator();
    ui.checkbox(&mut next.fog, "Niebla");
    color_row(ui, "Color niebla", &mut next.fog_color);
    ui.horizontal(|ui| {
        ui.label("Inicio");
        ui.add(
            egui::DragValue::new(&mut next.fog_start)
                .speed(0.5)
                .range(0.0..=100.0),
        );
        ui.label("Fin");
        ui.add(
            egui::DragValue::new(&mut next.fog_end)
                .speed(0.5)
                .range(0.0..=100.0),
        );
    });
    next.normalize_fog();
    ui.colored_label(
        Color32::from_gray(150),
        "Cielo CPU y niebla por cara; no es HDRI ni iluminación de runtime",
    );
    if next != app.environment {
        app.commit_environment(next);
    }
}

fn color_row(ui: &mut Ui, label: &str, color: &mut [f32; 3]) {
    ui.horizontal(|ui| {
        let mut rgba = [color[0], color[1], color[2], 1.0];
        ui.label(label);
        if ui.color_edit_button_rgba_unmultiplied(&mut rgba).changed() {
            *color = [rgba[0], rgba[1], rgba[2]];
        }
    });
}

fn sky_preview(ui: &mut Ui, env: &Environment) {
    let (rect, _) = ui.allocate_exact_size(
        Vec2::new(ui.available_width().min(240.0), 36.0),
        Sense::hover(),
    );
    let step = rect.height() / SKY_BANDS as f32;
    for index in 0..SKY_BANDS {
        let t = (index as f32 + 0.5) / SKY_BANDS as f32;
        let top = rect.min.y + index as f32 * step;
        let band = Rect::from_min_max(
            Pos2::new(rect.min.x, top),
            Pos2::new(rect.max.x, top + step + 0.5),
        );
        ui.painter().rect_filled(band, 0.0, sky_color(env, t));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn environment_round_trip_preserves_every_field() {
        let env = Environment {
            mode: SkyMode::Gradient,
            top: rgb(10, 20, 30),
            middle: rgb(100, 110, 120),
            bottom: rgb(200, 210, 220),
            fog: true,
            fog_color: rgb(1, 2, 3),
            fog_start: 12.5,
            fog_end: 73.25,
        };
        let decoded = Environment::decode(&env.encode());
        assert_eq!(decoded.mode, env.mode);
        assert_eq!(decoded.fog, env.fog);
        assert_eq!(decoded.fog_start, env.fog_start);
        assert_eq!(decoded.fog_end, env.fog_end);
        for (decoded, original) in decoded
            .top
            .iter()
            .chain(decoded.middle.iter())
            .chain(decoded.bottom.iter())
            .chain(decoded.fog_color.iter())
            .zip(
                env.top
                    .iter()
                    .chain(env.middle.iter())
                    .chain(env.bottom.iter())
                    .chain(env.fog_color.iter()),
            )
        {
            assert!((decoded - original).abs() < 0.0001);
        }
    }

    #[test]
    fn invalid_fields_fall_back_and_fog_range_stays_valid() {
        let text = concat!(
            "mode=x\n",
            "top=abc,0.5,9\n",
            "fog=quizá\n",
            "fog_start=NaN\n",
            "fog_end=NaN\n",
        );
        let env = Environment::decode(text);
        assert_eq!(env.mode, SkyMode::Gradient);
        assert!((env.top[0] - default_top()[0]).abs() < 0.0001);
        assert_eq!(env.top[1], 0.5);
        assert_eq!(env.top[2], 1.0);
        assert!(!env.fog);
        assert_eq!(env.fog_start, 15.0);
        assert_eq!(env.fog_end, 60.0);
        let swapped = Environment::decode("fog_start=80\nfog_end=10\n");
        assert!(swapped.fog_end > swapped.fog_start);
    }

    #[test]
    fn solid_sky_is_flat_and_gradient_is_deterministic() {
        let solid = Environment {
            mode: SkyMode::Solid,
            ..Default::default()
        };
        assert_eq!(sky_color(&solid, 0.0), sky_color(&solid, 1.0));
        assert_eq!(sky_color(&solid, 0.0), color(solid.middle));
        let mut gradient = solid;
        gradient.mode = SkyMode::Gradient;
        gradient.top = [0.0, 0.0, 0.0];
        gradient.middle = [0.5, 0.5, 0.5];
        gradient.bottom = [1.0, 1.0, 1.0];
        assert_eq!(sky_color(&gradient, 0.0), Color32::BLACK);
        assert_eq!(sky_color(&gradient, 0.5), color([0.5; 3]));
        assert_eq!(sky_color(&gradient, 1.0), Color32::WHITE);
        assert_eq!(sky_color(&gradient, f32::NAN), Color32::BLACK);
    }

    #[test]
    fn default_environment_is_a_subtle_gradient() {
        let env = Environment::default();
        assert_eq!(env.mode, SkyMode::Gradient);
        assert_ne!(sky_color(&env, 0.0), sky_color(&env, 1.0));
        assert!(!env.fog);
    }

    #[test]
    fn fog_is_disabled_by_default_and_mixes_only_when_enabled() {
        let base = Color32::from_rgba_premultiplied(200, 100, 50, 128);
        let env = Environment::default();
        assert_eq!(fog_factor(&env, 1000.0), 0.0);
        assert_eq!(
            apply_fog(base, &env, 1.0),
            Color32::from_rgba_premultiplied(158, 189, 220, 128)
        );
        let mut foggy = env;
        foggy.fog = true;
        foggy.fog_color = [1.0, 0.0, 0.0];
        assert_eq!(fog_factor(&foggy, 15.0), 0.0);
        assert!((fog_factor(&foggy, 37.5) - 0.5).abs() < 0.001);
        assert_eq!(fog_factor(&foggy, 60.0), 1.0);
        assert_eq!(fog_factor(&foggy, f32::NAN), 0.0);
        let mixed = apply_fog(base, &env, 0.5);
        assert_eq!(mixed.r(), 179);
        assert_eq!(mixed.g(), 145);
        assert_eq!(mixed.a(), 128);
        let mixed = apply_fog(base, &foggy, 0.5);
        assert_eq!(mixed.r(), 228);
        assert_eq!(mixed.g(), 50);
        assert_eq!(mixed.a(), 128);
    }

    #[test]
    fn presets_only_change_the_sky_palette() {
        let mut env = Environment {
            fog: true,
            ..Default::default()
        };
        for preset in SkyPreset::ALL {
            preset.apply(&mut env);
            assert_eq!(env.mode, SkyMode::Gradient);
            assert!(env.fog, "los presets conservan la niebla");
        }
        assert_eq!(env.top, rgb(8, 10, 26));
        assert_eq!(env.middle, rgb(26, 33, 56));
        assert_eq!(env.bottom, rgb(5, 8, 13));
    }
}
