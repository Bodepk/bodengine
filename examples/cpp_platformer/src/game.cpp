#include "game.hpp"

#include <algorithm>
#include <cmath>
#include <iostream>
#include <stdexcept>

namespace {

constexpr float kPixelsPerUnit = 32.0F;
constexpr float kViewHeight = 9.0F;
constexpr float kLevelHalfWidth = 12.0F;
constexpr float kMoveImpulse = 0.28F;
constexpr float kMaxSpeed = 5.0F;
constexpr float kJumpImpulse = 6.2F;
constexpr float kFixedDt = 1.0F / 60.0F;
constexpr std::uint8_t kDynamic = 1;
constexpr std::uint8_t kFixed = 2;

struct SpriteSpec {
    const char *asset;
    std::uint32_t frame_width;
    std::uint32_t frame_height;
};

constexpr SpriteSpec kSprites[] = {
    {"sprites/player_ninja_frog_idle.png", 32, 32},
    {"sprites/player_ninja_frog_run.png", 32, 32},
    {"sprites/player_ninja_frog_jump.png", 32, 32},
    {"sprites/player_ninja_frog_fall.png", 32, 32},
    {"sprites/terrain_tiles.png", 16, 16},
    {"sprites/platform_brown.png", 32, 8},
    {"sprites/goal_end.png", 64, 64},
    {"sprites/collectible_apple.png", 32, 32},
    {"sprites/crate_idle.png", 28, 24},
    {"sprites/background_blue.png", 64, 64},
};

constexpr const char *kPlayerIdle = "sprites/player_ninja_frog_idle.png";
constexpr const char *kPlayerRun = "sprites/player_ninja_frog_run.png";
constexpr const char *kPlayerJump = "sprites/player_ninja_frog_jump.png";
constexpr const char *kPlayerFall = "sprites/player_ninja_frog_fall.png";
constexpr const char *kApple = "sprites/collectible_apple.png";

std::string to_std(const rust::String &value) {
    return std::string(value.data(), value.size());
}

bool starts_with(const std::string &value, const char *prefix) {
    return value.rfind(prefix, 0) == 0;
}

}  // namespace

PlatformerGame::PlatformerGame(Engine &engine, const GameConfig &config) : engine_(engine) {
    // `assets_dir` ya vive en el Engine creado con esta misma configuración.
    (void)config;
    engine_.set_gravity(0.0F, -18.0F);
    if (!engine_.load_scene("platformer.scene.json")) {
        throw std::runtime_error("no se pudo cargar platformer.scene.json");
    }

    const std::uint32_t count = engine_.entity_count();
    entities_.reserve(count);
    for (std::uint32_t index = 0; index < count; ++index) {
        const EntityInfo info = engine_.entity(index);
        Entity entity;
        entity.id = info.id;
        entity.name = to_std(info.name);
        entity.asset = to_std(info.image_asset);
        entity.x = info.x;
        entity.y = info.y;
        entity.scale_x = info.scale_x;
        entity.scale_y = info.scale_y;
        entity.half_x = info.half_x;
        entity.half_y = info.half_y;
        entity.shape_kind = info.shape_kind;
        entities_.push_back(std::move(entity));

        if ((info.body_kind != kDynamic && info.body_kind != kFixed) || info.shape_kind == 0) {
            continue;
        }
        const std::uint64_t body = engine_.add_body(info.body_kind, info.x, info.y);
        if (body == 0) {
            throw std::runtime_error("no se pudo crear el cuerpo de " + entity.name);
        }
        const std::uint64_t collider =
            engine_.add_collider(body, info.shape_kind, info.half_x, info.half_y);
        if (collider == 0) {
            throw std::runtime_error("no se pudo crear el colisionador de " + entity.name);
        }
        if (info.body_kind == kDynamic) {
            player_entity_ = info.id;
            player_body_ = body;
            player_collider_ = collider;
            player_half_x_ = info.half_x;
            player_half_y_ = info.half_y;
        }
    }

    if (player_body_ == 0) {
        throw std::runtime_error("la escena no tiene un cuerpo dinámico");
    }
    camera_x_ = engine_.body_state(player_body_).x;

    for (const SpriteSpec &spec : kSprites) {
        const std::uint64_t image = engine_.load_image(spec.asset);
        if (image == 0) {
            throw std::runtime_error(std::string("no se pudo cargar ") + spec.asset);
        }
        const std::uint64_t handle =
            engine_.define_sprite(image, spec.frame_width, spec.frame_height);
        if (handle == 0) {
            throw std::runtime_error(std::string("no se pudo definir ") + spec.asset);
        }
        sprites_.emplace(
            spec.asset,
            Sprite{spec.asset, spec.frame_width, spec.frame_height, handle});
    }
}

