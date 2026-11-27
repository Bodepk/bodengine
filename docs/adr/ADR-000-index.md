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
| [008](ADR-008-scenes.md) | Formato de escenas | Aceptada |
| [009](ADR-009-render.md) | Render `wgpu` y tamaños de surface degenerados | Aceptada |
| [010](ADR-010-physics2d.md) | Física 2D con `rapier2d` | Aceptada |
| [011](ADR-011-physics3d.md) | Física 3D con `rapier3d` | Aceptada |
| [012](ADR-012-assets-image.md) | Assets de imagen y crate `lunar_assets` | Aceptada |
| [013](ADR-013-assets-gltf.md) | Assets glTF en `lunar_assets` | Aceptada |
| [014](ADR-014-audio.md) | Audio con `rodio` y crate `lunar_audio` | Aceptada |
| [015](ADR-015-editor.md) | Núcleo de editor H10 | Aceptada |
| [016](ADR-016-cross-platform.md) | Pulido multiplataforma H11 | Aceptada |
| [017](ADR-017-demo-render.md) | Gancho de frame y triángulo de demo | Aceptada |
| [018](ADR-018-sprite-platformer.md) | Juego de plataformas con sprites | Aceptada |
| [019](ADR-019-editor-ui.md) | Interfaz gráfica del editor | Aceptada |
| [020](ADR-020-visual-editor.md) | Editor visual con assets arrastrables | Aceptada |
| [021](ADR-021-cpp-scripting.md) | Scripting C++ de primera clase | Aceptada |
| [022](ADR-022-cpp-sdk.md) | SDK C++ distribuible | Aceptada |
| [023](ADR-023-cpp-mouse-input.md) | Input de ratón y viewport en C++ | Aceptada |
| [024](ADR-024-cpp-audio.md) | Audio desde C++ | Aceptada |
| [025](ADR-025-cpp-3d.md) | Servicios 3D en la bridge C++ | Aceptada |
| [026](ADR-026-cpp-scene-documents.md) | Documentos de escena y recarga desde C++ | Aceptada |
| [027](ADR-027-scene-v2-sprites.md) | Escenas v2 con sprites, animación y prefabs | Aceptada |
| [028](ADR-028-diagnostics-replay.md) | Diagnósticos, replay de input y hashes de estado | Aceptada |
| [029](ADR-029-release-0-2-0.md) | Publicación de `v0.2.0` | Aceptada |
| [030](ADR-030-inspector-componentes.md) | Inspector completo y edición de componentes | Aceptada |
| [031](ADR-031-preferencias-editor.md) | Preferencias persistentes del editor | Aceptada |
| [032](ADR-032-vista-3d-cpu-editor.md) | Vista 3D CPU en el editor | Aceptada |
| [033](ADR-033-scripting-fase2.md) | Scripting de Fase 2 en el editor | Aceptada |
| [034](ADR-034-guids-assets-editor.md) | GUIDs de assets en el editor | Aceptada |
| [035](ADR-035-prefabs-editor.md) | Prefabs en el editor | Aceptada |
| [036](ADR-036-import-options-reimport.md) | Opciones de importación y reimportado | Aceptada |
| [037](ADR-037-modos-viewport.md) | Modos de sombreado del viewport 3D | Aceptada |
| [038](ADR-038-content-browser-vistas-dnd.md) | Vistas del content browser y DnD tipado | Aceptada |
| [039](ADR-039-consola-comandos-editor.md) | Consola de comandos del editor | Aceptada |
| [040](ADR-040-scripts-map-diagnosticos.md) | Formato de scripts.map.txt y diagnósticos | Aceptada |
| [041](ADR-041-snap-rotacion-escala.md) | Snap de rotación y escala en el editor | Aceptada |

## Consecuencias

- Cada commit que introduce una decisión nueva debe tocar su ADR o crear uno.
- Las desviaciones del stack aprobado se registran como `Supersedida` + ADR nuevo, nunca como cambio silencioso.
- `README.md` mantiene el resumen; los ADR mantienen el detalle.
