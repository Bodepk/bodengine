//! Frontera `cxx` hacia la API pública C++ (ADR-004, ampliada en ADR-021).
//!
//! `Game` es una clase abstracta C++ que el juego implementa; Rust posee la
//! ventana, el reloj, la física y el render, y C++ solo entrega órdenes de
//! dibujo. Ningún tipo de `wgpu`, `Renderer` o Rapier cruza esta frontera.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::pin::Pin;

use std::time::Duration;

use lunar_assets::{ImageHandle, ImageStore};
use lunar_core::{
    Engine as CoreEngine, EngineConfig as CoreEngineConfig, InputState, Key, ManualClock,
    SystemClock, Tick,
};
use lunar_math::Vec2;
use lunar_physics2d::{
    BodyDesc2D, BodyId, BodyType2D, ColliderDesc2D, ColliderId, PhysicsWorld2D, Shape2D,
};
use lunar_platform::{PlatformApp, WindowConfig};
use lunar_render::{Renderer, SpriteDraw as RenderSpriteDraw, SpriteId};

use lunar_scenes::{BodyType, Component, Scene, Shape2D as SceneShape2D};

/// Motor opaco expuesto a C++.
pub struct Engine {
    inner: CoreEngine,
    manual_clock: ManualClock,
    system_clock: SystemClock,
    input_snapshot: InputState,
    last_tick: Tick,
    physics: PhysicsWorld2D,
    bodies: HashMap<u64, BodyId>,
    colliders: HashMap<u64, ColliderId>,
    collider_ids: HashMap<ColliderId, u64>,
    next_body: u64,
    next_collider: u64,
    assets_dir: PathBuf,
    images: ImageStore,
    image_ids: HashMap<u64, ImageHandle>,
    next_image: u64,
    scene: Option<Scene>,
    sprites: Vec<SpriteDef>,
    sprite_ids: HashMap<u64, usize>,
    uploaded: Vec<Option<SpriteId>>,
    next_sprite: u64,
    draws: Vec<PendingDraw>,
    invalid_draw: bool,
    viewport: ffi::ViewportInfo,
}

struct SpriteDef {
    image: u64,
    frame_width: u32,
    frame_height: u32,
}

struct PendingDraw {
    index: usize,
    frame: u32,
    flip_x: bool,
    center: [f32; 2],
    size: [f32; 2],
}

fn empty_tick() -> Tick {
    Tick {
        substeps: 0,
        alpha: 0.0,
        dropped_time: Duration::ZERO,
    }
}

fn key_code(key: Key) -> u32 {
    match key {
        Key::Escape => 1,
        Key::W => 2,
        Key::A => 3,
        Key::S => 4,
        Key::D => 5,
        Key::ArrowUp => 6,
        Key::ArrowDown => 7,
        Key::ArrowLeft => 8,
        Key::ArrowRight => 9,
        Key::Other => 0,
    }
}

fn from_key_code(code: u32) -> Option<Key> {
    match code {
        1 => Some(Key::Escape),
        2 => Some(Key::W),
        3 => Some(Key::A),
        4 => Some(Key::S),
        5 => Some(Key::D),
        6 => Some(Key::ArrowUp),
        7 => Some(Key::ArrowDown),
        8 => Some(Key::ArrowLeft),
        9 => Some(Key::ArrowRight),
        _ => None,
    }
}

/// Rechaza rutas absolutas y `..` antes de resolverlas dentro de `assets_dir`.
fn safe_asset_path(base: &Path, asset: &str) -> Option<PathBuf> {
    let relative = Path::new(asset);
    if relative.is_absolute()
        || relative.components().any(|component| {
            matches!(
                component,
                std::path::Component::ParentDir
                    | std::path::Component::RootDir
                    | std::path::Component::Prefix(_)
            )
        })
    {
        return None;
    }
    Some(base.join(relative))
}

impl Engine {
    fn title(&self) -> String {
        self.inner.config.title.clone()
    }

    fn width(&self) -> u32 {
        self.inner.config.width
    }

    fn height(&self) -> u32 {
        self.inner.config.height
    }

    fn tick_count(&self) -> u64 {
        self.inner.tick_count
    }

    fn frame_count(&self) -> u64 {
        self.inner.frame_count
    }

    fn stats(&self) -> ffi::FrameStats {
        ffi::FrameStats {
            substeps: self.last_tick.substeps,
            alpha: self.last_tick.alpha,
            dropped_ns: self.last_tick.dropped_time.as_nanos() as u64,
            frame_count: self.inner.frame_count,
            tick_count: self.inner.tick_count,
        }
    }

