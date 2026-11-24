# ADR-000: Índice y proceso de decisiones

- Estado: Aceptada
- Fecha: 2026-11-24

## Contexto

El motor se construye por hitos incrementales y necesita un registro auditable de por qué se eligió cada tecnología y cada frontera entre módulos. Sin ese registro, las decisiones se rehacen y las excepciones se vuelven deuda invisible.

## Decisión

Toda decisión arquitectónica se documenta como un ADR numerado en este directorio, con estado `Propuesta`, `Aceptada`, `Pendiente de validar` o `Supersedida`. Un ADR se escribe **antes** de implementar la decisión que describe. Si la implementación difiere, se actualiza el ADR en el mismo commit o se crea uno nuevo que lo supere.

Los ADR no se editan para borrar el historial de una decisión: si cambia el criterio, se marca como `Supersedida` y se apunta al ADR nuevo.

## Índice

| ADR | Título | Estado |
| --- | --- | --- |
| [001](ADR-001-stack.md) | Stack tecnológico y restricción de dependencias | Aceptada |
| [002](ADR-002-workspace.md) | Workspace, features y límites entre crates | Aceptada |
| [003](ADR-003-math.md) | Tipos matemáticos propios y conversores | Aceptada |
| [004](ADR-004-ffi.md) | Frontera cxx y firma de `EngineConfig` | Aceptada |
| [005](ADR-005-loop.md) | Bucle de paso fijo a 60 Hz | Aceptada |
| [006](ADR-006-winit.md) | winit 0.30 confinado en `lunar_platform` | Aceptada |
| [007](ADR-007-editor.md) | Editor como crate opcional | Aceptada |
| [008](ADR-008-scenes.md) | Formato de escenas | Pendiente de decidir antes de H9 |
| [009](ADR-009-render.md) | Render `wgpu` y tamaños de surface degenerados | Aceptada |
| [010](ADR-010-physics2d.md) | Física 2D con `rapier2d` | Aceptada |

## Consecuencias

- Cada commit que introduce una decisión nueva debe tocar su ADR o crear uno.
- Las desviaciones del stack aprobado se registran como `Supersedida` + ADR nuevo, nunca como cambio silencioso.
- `README.md` mantiene el resumen; los ADR mantienen el detalle.
