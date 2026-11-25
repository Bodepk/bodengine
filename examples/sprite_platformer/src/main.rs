//! Plataformas 2D jugable con sprites reales (ADR-018).
//!
//! A/ArrowLeft y D/ArrowRight mueven, W/ArrowUp salta, Escape sale.
//! Las manzanas se recogen por solapamiento y la bandera marca la meta.

use std::collections::{HashMap, HashSet};
use std::error::Error;
use std::path::PathBuf;

use lunar::assets::{ImageData, ImageStore};
use lunar::core::{Engine, EngineConfig, FIXED_DT, Key, SystemClock};
use lunar::math::Vec2;
use lunar::physics2d::{
    BodyDesc2D, BodyId, BodyState2D, BodyType2D, ColliderDesc2D, ColliderId, PhysicsWorld2D,
    Shape2D as PhysicsShape2D,
};
use lunar::platform::{PlatformApp, WindowConfig, run};
use lunar::render::{Renderer, SpriteDraw, SpriteId};
use lunar::scenes::{BodyType, Component, Scene, Shape2D as SceneShape2D};

const SPRITES: &[(&str, u32, u32)] = &[
    ("sprites/player_ninja_frog_idle.png", 32, 32),
    ("sprites/player_ninja_frog_run.png", 32, 32),
    ("sprites/player_ninja_frog_jump.png", 32, 32),
    ("sprites/player_ninja_frog_fall.png", 32, 32),
    ("sprites/terrain_tiles.png", 16, 16),
    ("sprites/platform_brown.png", 32, 8),
    ("sprites/goal_end.png", 64, 64),
    ("sprites/collectible_apple.png", 32, 32),
    ("sprites/crate_idle.png", 28, 24),
    ("sprites/background_blue.png", 64, 64),
];

const PIXELS_PER_UNIT: f32 = 32.0;
const VIEW_HEIGHT: f32 = 9.0;
const LEVEL_HALF_WIDTH: f32 = 12.0;
const MOVE_IMPULSE: f32 = 0.28;
const MAX_SPEED: f32 = 5.0;
const JUMP_IMPULSE: f32 = 6.2;

#[derive(Debug, Clone, Copy)]
struct SpriteInfo {
    id: SpriteId,
    frame_width: u32,
    frame_height: u32,
}

struct Game {
    engine: Engine,
    clock: SystemClock,
    scene: Scene,
    images: HashMap<String, ImageData>,
    sprites: HashMap<String, SpriteInfo>,
    sprites_ready: bool,
    physics: PhysicsWorld2D,
    player_entity: u64,
    player_body: BodyId,
    player_collider: ColliderId,
    player_half: Vec2,
    collected: HashSet<u64>,
    won: bool,
    elapsed: f32,
    camera_x: f32,
    facing_right: bool,
}

impl Game {
    fn is_grounded(&self) -> bool {
        let Some(state) = self.physics.body_state(self.player_body) else {
            return false;
        };
        let origin = Vec2::new(
            state.position.x,
            state.position.y - self.player_half.y - 0.03,
        );
        self.physics
            .cast_ray(origin, Vec2::new(0.0, -1.0), 0.18)
            .is_some_and(|hit| hit.collider != self.player_collider)
    }

    fn player_sprite(&self, grounded: bool, state: BodyState2D) -> (&'static str, u32) {
        if !grounded {
            if state.linvel.y > 0.0 {
                ("sprites/player_ninja_frog_jump.png", 0)
            } else {
                ("sprites/player_ninja_frog_fall.png", 0)
            }
        } else if state.linvel.x.abs() > 0.15 {
            (
                "sprites/player_ninja_frog_run.png",
                (self.elapsed * 12.0) as u32 % 12,
            )
        } else {
            (
                "sprites/player_ninja_frog_idle.png",
                (self.elapsed * 8.0) as u32 % 11,
            )
        }
    }