    fn substeps(&self) -> u32 {
        self.last_tick.substeps
    }

    fn alpha(&self) -> f32 {
        self.last_tick.alpha
    }

    /// Avanza el reloj inyectado y devuelve el resultado del frame.
    fn tick(&mut self, elapsed_ns: u64) -> ffi::FrameStats {
        self.input_snapshot = self.inner.input().clone();
        self.manual_clock
            .advance(std::time::Duration::from_nanos(elapsed_ns));
        self.last_tick = self.inner.update(&self.manual_clock);
        self.stats()
    }

    /// Avanza con el reloj del sistema; el host lo llama una vez por frame.
    fn tick_system(&mut self) -> ffi::FrameStats {
        self.input_snapshot = self.inner.input().clone();
        self.last_tick = self.inner.update(&self.system_clock);
        self.stats()
    }

    fn press_key(&mut self, code: u32, pressed: bool) {
        let Some(key) = from_key_code(code) else {
            return;
        };
        let input = self.inner.input_mut();
        if pressed {
            input.press(key);
        } else {
            input.release(key);
        }
    }

    fn is_key_down(&self, code: u32) -> bool {
        from_key_code(code).is_some_and(|key| self.inner.input().is_down(key))
    }

    /// Consulta la pulsación del frame, preservada antes de `Engine::update`.
    fn was_key_pressed(&self, code: u32) -> bool {
        from_key_code(code).is_some_and(|key| self.input_snapshot.was_pressed(key))
    }

    fn set_gravity(&mut self, x: f32, y: f32) {
        if x.is_finite() && y.is_finite() {
            self.physics.set_gravity(Vec2::new(x, y));
        }
    }

    fn add_body(&mut self, kind: u8, x: f32, y: f32) -> u64 {
        let body_type = match kind {
            1 => BodyType2D::Dynamic,
            2 => BodyType2D::Fixed,
            4 => BodyType2D::KinematicPositionBased,
            _ => return 0,
        };
        let Some(id) = self
            .physics
            .add_body(BodyDesc2D::new(body_type, Vec2::new(x, y)))
        else {
            return 0;
        };
        let public_id = self.next_body;
        self.next_body += 1;
        self.bodies.insert(public_id, id);
        public_id
    }

    fn add_collider(&mut self, body: u64, shape: u8, half_x: f32, half_y: f32) -> u64 {
        let Some(body_id) = self.bodies.get(&body).copied() else {
            return 0;
        };
        let shape = match shape {
            1 => Shape2D::Ball { radius: half_x },
            2 => Shape2D::Cuboid {
                half_extents: Vec2::new(half_x, half_y),
            },
            _ => return 0,
        };
        let Some(id) = self
            .physics
            .add_collider(body_id, ColliderDesc2D::new(shape))
        else {
            return 0;
        };
        let public_id = self.next_collider;
        self.next_collider += 1;
        self.colliders.insert(public_id, id);
        self.collider_ids.insert(id, public_id);
        public_id
    }

    fn body_state(&self, body: u64) -> ffi::BodyState {
        let Some(id) = self.bodies.get(&body) else {
            return ffi::BodyState::default();
        };
        let Some(state) = self.physics.body_state(*id) else {
            return ffi::BodyState::default();
        };
        ffi::BodyState {
            valid: true,
            x: state.position.x,
            y: state.position.y,
            vx: state.linvel.x,
            vy: state.linvel.y,
        }
    }

    fn apply_impulse(&mut self, body: u64, x: f32, y: f32) -> bool {
        let Some(id) = self.bodies.get(&body).copied() else {
            return false;
        };
        self.physics.apply_impulse(id, Vec2::new(x, y))
    }

    fn cast_ray(
        &self,
        origin_x: f32,
        origin_y: f32,
        direction_x: f32,
        direction_y: f32,
        max_toi: f32,
    ) -> ffi::RayHit {
        let Some(hit) = self.physics.cast_ray(
            Vec2::new(origin_x, origin_y),
            Vec2::new(direction_x, direction_y),
            max_toi,
        ) else {
            return ffi::RayHit::default();
        };
        ffi::RayHit {
            hit: true,
            collider: self.collider_ids.get(&hit.collider).copied().unwrap_or(0),
            toi: hit.toi,
        }
    }

    fn step_physics(&mut self, dt: f32) -> bool {
        self.physics.step(dt).is_ok()
    }

