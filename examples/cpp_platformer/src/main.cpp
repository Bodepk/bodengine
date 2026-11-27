#include "game.hpp"

#include <exception>
#include <iostream>
#include <string>


int main(int argc, char **argv) {
#ifdef LUNAR_ASSETS_DIR
    const std::string assets_dir = LUNAR_ASSETS_DIR;
#else
    const std::string assets_dir = "examples/sprite_platformer/assets";
#endif

    try {
        if (argc > 1 && std::string(argv[1]) == "--self-test") {
            const bool ok = lunar_behaviors::self_test();
            std::cout << (ok ? "comportamientos C++ verificados\n"
                             : "fallaron los comportamientos C++\n");
            return ok ? 0 : 1;
        }

        GameConfig config{"Lunar C++ platformer", 960, 540, rust::String(assets_dir)};
        auto engine = new_game_engine(config);
        PlatformerGame game(*engine, config);

        if (argc > 1 && std::string(argv[1]) == "--headless") {
            const bool ok = game.headless_check();
            std::cout << (ok ? "bucle C++ verificado\n" : "falló el bucle C++\n");
            return ok ? 0 : 1;
        }

        std::cout << "A/D o flechas mueven; W o ArrowUp salta; Escape sale\n";
        const bool ok = run(config, *engine, game);
        if (!ok) {
            std::cerr << "error en la ventana del motor\n";
            return 1;
        }
        return 0;
    } catch (const std::exception &error) {
        std::cerr << "error en el juego C++: " << error.what() << '\n';
        return 1;
    }
}