void PlatformerGame::on_key(std::uint32_t code, bool pressed) {
    // El host ya reenvió la tecla al Engine; el juego solo consulta el estado.
    (void)code;
    (void)pressed;
}

void PlatformerGame::on_resize(std::uint32_t width, std::uint32_t height) {
    (void)width;
    (void)height;
}

bool PlatformerGame::is_grounded() const {
    const BodyState state = engine_.body_state(player_body_);
    if (!state.valid) {
        return false;
    }
    const RayHit hit = engine_.cast_ray(
        state.x, state.y - player_half_y_ - 0.03F, 0.0F, -1.0F, 0.18F);
    return hit.hit && hit.collider != player_collider_;
}

void PlatformerGame::check_triggers(const BodyState &state) {
    for (const Entity &entity : entities_) {
        if (entity.id == player_entity_ || collected_.count(entity.id) != 0) {
            continue;
        }
        const bool goal = starts_with(entity.name, "Meta");
        const bool apple = starts_with(entity.name, "Manzana");
        if (!goal && !apple) {
            continue;
        }
        const Sprite *sprite = sprite_for(entity.asset);
        if (sprite == nullptr) {
            continue;
        }
        const float half_width =
            sprite->frame_width / kPixelsPerUnit * entity.scale_x * 0.5F;
        const float half_height =
            sprite->frame_height / kPixelsPerUnit * entity.scale_y * 0.5F;
        const bool overlaps =
            std::abs(state.x - entity.x) < player_half_x_ + half_width &&
            std::abs(state.y - entity.y) < player_half_y_ + half_height;
        if (!overlaps) {
            continue;
        }
        if (goal) {
            if (!won_) {
                won_ = true;
                std::cout << "meta alcanzada\n";
            }
        } else {
            collected_.insert(entity.id);
            std::cout << "manzana recogida\n";
        }
    }
}

const PlatformerGame::Sprite *PlatformerGame::sprite_for(const std::string &asset) const {
    const auto found = sprites_.find(asset);
    return found == sprites_.end() ? nullptr : &found->second;
}