    fn load_scene(&mut self, path: &str) -> bool {
        let Some(path) = safe_asset_path(&self.assets_dir, path) else {
            return false;
        };
        let Ok(mut scene) = Scene::load_from_path(path) else {
            return false;
        };
        if scene.normalize_asset_paths().is_err() {
            return false;
        }
        self.scene = Some(scene);
        true
    }

    fn entity_count(&self) -> u32 {
        self.scene
            .as_ref()
            .map_or(0, |scene| scene.entities.len() as u32)
    }

    fn entity(&self, index: u32) -> ffi::EntityInfo {
        let Some(entity) = self
            .scene
            .as_ref()
            .and_then(|scene| scene.entities.get(index as usize))
        else {
            return ffi::EntityInfo::default();
        };
        let mut info = ffi::EntityInfo {
            id: entity.id,
            name: entity.name.clone().unwrap_or_default(),
            x: entity.transform.translation[0],
            y: entity.transform.translation[1],
            scale_x: entity.transform.scale[0],
            scale_y: entity.transform.scale[1],
            ..Default::default()
        };
        for component in &entity.components {
            match component {
                Component::Image { asset } => info.image_asset = asset.clone(),
                Component::Collider2D { shape, body } => {
                    info.body_kind = match body {
                        BodyType::Dynamic => 1,
                        BodyType::Fixed => 2,
                        BodyType::Sensor => 3,
                    };
                    match shape {
                        SceneShape2D::Ball { radius } => {
                            info.shape_kind = 1;
                            info.half_x = *radius;
                            info.half_y = *radius;
                        }
                        SceneShape2D::Cuboid { half_extents } => {
                            info.shape_kind = 2;
                            info.half_x = half_extents[0];
                            info.half_y = half_extents[1];
                        }
                    }
                }
                _ => {}
            }
        }
        info
    }

    fn load_image(&mut self, asset: &str) -> u64 {
        let Some(path) = safe_asset_path(&self.assets_dir, asset) else {
            return 0;
        };
        let Ok(handle) = self.images.load_from_path(path) else {
            return 0;
        };
        let public_id = self.next_image;
        self.next_image += 1;
        self.image_ids.insert(public_id, handle);
        public_id
    }

    fn define_sprite(&mut self, image: u64, frame_width: u32, frame_height: u32) -> u64 {
        if !self.image_ids.contains_key(&image) || frame_width == 0 || frame_height == 0 {
            return 0;
        }
        let public_id = self.next_sprite;
        self.next_sprite += 1;
        let index = self.sprites.len();
        self.sprites.push(SpriteDef {
            image,
            frame_width,
            frame_height,
        });
        self.uploaded.push(None);
        self.sprite_ids.insert(public_id, index);
        public_id
    }

    fn viewport(&self) -> ffi::ViewportInfo {
        self.viewport
    }

    /// C++ emite órdenes; Rust las valida y las envía al renderer en `on_render`.
    fn submit_sprites(&mut self, draws: &[ffi::SpriteDrawCpp]) -> bool {
        let mut pending = Vec::with_capacity(draws.len());
        for draw in draws {
            let Some(index) = self.sprite_ids.get(&draw.sprite).copied() else {
                self.invalid_draw = true;
                return false;
            };
            pending.push(PendingDraw {
                index,
                frame: draw.frame,
                flip_x: draw.flip_x,
                center: [draw.center_x, draw.center_y],
                size: [draw.size_x, draw.size_y],
            });
        }
        self.draws = pending;
        true
    }

    fn begin_render(&mut self) {
        self.draws.clear();
        self.invalid_draw = false;
    }

    /// Sube las texturas pendientes y presenta la lista acumulada del frame.
    fn flush_render(&mut self, renderer: &mut Renderer<'_>) -> bool {
        if self.invalid_draw {
            renderer.clear_sprites();
            return false;
        }
        for (index, def) in self.sprites.iter().enumerate() {
            if self.uploaded[index].is_some() {
                continue;
            }
            let Some(image) = self
                .image_ids
                .get(&def.image)
                .and_then(|handle| self.images.get(*handle))
            else {
                renderer.clear_sprites();
                return false;
            };
            match renderer.define_sprite(
                image.as_rgba(),
                image.width,
                image.height,
                [def.frame_width, def.frame_height],
            ) {
                Ok(id) => self.uploaded[index] = Some(id),
                Err(_) => {
                    renderer.clear_sprites();
                    return false;
                }
            }
        }
        let mut converted = Vec::with_capacity(self.draws.len());
        for draw in &self.draws {
            let Some(id) = self.uploaded[draw.index] else {
                renderer.clear_sprites();
                return false;
            };
            converted.push(RenderSpriteDraw {
                sprite: id,
                frame: draw.frame,
                flip_x: draw.flip_x,
                center: draw.center,
                size: draw.size,
            });
        }
        if renderer.set_sprites(&converted).is_err() {
            renderer.clear_sprites();
            return false;
        }
        true
    }
}

