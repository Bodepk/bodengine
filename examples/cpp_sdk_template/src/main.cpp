// Plantilla de consumo del SDK instalado: solo incluye headers públicos y
// enlaza el target importado `lunar::bridge` (ADR-022).
#include <cstdint>
#include <iostream>
#include <string>
#include <utility>

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

    // Los 60 frames siguientes producen un tick cada uno.
    for (int frame = 1; frame < 61; ++frame) {
        stats = engine->tick(kFixedDtNs);
    }

    if (stats.substeps != 1 || stats.tick_count != 60 ||
        stats.frame_count != 61) {
        std::cerr << "el reloj del SDK no produjo 60 ticks\n";
        return 1;
    }

    std::cout << "SDK template: " << std::string(engine->title()) << ' '
              << engine->width() << "x" << engine->height() << " | frames "
              << stats.frame_count << " | ticks " << stats.tick_count << '\n';
    return 0;
}
