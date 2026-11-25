// Plantilla de consumo del SDK instalado: solo incluye headers públicos y
// enlaza el target importado `lunar::bridge` (ADR-022).
#include <cstdint>
#include <iostream>
#include <string>
#include <utility>
#include <vector>

#include <lunar_bridge/game.h>
#include <lunar_bridge/src/lib.rs.h>

namespace {

class TemplateGame final : public Game {
  public:
    void on_key(std::uint32_t, bool) override {}
    void on_resize(std::uint32_t, std::uint32_t) override {}
    bool on_frame() override { return true; }
    void on_render() override {}
};

constexpr std::uint64_t kFixedDtNs = 16'666'667;

}  // namespace

int main() {
    TemplateGame game;
    game.on_key(static_cast<std::uint32_t>(lunar::KeyCode::W), true);

    auto engine = new_engine(EngineConfig{"SDK template", 640, 480});
    if (std::string(engine->title()) != "SDK template" ||
        engine->width() != 640 || engine->height() != 480) {
        std::cerr << "la configuración del SDK no llegó al motor\n";
        return 1;
    }

    engine->press_mouse(static_cast<std::uint32_t>(lunar::MouseButtonCode::Left), true);
    engine->set_cursor(320.0, 240.0);
    engine->add_scroll(0.0, 1.0, false);

    // El primer frame ancla el reloj y preserva el estado de ratón del evento.
    FrameStats stats = engine->tick(kFixedDtNs);
    const MouseButtonState mouse = engine->mouse_state(
        static_cast<std::uint32_t>(lunar::MouseButtonCode::Left));
    if (!mouse.down || !mouse.pressed) {
        std::cerr << "el estado del ratón no llegó al frame\n";
        return 1;
    }
    const CursorPoint ndc = engine->cursor_ndc();
    if (!ndc.valid || ndc.x != 0.0f || ndc.y != 0.0f) {
        std::cerr << "la conversión de cursor a NDC falló\n";
        return 1;
    }
    if (!engine->scroll_delta().valid) {
        std::cerr << "el scroll del frame no llegó a C++\n";
        return 1;
    }

    // Audio sin dispositivo: los handles inválidos se rechazan en Rust.
    if (engine->load_sound("beep.wav") != 0 || engine->play_sound(999) != 0 ||
        engine->stop_sound(999) || engine->set_volume(999, 0.5f) ||
        engine->is_sound_playing(999)) {
        std::cerr << "la validación de audio desde C++ falló\n";
        return 1;
    }
    engine->set_master_volume(0.4f);
    if (engine->master_volume() != 0.4f) {
        std::cerr << "el volumen maestro no llegó a C++\n";
        return 1;
    }
    engine->stop_all_sounds();

    // Servicios 3D sin assets: los handles inválidos se rechazan en Rust.
    engine->set_gravity3d(0.0f, 0.0f, 0.0f);
    const std::uint64_t body3d = engine->add_body3d(2, 0.0f, 0.0f, 0.0f);
    const std::uint64_t collider3d = engine->add_collider3d(body3d, 2, 1.0f, 1.0f, 1.0f);
    if (body3d == 0 || collider3d == 0 || !engine->step_physics3d(1.0f / 60.0f) ||
        !engine->body_state3d(body3d).valid || engine->apply_impulse3d(999, 1.0f, 0.0f, 0.0f) ||
        engine->cast_ray3d(0.0f, 0.0f, 0.0f, 0.0f, 0.0f, 0.0f, 1.0f).hit) {
        std::cerr << "la validación de física 3D desde C++ falló\n";
        return 1;
    }
    if (!engine->set_camera3d(0.0f, 0.0f, 3.0f, 0.0f, 0.0f, 0.0f,
                              0.0f, 1.0f, 0.0f, 1.0f, 1.5f, 0.1f, 10.0f) ||
        engine->view_projection3d().values.at(0) == 0.0f) {
        std::cerr << "la cámara 3D desde C++ falló\n";
        return 1;
    }
    const std::vector<MeshDrawCpp> invalid_draws{
        MeshDrawCpp{999, engine->view_projection3d().values, {1.0f, 1.0f, 1.0f, 1.0f}}};
    if (engine->submit_mesh_draws(rust::Slice<const MeshDrawCpp>(invalid_draws)) ||
        engine->mesh_draw_count() != 0) {
        std::cerr << "una orden 3D inválida fue aceptada\n";
        return 1;
    }

    // Documento de escena: sin ruta conocida no hay guardado ni recarga.
    if (!engine->scene_path().empty() || engine->save_scene_current() ||
        engine->reload_scene() || engine->poll_scene_reload()) {
        std::cerr << "el documento de escena del SDK no está vacío\n";
        return 1;
    }

    // Los 60 frames siguientes producen un tick cada uno.
    for (int frame = 1; frame < 61; ++frame) {
        stats = engine->tick(kFixedDtNs);
    }

    if (stats.substeps != 1 || stats.tick_count != 60 ||
        stats.frame_count != 61) {
        std::cerr << "el reloj del SDK no produjo 60 ticks\n";
        return 1;
    }

    // Diagnósticos del SDK: métricas, log y replay sin dispositivos externos.
    if (engine->frame_metrics().frame_count != stats.frame_count ||
        engine->state_hash() == 0 || !engine->log_event(2, 10, 1, 2) ||
        engine->log_entry(0).code != 10) {
        std::cerr << "los diagnósticos del SDK fallaron\n";
        return 1;
    }
    if (!engine->begin_replay_recording() || engine->end_replay_recording() != 0 ||
        !engine->load_replay_frames(rust::Slice<const ReplayFrameCpp>()) ||
        engine->start_replay_playback() || engine->stop_replay_playback()) {
        std::cerr << "el replay vacío del SDK falló\n";
        return 1;
    }

    std::cout << "SDK template: " << std::string(engine->title()) << ' '
              << engine->width() << "x" << engine->height() << " | frames "
              << stats.frame_count << " | ticks " << stats.tick_count << '\n';
    return 0;
}
