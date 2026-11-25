//! Frontera `cxx` hacia la API pública C++ (ADR-004, ampliada en ADR-021).
//!
//! `Game` es una clase abstracta C++ que el juego implementa; Rust posee la
//! ventana, el reloj, la física y el render, y C++ solo entrega órdenes de
//! dibujo. Ningún tipo de `wgpu`, `Renderer` o Rapier cruza esta frontera.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::pin::Pin;

use std::time::{Duration, SystemTime};

use lunar_assets::{
    ImageHandle, ImageStore,
    gltf::{GltfHandle, GltfStore},
};
use lunar_audio::{AudioHandle, AudioPlayer, AudioStore, SoundHandle};
use lunar_core::{
    Engine as CoreEngine, EngineConfig as CoreEngineConfig, InputState, Key, ManualClock,
    SystemClock, Tick,
};
use lunar_math::{Vec2, Vec3};
use lunar_physics2d::{
    BodyDesc2D, BodyId, BodyType2D, ColliderDesc2D, ColliderId, PhysicsWorld2D, Shape2D,
};
use lunar_physics3d::{
    BodyDesc3D, BodyId as BodyId3D, BodyType3D, ColliderDesc3D, ColliderId as ColliderId3D,
    PhysicsWorld3D, Shape3D as PhysicsShape3D,
};
use lunar_platform::{PlatformApp, WindowConfig};
use lunar_render::{Renderer, SpriteDraw as RenderSpriteDraw, SpriteId};
use lunar_render3d::{MeshDraw, MeshId, Renderer3D};

use lunar_editor::{EditorCommand, EditorDocument};
use lunar_scenes::{BodyType, Component, Scene, Shape2D as SceneShape2D, Transform};

/// Motor opaco expuesto a C++.
pub struct Engine {
    inner: CoreEngine,
    manual_clock: ManualClock,
    system_clock: SystemClock,
    input_snapshot: InputState,
    mouse: MouseState,
    mouse_snapshot: MouseSnapshot,
    last_tick: Tick,
    physics: PhysicsWorld2D,
    physics3d: PhysicsWorld3D,
    bodies: HashMap<u64, BodyId>,
    colliders: HashMap<u64, ColliderId>,
    collider_ids: HashMap<ColliderId, u64>,
    next_body: u64,
    next_collider: u64,
    bodies3d: HashMap<u64, BodyId3D>,
    colliders3d: HashMap<u64, ColliderId3D>,
    collider_ids3d: HashMap<ColliderId3D, u64>,
    next_body3d: u64,
    next_collider3d: u64,
    assets_dir: PathBuf,
    images: ImageStore,
    image_ids: HashMap<u64, ImageHandle>,
    next_image: u64,
    gltfs: GltfStore,
    gltf_ids: HashMap<u64, GltfHandle>,
    next_gltf: u64,
    render3d: Renderer3D,
    sounds: AudioStore,
    sound_ids: HashMap<u64, SoundHandle>,
    next_sound: u64,
    player: AudioPlayer,
    plays: HashMap<u64, AudioHandle>,
    next_play: u64,
    editor: Option<EditorDocument>,
    scene_path: Option<PathBuf>,
    scene_relative: String,
    scene_stamp: Option<SceneStamp>,
    sprites: Vec<SpriteDef>,
    sprite_ids: HashMap<u64, usize>,
    uploaded: Vec<Option<SpriteId>>,
    next_sprite: u64,
    draws: Vec<PendingDraw>,
    invalid_draw: bool,
    viewport: ffi::ViewportInfo,
}

/// Huella barata del archivo de escena para la recarga cooperativa.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct SceneStamp {
    modified: SystemTime,
    len: u64,
    hash: u64,
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

#[derive(Default)]
struct MouseState {
    down: HashSet<u32>,
    just_pressed: HashSet<u32>,
    just_released: HashSet<u32>,
    cursor_x: f64,
    cursor_y: f64,
    cursor_valid: bool,
    scroll_x: f32,
    scroll_y: f32,
    scroll_pixels: bool,
    scroll_pending: bool,
}

impl MouseState {
    fn snapshot(&self) -> MouseSnapshot {
        MouseSnapshot {
            pressed: self.just_pressed.clone(),
            released: self.just_released.clone(),
            scroll_x: self.scroll_x,
            scroll_y: self.scroll_y,
            scroll_pixels: self.scroll_pixels,
            scroll_pending: self.scroll_pending,
        }
    }

    fn end_frame(&mut self) {
        self.just_pressed.clear();
        self.just_released.clear();
        self.scroll_pending = false;
        self.scroll_x = 0.0;
        self.scroll_y = 0.0;
    }
}

#[derive(Default)]
struct MouseSnapshot {
    pressed: HashSet<u32>,
    released: HashSet<u32>,
    scroll_x: f32,
    scroll_y: f32,
    scroll_pixels: bool,
    scroll_pending: bool,
}