    fn check_triggers(&mut self, state: BodyState2D) {
        for entity in &self.scene.entities {
            if entity.id == self.player_entity || self.collected.contains(&entity.id) {
                continue;
            }
            let name = entity.name.as_deref().unwrap_or_default();
            let is_goal = name.starts_with("Meta");
            let is_apple = name.starts_with("Manzana");
            if !is_goal && !is_apple {
                continue;
            }
            let Some(asset) = entity
                .components
                .iter()
                .find_map(|component| match component {
                    Component::Image { asset } => Some(asset.as_str()),
                    _ => None,
                })
            else {
                continue;
            };
            let Some(sprite) = self.sprites.get(asset) else {
                continue;
            };
            let [x, y, _] = entity.transform.translation;
            let [sx, sy, _] = entity.transform.scale;
            let half_width = sprite.frame_width as f32 / PIXELS_PER_UNIT * sx * 0.5;
            let half_height = sprite.frame_height as f32 / PIXELS_PER_UNIT * sy * 0.5;
            let overlaps = (state.position.x - x).abs() < self.player_half.x + half_width
                && (state.position.y - y).abs() < self.player_half.y + half_height;
            if !overlaps {
                continue;
            }
            if is_goal {
                if !self.won {
                    self.won = true;
                    println!("meta alcanzada");
                }
            } else {
                self.collected.insert(entity.id);
                println!("manzana recogida");
            }
        }
    }

    fn upload_sprites(&mut self, renderer: &mut Renderer<'_>) {
        for (asset, frame_width, frame_height) in SPRITES {
            let Some(image) = self.images.get(*asset) else {
                continue;
            };
            match renderer.define_sprite(
                image.as_rgba(),
                image.width,
                image.height,
                [*frame_width, *frame_height],
            ) {
                Ok(id) => {
                    self.sprites.insert(
                        (*asset).to_owned(),
                        SpriteInfo {
                            id,
                            frame_width: *frame_width,
                            frame_height: *frame_height,
                        },
                    );
                }
                Err(error) => {
                    eprintln!("no se pudo subir {asset}: {error}");
                    return;
                }
            }
        }
        self.sprites_ready = true;
    }
}

impl PlatformApp for Game {
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
        let tick = self.engine.update(&self.clock);
        if input.was_pressed(Key::Escape) {
            return false;
        }

        let grounded = self.is_grounded();
        if (input.was_pressed(Key::W) || input.was_pressed(Key::ArrowUp)) && grounded {
            self.physics
                .apply_impulse(self.player_body, Vec2::new(0.0, JUMP_IMPULSE));
        }

        let mut axis = 0.0;
        if input.is_down(Key::A) || input.is_down(Key::ArrowLeft) {
            axis -= 1.0;
        }
        if input.is_down(Key::D) || input.is_down(Key::ArrowRight) {
            axis += 1.0;
        }
        if axis != 0.0 {
            self.facing_right = axis > 0.0;
        }

        for _ in 0..tick.substeps {
            if axis != 0.0 {
                let state = self.physics.body_state(self.player_body);
                let speed_along_axis = state.map_or(0.0, |body| body.linvel.x * axis);
                if speed_along_axis < MAX_SPEED {
                    self.physics
                        .apply_impulse(self.player_body, Vec2::new(axis * MOVE_IMPULSE, 0.0));
                }
            }
            if let Err(error) = self.physics.step(FIXED_DT.as_secs_f32()) {
                eprintln!("física: {error}");
                return false;
            }
        }
        self.elapsed += FIXED_DT.as_secs_f32() * tick.substeps as f32;

        let Some(state) = self.physics.body_state(self.player_body) else {
            return false;
        };
        self.check_triggers(state);
        self.camera_x += (state.position.x - self.camera_x) * 0.08;
        true
    }

    fn on_render(&mut self, renderer: &mut Renderer<'_>) {
        if !self.sprites_ready {
            self.upload_sprites(renderer);
        }

        let viewport = renderer.viewport().unwrap_or(lunar::render::Viewport {
            width: 960,
            height: 540,
        });
        let aspect = viewport.width as f32 / viewport.height as f32;
        let half_height = VIEW_HEIGHT * 0.5;
        let half_width = half_height * aspect;
        let limit = (LEVEL_HALF_WIDTH - half_width).max(0.0);
        self.camera_x = self.camera_x.clamp(-limit, limit);
        let camera = Vec2::new(self.camera_x, 0.0);

        let Some(player_state) = self.physics.body_state(self.player_body) else {
            renderer.clear_sprites();
            return;
        };
        let (player_asset, player_frame) = self.player_sprite(self.is_grounded(), player_state);
        let mut draws = Vec::with_capacity(self.scene.entities.len());

        for entity in &self.scene.entities {
            let Some(scene_asset) =
                entity
                    .components
                    .iter()
                    .find_map(|component| match component {
                        Component::Image { asset } => Some(asset.as_str()),
                        _ => None,
                    })
            else {
                continue;
            };
            let is_player = entity.id == self.player_entity;
            let asset = if is_player { player_asset } else { scene_asset };
            let Some(sprite) = self.sprites.get(asset) else {
                continue;
            };

            let frame = if is_player {
                player_frame
            } else if asset == "sprites/collectible_apple.png" {
                (self.elapsed * 8.0) as u32 % 17
            } else {
                0
            };
            let position = if is_player {
                player_state.position
            } else {
                Vec2::new(
                    entity.transform.translation[0],
                    entity.transform.translation[1],
                )
            };
            let [sx, sy, _] = entity.transform.scale;
            let world_size = Vec2::new(
                sprite.frame_width as f32 / PIXELS_PER_UNIT * sx,
                sprite.frame_height as f32 / PIXELS_PER_UNIT * sy,
            );
            draws.push(SpriteDraw {
                sprite: sprite.id,
                frame,
                flip_x: is_player && !self.facing_right,
                center: [
                    (position.x - camera.x) / half_width,
                    (position.y - camera.y) / half_height,
                ],
                size: [world_size.x / half_width, world_size.y / half_height],
            });
        }

        if let Err(error) = renderer.set_sprites(&draws) {
            eprintln!("sprite: {error}");
            renderer.clear_sprites();
        }
    }
}