/// Crea el motor opaco desde la configuración del smoke de reloj manual.
fn new_engine(config: ffi::EngineConfig) -> Box<Engine> {
    new_with_config(
        CoreEngineConfig::new(config.title, config.width, config.height),
        PathBuf::new(),
    )
}

/// Crea el motor opaco con la carpeta de assets del juego C++.
fn new_game_engine(config: ffi::GameConfig) -> Box<Engine> {
    new_with_config(
        CoreEngineConfig::new(config.title, config.width, config.height),
        PathBuf::from(config.assets_dir),
    )
}

fn new_with_config(config: CoreEngineConfig, assets_dir: PathBuf) -> Box<Engine> {
    let viewport = ffi::ViewportInfo {
        width: config.width,
        height: config.height,
    };
    Box::new(Engine {
        inner: CoreEngine::new(config),
        manual_clock: ManualClock::default(),
        system_clock: SystemClock::new(),
        input_snapshot: InputState::default(),
        last_tick: empty_tick(),
        physics: PhysicsWorld2D::new(Vec2::new(0.0, -9.81)),
        bodies: HashMap::new(),
        colliders: HashMap::new(),
        collider_ids: HashMap::new(),
        next_body: 1,
        next_collider: 1,
        assets_dir,
        images: ImageStore::new(),
        image_ids: HashMap::new(),
        next_image: 1,
        scene: None,
        sprites: Vec::new(),
        sprite_ids: HashMap::new(),
        uploaded: Vec::new(),
        next_sprite: 1,
        draws: Vec::new(),
        invalid_draw: false,
        viewport,
    })
}

struct Host<'a> {
    engine: &'a mut Engine,
    game: Pin<&'a mut ffi::Game>,
}

impl PlatformApp for Host<'_> {
    fn on_key(&mut self, key: Key, pressed: bool) {
        let code = key_code(key);
        if code == 0 {
            return;
        }
        self.engine.press_key(code, pressed);
        self.game.as_mut().on_key(code, pressed);
    }

    fn on_resize(&mut self, width: u32, height: u32) {
        self.engine.viewport = ffi::ViewportInfo { width, height };
        self.game.as_mut().on_resize(width, height);
    }

    fn on_frame(&mut self) -> bool {
        self.engine.tick_system();
        self.game.as_mut().on_frame()
    }

    fn on_render(&mut self, renderer: &mut Renderer<'_>) {
        if let Some(viewport) = renderer.viewport() {
            self.engine.viewport = ffi::ViewportInfo {
                width: viewport.width,
                height: viewport.height,
            };
        }
        self.engine.begin_render();
        self.game.as_mut().on_render();
        if !self.engine.flush_render(renderer) {
            eprintln!("no se pudieron enviar las órdenes de dibujo de C++");
        }
    }
}

/// Crea la ventana y conduce el bucle hasta que C++ pide salir.
fn run(config: ffi::GameConfig, engine: &mut Engine, game: Pin<&mut ffi::Game>) -> bool {
    let mut host = Host { engine, game };
    lunar_platform::run(
        WindowConfig::new(config.title, config.width, config.height),
        &mut host,
    )
    .is_ok()
}

#[cxx::bridge]
mod ffi {
    /// Configuración compartida con C++.
    struct EngineConfig {
        title: String,
        width: u32,
        height: u32,
    }

    /// Configuración del juego, con la carpeta raíz de assets.
    struct GameConfig {
        title: String,
        width: u32,
        height: u32,
        assets_dir: String,
    }

    /// Resultado de un frame, sin `Duration` en la ABI.
    #[derive(Debug, Default, Clone, Copy, PartialEq)]
    struct FrameStats {
        substeps: u32,
        alpha: f32,
        dropped_ns: u64,
        frame_count: u64,
        tick_count: u64,
    }

    #[derive(Debug, Default, Clone, Copy, PartialEq)]
    struct ViewportInfo {
        width: u32,
        height: u32,
    }

