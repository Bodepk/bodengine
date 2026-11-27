//! Validación legible de la escena del editor (E46, ADR-046).
//!
//! El análisis es puro: recibe la escena y el estado ya cargado de assets y
//! scripts, y devuelve una lista de problemas. La ventana solo muestra esa
//! lista; ninguna comprobación muta la escena ni escribe ficheros.

use std::collections::HashSet;

use egui::Color32;
use egui::Ui;
use lunar_scenes::{Component, Scene};

use crate::app::EditorApp;
use crate::assets::AssetEntry;
use crate::guids::GuidIndex;
use crate::scripts::ScriptMap;

/// Severidad de un problema detectado.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Severity {
    Error,
    Warn,
}

/// Problema de escena, assets o scripts listo para mostrar.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Issue {
    pub(crate) severity: Severity,
    pub(crate) message: String,
}

impl Issue {
    fn warn(message: impl Into<String>) -> Self {
        Self {
            severity: Severity::Warn,
            message: message.into(),
        }
    }

    fn error(message: impl Into<String>) -> Self {
        Self {
            severity: Severity::Error,
            message: message.into(),
        }
    }
}

/// Comprueba referencias y consistencia sin tocar el disco ni la escena.
#[must_use]
pub(crate) fn validate(
    scene: &Scene,
    entries: &[AssetEntry],
    guids: &GuidIndex,
    scripts: &ScriptMap,
    diagnostics_text: &str,
) -> Vec<Issue> {
    let mut issues = Vec::new();

    for id in duplicate_ids(&scene.entities) {
        issues.push(Issue::error(format!("Id de entidad duplicado: #{id}")));
    }
    for prefab in &scene.prefabs {
        for id in duplicate_ids(&prefab.entities) {
            issues.push(Issue::error(format!(
                "Plantilla «{}»: id duplicado #{id}",
                prefab.name
            )));
        }
    }

    for entity in &scene.entities {
        for component in &entity.components {
            let asset = match component {
                Component::Mesh { asset, .. }
                | Component::Image { asset }
                | Component::Sprite { asset, .. }
                | Component::Audio { asset, .. } => Some(asset.as_str()),
                Component::PrefabRef { name } => {
                    if !scene.prefabs.iter().any(|prefab| prefab.name == *name) {
                        issues.push(Issue::warn(format!(
                            "Entidad #{}: plantilla desconocida «{}»",
                            entity.id, name
                        )));
                    }
                    None
                }
                _ => None,
            };
            if let Some(asset) =
                asset.filter(|path| !path.is_empty() && !asset_exists(path, entries, guids))
            {
                issues.push(Issue::warn(format!(
                    "Entidad #{}: asset no encontrado «{}»",
                    entity.id, asset
                )));
            }
            if let Component::Sprite {
                meta, animations, ..
            } = component
            {
                for clip in animations {
                    if !crate::timeline::valid_clip(clip, *meta) {
                        issues.push(Issue::warn(format!(
                            "Entidad #{}: el clip «{}» tiene celdas fuera de la hoja",
                            entity.id, clip.name
                        )));
                    }
                }
            }
        }
    }

    for id in scripts.ids() {
        if !scene.entities.iter().any(|entity| entity.id == id) {
            issues.push(Issue::warn(format!(
                "scripts.map.txt: comportamiento sin entidad #{id}"
            )));
        }
    }
    for diagnostic in crate::scripts::diagnostics(diagnostics_text) {
        issues.push(Issue::error(format!("Scripts: {diagnostic}")));
    }

    issues
}

fn duplicate_ids(entities: &[lunar_scenes::SceneEntity]) -> Vec<u64> {
    let mut seen = HashSet::new();
    let mut duplicates = Vec::new();
    for entity in entities {
        if !seen.insert(entity.id) {
            duplicates.push(entity.id);
        }
    }
    duplicates.sort_unstable();
    duplicates.dedup();
    duplicates
}

fn asset_exists(asset: &str, entries: &[AssetEntry], guids: &GuidIndex) -> bool {
    if crate::guids::is_guid(asset) {
        return entries
            .iter()
            .any(|entry| guids.guid_for(&entry.path) == Some(asset));
    }
    let resolved = guids.resolve(asset).unwrap_or_else(|| asset.to_owned());
    entries.iter().any(|entry| entry.path == resolved)
}

