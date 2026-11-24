// Smoke de la API pública C++ del H3 (ADR-004).
#include <cstdint>
#include <iostream>
#include <utility>

#include <lunar_bridge/src/lib.rs.h>

namespace {
constexpr std::uint64_t kFixedDtNs = 16'666'667;
}

int main() {
    auto engine = new_engine(EngineConfig{"Lunar H3 smoke", 800, 600});

    if (std::string(engine->title()) != "Lunar H3 smoke" ||
        engine->width() != 800 || engine->height() != 600) {
        std::cerr << "la configuración compartida no llegó al motor\n";
        return 1;
    }

    FrameStats stats;
    // El primer frame ancla el reloj; los 60 siguientes producen un tick cada uno.
    for (int frame = 0; frame < 61; ++frame) {
        stats = engine->tick(kFixedDtNs);
    }

    if (stats.substeps != 1 || stats.tick_count != 60 ||
        stats.frame_count != 61) {
        std::cerr << "el bucle de paso fijo no produjo 60 ticks\n";
        return 1;
    }

    std::cout << "C++ smoke: " << std::string(engine->title()) << ' '
              << engine->width() << "x" << engine->height() << " | frames "
              << stats.frame_count << " | ticks " << stats.tick_count << '\n';
    return 0;
}
