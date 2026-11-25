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

    FrameStats stats;
    // El primer frame ancla el reloj; los 60 siguientes producen un tick cada uno.
    for (int frame = 0; frame < 61; ++frame) {
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
