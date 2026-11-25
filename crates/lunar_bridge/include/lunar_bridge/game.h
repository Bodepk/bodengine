// Contrato de la lógica de juego C++ con la bridge de Lunar (ADR-021).
// Este header es fuente del repositorio; el header de cxx se genera en OUT_DIR.
#pragma once

#include <cstdint>

namespace lunar {

// Paso fijo de ADR-005, para pruebas deterministas del bucle C++.
constexpr std::uint64_t kFixedDtNs = 16'666'667;

enum class KeyCode : std::uint32_t {
    Escape = 1,
    W = 2,
    A = 3,
    S = 4,
    D = 5,
    ArrowUp = 6,
    ArrowDown = 7,
    ArrowLeft = 8,
    ArrowRight = 9,
};

// Códigos de botón traducidos por `lunar_platform` (ADR-023).
enum class MouseButtonCode : std::uint32_t {
    Unknown = 0,
    Left = 1,
    Right = 2,
    Middle = 3,
    Back = 4,
    Forward = 5,
};

enum class BodyKind : std::uint8_t {
    None = 0,
    Dynamic = 1,
    Fixed = 2,
    Sensor = 3,
    KinematicPositionBased = 4,
};

enum class ShapeKind : std::uint8_t {
    None = 0,
    Ball = 1,
    Cuboid = 2,
};

}  // namespace lunar

// Clase abstracta que implementa el juego C++ y conduce el bucle desde Rust.
class Game {
  public:
    virtual ~Game() = default;
    virtual void on_key(std::uint32_t key, bool pressed) = 0;
    // Opcionales: los juegos existentes no tienen que implementarlas (ADR-023).
    virtual void on_mouse(std::uint32_t, bool) {}
    virtual void on_cursor(double, double) {}
    virtual void on_scroll(float, float, bool) {}
    virtual void on_resize(std::uint32_t width, std::uint32_t height) = 0;
    virtual bool on_frame() = 0;
    virtual void on_render() = 0;
};
