#pragma once

#include <lunar_bridge/game.h>
#include <lunar_bridge/src/lib.rs.h>

#include "behaviors.hpp"

#include <cstdint>
#include <filesystem>
#include <string>
#include <unordered_map>
#include <unordered_set>
#include <vector>

// Banco de pruebas del contrato C++ de ADR-021: toda la lógica del plataformas
// vive aquí; Rust conserva la ventana, el reloj, la física y el render.
class PlatformerGame final : public Game {
  public:
    PlatformerGame(Engine &engine, const GameConfig &config);

    void on_key(std::uint32_t code, bool pressed) override;
    void on_resize(std::uint32_t width, std::uint32_t height) override;
    bool on_frame() override;
    void on_render() override;

    /// Ejecuta el bucle sin ventana para la verificación en CI.
    bool headless_check();

  private:
    struct Sprite {
        std::string asset;
        std::uint32_t frame_width = 0;
        std::uint32_t frame_height = 0;
        std::uint64_t handle = 0;
    };

    struct Entity {
        std::uint64_t id = 0;
        std::string name;
        std::string asset;
        float x = 0.0F;
        float y = 0.0F;
        float scale_x = 1.0F;
        float scale_y = 1.0F;
        float half_x = 0.0F;
        float half_y = 0.0F;
        std::uint8_t shape_kind = 0;
    };

    struct BehaviorVisual {
        float dx = 0.0F;
        float dy = 0.0F;
        bool flip = false;
    };

    bool is_grounded() const;
    void check_triggers(const BodyState &state);
    const Sprite *sprite_for(const std::string &asset) const;
    void reload_scripts_if_changed();
    void update_behaviors(float dt);

    Engine &engine_;
    std::vector<Entity> entities_;
    std::unordered_map<std::string, Sprite> sprites_;
    std::unordered_set<std::uint64_t> collected_;
    std::uint64_t player_entity_ = 0;
    std::uint64_t player_body_ = 0;
    std::uint64_t player_collider_ = 0;
    float player_half_x_ = 0.5F;
    float player_half_y_ = 0.5F;
    float elapsed_ = 0.0F;
    float camera_x_ = 0.0F;
    bool facing_right_ = true;
    bool won_ = false;

    std::string scripts_path_;
    std::string diagnostics_path_;
    std::filesystem::file_time_type scripts_stamp_{};
    bool scripts_stamp_valid_ = false;
    std::vector<lunar_behaviors::Binding> scripts_;
    std::unordered_map<std::uint64_t, lunar_behaviors::Vec2> follow_state_;
    std::unordered_map<std::uint64_t, BehaviorVisual> behavior_visuals_;
};
