//! Demo jugable de `v0.2.0`: escena JSON, glTF, PNG, WAV y editor headless.
//!
//! WASD mueve el triángulo, W también reproduce el WAV, S detiene el audio,
//! ArrowUp aplica un cambio con el editor, ArrowDown deshace y ArrowLeft rehace.
//! Escape cierra la ventana.

use std::error::Error;
use std::path::PathBuf;

use lunar::assets::{GltfStore, ImageStore};
use lunar::audio::{AudioHandle, AudioPlayer, AudioStore, SoundData};
use lunar::core::{Engine, EngineConfig, Key, SystemClock};
use lunar::editor::{EditorCommand, EditorDocument};
use lunar::platform::{PlatformApp, WindowConfig, run};
use lunar::render::Renderer;
use lunar::scenes::{Component, Scene};

struct Demo {
    engine: Engine,
    clock: SystemClock,
    document: EditorDocument,
    player: AudioPlayer,
    sound: SoundData,
    position: [f32; 2],
    color: [f32; 4],
    playing: Option<AudioHandle>,
    rename_alt: bool,
    autoplay: bool,
    autoplayed: bool,
}

impl PlatformApp for Demo {
    fn on_key(&mut self, key: Key, pressed: bool) {
        let input = self.engine.input_mut();
        if pressed {
            input.press(key);
        } else {
            input.release(key);
        }
    }

    fn on_resize(&mut self, _width: u32, _height: u32) {}

    fn on_frame(&mut self) -> bool {
        let input = self.engine.input().clone();
        let _tick = self.engine.update(&self.clock);

        if self.autoplay && !self.autoplayed {
            self.autoplayed = true;
            self.play_beep();
        }

        let speed = 0.035;
        if input.is_down(Key::A) {
            self.position[0] -= speed;
        }
        if input.is_down(Key::D) {
            self.position[0] += speed;
        }
        if input.is_down(Key::W) {
            self.position[1] += speed;
        }
        if input.is_down(Key::S) {
            self.position[1] -= speed;
        }

        if input.was_pressed(Key::W) {
            self.play_beep();
        }
        if input.was_pressed(Key::S) {
            self.player.stop_all();
            self.playing = None;
        }
        if input.was_pressed(Key::ArrowUp) {
            self.rename_alt = !self.rename_alt;
            let name = if self.rename_alt { "Nave demo" } else { "Nave" };
            match self.document.apply(EditorCommand::RenameEntity {
                id: 1,
                name: Some(name.to_owned()),
            }) {
                Ok(()) => println!(
                    "editor: renombrado a {name}; undo={}",
                    self.document.undo_len()
                ),
                Err(error) => eprintln!("editor: {error}"),
            }
        }
        if input.was_pressed(Key::ArrowDown) && self.document.undo() {
            println!("editor: undo; undo={}", self.document.undo_len());
        }
        if input.was_pressed(Key::ArrowLeft) && self.document.redo() {
            println!("editor: redo; undo={}", self.document.undo_len());
        }

        true
    }

    fn on_render(&mut self, renderer: &mut Renderer<'_>) {
        let [x, y] = self.position;
        let size = 0.16;
        renderer.set_triangle(
            [[x, y + size], [x - size, y - size], [x + size, y - size]],
            self.color,
        );
    }
}

impl Demo {
    fn play_beep(&mut self) {
        match self.player.play(&self.sound) {
            Ok(handle) => {
                if let Some(previous) = self.playing.take() {
                    self.player.stop(previous);
                }
                self.playing = Some(handle);
                println!("audio: reproduciendo beep");
            }
            Err(error) => eprintln!("audio: {error}"),
        }
    }
}

struct LoadedDemo {
    document: EditorDocument,
    color: [f32; 4],
    position: [f32; 2],
    sound: SoundData,
    volume: f32,
    autoplay: bool,
}

fn assets_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("assets")
}