fn is_mouse_button(button: u32) -> bool {
    (1..=5).contains(&button)
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
    /// Escena del documento activo; todas las lecturas pasan por el editor.
    fn scene(&self) -> Option<&Scene> {
        self.editor.as_ref().map(EditorDocument::scene)
    }

    /// Huella del archivo: fecha, longitud y hash FNV-1a, sin dependencias nuevas.
    fn file_stamp(path: &Path) -> Option<SceneStamp> {
        let modified = std::fs::metadata(path).ok()?.modified().ok()?;
        let bytes = std::fs::read(path).ok()?;
        let mut hash = 0xcbf2_9ce4_8422_2325_u64;
        for byte in &bytes {
            hash ^= u64::from(*byte);
            hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
        }
        Some(SceneStamp {
            modified,
            len: bytes.len() as u64,
            hash,
        })
    }

    /// Instala un documento y recuerda la ruta y la marca de tiempo leídas.
    fn install_scene(&mut self, document: EditorDocument, relative: &str, path: &Path) {
        self.editor = Some(document);
        self.scene_relative = relative.to_owned();
        self.scene_path = Some(path.to_path_buf());
        self.scene_stamp = Self::file_stamp(path);
    }

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
        self.mouse_snapshot = self.mouse.snapshot();
        self.mouse.end_frame();
        self.manual_clock
            .advance(std::time::Duration::from_nanos(elapsed_ns));
        self.last_tick = self.inner.update(&self.manual_clock);
        self.stats()
    }

    /// Avanza con el reloj del sistema; el host lo llama una vez por frame.
    fn tick_system(&mut self) -> ffi::FrameStats {
        self.input_snapshot = self.inner.input().clone();
        self.mouse_snapshot = self.mouse.snapshot();
        self.mouse.end_frame();
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

    fn mouse_state(&self, button: u32) -> ffi::MouseButtonState {
        ffi::MouseButtonState {
            down: is_mouse_button(button) && self.mouse.down.contains(&button),
            pressed: self.mouse_snapshot.pressed.contains(&button),
            released: self.mouse_snapshot.released.contains(&button),
        }
    }

    fn press_mouse(&mut self, button: u32, pressed: bool) {
        if !is_mouse_button(button) {
            return;
        }
        if pressed {
            if self.mouse.down.insert(button) {
                self.mouse.just_pressed.insert(button);
            }
        } else if self.mouse.down.remove(&button) {
            self.mouse.just_released.insert(button);
        }
    }

    fn set_cursor(&mut self, x: f64, y: f64) {
        if x.is_finite() && y.is_finite() {
            self.mouse.cursor_x = x;
            self.mouse.cursor_y = y;
            self.mouse.cursor_valid = true;
        }
    }

    fn cursor_position(&self) -> ffi::CursorPoint {
        ffi::CursorPoint {
            x: self.mouse.cursor_x as f32,
            y: self.mouse.cursor_y as f32,
            valid: self.mouse.cursor_valid,
        }
    }

    fn cursor_ndc(&self) -> ffi::CursorPoint {
        if !self.mouse.cursor_valid {
            return ffi::CursorPoint::default();
        }
        self.to_ndc(self.mouse.cursor_x as f32, self.mouse.cursor_y as f32)
    }

    fn screen_to_ndc(&self, x: f32, y: f32) -> ffi::CursorPoint {
        self.to_ndc(x, y)
    }

    fn to_ndc(&self, x: f32, y: f32) -> ffi::CursorPoint {
        let width = self.viewport.width;
        let height = self.viewport.height;
        if width == 0 || height == 0 || !x.is_finite() || !y.is_finite() {
            return ffi::CursorPoint::default();
        }
        ffi::CursorPoint {
            x: x / width as f32 * 2.0 - 1.0,
            y: 1.0 - y / height as f32 * 2.0,
            valid: true,
        }
    }

    fn add_scroll(&mut self, x: f32, y: f32, pixels: bool) {
        if !x.is_finite() || !y.is_finite() {
            return;
        }
        if !self.mouse.scroll_pending || self.mouse.scroll_pixels != pixels {
            self.mouse.scroll_x = 0.0;
            self.mouse.scroll_y = 0.0;
            self.mouse.scroll_pixels = pixels;
        }
        self.mouse.scroll_x += x;
        self.mouse.scroll_y += y;
        self.mouse.scroll_pending = true;
    }

    fn scroll_delta(&self) -> ffi::ScrollDelta {
        ffi::ScrollDelta {
            x: self.mouse_snapshot.scroll_x,
            y: self.mouse_snapshot.scroll_y,
            pixels: self.mouse_snapshot.scroll_pixels,
            valid: self.mouse_snapshot.scroll_pending,
        }
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

    fn set_gravity3d(&mut self, x: f32, y: f32, z: f32) {
        if x.is_finite() && y.is_finite() && z.is_finite() {
            self.physics3d.set_gravity(Vec3::new(x, y, z));
        }
    }

    fn add_body3d(&mut self, kind: u8, x: f32, y: f32, z: f32) -> u64 {
        let body_type = match kind {
            1 => BodyType3D::Dynamic,
            2 => BodyType3D::Fixed,
            3 => BodyType3D::KinematicPositionBased,
            _ => return 0,
        };
        if ![x, y, z].iter().all(|value| value.is_finite()) {
            return 0;
        }
        let Some(id) = self
            .physics3d
            .add_body(BodyDesc3D::new(body_type, Vec3::new(x, y, z)))
        else {
            return 0;
        };
        let public_id = self.next_body3d;
        self.next_body3d += 1;
        self.bodies3d.insert(public_id, id);
        public_id
    }

    fn add_collider3d(
        &mut self,
        body: u64,
        shape: u8,
        half_x: f32,
        half_y: f32,
        half_z: f32,
    ) -> u64 {
        let Some(body_id) = self.bodies3d.get(&body).copied() else {
            return 0;
        };
        let shape = match shape {
            1 => PhysicsShape3D::Ball {
                radius: half_x.max(0.0),
            },
            2 => PhysicsShape3D::Cuboid {
                half_extents: Vec3::new(half_x.max(0.0), half_y.max(0.0), half_z.max(0.0)),
            },
            3 => PhysicsShape3D::Capsule {
                half_height: half_x.max(0.0),
                radius: half_y.max(0.0),
            },
            _ => return 0,
        };
        if !half_x.is_finite() || !half_y.is_finite() || !half_z.is_finite() {
            return 0;
        }
        let Some(id) = self
            .physics3d
            .add_collider(body_id, ColliderDesc3D::new(shape))
        else {
            return 0;
        };
        let public_id = self.next_collider3d;
        self.next_collider3d += 1;
        self.colliders3d.insert(public_id, id);
        self.collider_ids3d.insert(id, public_id);
        public_id
    }

    fn body_state3d(&self, body: u64) -> ffi::BodyState3DCpp {
        let Some(id) = self.bodies3d.get(&body).copied() else {
            return ffi::BodyState3DCpp::default();
        };
        let Some(state) = self.physics3d.body_state(id) else {
            return ffi::BodyState3DCpp::default();
        };
        ffi::BodyState3DCpp {
            valid: true,
            x: state.position.x,
            y: state.position.y,
            z: state.position.z,
            qx: state.rotation.x,
            qy: state.rotation.y,
            qz: state.rotation.z,
            qw: state.rotation.w,
            vx: state.linvel.x,
            vy: state.linvel.y,
            vz: state.linvel.z,
        }
    }

    fn apply_impulse3d(&mut self, body: u64, x: f32, y: f32, z: f32) -> bool {
        let Some(id) = self.bodies3d.get(&body).copied() else {
            return false;
        };
        self.physics3d.apply_impulse(id, Vec3::new(x, y, z))
    }

    #[allow(clippy::too_many_arguments)] // origen, dirección y distancia como escalares POD
    fn cast_ray3d(
        &self,
        origin_x: f32,
        origin_y: f32,
        origin_z: f32,
        direction_x: f32,
        direction_y: f32,
        direction_z: f32,
        max_toi: f32,
    ) -> ffi::RayHit3DCpp {
        let Some(hit) = self.physics3d.cast_ray(
            Vec3::new(origin_x, origin_y, origin_z),
            Vec3::new(direction_x, direction_y, direction_z),
            max_toi,
        ) else {
            return ffi::RayHit3DCpp::default();
        };
        ffi::RayHit3DCpp {
            hit: true,
            collider: self.collider_ids3d.get(&hit.collider).copied().unwrap_or(0),
            toi: hit.toi,
        }
    }

    fn step_physics3d(&mut self, dt: f32) -> bool {
        self.physics3d.step(dt).is_ok()
    }

    fn load_gltf(&mut self, asset: &str) -> u64 {
        let Some(path) = safe_asset_path(&self.assets_dir, asset) else {
            return 0;
        };
        let Ok(handle) = self.gltfs.load_from_path(path) else {
            return 0;
        };
        let public_id = self.next_gltf;
        self.next_gltf += 1;
        self.gltf_ids.insert(public_id, handle);
        public_id
    }

    fn define_mesh(&mut self, gltf: u64, mesh: u32, primitive: u32) -> u64 {
        let Some(handle) = self.gltf_ids.get(&gltf).copied() else {
            return 0;
        };
        let Some(document) = self.gltfs.get(handle) else {
            return 0;
        };
        let Ok(id) = self
            .render3d
            .from_gltf(document, mesh as usize, primitive as usize)
        else {
            return 0;
        };
        id.get()
    }

    fn mesh_vertex_count(&self, mesh: u64) -> u32 {
        self.render3d
            .vertex_count(MeshId::from_u64(mesh))
            .unwrap_or_default() as u32
    }

    fn mesh_index_count(&self, mesh: u64) -> u32 {
        self.render3d
            .index_count(MeshId::from_u64(mesh))
            .unwrap_or_default() as u32
    }

    #[allow(clippy::too_many_arguments)] // parámetros vectoriales explícitos para la ABI C++
    fn set_camera3d(
        &mut self,
        eye_x: f32,
        eye_y: f32,
        eye_z: f32,
        target_x: f32,
        target_y: f32,
        target_z: f32,
        up_x: f32,
        up_y: f32,
        up_z: f32,
        fov: f32,
        aspect: f32,
        near: f32,
        far: f32,
    ) -> bool {
        self.render3d.set_camera(
            [eye_x, eye_y, eye_z],
            [target_x, target_y, target_z],
            [up_x, up_y, up_z],
            fov,
            aspect,
            near,
            far,
        )
    }

    fn view_projection3d(&self) -> ffi::Mat4Cpp {
        ffi::Mat4Cpp {
            values: self.render3d.view_projection(),
        }
    }

    fn submit_mesh_draws(&mut self, draws: &[ffi::MeshDrawCpp]) -> bool {
        let pending: Vec<MeshDraw> = draws
            .iter()
            .map(|draw| MeshDraw {
                mesh: draw.mesh,
                model: draw.model,
                color: draw.color,
            })
            .collect();
        self.render3d.submit(&pending)
    }

    fn mesh_draw_count(&self) -> u32 {
        self.render3d.draws().len() as u32
    }

    fn mesh_draw(&self, index: u32) -> ffi::MeshDrawCpp {
        let Some(draw) = self.render3d.draws().get(index as usize) else {
            return ffi::MeshDrawCpp::default();
        };
        ffi::MeshDrawCpp {
            mesh: draw.mesh,
            model: draw.model,
            color: draw.color,
        }
    }

    fn load_scene(&mut self, path: &str) -> bool {
        let Some(resolved) = safe_asset_path(&self.assets_dir, path) else {
            return false;
        };
        let Ok(mut scene) = Scene::load_from_path(&resolved) else {
            return false;
        };
        if scene.normalize_asset_paths().is_err() {
            return false;
        }
        let Ok(document) = EditorDocument::from_scene(scene) else {
            return false;
        };
        self.install_scene(document, path, &resolved);
        true
    }

    /// Guarda el documento en la ruta indicada y la recuerda para futuras recargas.
    fn save_scene(&mut self, path: &str) -> bool {
        let Some(resolved) = safe_asset_path(&self.assets_dir, path) else {
            return false;
        };
        let Some(editor) = self.editor.as_mut() else {
            return false;
        };
        if editor.save_to_path(&resolved).is_err() {
            return false;
        }
        self.scene_relative = path.to_owned();
        self.scene_path = Some(resolved.clone());
        self.scene_stamp = Self::file_stamp(&resolved);
        true
    }

    /// Alias de `save_scene`, explícito para los callers de C++.
    fn save_scene_as(&mut self, path: &str) -> bool {
        self.save_scene(path)
    }

    /// Guarda en la última ruta conocida, sin permitir cambiarla desde C++.
    fn save_scene_current(&mut self) -> bool {
        if self.scene_path.is_none() {
            return false;
        }
        let relative = self.scene_relative.clone();
        self.save_scene(&relative)
    }

    fn scene_path(&self) -> String {
        self.scene_relative.clone()
    }

    fn scene_is_dirty(&self) -> bool {
        self.editor.as_ref().is_some_and(|editor| editor.is_dirty())
    }

    fn scene_can_undo(&self) -> bool {
        self.editor.as_ref().is_some_and(|editor| editor.can_undo())
    }

    fn scene_can_redo(&self) -> bool {
        self.editor.as_ref().is_some_and(|editor| editor.can_redo())
    }

    fn scene_undo(&mut self) -> bool {
        self.editor.as_mut().is_some_and(EditorDocument::undo)
    }

    fn scene_redo(&mut self) -> bool {
        self.editor.as_mut().is_some_and(EditorDocument::redo)
    }

    fn editor_create_entity(&mut self, id: u64, name: &str) -> bool {
        let Some(editor) = self.editor.as_mut() else {
            return false;
        };
        let name = (!name.is_empty()).then(|| name.to_owned());
        editor
            .apply(EditorCommand::CreateEntity { id, name })
            .is_ok()
    }

    fn editor_delete_entity(&mut self, id: u64) -> bool {
        let Some(editor) = self.editor.as_mut() else {
            return false;
        };
        editor.apply(EditorCommand::DeleteEntity { id }).is_ok()
    }

    fn editor_rename_entity(&mut self, id: u64, name: &str) -> bool {
        let Some(editor) = self.editor.as_mut() else {
            return false;
        };
        let name = (!name.is_empty()).then(|| name.to_owned());
        editor
            .apply(EditorCommand::RenameEntity { id, name })
            .is_ok()
    }

    /// Sustituye la transformación completa; los arrays se pasan como escalares POD.
    #[allow(clippy::too_many_arguments)] // vectoriales y cuaternión en la ABI C++
    fn editor_set_transform(
        &mut self,
        id: u64,
        tx: f32,
        ty: f32,
        tz: f32,
        qx: f32,
        qy: f32,
        qz: f32,
        qw: f32,
        sx: f32,
        sy: f32,
        sz: f32,
    ) -> bool {
        let Some(editor) = self.editor.as_mut() else {
            return false;
        };
        let transform = Transform {
            translation: [tx, ty, tz],
            rotation: [qx, qy, qz, qw],
            scale: [sx, sy, sz],
        };
        editor
            .apply(EditorCommand::SetTransform { id, transform })
            .is_ok()
    }

    /// Recarga explícita: reemplaza el documento aunque tenga cambios sin guardar.
    fn reload_scene(&mut self) -> bool {
        if self.scene_path.is_none() {
            return false;
        }
        let relative = self.scene_relative.clone();
        self.load_scene(&relative)
    }

    /// Recarga cooperativa: solo actúa si el archivo cambió y no hay cambios locales.
    fn poll_scene_reload(&mut self) -> bool {
        let Some(path) = self.scene_path.clone() else {
            return false;
        };
        if self.scene_is_dirty() {
            return false;
        }
        let Some(stamp) = Self::file_stamp(&path) else {
            return false;
        };
        if self.scene_stamp == Some(stamp) {
            return false;
        }
        let relative = self.scene_relative.clone();
        self.load_scene(&relative)
    }

    fn entity_count(&self) -> u32 {
        self.scene().map_or(0, |scene| scene.entities.len() as u32)
    }

    fn entity(&self, index: u32) -> ffi::EntityInfo {
        let Some(entity) = self
            .scene()
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

    fn load_sound(&mut self, asset: &str) -> u64 {
        let Some(path) = safe_asset_path(&self.assets_dir, asset) else {
            return 0;
        };
        let Ok(handle) = self.sounds.load_from_path(path) else {
            return 0;
        };
        let public_id = self.next_sound;
        self.next_sound += 1;
        self.sound_ids.insert(public_id, handle);
        public_id
    }

    fn play_sound(&mut self, sound: u64) -> u64 {
        let Some(handle) = self.sound_ids.get(&sound).copied() else {
            return 0;
        };
        let Some(data) = self.sounds.get(handle) else {
            return 0;
        };
        let Ok(handle) = self.player.play(data) else {
            return 0;
        };
        let public_id = self.next_play;
        self.next_play += 1;
        self.plays.insert(public_id, handle);
        public_id
    }

    fn stop_sound(&mut self, play: u64) -> bool {
        let Some(handle) = self.plays.get(&play).copied() else {
            return false;
        };
        let stopped = self.player.stop(handle);
        if stopped {
            self.plays.remove(&play);
        }
        stopped
    }

    fn stop_all_sounds(&mut self) {
        self.player.stop_all();
        self.plays.clear();
    }

    fn set_volume(&mut self, play: u64, volume: f32) -> bool {
        let Some(handle) = self.plays.get(&play).copied() else {
            return false;
        };
        self.player.set_volume(handle, volume).is_ok()
    }

    fn set_master_volume(&mut self, volume: f32) {
        self.player.set_master_volume(volume);
    }

    fn master_volume(&self) -> f32 {
        self.player.master_volume()
    }

    fn is_sound_playing(&self, play: u64) -> bool {
        self.plays
            .get(&play)
            .is_some_and(|handle| self.player.is_playing(*handle))
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
        self.render3d.begin_frame();
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
        mouse: MouseState::default(),
        mouse_snapshot: MouseSnapshot::default(),
        last_tick: empty_tick(),
        physics: PhysicsWorld2D::new(Vec2::new(0.0, -9.81)),
        physics3d: PhysicsWorld3D::new(Vec3::new(0.0, -9.81, 0.0)),
        bodies: HashMap::new(),
        colliders: HashMap::new(),
        collider_ids: HashMap::new(),
        next_body: 1,
        next_collider: 1,
        bodies3d: HashMap::new(),
        colliders3d: HashMap::new(),
        collider_ids3d: HashMap::new(),
        next_body3d: 1,
        next_collider3d: 1,
        assets_dir,
        images: ImageStore::new(),
        image_ids: HashMap::new(),
        next_image: 1,
        gltfs: GltfStore::new(),
        gltf_ids: HashMap::new(),
        next_gltf: 1,
        render3d: Renderer3D::new(),
        sounds: AudioStore::new(),
        sound_ids: HashMap::new(),
        next_sound: 1,
        player: AudioPlayer::new(),
        plays: HashMap::new(),
        next_play: 1,
        editor: None,
        scene_path: None,
        scene_relative: String::new(),
        scene_stamp: None,
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

    fn on_mouse_button(&mut self, button: u32, pressed: bool) {
        self.engine.press_mouse(button, pressed);
        self.game.as_mut().on_mouse(button, pressed);
    }

    fn on_cursor(&mut self, x: f64, y: f64) {
        self.engine.set_cursor(x, y);
        self.game.as_mut().on_cursor(x, y);
    }

    fn on_scroll(&mut self, x: f32, y: f32, pixels: bool) {
        self.engine.add_scroll(x, y, pixels);
        self.game.as_mut().on_scroll(x, y, pixels);
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

    /// Estado de un botón de ratón en el frame (ADR-023).
    #[derive(Debug, Default, Clone, Copy, PartialEq)]
    struct MouseButtonState {
        down: bool,
        pressed: bool,
        released: bool,
    }

    /// Punto en píxeles físicos o en NDC; `valid=false` si no se puede convertir.
    #[derive(Debug, Default, Clone, Copy, PartialEq)]
    struct CursorPoint {
        x: f32,
        y: f32,
        valid: bool,
    }

    /// Scroll acumulado en el frame; `pixels` distingue la unidad del evento.
    #[derive(Debug, Default, Clone, Copy, PartialEq)]
    struct ScrollDelta {
        x: f32,
        y: f32,
        pixels: bool,
        valid: bool,
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

    /// Estado de un cuerpo 3D: posición, rotación `(x, y, z, w)` y velocidad.
    #[derive(Debug, Default, Clone, Copy, PartialEq)]
    struct BodyState3DCpp {
        valid: bool,
        x: f32,
        y: f32,
        z: f32,
        qx: f32,
        qy: f32,
        qz: f32,
        qw: f32,
        vx: f32,
        vy: f32,
        vz: f32,
    }

    /// Resultado de un raycast 3D; `collider` es el id público de la bridge.
    #[derive(Debug, Default, Clone, Copy, PartialEq)]
    struct RayHit3DCpp {
        hit: bool,
        collider: u64,
        toi: f32,
    }

    /// Matriz 4×4 en columna mayor, como array plano de 16 escalares.
    #[derive(Debug, Default, Clone, Copy, PartialEq)]
    struct Mat4Cpp {
        values: [f32; 16],
    }

    /// Orden de dibujo 3D; `mesh` es un handle de esta bridge.
    #[derive(Debug, Default, Clone, Copy, PartialEq)]
    struct MeshDrawCpp {
        mesh: u64,
        model: [f32; 16],
        color: [f32; 4],
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
        fn on_mouse(self: Pin<&mut Game>, button: u32, pressed: bool);
        fn on_cursor(self: Pin<&mut Game>, x: f64, y: f64);
        fn on_scroll(self: Pin<&mut Game>, x: f32, y: f32, pixels: bool);
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

        fn mouse_state(self: &Engine, button: u32) -> MouseButtonState;
        fn press_mouse(self: &mut Engine, button: u32, pressed: bool);
        fn set_cursor(self: &mut Engine, x: f64, y: f64);
        fn cursor_position(self: &Engine) -> CursorPoint;
        fn cursor_ndc(self: &Engine) -> CursorPoint;
        fn screen_to_ndc(self: &Engine, x: f32, y: f32) -> CursorPoint;
        fn add_scroll(self: &mut Engine, x: f32, y: f32, pixels: bool);
        fn scroll_delta(self: &Engine) -> ScrollDelta;

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

        fn set_gravity3d(self: &mut Engine, x: f32, y: f32, z: f32);
        fn add_body3d(self: &mut Engine, kind: u8, x: f32, y: f32, z: f32) -> u64;
        fn add_collider3d(
            self: &mut Engine,
            body: u64,
            shape: u8,
            half_x: f32,
            half_y: f32,
            half_z: f32,
        ) -> u64;
        fn body_state3d(self: &Engine, body: u64) -> BodyState3DCpp;
        fn apply_impulse3d(self: &mut Engine, body: u64, x: f32, y: f32, z: f32) -> bool;
        #[allow(clippy::too_many_arguments)]
        fn cast_ray3d(
            self: &Engine,
            origin_x: f32,
            origin_y: f32,
            origin_z: f32,
            direction_x: f32,
            direction_y: f32,
            direction_z: f32,
            max_toi: f32,
        ) -> RayHit3DCpp;
        fn step_physics3d(self: &mut Engine, dt: f32) -> bool;

        fn load_gltf(self: &mut Engine, asset: &str) -> u64;
        fn define_mesh(self: &mut Engine, gltf: u64, mesh: u32, primitive: u32) -> u64;
        fn mesh_vertex_count(self: &Engine, mesh: u64) -> u32;
        fn mesh_index_count(self: &Engine, mesh: u64) -> u32;
        #[allow(clippy::too_many_arguments)]
        fn set_camera3d(
            self: &mut Engine,
            eye_x: f32,
            eye_y: f32,
            eye_z: f32,
            target_x: f32,
            target_y: f32,
            target_z: f32,
            up_x: f32,
            up_y: f32,
            up_z: f32,
            fov: f32,
            aspect: f32,
            near: f32,
            far: f32,
        ) -> bool;
        fn view_projection3d(self: &Engine) -> Mat4Cpp;
        fn submit_mesh_draws(self: &mut Engine, draws: &[MeshDrawCpp]) -> bool;
        fn mesh_draw_count(self: &Engine) -> u32;
        fn mesh_draw(self: &Engine, index: u32) -> MeshDrawCpp;

        fn load_scene(self: &mut Engine, path: &str) -> bool;
        fn save_scene(self: &mut Engine, path: &str) -> bool;
        fn save_scene_as(self: &mut Engine, path: &str) -> bool;
        fn save_scene_current(self: &mut Engine) -> bool;
        fn scene_path(self: &Engine) -> String;
        fn scene_is_dirty(self: &Engine) -> bool;
        fn scene_can_undo(self: &Engine) -> bool;
        fn scene_can_redo(self: &Engine) -> bool;
        fn scene_undo(self: &mut Engine) -> bool;
        fn scene_redo(self: &mut Engine) -> bool;
        fn editor_create_entity(self: &mut Engine, id: u64, name: &str) -> bool;
        fn editor_delete_entity(self: &mut Engine, id: u64) -> bool;
        fn editor_rename_entity(self: &mut Engine, id: u64, name: &str) -> bool;
        #[allow(clippy::too_many_arguments)]
        fn editor_set_transform(
            self: &mut Engine,
            id: u64,
            tx: f32,
            ty: f32,
            tz: f32,
            qx: f32,
            qy: f32,
            qz: f32,
            qw: f32,
            sx: f32,
            sy: f32,
            sz: f32,
        ) -> bool;
        fn reload_scene(self: &mut Engine) -> bool;
        fn poll_scene_reload(self: &mut Engine) -> bool;
        fn entity_count(self: &Engine) -> u32;
        fn entity(self: &Engine, index: u32) -> EntityInfo;
        fn load_sound(self: &mut Engine, asset: &str) -> u64;
        fn play_sound(self: &mut Engine, sound: u64) -> u64;
        fn stop_sound(self: &mut Engine, play: u64) -> bool;
        fn stop_all_sounds(self: &mut Engine);
        fn set_volume(self: &mut Engine, play: u64, volume: f32) -> bool;
        fn set_master_volume(self: &mut Engine, volume: f32);
        fn master_volume(self: &Engine) -> f32;
        fn is_sound_playing(self: &Engine, play: u64) -> bool;
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

    fn scene_demo_config() -> ffi::GameConfig {
        ffi::GameConfig {
            title: "scene demo".to_owned(),
            width: 960,
            height: 540,
            assets_dir: Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("../../examples/scene_demo/assets")
                .to_string_lossy()
                .into_owned(),
        }
    }

    fn config_for_dir(dir: &Path) -> ffi::GameConfig {
        ffi::GameConfig {
            title: "scene doc".to_owned(),
            width: 320,
            height: 240,
            assets_dir: dir.to_string_lossy().into_owned(),
        }
    }

    /// Escena mínima en un directorio temporal, sin tocar los assets del repo.
    fn temp_scene_config() -> (ffi::GameConfig, PathBuf) {
        let dir = std::env::temp_dir().join(format!("lunar_h17_{}", std::process::id()));
        std::fs::create_dir_all(&dir).expect("directorio temporal");
        let scene = r#"{"version":1,"name":"temp","entities":[]}"#;
        std::fs::write(dir.join("scene.json"), scene).expect("escena inicial");
        (config_for_dir(&dir), dir)
    }

    #[test]
    fn scene_documents_save_undo_redo_and_reload() {
        let (config, dir) = temp_scene_config();
        let mut engine = new_game_engine(config);
        assert!(engine.load_scene("scene.json"));
        assert_eq!(engine.scene_path(), "scene.json");
        assert_eq!(engine.entity_count(), 0);
        assert!(!engine.scene_is_dirty());

        assert!(engine.editor_create_entity(7, "Nave"));
        assert_eq!(engine.entity_count(), 1);
        assert_eq!(engine.entity(0).name, "Nave");
        assert!(engine.scene_is_dirty());
        assert!(engine.scene_can_undo());
        assert!(engine.editor_rename_entity(7, "Nave 2"));
        assert_eq!(engine.entity(0).name, "Nave 2");
        assert!(engine.editor_set_transform(7, 1.0, 2.0, 3.0, 0.0, 0.0, 0.0, 1.0, 2.0, 2.0, 2.0));
        assert_eq!(engine.entity(0).x, 1.0);
        assert_eq!(engine.entity(0).scale_x, 2.0);

        assert!(engine.scene_undo());
        assert!(engine.scene_undo());
        assert!(engine.scene_undo());
        assert_eq!(engine.entity_count(), 0);
        assert!(!engine.scene_can_undo());
        assert!(engine.scene_redo());
        assert!(engine.scene_redo());
        assert!(engine.scene_redo());
        assert_eq!(engine.entity(0).name, "Nave 2");

        assert!(engine.save_scene("saved.json"));
        assert!(!engine.scene_is_dirty());
        assert!(dir.join("saved.json").exists());
        assert!(engine.save_scene_as("saved.json"));
        assert!(engine.save_scene_current());
        assert!(engine.reload_scene());
        assert_eq!(engine.scene_path(), "saved.json");
        assert_eq!(engine.entity_count(), 1);

        // Sin cambios en disco no hay recarga; con cambios y documento limpio, sí.
        assert!(!engine.poll_scene_reload());
        std::fs::write(
            dir.join("saved.json"),
            r#"{"version":1,"name":"externo","entities":[]}"#,
        )
        .expect("escena externa");
        assert!(engine.poll_scene_reload());
        assert_eq!(engine.entity_count(), 0);

        assert!(engine.editor_create_entity(9, "Local"));
        assert!(
            std::fs::write(
                dir.join("saved.json"),
                r#"{"version":1,"name":"otro","entities":[]}"#
            )
            .is_ok()
        );
        assert!(!engine.poll_scene_reload());
        assert!(engine.scene_is_dirty());
        assert!(engine.reload_scene());
        assert_eq!(engine.entity_count(), 0);

        assert!(!engine.load_scene("../escape.json"));
        assert!(!engine.save_scene("../escape.json"));

        let mut empty = new_game_engine(config_for_dir(&dir));
        assert!(!empty.editor_create_entity(9, "sin documento"));
        assert!(!empty.save_scene_current());
        assert!(!empty.reload_scene());
        assert!(!empty.poll_scene_reload());
        assert_eq!(empty.scene_path(), "");
        let _ = std::fs::remove_dir_all(&dir);
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
    fn mouse_state_and_scroll_survive_one_tick() {
        let mut engine = new_engine(config());
        engine.press_mouse(0, true);
        engine.press_mouse(1, true);
        engine.set_cursor(400.0, 300.0);
        engine.add_scroll(1.0, -2.0, false);

        assert!(!engine.mouse_state(0).down);
        let position = engine.cursor_position();
        assert!(position.valid);
        assert!((position.x - 400.0).abs() < f32::EPSILON);
        assert!((position.y - 300.0).abs() < f32::EPSILON);

        engine.tick(FIXED_DT.as_nanos() as u64);
        let state = engine.mouse_state(1);
        assert!(state.down && state.pressed && !state.released);
        let scroll = engine.scroll_delta();
        assert!(scroll.valid && !scroll.pixels);
        assert!((scroll.x - 1.0).abs() < f32::EPSILON);
        assert!((scroll.y + 2.0).abs() < f32::EPSILON);

        engine.tick(FIXED_DT.as_nanos() as u64);
        assert!(!engine.mouse_state(1).pressed);
        assert!(!engine.scroll_delta().valid);

        engine.press_mouse(1, false);
        engine.tick(FIXED_DT.as_nanos() as u64);
        assert!(engine.mouse_state(1).released);
        assert!(!engine.mouse_state(1).down);
    }

    #[test]
    fn screen_to_ndc_uses_the_current_viewport() {
        let mut engine = new_engine(config());
        let center = engine.screen_to_ndc(400.0, 300.0);
        assert!(center.valid);
        assert!((center.x).abs() < f32::EPSILON);
        assert!((center.y).abs() < f32::EPSILON);

        let corner = engine.screen_to_ndc(0.0, 0.0);
        assert!((corner.x + 1.0).abs() < f32::EPSILON);
        assert!((corner.y - 1.0).abs() < f32::EPSILON);

        engine.set_cursor(200.0, 150.0);
        let cursor = engine.cursor_ndc();
        assert!(cursor.valid);
        assert!((cursor.x + 0.5).abs() < f32::EPSILON);
        assert!((cursor.y - 0.5).abs() < f32::EPSILON);

        engine.viewport = ffi::ViewportInfo {
            width: 0,
            height: 0,
        };
        assert!(!engine.screen_to_ndc(1.0, 1.0).valid);
        assert!(!engine.cursor_ndc().valid);
        assert!(!engine.screen_to_ndc(f32::NAN, 1.0).valid);
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
    fn sounds_load_without_a_device_and_handles_are_validated() {
        let mut engine = new_game_engine(scene_demo_config());
        let sound = engine.load_sound("beep.wav");
        assert_ne!(sound, 0);
        assert_eq!(engine.load_sound("../../secret.wav"), 0);
        assert_eq!(engine.load_sound("/etc/passwd"), 0);

        // No abrimos dispositivo: los handles de reproducción inválidos se
        // rechazan antes de llamar a `rodio`.
        assert_eq!(engine.play_sound(999), 0);
        assert!(!engine.stop_sound(999));
        assert!(!engine.set_volume(999, 0.5));
        assert!(!engine.is_sound_playing(999));
        engine.stop_all_sounds();
    }

    #[test]
    fn master_volume_is_exposed_without_a_device() {
        let mut engine = new_engine(config());
        engine.set_master_volume(0.4);
        assert!((engine.master_volume() - 0.4).abs() < f32::EPSILON);
        engine.set_master_volume(2.0);
        assert!((engine.master_volume() - 1.0).abs() < f32::EPSILON);
    }

    #[test]
    fn physics3d_handles_and_ray_use_the_bridge_ids() {
        let mut engine = new_engine(config());
        engine.set_gravity3d(0.0, 0.0, 0.0);
        let body = engine.add_body3d(2, 0.0, 0.0, 0.0);
        let collider = engine.add_collider3d(body, 2, 1.0, 1.0, 1.0);
        assert_ne!(body, 0);
        assert_ne!(collider, 0);
        assert_eq!(engine.add_body3d(99, 0.0, 0.0, 0.0), 0);
        assert_eq!(engine.add_collider3d(999, 2, 1.0, 1.0, 1.0), 0);
        assert!(engine.step_physics3d(1.0 / 60.0));

        let hit = engine.cast_ray3d(0.0, 5.0, 0.0, 0.0, -1.0, 0.0, 10.0);
        assert!(hit.hit);
        assert_eq!(hit.collider, collider);
        assert!((3.9..4.1).contains(&hit.toi));
        assert!(!engine.cast_ray3d(0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 1.0).hit);

        let state = engine.body_state3d(body);
        assert!(state.valid);
        assert!(!engine.body_state3d(999).valid);
        assert!(engine.apply_impulse3d(body, 1.0, 0.0, 0.0));
        assert!(!engine.apply_impulse3d(999, 1.0, 0.0, 0.0));
    }

    #[test]
    fn gltf_meshes_and_draw_orders_are_validated() {
        let mut engine = new_game_engine(scene_demo_config());
        let gltf = engine.load_gltf("triangle.glb");
        assert_ne!(gltf, 0);
        assert_eq!(engine.load_gltf("../secret.glb"), 0);

        let mesh = engine.define_mesh(gltf, 0, 0);
        assert_ne!(mesh, 0);
        assert!(engine.mesh_vertex_count(mesh) >= 3);
        assert!(engine.mesh_index_count(mesh) >= 3);
        assert_eq!(engine.define_mesh(999, 0, 0), 0);

        assert!(engine.set_camera3d(
            0.0, 0.0, 3.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 1.0, 1.5, 0.1, 10.0
        ));
        assert!(
            engine
                .view_projection3d()
                .values
                .iter()
                .all(|value| value.is_finite())
        );
        assert!(!engine.set_camera3d(
            0.0, 0.0, 3.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 1.0, 0.0, 0.1, 10.0
        ));

        let draw = ffi::MeshDrawCpp {
            mesh,
            model: engine.view_projection3d().values,
            color: [1.0, 1.0, 1.0, 1.0],
        };
        assert!(engine.submit_mesh_draws(&[draw]));
        assert_eq!(engine.mesh_draw_count(), 1);
        assert_eq!(engine.mesh_draw(0), draw);
        assert!(!engine.submit_mesh_draws(&[ffi::MeshDrawCpp { mesh: 999, ..draw }]));

        engine.begin_render();
        assert_eq!(engine.mesh_draw_count(), 0);
    }

    #[test]
    fn asset_paths_cannot_escape_the_configured_directory() {
        let engine = new_game_engine(game_config());
        assert!(safe_asset_path(&engine.assets_dir, "sprites/goal_end.png").is_some());
        assert!(safe_asset_path(&engine.assets_dir, "../secret.png").is_none());
        assert!(safe_asset_path(&engine.assets_dir, "/etc/passwd").is_none());
    }
}