    /// Datos POD de una entidad de la escena v1.
    #[derive(Debug, Default, Clone, PartialEq)]
    struct EntityInfo {
        id: u64,
        name: String,
        x: f32,
        y: f32,
        scale_x: f32,
        scale_y: f32,
        image_asset: String,
        body_kind: u8,
        shape_kind: u8,
        half_x: f32,
        half_y: f32,
    }

    #[derive(Debug, Default, Clone, Copy, PartialEq)]
    struct BodyState {
        valid: bool,
        x: f32,
        y: f32,
        vx: f32,
        vy: f32,
    }

    #[derive(Debug, Default, Clone, Copy, PartialEq)]
    struct RayHit {
        hit: bool,
        collider: u64,
        toi: f32,
    }

    /// Orden de dibujo en NDC; `sprite` es un handle de esta bridge.
    #[derive(Debug, Default, Clone, Copy, PartialEq)]
    struct SpriteDrawCpp {
        sprite: u64,
        frame: u32,
        flip_x: bool,
        center_x: f32,
        center_y: f32,
        size_x: f32,
        size_y: f32,
    }

    unsafe extern "C++" {
        include!("lunar_bridge/game.h");

        /// Lógica de juego implementada en C++; el bucle la llama desde Rust.
        type Game;

        fn on_key(self: Pin<&mut Game>, code: u32, pressed: bool);
        fn on_resize(self: Pin<&mut Game>, width: u32, height: u32);
        fn on_frame(self: Pin<&mut Game>) -> bool;
        fn on_render(self: Pin<&mut Game>);
    }

