# ADR-001: Stack tecnológico y restricción de dependencias

- Estado: Aceptada
- Fecha: 2026-11-24

## Contexto

El motor debe ser portable entre Windows, Linux y macOS, y tener una API pública C++ estable. Se necesita un conjunto mínimo de tecnologías probadas, sin proliferación de librerías.

## Decisión

El stack aprobado es, y se limita a: `winit` (ventana), `wgpu` (render), `rapier` (física 2D/3D), `bevy_ecs` (ECS), `image` y `gltf` (assets), `rodio` (audio) y `cxx` (FFI). Se añaden `serde`/`serde_json` para escenas, aprobados explícitamente antes de H9 (ver ADR-008).

Versiones fijadas hasta ahora:

- `winit` = 0.30.13 (MSRV 1.70).
- `bevy_ecs` = 0.20.1 (MSRV 1.95, edition 2024). Reason: la toolchain instalada es Rust 1.100.0.
- Toolchain Rust = 1.100.0, edition 2024, resolver 3.

Cualquier dependencia nueva fuera de esta lista requiere justificación escrita y aprobación del usuario antes de entrar en `Cargo.toml`.

## Consecuencias

- Las versiones de `wgpu`, `rapier`, `image`, `gltf`, `rodio` y `cxx` se fijarán en su hito correspondiente, con su MSRV verificado.
- CI ejecuta `cargo tree -e features` para detectar arrastres accidentales de features 2D/3D.
- No se introduce `tokio`, `async-std` ni frameworks GUI antes de H10.
