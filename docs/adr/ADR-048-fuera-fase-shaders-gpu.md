# ADR-048: Fuera de Fase 3 — shader graph, shaders de usuario y PBR GPU

- Estado: Aceptada
- Fecha: 2026-11-27
- Relacionada: ADR-001 (stack), ADR-025 (3D en C++), ADR-032 (vista 3D CPU), ADR-037 (modos de sombreado), ADR-047 (materiales editor-owned)

## Contexto

La Fase 3 menciona un editor de nodos «si es viable», shaders personalizados asignables desde el Content Browser y previsualización PBR. Hoy no hay sistema de shaders por nodos en el motor, ni registro de pipelines, ni carga de `.wgsl` desde disco, ni uniforms propios. `lunar_render` compila dos shaders con `include_str!` y sus pipelines no tienen `depth_stencil`; `lunar_render3d` es una cola CPU sin `wgpu`. ADR-032 y ADR-037 dejan la presentación GPU para una decisión futura.

## Decisión

- **Quedan fuera de la Fase 3**: editor de nodos, compilación de shaders de usuario, asignación de un shader que cambie el render real, texturas de material, normal maps, z-buffer, PBR metálico-rugoso en GPU y su presentación.
- El campo `shader` de ADR-047 se conserva como **referencia textual validada** con etiqueta visible «referencia; compilación fuera de fase». No se finge que se aplica: no se compila ni se envía a ningún pipeline.
- Esta decisión evita tocar la API pública del motor, la ABI C++ `0.2.0`, el esquema v2 y el gate de CI. No se implementa a medias ninguna capacidad de render.
- Para abrir cualquiera de estas capacidades hace falta una ADR nueva que defina pipeline con profundidad, gestión del dispositivo compartido con `eframe`, formato de shader, gates de CI y versión del SDK, con autorización explícita del usuario.

## Consecuencias

- El bloque 1 queda completo como autoría de datos + previsualización CPU coherente, sin promesas de render real.
- No se añaden dependencias ni riesgo de romper el stack aprobado en ADR-001.
