//! Timeline de clips de sprites del editor (E42, ADR-042).
//!
//! La selección del fotograma es pura: recibe un clip y un tiempo, y no toca la
//! escena ni el reloj del runtime. La GUI solo muestra y mueve el estado en
//! memoria de [`TimelineState`].

use egui::{Color32, Pos2, Rect, Sense, Stroke, Ui, Vec2};
use lunar_scenes::{AnimationClip, Scene, SpriteMeta};

use crate::app::EditorApp;

/// Estado de reproducción de la ventana de timeline. No se persiste.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct TimelineState {
    pub(crate) clip: usize,
    pub(crate) elapsed: f32,
    pub(crate) playing: bool,
}

impl Default for TimelineState {
    fn default() -> Self {
        Self {
            clip: 0,
            elapsed: 0.0,
            playing: false,
        }
    }
}

/// Duración de un clip. Sin fotogramas o con `fps` inválido, es cero.
#[must_use]
pub(crate) fn duration(clip: &AnimationClip) -> f32 {
    if clip.frames.is_empty() || !clip.fps.is_finite() || clip.fps <= 0.0 {
        0.0
    } else {
        clip.frames.len() as f32 / clip.fps
    }
}

/// Posición dentro de `clip.frames` para un instante dado.
#[must_use]
pub(crate) fn position(clip: &AnimationClip, elapsed: f32) -> usize {
    let length = clip.frames.len();
    if length == 0 || !elapsed.is_finite() || elapsed <= 0.0 || duration(clip) <= 0.0 {
        return 0;
    }
    let frame = (elapsed * clip.fps) as usize;
    if clip.looping {
        frame % length
    } else {
        frame.min(length - 1)
    }
}

/// Índice de celda de la hoja que corresponde al instante dado.
#[must_use]
pub(crate) fn frame_at(clip: &AnimationClip, elapsed: f32) -> usize {
    clip.frames
        .get(position(clip, elapsed))
        .map_or(0, |frame| (*frame) as usize)
}

/// Avanza el reloj de la timeline con el delta de la GUI.
pub(crate) fn advance(state: &mut TimelineState, dt: f32, clip: &AnimationClip) {
    if !state.playing || !dt.is_finite() || dt <= 0.0 {
        return;
    }
    let total = duration(clip);
    if total <= 0.0 {
        return;
    }
    state.elapsed += dt;
    if !state.elapsed.is_finite() || state.elapsed < 0.0 {
        state.elapsed = 0.0;
    } else if clip.looping {
        state.elapsed %= total;
    } else {
        state.elapsed = state.elapsed.min(total);
    }
}

/// Indica si todos los fotogramas del clip caben en la hoja.
#[must_use]
pub(crate) fn valid_clip(clip: &AnimationClip, meta: SpriteMeta) -> bool {
    meta.frame_count() > 0 && clip.frames.iter().all(|frame| *frame < meta.frame_count())
}

/// Clips de la entidad seleccionada, si tiene un `Sprite` con animaciones.
#[must_use]
pub(crate) fn selected_clips(scene: &Scene, id: u64) -> Option<(SpriteMeta, Vec<AnimationClip>)> {
    let entity = scene.entities.iter().find(|entity| entity.id == id)?;
    entity
        .components
        .iter()
        .find_map(|component| match component {
            lunar_scenes::Component::Sprite {
                meta, animations, ..
            } if !animations.is_empty() => Some((*meta, animations.clone())),
            _ => None,
        })
}