bool PlatformerGame::on_frame() {
    if (engine_.was_key_pressed(static_cast<std::uint32_t>(lunar::KeyCode::Escape))) {
        return false;
    }

    float axis = 0.0F;
    if (engine_.is_key_down(static_cast<std::uint32_t>(lunar::KeyCode::A)) ||
        engine_.is_key_down(static_cast<std::uint32_t>(lunar::KeyCode::ArrowLeft))) {
        axis -= 1.0F;
    }
    if (engine_.is_key_down(static_cast<std::uint32_t>(lunar::KeyCode::D)) ||
        engine_.is_key_down(static_cast<std::uint32_t>(lunar::KeyCode::ArrowRight))) {
        axis += 1.0F;
    }

    const bool grounded = is_grounded();
    if ((engine_.was_key_pressed(static_cast<std::uint32_t>(lunar::KeyCode::W)) ||
         engine_.was_key_pressed(static_cast<std::uint32_t>(lunar::KeyCode::ArrowUp))) &&
        grounded) {
        engine_.apply_impulse(player_body_, 0.0F, kJumpImpulse);
    }
    if (axis != 0.0F) {
        facing_right_ = axis > 0.0F;
    }

    const std::uint32_t substeps = engine_.substeps();
    for (std::uint32_t step = 0; step < substeps; ++step) {
        if (axis != 0.0F) {
            const BodyState state = engine_.body_state(player_body_);
            const float speed_along_axis = state.valid ? state.vx * axis : 0.0F;
            if (speed_along_axis < kMaxSpeed) {
                engine_.apply_impulse(player_body_, axis * kMoveImpulse, 0.0F);
            }
        }
        if (!engine_.step_physics(kFixedDt)) {
            std::cerr << "error en la física\n";
            return false;
        }
    }
    elapsed_ += kFixedDt * static_cast<float>(substeps);

    const BodyState state = engine_.body_state(player_body_);
    if (!state.valid) {
        return false;
    }
    check_triggers(state);
    camera_x_ += (state.x - camera_x_) * 0.08F;
    return true;
}

bool PlatformerGame::headless_check() {
    engine_.press_key(static_cast<std::uint32_t>(lunar::KeyCode::D), true);
    for (int frame = 0; frame < 120; ++frame) {
        engine_.tick(lunar::kFixedDtNs);
        if (!on_frame()) {
            return false;
        }
        on_render();
    }
    const BodyState state = engine_.body_state(player_body_);
    return state.valid && state.x > -7.5F;
}

void PlatformerGame::on_render() {
    const ViewportInfo viewport = engine_.viewport();
    if (viewport.width == 0 || viewport.height == 0) {
        return;
    }
    const float aspect = static_cast<float>(viewport.width) /
                         static_cast<float>(viewport.height);
    const float half_height = kViewHeight * 0.5F;
    const float half_width = half_height * aspect;
    const float limit = std::max(0.0F, kLevelHalfWidth - half_width);
    camera_x_ = std::clamp(camera_x_, -limit, limit);

    const BodyState player = engine_.body_state(player_body_);
    std::vector<SpriteDrawCpp> draws;
    if (!player.valid) {
        engine_.submit_sprites(rust::Slice<const SpriteDrawCpp>(draws));
        return;
    }

    const bool grounded = is_grounded();
    std::string player_asset = kPlayerIdle;
    std::uint32_t player_frame = 0;
    if (!grounded) {
        player_asset = player.vy > 0.0F ? kPlayerJump : kPlayerFall;
    } else if (std::abs(player.vx) > 0.15F) {
        player_asset = kPlayerRun;
        player_frame = static_cast<std::uint32_t>(elapsed_ * 12.0F) % 12;
    } else {
        player_frame = static_cast<std::uint32_t>(elapsed_ * 8.0F) % 11;
    }

    for (const Entity &entity : entities_) {
        const bool is_player = entity.id == player_entity_;
        const std::string &asset = is_player ? player_asset : entity.asset;
        const Sprite *sprite = sprite_for(asset);
        if (sprite == nullptr) {
            continue;
        }

        std::uint32_t frame = 0;
        if (is_player) {
            frame = player_frame;
        } else if (entity.asset == kApple) {
            frame = static_cast<std::uint32_t>(elapsed_ * 8.0F) % 17;
        }
        const float x = is_player ? player.x : entity.x;
        const float y = is_player ? player.y : entity.y;
        const float world_width =
            sprite->frame_width / kPixelsPerUnit * entity.scale_x;
        const float world_height =
            sprite->frame_height / kPixelsPerUnit * entity.scale_y;

        draws.push_back(SpriteDrawCpp{
            sprite->handle,
            frame,
            is_player && !facing_right_,
            (x - camera_x_) / half_width,
            y / half_height,
            world_width / half_width,
            world_height / half_height,
        });
    }
    engine_.submit_sprites(rust::Slice<const SpriteDrawCpp>(draws));
}
