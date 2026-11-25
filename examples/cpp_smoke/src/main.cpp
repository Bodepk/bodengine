// Smoke de la API pública C++ del H3 (ADR-004).
#include <cstdint>
#include <iostream>
#include <utility>
#include <vector>

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

    // Servicios 3D sin assets: física, cámara y validación de órdenes.
    engine->set_gravity3d(0.0f, 0.0f, 0.0f);
    const std::uint64_t body = engine->add_body3d(2, 0.0f, 0.0f, 0.0f);
    const std::uint64_t collider = engine->add_collider3d(body, 2, 1.0f, 1.0f, 1.0f);
    if (body == 0 || collider == 0 || !engine->step_physics3d(1.0f / 60.0f) ||
        !engine->body_state3d(body).valid || !engine->apply_impulse3d(body, 1.0f, 0.0f, 0.0f)) {
        std::cerr << "la física 3D desde C++ falló\n";
        return 1;
    }
    const RayHit3DCpp hit = engine->cast_ray3d(
        0.0f, 5.0f, 0.0f, 0.0f, -1.0f, 0.0f, 10.0f);
    if (!hit.hit || hit.collider != collider) {
        std::cerr << "el raycast 3D desde C++ falló\n";
        return 1;
    }
    if (!engine->set_camera3d(0.0f, 0.0f, 3.0f, 0.0f, 0.0f, 0.0f,
                              0.0f, 1.0f, 0.0f, 1.0f, 1.5f, 0.1f, 10.0f)) {
        std::cerr << "la cámara 3D desde C++ falló\n";
        return 1;
    }
    const std::vector<MeshDrawCpp> invalid_draws{
        MeshDrawCpp{999, engine->view_projection3d().values, {1.0f, 1.0f, 1.0f, 1.0f}}};
    if (engine->submit_mesh_draws(rust::Slice<const MeshDrawCpp>(invalid_draws))) {
        std::cerr << "una orden 3D con handle inválido fue aceptada\n";
        return 1;
    }

    // Sin escena cargada, el documento está vacío y las operaciones se rechazan.
    if (!engine->scene_path().empty() || engine->scene_is_dirty() ||
        engine->scene_can_undo() || engine->scene_can_redo() ||
        engine->save_scene_current() || engine->reload_scene() ||
        engine->poll_scene_reload() ||
        engine->editor_create_entity(1, "Nave")) {
        std::cerr << "el documento de escena vacío no se comportó como tal\n";
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
