# ADR-006: winit 0.30 confinado en `lunar_platform`

- Estado: Aceptada
- Fecha: 2026-11-24

## Contexto

winit cambió su API en 0.30 (`ApplicationHandler`, `ActiveEventLoop`) y su presencia en más de un crate multiplicaría el acoplamiento y las incompatibilidades de versión.

## Decisión

Fijar `winit` = 0.30.13. Solo `lunar_platform` declara esa dependencia y traduce eventos a tipos propios del motor (`WindowEvent`, `KeyCode`, `InputState`). El resto del motor recibe eventos abstractos.

`lunar_platform` usa `ApplicationHandler` y un `ActiveEventLoop` provisto por winit. El cierre se solicita con ESC en el smoke de H1. En H2, además, es el único lugar que convierte el `Window` de winit en una surface de `wgpu` (ADR-009); mantiene el `Instance` y un envoltorio `Arc<Window>` para que la surface sobreviva al préstamo del evento `resumed`.

## Consecuencias

- H2 debe cubrir resize a 0×0 y ventanas minimizadas sin panic; esa lógica vive en `lunar_platform`/`lunar_render`, no en `lunar_core`.
- Actualizar winit es un cambio localizado a un crate.
- Los tests del bucle y del ECS no abren ventana.
