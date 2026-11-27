#include "behaviors.hpp"

#include <algorithm>
#include <cassert>
#include <cctype>
#include <cmath>
#include <sstream>
#include <utility>

namespace lunar_behaviors {
namespace {

constexpr float kPi = 3.14159265358979323846F;

enum class Kind { Float, Entity, Axis };

struct ParamSpec {
    const char *key;
    const char *fallback;
    Kind kind;
};

const ParamSpec kPatrol[] = {
    {"speed", "1", Kind::Float},
    {"radius", "2", Kind::Float},
    {"axis", "x", Kind::Axis},
};
const ParamSpec kFollow[] = {
    {"target", "0", Kind::Entity},
    {"speed", "1", Kind::Float},
    {"radius", "0.5", Kind::Float},
};
const ParamSpec kSpin[] = {
    {"speed", "90", Kind::Float},
    {"axis", "z", Kind::Axis},
};
const ParamSpec kBob[] = {
    {"speed", "1", Kind::Float},
    {"amplitude", "0.5", Kind::Float},
};

struct Schema {
    const char *name;
    const ParamSpec *params;
    std::size_t count;
};

const Schema kSchemas[] = {
    {"Patrol", kPatrol, 3},
    {"Follow", kFollow, 3},
    {"Spin", kSpin, 2},
    {"Bob", kBob, 2},
};

std::string trim(const std::string &text) {
    const auto first = std::find_if_not(text.begin(), text.end(), [](unsigned char ch) {
        return std::isspace(ch) != 0;
    });
    const auto last = std::find_if_not(text.rbegin(), text.rend(), [](unsigned char ch) {
                          return std::isspace(ch) != 0;
                      }).base();
    if (first >= last) {
        return {};
    }
    return std::string(first, last);
}

std::string lower(std::string text) {
    std::transform(text.begin(), text.end(), text.begin(), [](unsigned char ch) {
        return static_cast<char>(std::tolower(ch));
    });
    return text;
}

bool parses_float(const std::string &text) {
    if (text.empty()) {
        return false;
    }
    try {
        std::size_t used = 0;
        const float value = std::stof(text, &used);
        return used == text.size() && std::isfinite(value);
    } catch (...) {
        return false;
    }
}

bool parses_entity(const std::string &text) {
    if (text.empty()) {
        return false;
    }
    try {
        std::size_t used = 0;
        const auto value = std::stoull(text, &used);
        (void)value;
        return used == text.size();
    } catch (...) {
        return false;
    }
}

bool accepts(const ParamSpec &spec, const std::string &value) {
    switch (spec.kind) {
        case Kind::Float:
            return parses_float(value);
        case Kind::Entity:
            return parses_entity(value);
        case Kind::Axis: {
            const std::string axis = lower(value);
            return axis == "x" || axis == "y" || axis == "z";
        }
    }
    return false;
}

const Schema *schema_for(const std::string &name) {
    for (const Schema &schema : kSchemas) {
        if (lower(name) == lower(schema.name)) {
            return &schema;
        }
    }
    return nullptr;
}

}  // namespace

float number(const Binding &binding, const char *key, float fallback) {
    const auto found = binding.params.find(key);
    if (found == binding.params.end() || !parses_float(found->second)) {
        return fallback;
    }
    return std::stof(found->second);
}

bool axis_is(const Binding &binding, const char *key, char expected) {
    const auto found = binding.params.find(key);
    if (found == binding.params.end()) {
        return false;
    }
    return lower(found->second) == std::string(1, static_cast<char>(std::tolower(expected)));
}

ParseResult parse_map(const std::string &text) {
    ParseResult result;
    std::istringstream stream(text);
    std::string raw;
    std::size_t line_number = 0;
    while (std::getline(stream, raw)) {
        ++line_number;
        const std::string line = trim(raw);
        if (line.empty() || line.front() == '#') {
            continue;
        }
        const auto separator = line.find('=');
        if (separator == std::string::npos) {
            result.errors.push_back(
                "línea " + std::to_string(line_number) + ": falta '=' en '" + line + "'");
            continue;
        }
        const std::string id_text = trim(line.substr(0, separator));
        if (!parses_entity(id_text)) {
            result.errors.push_back(
                "línea " + std::to_string(line_number) + ": identificador no válido '" + id_text + "'");
            continue;
        }
        const std::string value = trim(line.substr(separator + 1));
        const auto behavior_end = value.find(';');
        const std::string behavior_name = trim(value.substr(0, behavior_end));
        const Schema *schema = schema_for(behavior_name);
        if (schema == nullptr) {
            result.errors.push_back(
                "línea " + std::to_string(line_number) + ": comportamiento desconocido '" +
                behavior_name + "'");
            continue;
        }

        Binding binding;
        binding.entity = std::stoull(id_text);
        binding.behavior = schema->name;
        std::size_t cursor = behavior_end == std::string::npos ? value.size() : behavior_end;
        while (cursor < value.size()) {
            const std::size_t end = value.find(';', cursor + 1);
            const std::string field = trim(value.substr(cursor + 1, end - cursor - 1));
            cursor = end == std::string::npos ? value.size() : end;
            if (field.empty()) {
                continue;
            }
            const auto pair = field.find('=');
            if (pair == std::string::npos) {
                result.errors.push_back(
                    "línea " + std::to_string(line_number) + ": parámetro mal escrito '" + field + "'");
                continue;
            }
            const std::string key = trim(field.substr(0, pair));
            const std::string field_value = trim(field.substr(pair + 1));
            const ParamSpec *spec = nullptr;
            for (std::size_t index = 0; index < schema->count; ++index) {
                const ParamSpec &candidate = schema->params[index];
                if (key == candidate.key) {
                    spec = &candidate;
                    break;
                }
            }
            if (spec == nullptr) {
                result.errors.push_back(
                    "línea " + std::to_string(line_number) + ": " + schema->name +
                    " no acepta el parámetro '" + key + "'");
            } else if (!accepts(*spec, field_value)) {
                result.errors.push_back(
                    "línea " + std::to_string(line_number) + ": valor no válido '" + field_value +
                    "' para " + key);
            } else {
                binding.params[key] = field_value;
            }
        }
        for (std::size_t index = 0; index < schema->count; ++index) {
            const ParamSpec &spec = schema->params[index];
            if (binding.params.count(spec.key) == 0) {
                binding.params[spec.key] = spec.fallback;
            }
        }
        result.bindings.push_back(std::move(binding));
    }
    return result;
}

std::string encode_diagnostics(const std::vector<std::string> &errors) {
    std::string text;
    for (const std::string &error : errors) {
        text += error;
        text += '\n';
    }
    return text;
}

float patrol_offset(float elapsed, float speed, float radius) {
    return std::sin(elapsed * speed) * radius;
}

float bob_offset(float elapsed, float speed, float amplitude) {
    return std::sin(elapsed * speed) * amplitude;
}

float spin_angle_degrees(float elapsed, float speed) { return elapsed * speed; }

Vec2 follow_step(Vec2 current, Vec2 target, float speed, float radius, float dt) {
    const float dx = target.x - current.x;
    const float dy = target.y - current.y;
    if (std::sqrt(dx * dx + dy * dy) <= radius) {
        return current;
    }
    const float factor = std::clamp(speed * dt, 0.0F, 1.0F);
    return {current.x + dx * factor, current.y + dy * factor};
}

bool self_test() {
    assert(std::abs(patrol_offset(0.25F, 2.0F * kPi, 2.0F) - 2.0F) < 0.001F);
    assert(std::abs(bob_offset(0.25F, 2.0F * kPi, 0.5F) - 0.5F) < 0.001F);
    assert(std::abs(spin_angle_degrees(2.0F, 90.0F) - 180.0F) < 0.001F);

    Vec2 position{0.0F, 0.0F};
    for (int frame = 0; frame < 60; ++frame) {
        position = follow_step(position, {10.0F, 0.0F}, 5.0F, 0.5F, 1.0F / 60.0F);
    }
    assert(position.x > 7.0F && position.x <= 10.0F);
    assert(std::abs(position.y) < 0.001F);

    const ParseResult parsed = parse_map(
        "1=Patrol;speed=2;axis=y\n"
        "2=Nope\n"
        "3=Bob;amplitude=mal\n");
    assert(parsed.bindings.size() == 2);
    assert(parsed.errors.size() == 2);
    assert(parsed.bindings[0].entity == 1);
    assert(number(parsed.bindings[0], "speed", 0.0F) == 2.0F);
    assert(axis_is(parsed.bindings[0], "axis", 'y'));
    assert(number(parsed.bindings[1], "amplitude", 0.5F) == 0.5F);
    assert(encode_diagnostics(parsed.errors).find("línea 2") != std::string::npos);
    return true;
}

}  // namespace lunar_behaviors