/// Dibuja el informe. El botón «Comprobar» recalcula el estado en memoria.
pub(crate) fn validation_ui(app: &mut EditorApp, ui: &mut Ui) {
    ui.heading("Validación de escena");
    if ui.button("Comprobar").clicked() {
        app.run_validation();
    }
    let errors = app
        .validation_issues
        .iter()
        .filter(|issue| issue.severity == Severity::Error)
        .count();
    let warnings = app.validation_issues.len() - errors;
    ui.label(format!(
        "{} problema(s) · {errors} error(es) · {warnings} aviso(s)",
        app.validation_issues.len()
    ));
    ui.separator();
    egui::ScrollArea::vertical().show(ui, |ui| {
        if app.validation_issues.is_empty() {
            ui.label("Sin problemas. Pulsa «Comprobar» para analizar la escena.");
        }
        for issue in &app.validation_issues {
            let (label, color) = match issue.severity {
                Severity::Error => ("Error", Color32::from_rgb(230, 90, 90)),
                Severity::Warn => ("Aviso", Color32::YELLOW),
            };
            ui.horizontal(|ui| {
                ui.colored_label(color, label);
                ui.label(&issue.message);
            });
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scripts::Binding;
    use lunar_scenes::{AnimationClip, SceneEntity, SpriteMeta};
    use std::collections::HashMap;

    fn meta() -> SpriteMeta {
        SpriteMeta {
            frame_width: 16,
            frame_height: 16,
            columns: 2,
            rows: 1,
            fps: 8.0,
            looping: true,
        }
    }

    #[test]
    fn a_clean_scene_has_no_issues() {
        let mut scene = Scene::new();
        let mut entity = SceneEntity::new(1);
        entity.components.push(Component::Image {
            asset: "hero.png".to_owned(),
        });
        scene.entities.push(entity);
        let entries = vec![AssetEntry {
            path: "hero.png".to_owned(),
            kind: crate::assets::AssetKind::Image,
        }];
        assert!(
            validate(
                &scene,
                &entries,
                &GuidIndex::default(),
                &ScriptMap::default(),
                ""
            )
            .is_empty()
        );
    }

    #[test]
    fn broken_references_clips_and_diagnostics_are_reported() {
        let mut scene = Scene::new();
        let mut entity = SceneEntity::new(1);
        entity.components.push(Component::Audio {
            asset: "falta.wav".to_owned(),
            volume: 1.0,
            autoplay: false,
        });
        entity.components.push(Component::PrefabRef {
            name: "inexistente".to_owned(),
        });
        entity.components.push(Component::Sprite {
            asset: "hero.png".to_owned(),
            meta: meta(),
            animations: vec![AnimationClip {
                name: "caminar".to_owned(),
                frames: vec![9],
                fps: 4.0,
                looping: true,
            }],
        });
        scene.entities.push(entity);
        let entries = vec![AssetEntry {
            path: "hero.png".to_owned(),
            kind: crate::assets::AssetKind::Image,
        }];
        let mut scripts = ScriptMap::default();
        scripts.set(99, Some(Binding::new(crate::scripts::Behavior::Spin)));

        let issues = validate(
            &scene,
            &entries,
            &GuidIndex::default(),
            &scripts,
            "1=Patrol;target=999\n",
        );

        assert!(
            issues
                .iter()
                .any(|issue| issue.message.contains("falta.wav"))
        );
        assert!(
            issues
                .iter()
                .any(|issue| issue.message.contains("inexistente"))
        );
        assert!(
            issues
                .iter()
                .any(|issue| issue.message.contains("celdas fuera"))
        );
        assert!(issues.iter().any(|issue| issue.message.contains("#99")));
        assert!(issues.iter().any(|issue| issue.severity == Severity::Error));
    }

    #[test]
    fn aliases_and_guids_resolve_existing_assets() {
        let mut scene = Scene::new();
        let mut entity = SceneEntity::new(1);
        entity.components.push(Component::Image {
            asset: "viejo.png".to_owned(),
        });
        scene.entities.push(entity);
        let guid = "0123456789abcdef0123456789abcdef".to_owned();
        let previous =
            GuidIndex::from_previous(HashMap::from([(guid.clone(), "viejo.png".to_owned())]));
        let guids = GuidIndex::from_pairs(&[("nuevo.png".to_owned(), guid.clone())], &previous);
        let entries = vec![AssetEntry {
            path: "nuevo.png".to_owned(),
            kind: crate::assets::AssetKind::Image,
        }];

        assert!(validate(&scene, &entries, &guids, &ScriptMap::default(), "").is_empty());
        if let Component::Image { asset } = &mut scene.entities[0].components[0] {
            *asset = guid;
        }
        assert!(validate(&scene, &entries, &guids, &ScriptMap::default(), "").is_empty());
    }
}
