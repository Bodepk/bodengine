# ADR-002: Workspace, features y límites entre crates

- Estado: Aceptada
- Fecha: 2026-11-24

## Contexto

Un motor híbrido 2D/3D necesita poder compilarse solo con 2D o solo con 3D, y necesita una separación estricta entre lógica pura y efectos externos (ventana, GPU, audio, FFI).

## Decisión

Un único workspace Cargo con crate raíz `lunar` (fachada de features) y crates especializados bajo `crates/`. `default` activa todas las features (`dim2`, `dim3`, `assets`, `audio`, `bridge`, `runtime`); `--no-default-features --features dim2` y `--no-default-features --features dim3` son los builds parciales soportados y no incluyen assets.

Dependencias siempre presentes: `lunar_math`, `lunar_ecs`, `lunar_core`, `lunar_platform`. Opcionales: `lunar_render`, `lunar_render2d`, `lunar_render3d`, `lunar_physics2d`, `lunar_physics3d`, `lunar_assets`, `lunar_audio`, `lunar_bridge`, `lunar_runtime`.

Reglas de dependencia:

- `lunar_math` no depende de ningún crate del motor ni de librerías de GPU/física.
- `lunar_core` no depende de `winit`, `wgpu`, `rapier` ni `rodio`.
- `lunar_platform` es el único crate que menciona `winit`.
- La capa de render convierte desde `lunar_math`; `lunar_math` nunca convierte hacia librerías externas (ver ADR-003).
- `lunar_runtime` compone subsistemas; ningún otro crate depende de él.
- Excepción H1: `lunar_core` tiene `bevy_ecs` solo como *dev-dependency* porque el derive `Component` de bevy genera rutas `bevy_ecs::…`. No se usa en código de producción; la API pública de `lunar_core` sigue pasando solo por `lunar_ecs`.

## Consecuencias

- La lógica del bucle y del ECS se testea sin crear ventana ni dispositivo GPU.
- Los gates de features se comprueban en CI con `cargo tree -e features` sobre `-p lunar`.
- `examples/minimal_game` es un workspace member con `autoexamples = false` en la raíz para evitar targets duplicados.