fn assets_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("assets")
}

fn load_game() -> Result<Game, Box<dyn Error>> {
    let assets = assets_dir();
    let mut scene = Scene::load_from_path(assets.join("platformer.scene.json"))?;
    scene.normalize_asset_paths()?;

    let mut store = ImageStore::new();
    let mut images = HashMap::new();
    for (asset, _, _) in SPRITES {
        let path = assets.join(asset);
        let handle = store
            .load_from_path(&path)
            .map_err(|error| format!("{}: {error}", path.display()))?;
        let data = store.get(handle).ok_or("imagen no almacenada")?.clone();
        images.insert((*asset).to_owned(), data);
    }

    let mut physics = PhysicsWorld2D::new(Vec2::new(0.0, -18.0));
    let mut player = None;
    for entity in &scene.entities {
        for component in &entity.components {
            let Component::Collider2D { shape, body } = component else {
                continue;
            };
            let body_type = match body {
                BodyType::Dynamic => BodyType2D::Dynamic,
                BodyType::Fixed => BodyType2D::Fixed,
                BodyType::Sensor => continue,
            };
            let [x, y, _] = entity.transform.translation;
            let body_id = physics
                .add_body(BodyDesc2D::new(body_type, Vec2::new(x, y)))
                .ok_or("no se pudo crear el cuerpo del jugador")?;
            let physics_shape = match shape {
                SceneShape2D::Ball { radius } => PhysicsShape2D::Ball { radius: *radius },
                SceneShape2D::Cuboid { half_extents } => PhysicsShape2D::Cuboid {
                    half_extents: Vec2::new(half_extents[0], half_extents[1]),
                },
            };
            let collider = physics
                .add_collider(body_id, ColliderDesc2D::new(physics_shape))
                .ok_or("no se pudo crear el colisionador")?;
            if body_type == BodyType2D::Dynamic {
                let half = match shape {
                    SceneShape2D::Ball { radius } => Vec2::new(*radius, *radius),
                    SceneShape2D::Cuboid { half_extents } => {
                        Vec2::new(half_extents[0], half_extents[1])
                    }
                };
                player = Some((entity.id, body_id, collider, half));
            }
        }
    }
    let (player_entity, player_body, player_collider, player_half) =
        player.ok_or("la escena no tiene un cuerpo dinámico")?;
    let camera_x = physics
        .body_state(player_body)
        .map_or(0.0, |state| state.position.x);

    Ok(Game {
        engine: Engine::new(EngineConfig::new("Lunar sprite platformer", 960, 540)),
        clock: SystemClock::new(),
        scene,
        images,
        sprites: HashMap::new(),
        sprites_ready: false,
        physics,
        player_entity,
        player_body,
        player_collider,
        player_half,
        collected: HashSet::new(),
        won: false,
        elapsed: 0.0,
        camera_x,
        facing_right: true,
    })
}

fn main() {
    let game = match load_game() {
        Ok(game) => game,
        Err(error) => {
            eprintln!("error cargando el plataformas: {error}");
            std::process::exit(1);
        }
    };
    let mut game = game;
    println!("A/D o flechas mueven; W o ArrowUp salta; Escape sale");
    if let Err(error) = run(
        WindowConfig::new("Lunar sprite platformer", 960, 540),
        &mut game,
    ) {
        eprintln!("error en la ventana: {error}");
        std::process::exit(1);
    }
}
