# ADR-011: Física 3D con `rapier3d`

- Estado: Aceptada
- Fecha: 2026-11-24

## Contexto

H5 completa la capa de simulación con el caso 3D. La API pública debe mantener la misma separación que H4: tipos POD propios (ADR-003), sin handles de Rapier, sin reloj propio y sin acoplamiento con render, ventana ni C++. El build 3D-only debe seguir excluyendo `rapier2d` y los crates 2D (ADR-002).

## Decisión

- `rapier3d` queda fijado en **0.36.1** (MSRV 1.86), con `default-features = false` y las features `dim3`, `f32` y `std`. Igual que en H4, se descartan `parallel`, `serde-serialize` y SIMD.
- `lunar_physics3d` posee todo el estado de Rapier (`PhysicsWorld`, conjuntos de cuerpos y colisionadores, fases amplia/estrecha y solver).
- La API usa `lunar_math::Vec3` y `lunar_math::Quat` en orden `(x, y, z, w)`. Los conversores a los tipos de Rapier viven dentro de `lunar_physics3d` (ADR-003).
- Cuerpos y colisionadores se identifican con `BodyId`/`ColliderId` opacos; nunca se exponen `RigidBodyHandle` ni `ColliderHandle`.
- El step recibe `dt: f32` del llamador. El motor lo alimentará con `FIXED_DT` de ADR-005; el crate de física no decide cuándo avanzar el reloj.
- Formas iniciales: `Ball`, `Cuboid` y `Capsule` (eje Y, con semialtura y radio). Tipos de cuerpo: `Dynamic`, `Fixed` y `KinematicPositionBased`.
- Se expone `cast_ray` como consulta pura sobre el mundo tras cada step.
- El módulo no depende de `winit`, `wgpu`, ECS ni C++; sus tests son deterministas y corren sin ventana.

## Consecuencias

- El build 3D sigue siendo aislado: `--features dim3` arrastra `lunar_physics3d`, y `--features dim2` no.
- H5 entrega únicamente la capa 3D; la composición con el runtime y la exposición en la bridge C++ quedan para sus propios hitos.
- Cambiar de versión de Rapier o reemplazarlo no obliga a tocar `lunar_math` ni la lógica de juego.
