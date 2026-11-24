# ADR-010: Física 2D con `rapier2d`

- Estado: Aceptada
- Fecha: 2026-11-24

## Contexto

H4 añade simulación 2D sin acoplar la lógica de juego a Rapier. El bucle ya es de paso fijo (ADR-005) y los tipos vectoriales son propios y POD (ADR-003). La frontera C++ (ADR-004) también lo desea: no conviene exponer handles ni vectores de Rapier en la API pública.

## Decisión

- `rapier2d` queda fijado en **0.36.1** (MSRV 1.86), con `default-features = false` y las features `dim2`, `f32` y `std`. Se descartan `parallel`, `serde-serialize` y SIMD por ahora: no aportan al smoke y añaden superficie de build.
- `lunar_physics2d` posee todo el estado de Rapier (`PhysicsPipeline`, `RigidBodySet`, `ColliderSet`, `QueryPipeline`, `IntegrationParameters`, fases amplia/estrecha y solver).
- La API pública usa `lunar_math::Vec2` y tipos propios. Los conversores a `nalgebra`/Rapier viven dentro de `lunar_physics2d` (ADR-003).
- Cuerpos y colisionadores se identifican con `BodyId`/`ColliderId` opacos derivados de índices, nunca con `RigidBodyHandle` público.
- El step recibe `dt: f32` del llamador. El motor lo alimentará con `FIXED_DT` de ADR-005; el crate de física no decide cuándo avanzar el reloj.
- Formas iniciales: `Ball` y `Cuboid`. Tipos de cuerpo: `Dynamic`, `Fixed` y `KinematicPositionBased`.
- Se expone `cast_ray` como consulta pura sobre el `QueryPipeline` actualizado tras cada step.
- El módulo no depende de `winit`, `wgpu`, ECS ni C++; sus tests son deterministas y corren sin ventana.

## Consecuencias

- El build 2D sigue siendo aislado: `--features dim2` arrastra `lunar_physics2d`, y `--features dim3` no.
- Cambiar de versión de Rapier o reemplazarlo no obliga a tocar `lunar_math` ni la lógica de juego.
- H4 entrega únicamente la capa 2D; H5 (física 3D) y la composición con el runtime quedan para sus propios hitos.
