# ADR-001: Stack tecnológico y restricción de dependencias

- Estado: Aceptada
- Fecha: 2026-11-24

## Contexto

El motor debe ser portable entre Windows, Linux y macOS, y tener una API pública C++ estable. Se necesita un conjunto mínimo de tecnologías probadas, sin proliferación de librerías.

## Decisión

El stack aprobado es, y se limita a: `winit` (ventana), `wgpu` (render), `rapier` (física 2D/3D), `bevy_ecs` (ECS), `image` y `gltf` (assets), `rodio` (audio) y `cxx` (FFI). Se añaden `serde`/`serde_json` para escenas, aprobados explícitamente antes de H9 (ver ADR-008).

Versiones fijadas hasta ahora:

- `winit` = 0.30.13 (MSRV 1.70).
- `wgpu` = 30.0.2 (MSRV 1.87.0), fijado en H2 (ADR-009).
- `cxx` y `cxx-build` = 1.0.205 (MSRV 1.88), fijados en H3 (ADR-004).
- `rapier2d` y `rapier3d` = 0.36.1 (MSRV 1.86), fijados en H4/H5 (ADR-010, ADR-011).
- `image` = 0.25.10 (MSRV 1.88), fijado en H6 (ADR-012) con `default-features = false` y solo `png`.
- `bevy_ecs` = 0.20.1 (MSRV 1.95, edition 2024). Reason: la toolchain instalada es Rust 1.100.0.
- Toolchain Rust = 1.100.0, edition 2024, resolver 3.

Cualquier dependencia nueva fuera de esta lista requiere justificación escrita y aprobación del usuario antes de entrar en `Cargo.toml`.

## Consecuencias

- Las versiones de `image`, `gltf` y `rodio` se fijarán en su hito correspondiente, con su MSRV verificado.
- CI ejecuta `cargo tree -e features` para detectar arrastres accidentales de features 2D/3D.
- No se introduce `tokio`, `async-std` ni frameworks GUI antes de H10.