    extern "Rust" {
        type Engine;

        fn new_engine(config: EngineConfig) -> Box<Engine>;
        fn new_game_engine(config: GameConfig) -> Box<Engine>;
        fn run(config: GameConfig, engine: &mut Engine, game: Pin<&mut Game>) -> bool;

        fn title(self: &Engine) -> String;
        fn width(self: &Engine) -> u32;
        fn height(self: &Engine) -> u32;
        fn tick_count(self: &Engine) -> u64;
        fn frame_count(self: &Engine) -> u64;
        fn substeps(self: &Engine) -> u32;
        fn alpha(self: &Engine) -> f32;
        fn tick(self: &mut Engine, elapsed_ns: u64) -> FrameStats;
        fn tick_system(self: &mut Engine) -> FrameStats;

        fn press_key(self: &mut Engine, code: u32, pressed: bool);
        fn is_key_down(self: &Engine, code: u32) -> bool;
        fn was_key_pressed(self: &Engine, code: u32) -> bool;

        fn set_gravity(self: &mut Engine, x: f32, y: f32);
        fn add_body(self: &mut Engine, kind: u8, x: f32, y: f32) -> u64;
        fn add_collider(self: &mut Engine, body: u64, shape: u8, half_x: f32, half_y: f32) -> u64;
        fn body_state(self: &Engine, body: u64) -> BodyState;
        fn apply_impulse(self: &mut Engine, body: u64, x: f32, y: f32) -> bool;
        fn cast_ray(
            self: &Engine,
            origin_x: f32,
            origin_y: f32,
            direction_x: f32,
            direction_y: f32,
            max_toi: f32,
        ) -> RayHit;
        fn step_physics(self: &mut Engine, dt: f32) -> bool;

        fn load_scene(self: &mut Engine, path: &str) -> bool;
        fn entity_count(self: &Engine) -> u32;
        fn entity(self: &Engine, index: u32) -> EntityInfo;
        fn load_image(self: &mut Engine, asset: &str) -> u64;
        fn define_sprite(self: &mut Engine, image: u64, frame_width: u32, frame_height: u32)
        -> u64;
        fn viewport(self: &Engine) -> ViewportInfo;
        fn submit_sprites(self: &mut Engine, draws: &[SpriteDrawCpp]) -> bool;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use lunar_core::FIXED_DT;

    fn config() -> ffi::EngineConfig {
        ffi::EngineConfig {
            title: "smoke".to_owned(),
            width: 800,
            height: 600,
        }
    }

    fn game_config() -> ffi::GameConfig {
        ffi::GameConfig {
            title: "game".to_owned(),
            width: 960,
            height: 540,
            assets_dir: Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("../../examples/sprite_platformer/assets")
                .to_string_lossy()
                .into_owned(),
        }
    }

    #[test]
    fn shared_config_reaches_opaque_engine() {
        let engine = new_engine(config());
        assert_eq!(engine.title(), "smoke");
        assert_eq!((engine.width(), engine.height()), (800, 600));
        assert_eq!(engine.viewport().width, 800);
        assert_eq!(engine.viewport().height, 600);
    }

    #[test]
    fn tick_uses_injected_time_instead_of_system_clock() {
        let mut engine = new_engine(config());
        let mut final_stats = ffi::FrameStats::default();

        // El primer frame solo ancla el reloj, como en `lunar_core::Engine`.
        for _ in 0..61 {
            final_stats = engine.tick(FIXED_DT.as_nanos() as u64);
        }

        assert_eq!(final_stats.substeps, 1);
        assert_eq!(final_stats.tick_count, 60);
        assert_eq!(final_stats.frame_count, 61);
        assert_eq!(engine.tick_count(), 60);
    }

    #[test]
    fn key_codes_round_trip_through_the_bridge() {
        for key in [
            Key::Escape,
            Key::W,
            Key::A,
            Key::S,
            Key::D,
            Key::ArrowUp,
            Key::ArrowDown,
            Key::ArrowLeft,
            Key::ArrowRight,
        ] {
            assert_eq!(from_key_code(key_code(key)), Some(key));
        }
        assert_eq!(key_code(Key::Other), 0);
        assert_eq!(from_key_code(0), None);
    }

    #[test]
    fn input_state_survives_the_tick_for_cpp_logic() {
        let mut engine = new_game_engine(game_config());
        engine.tick(FIXED_DT.as_nanos() as u64);
        engine.press_key(2, true);

        assert!(engine.is_key_down(2));
        let _stats = engine.tick_system();
        assert!(engine.was_key_pressed(2));
        engine.tick_system();
        assert!(!engine.was_key_pressed(2));
    }

    #[test]
    fn scene_entities_are_exposed_as_pod() {
        let mut engine = new_game_engine(game_config());
        assert!(engine.load_scene("platformer.scene.json"));
        assert_eq!(engine.entity_count(), 9);

        let ground = (0..engine.entity_count())
            .map(|index| engine.entity(index))
            .find(|entity| entity.body_kind == 2)
            .expect("cuerpo fijo");
        assert_eq!(ground.shape_kind, 2);
        assert!(ground.half_x > 0.0 && ground.half_y > 0.0);
    }

    #[test]
    fn physics_handles_and_ray_use_the_bridge_ids() {
        let mut engine = new_game_engine(game_config());
        engine.set_gravity(0.0, 0.0);
        let body = engine.add_body(2, 0.0, 0.0);
        let collider = engine.add_collider(body, 1, 1.0, 1.0);
        assert_ne!(body, 0);
        assert_ne!(collider, 0);
        assert!(engine.step_physics(1.0 / 60.0));

        let hit = engine.cast_ray(0.0, 5.0, 0.0, -1.0, 10.0);
        assert!(hit.hit);
        assert_eq!(hit.collider, collider);
        assert!((3.9..4.1).contains(&hit.toi));

        assert!(engine.body_state(body).valid);
        assert!(!engine.body_state(999).valid);
        assert!(engine.apply_impulse(body, 1.0, 0.0));
        assert!(!engine.apply_impulse(999, 1.0, 0.0));
    }

    #[test]
    fn sprites_are_registered_without_a_gpu_and_validated_on_submit() {
        let mut engine = new_game_engine(game_config());
        let image = engine.load_image("sprites/goal_end.png");
        assert_ne!(image, 0);
        assert_eq!(engine.load_image("../../secret.png"), 0);

        let sprite = engine.define_sprite(image, 64, 64);
        assert_ne!(sprite, 0);
        assert_eq!(engine.define_sprite(999, 64, 64), 0);

        let draw = ffi::SpriteDrawCpp {
            sprite,
            center_x: 0.0,
            center_y: 0.0,
            size_x: 0.5,
            size_y: 0.5,
            ..Default::default()
        };
        assert!(engine.submit_sprites(&[draw]));
        assert!(!engine.submit_sprites(&[ffi::SpriteDrawCpp {
            sprite: 999,
            ..Default::default()
        }]));
    }

    #[test]
    fn asset_paths_cannot_escape_the_configured_directory() {
        let engine = new_game_engine(game_config());
        assert!(safe_asset_path(&engine.assets_dir, "sprites/goal_end.png").is_some());
        assert!(safe_asset_path(&engine.assets_dir, "../secret.png").is_none());
        assert!(safe_asset_path(&engine.assets_dir, "/etc/passwd").is_none());
    }
}
