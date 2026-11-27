#pragma once

// Comportamientos declarativos del ejemplo C++ (ADR-033/040).
// Funciones puras y parser del fichero scripts.map.txt: sin GPU, sin hilos y
// sin llamadas a la bridge, para poder probarlas con --self-test.

#include <cstdint>
#include <map>
#include <string>
#include <vector>

namespace lunar_behaviors {

struct Binding {
    std::uint64_t entity = 0;
    std::string behavior;
    std::map<std::string, std::string> params;
};

struct ParseResult {
    std::vector<Binding> bindings;
    std::vector<std::string> errors;
};

struct Vec2 {
    float x = 0.0F;
    float y = 0.0F;
};

/// Parsea `id=Behavior;clave=valor;...`, ignorando líneas inválidas.
ParseResult parse_map(const std::string &text);
/// Convierte los errores en líneas para scripts.diagnostics.txt.
std::string encode_diagnostics(const std::vector<std::string> &errors);

/// Valores con los que se construye el desplazamiento visual de cada frame.
float number(const Binding &binding, const char *key, float fallback);
bool axis_is(const Binding &binding, const char *key, char expected);

float patrol_offset(float elapsed, float speed, float radius);
float bob_offset(float elapsed, float speed, float amplitude);
float spin_angle_degrees(float elapsed, float speed);
Vec2 follow_step(Vec2 current, Vec2 target, float speed, float radius, float dt);

/// Pruebas headless de las funciones puras y del parser.
bool self_test();

}  // namespace lunar_behaviors