fn load_scene() -> Result<LoadedDemo, Box<dyn Error>> {
    let assets = assets_dir();
    let mut scene = Scene::load_from_path(assets.join("demo.scene.json"))?;
    scene.normalize_asset_paths()?;

    let mut images = ImageStore::new();
    let mut gltfs = GltfStore::new();
    let mut audio = AudioStore::new();
    let mut color = [0.25, 0.86, 0.7, 1.0];
    let mut sound = None;
    let mut volume = 1.0;
    let mut autoplay = false;

    for entity in &scene.entities {
        for component in &entity.components {
            let Some(asset) = component.asset_path() else {
                continue;
            };
            let path = assets.join(asset);
            match component {
                Component::Image { .. } | Component::Sprite { .. } => {
                    let handle = images.load_from_path(&path)?;
                    let image = images.get(handle).ok_or("imagen no almacenada")?;
                    let pixel = &image.as_rgba()[..4];
                    color = [
                        f32::from(pixel[0]) / 255.0,
                        f32::from(pixel[1]) / 255.0,
                        f32::from(pixel[2]) / 255.0,
                        1.0,
                    ];
                    println!(
                        "imagen: {asset} {}x{} ({})",
                        image.width,
                        image.height,
                        image.as_rgba().len()
                    );
                }
                Component::Mesh { .. } => {
                    let handle = gltfs.load_from_path(&path)?;
                    let document = gltfs.get(handle).ok_or("glTF no almacenado")?;
                    println!(
                        "glTF: {asset} {} mallas, {} nodos, {} accesores",
                        document.meshes.len(),
                        document.nodes.len(),
                        document.accessors.len()
                    );
                }
                Component::Audio {
                    volume: level,
                    autoplay: play_on_start,
                    ..
                } => {
                    let handle = audio.load_from_path(&path)?;
                    sound = Some(audio.get(handle).ok_or("audio no almacenado")?.clone());
                    volume = *level;
                    autoplay = *play_on_start;
                    println!(
                        "audio: {asset} {} bytes, volumen {volume}, autoplay {autoplay}",
                        sound.as_ref().unwrap().len()
                    );
                }
                Component::Collider2D { .. }
                | Component::Collider3D { .. }
                | Component::PrefabRef { .. } => {}
            }
        }
    }

    let sound = sound.ok_or("la escena no contiene audio")?;
    let start = scene
        .entities
        .first()
        .map_or([0.0; 3], |entity| entity.transform.translation);
    let document = EditorDocument::from_scene(scene.clone())?;
    println!(
        "escena: {} ({} entidades, versión {})",
        scene.name.as_deref().unwrap_or("sin nombre"),
        scene.entities.len(),
        scene.version
    );
    Ok(LoadedDemo {
        document,
        color,
        position: [start[0], start[1]],
        sound,
        volume,
        autoplay,
    })
}

fn run_demo() -> Result<(), Box<dyn Error>> {
    let LoadedDemo {
        document,
        color,
        position,
        sound,
        volume,
        autoplay,
    } = load_scene()?;
    let config = WindowConfig::new("Lunar scene demo", 800, 600);
    let engine = Engine::new(EngineConfig::new("Lunar scene demo", 800, 600));
    let mut player = AudioPlayer::new();
    player.set_master_volume(volume);
    let mut app = Demo {
        engine,
        clock: SystemClock::new(),
        document,
        player,
        sound,
        position,
        color,
        playing: None,
        rename_alt: false,
        autoplay,
        autoplayed: false,
    };

    println!("WASD mueve; W reproduce; S detiene; ↑ renombra; ↓ deshace; ← rehace; Esc sale");
    run(config, &mut app)?;
    Ok(())
}

fn main() {
    if let Err(error) = run_demo() {
        eprintln!("error en la demo: {error}");
        std::process::exit(1);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn demo_assets_load_without_devices() {
        let loaded = load_scene().expect("los assets de la demo deben cargar");
        assert_eq!(loaded.position, [0.15, 0.0]);
        assert_eq!(
            loaded.color,
            [64.0 / 255.0, 220.0 / 255.0, 180.0 / 255.0, 1.0]
        );
        assert_eq!(loaded.volume, 0.4);
        assert!(loaded.autoplay);
        assert!(!loaded.sound.is_empty());
        assert_eq!(loaded.document.scene().name.as_deref(), Some("Lunar demo"));
    }
}