/// Dibuja la ventana de timeline y actualiza solo el estado de la vista.
pub(crate) fn timeline_ui(app: &mut EditorApp, ui: &mut Ui) {
    ui.heading("Timeline de animación");
    let Some(id) = app.document.selection().first().copied() else {
        ui.label("Selecciona una entidad con un sprite animado.");
        return;
    };
    let Some((meta, clips)) = selected_clips(app.document.scene(), id) else {
        ui.label("La entidad seleccionada no tiene clips de animación.");
        return;
    };
    if app.timeline_state.clip >= clips.len() {
        app.timeline_state.clip = 0;
        app.timeline_state.elapsed = 0.0;
    }
    let clip = clips[app.timeline_state.clip].clone();
    let total = duration(&clip);
    egui::ComboBox::from_label("Clip")
        .selected_text(clip.name.clone())
        .show_ui(ui, |ui| {
            for (index, candidate) in clips.iter().enumerate() {
                if ui
                    .selectable_label(index == app.timeline_state.clip, candidate.name.clone())
                    .clicked()
                {
                    app.timeline_state.clip = index;
                    app.timeline_state.elapsed = 0.0;
                    app.timeline_state.playing = false;
                }
            }
        });
    ui.horizontal(|ui| {
        let label = if app.timeline_state.playing {
            "⏸ Pausa"
        } else {
            "▶ Reproducir"
        };
        if ui.button(label).clicked() && total > 0.0 {
            app.timeline_state.playing = !app.timeline_state.playing;
        }
        if ui.button("Reiniciar").clicked() {
            app.timeline_state.elapsed = 0.0;
            app.timeline_state.playing = false;
        }
    });
    ui.add(
        egui::Slider::new(&mut app.timeline_state.elapsed, 0.0..=total.max(0.0)).text("Tiempo (s)"),
    );
    let frame = frame_at(&clip, app.timeline_state.elapsed);
    ui.label(format!(
        "Fotograma {frame} · celda {} de {} · posición {}/{}",
        frame + 1,
        meta.frame_count(),
        position(&clip, app.timeline_state.elapsed) + 1,
        clip.frames.len()
    ));
    if !valid_clip(&clip, meta) {
        ui.colored_label(Color32::YELLOW, "Hay fotogramas fuera de la hoja");
    }
    let columns = meta.columns.max(1);
    let rows = meta.rows.max(1);
    let width = ui.available_width().min(260.0);
    let height = (width * rows as f32 / columns as f32).clamp(24.0, 260.0);
    let (rect, _) = ui.allocate_exact_size(Vec2::new(width, height), Sense::hover());
    for index in 0..meta.frame_count() {
        let column = index % columns;
        let row = index / columns;
        let cell = Rect::from_min_size(
            Pos2::new(
                rect.min.x + column as f32 * rect.width() / columns as f32,
                rect.min.y + row as f32 * rect.height() / rows as f32,
            ),
            Vec2::new(rect.width() / columns as f32, rect.height() / rows as f32),
        );
        let color = if index as usize == frame {
            Color32::from_rgb(60, 140, 80)
        } else {
            Color32::from_gray(48)
        };
        ui.painter().rect_filled(cell, 0.0, color);
        ui.painter().rect_stroke(
            cell,
            0.0,
            Stroke::new(1.0, Color32::GRAY),
            egui::StrokeKind::Middle,
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use egui::{Pos2, Rect};
    use lunar_editor::EditorDocument;
    use lunar_scenes::{Component, SceneEntity};

    fn clip(frames: &[u32], fps: f32, looping: bool) -> AnimationClip {
        AnimationClip {
            name: "caminar".to_owned(),
            frames: frames.to_vec(),
            fps,
            looping,
        }
    }

    #[test]
    fn frame_selection_loops_and_clamps() {
        let looping = clip(&[1, 2, 3], 4.0, true);
        assert_eq!(position(&looping, 0.0), 0);
        assert_eq!(frame_at(&looping, 0.3), 2);
        assert_eq!(frame_at(&looping, 0.9), 1);
        let once = clip(&[4, 5], 2.0, false);
        assert_eq!(frame_at(&once, 2.0), 5);
        assert_eq!(frame_at(&once, f32::INFINITY), 4);
    }

    #[test]
    fn degenerate_clips_are_inert() {
        let empty = clip(&[], 8.0, true);
        assert_eq!(duration(&empty), 0.0);
        assert_eq!(frame_at(&empty, 3.0), 0);
        let stopped = clip(&[1], 0.0, true);
        assert_eq!(duration(&stopped), 0.0);
        let mut state = TimelineState {
            clip: 0,
            elapsed: 1.0,
            playing: true,
        };
        advance(&mut state, 0.5, &stopped);
        assert_eq!(state.elapsed, 1.0);
    }

    #[test]
    fn advance_wraps_or_clamps_the_elapsed_time() {
        let looping = clip(&[0, 1, 2], 3.0, true);
        let mut state = TimelineState {
            clip: 0,
            elapsed: 0.9,
            playing: true,
        };
        advance(&mut state, 0.2, &looping);
        assert!((state.elapsed - 0.1).abs() < 1e-6, "{}", state.elapsed);
        state.playing = false;
        advance(&mut state, 1.0, &looping);
        assert!((state.elapsed - 0.1).abs() < 1e-6);
        let once = clip(&[0, 1], 2.0, false);
        state.playing = true;
        advance(&mut state, 9.0, &once);
        assert_eq!(state.elapsed, 1.0);
    }

    #[test]
    fn clips_are_validated_against_the_sheet() {
        let meta = SpriteMeta {
            frame_width: 16,
            frame_height: 16,
            columns: 4,
            rows: 2,
            fps: 8.0,
            looping: true,
        };
        assert!(valid_clip(&clip(&[0, 7], 8.0, true), meta));
        assert!(!valid_clip(&clip(&[8], 8.0, true), meta));
    }

    #[test]
    fn the_window_draws_the_selected_clip() {
        let ctx = egui::Context::default();
        let mut entity = SceneEntity::new(1);
        entity.components.push(Component::Sprite {
            asset: "hero.png".to_owned(),
            meta: SpriteMeta {
                frame_width: 16,
                frame_height: 16,
                columns: 2,
                rows: 1,
                fps: 4.0,
                looping: true,
            },
            animations: vec![clip(&[0, 1], 4.0, true)],
        });
        let mut scene = lunar_scenes::Scene::new();
        scene.entities.push(entity);
        let mut app = EditorApp::new(None, &ctx);
        app.document = EditorDocument::from_scene(scene).expect("document");
        app.select_clicked(1, false, false);
        let input = egui::RawInput {
            screen_rect: Some(Rect::from_min_size(Pos2::ZERO, egui::vec2(900.0, 700.0))),
            ..Default::default()
        };
        for _ in 0..2 {
            let mut warmup = ctx.run_pass(input.clone(), |ctx| {
                egui::Window::new("Timeline").show(ctx, |ui| timeline_ui(&mut app, ui));
            });
            warmup.textures_delta.clear();
        }
        let mut output = ctx.run_pass(input, |ctx| {
            egui::Window::new("Timeline").show(ctx, |ui| timeline_ui(&mut app, ui));
        });
        output.textures_delta.clear();
        let texts: Vec<String> = output
            .shapes
            .iter()
            .filter_map(|clipped| match &clipped.shape {
                egui::Shape::Text(text) => Some(text.galley.job.text.clone()),
                _ => None,
            })
            .collect();
        assert!(
            texts
                .iter()
                .any(|text| text.contains("Timeline de animación")),
            "{texts:?}"
        );
        assert!(texts.iter().any(|text| text.contains("caminar")));
        assert!(texts.iter().any(|text| text.contains("Fotograma")));
    }
}
