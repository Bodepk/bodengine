# Lunar

Motor híbrido 2D/3D en construcción: núcleo Rust, API pública C++ mediante `cxx` y editor como crate opcional.

> Estado actual: **H0, H1 y H2 completados**. H3 sigue cerrado hasta confirmación; el editor (H10) está bloqueado hasta aprobación explícita.

## Principios

1. Claridad y corrección antes que optimización.
2. Toda la lógica testeable vive sin GPU; el render es una capa de presentación.
3. Cada hito termina con `cargo fmt --check`, `cargo clippy -- -D warnings` y `cargo test --workspace` en verde.
4. Un commit atómico por cambio, siguiendo Conventional Commits.
5. Sin dependencias fuera del stack aprobado sin justificación y aprobación (ver ADR-001).

## Stack fijado

| Uso | Crate | Versión |
| --- | --- | --- |
| Ventana y eventos | `winit` | 0.30.13 |
| ECS | `bevy_ecs` | 0.20.1 |
| Render | `wgpu` | 30.0.2 |
| Física 2D/3D | `rapier` | por fijar en H4/H5 |
| Carga de assets | `image`, `gltf` | por fijar en H6/H7 |
| Audio | `rodio` | por fijar en H8 |
| FFI C++ | `cxx` | por fijar en H3 |

Toolchain fijada en `rust-toolchain.toml`: Rust 1.100.0.

## Estructura

```text
crates/
  lunar_math        Matemáticas POD, sin dependencias de GPU
  lunar_ecs         Fachada sobre bevy_ecs
  lunar_core        Reloj de paso fijo, input, estado del engine
  lunar_platform    winit confinado; crea la surface nativa de wgpu
  lunar_render      Render wgpu, viewport y presentación
  lunar_render2d    Render 2D (marcador de feature)
  lunar_render3d    Render 3D (marcador de feature)
  lunar_physics2d   Rapier 2D
  lunar_physics3d   Rapier 3D
  lunar_audio       rodio
  lunar_bridge      Frontera cxx / API C++
  lunar_runtime     Composición de todos los subsistemas
examples/minimal_game  Smoke 800x600 con render del H2
docs/adr           Decisiones arquitectónicas
tests/fixtures     Datos de prueba sin GPU
```

## Features

`default` activa todo (`dim2`, `dim3`, `audio`, `bridge`, `runtime`). Los builds parciales:

```bash
cargo build -p lunar --no-default-features --features dim2
cargo build -p lunar --no-default-features --features dim3
cargo build -p lunar --no-default-features
```

La última variante excluye también `wgpu`: `lunar_platform/render` se activa desde la feature `render` del crate raíz.

El workflow de CI verifica con `cargo tree -e features` que el árbol 2D-only no arrastra crates 3D y viceversa.

## Comandos

```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
cargo run -p minimal_game
```

## Hitos

| Hito | Contenido | Estado |
| --- | --- | --- |
| H0 | Workspace, ADR, CI, toolchain | ✅ |
| H1 | Math, ECS, bucle 60 Hz, winit, smoke | ✅ |
| H2 | wgpu y resize a 0×0 | ✅ |
| H3 | FFI cxx y ejemplo C++ | ⛔ requiere confirmación |
| H4/H5 | Física 2D/3D | ⛔ |
| H6/H7 | Assets de imagen/glTF | ⛔ |
| H8 | Audio | ⛔ |
| H9 | Escenas y serialización | ⛔ |
| H10 | Editor (crate opcional) | ⛔ aprobación explícita |
| H11 | Pulido multiplataforma | ⛔ |

Las decisiones y sus consecuencias están en [`docs/adr`](docs/adr/ADR-000-index.md).
