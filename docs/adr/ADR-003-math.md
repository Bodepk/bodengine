# ADR-003: Tipos matemáticos propios y conversores

- Estado: Aceptada
- Fecha: 2026-11-24

## Contexto

Se necesitan tipos compartidos por física, render y lógica de juego sin acoplar la lógica a rapier ni a wgpu. Además, la frontera C++ exige tipos POD (`#[repr(C)]`) para pasarlos por valor.

## Decisión

`lunar_math` define sus propios tipos POD (`Vec2`, `Vec3`, `Vec4`, `Quat`, `Mat4`), con operadores y tests propios. No depende de `rapier`, `wgpu`, `glam`, `nalgebra` ni de otro crate del motor.

**Los conversores a rapier y a wgpu viven en `lunar_physics*`/`lunar_render*`, no en `lunar_math`.** `lunar_math` solo expone representación y operaciones; cada crate consumidor decide cómo mapearla a su tipo nativo.

## Consecuencias

- Los tests de matemáticas corren sin ninguna dependencia gráfica o física.
- Si un día se cambia rapier o wgpu, `lunar_math` no se toca.
- La ABI C++ puede recibir estos structs por valor porque son `#[repr(C)]` y no contienen `String`, `Vec` ni punteros.
