# ADR-032: Vista 3D CPU en el editor

- Estado: Aceptada
- Fecha: 2026-11-26
- Supersede parcialmente: el punto «vista 3D» declarado fuera de alcance en ADR-020
- Nota: ADR-025 deja la presentación GPU de mallas para una decisión posterior. Esta ADR no la implementa.

## Contexto

El editor gráfico terminó los seis sub-hitos del MVP: su panel central es una vista 2D dibujada con `egui::Painter` y `TextureHandle` (ADR-020). Las escenas v2 ya pueden contener `Component::Mesh`, `Component::Collider3D` y transformaciones con cuaternión, así que el inspector puede editar datos 3D, pero no hay forma de verlos en el lienzo.

Presentarlos con `lunar_render` no es posible sin romper reglas vigentes: el gate del árbol `editor_ui` prohíbe `lunar_render`, `glow` y `rfd` (ADR-019, ADR-020, ADR-031); `lunar_render` no tiene pipeline 3D (sus pipelines tienen `depth_stencil: None` y los shaders escriben `z = 0`); y ADR-001 exige aprobación explícita para cualquier dependencia nueva. `lunar_render3d` es una superficie CPU sin `wgpu` que valida mallas y acumula órdenes, pero no dibuja en pantalla.

Para editar escenas 3D sin levantar esas restricciones, el usuario aprobó el 2026-11-26 una vista previa 3D calculada en CPU y dibujada con el mismo `Painter` de egui.

## Decisión

- Se añade el módulo `view3d.rs` en `lunar_editor_ui`, hermano de `canvas.rs`, y un enum `ViewMode { TwoD, ThreeD }` en `EditorApp`. El botón `2D`/`3D` de la barra del lienzo alterna el modo; el valor inicial sigue siendo `TwoD`. El modo no se añade a `ADR-031`: no es una preferencia persistida.
- El estado de cámara 3D son campos nuevos de `EditorApp`: `yaw`, `pitch`, `distance` y `target`. Arrastre derecho orbita, arrastre central desplaza y la rueda acerca con `distance` acotado a `[1, 200]`. La proyección, la base de la cámara, la rotación por cuaternión de las esquinas de la caja y el *hit-testing* son funciones puras en `view3d.rs`, implementadas con `f32` y arrays: no se añade `lunar_math` ni `lunar_render3d` al crate.
- La vista pinta con `egui::Painter`: rejilla XZ, ejes del mundo y, por entidad, una caja alámbrica a partir de `translation`/`rotation`/`scale`. Las entidades con `Component::Mesh` o `Collider3D` se dibujan como caja alámbrica; los marcadores de cámara y luz como una cruz de ejes. Sin relleno, sin texturas y sin materiales: no hay ordenación por profundidad porque no hay superficies opacas. La selección resalta la caja en azul y la etiqueta muestra nombre e id.
- La selección en 3D usa el mismo `EditorDocument::select` del lienzo 2D, con Shift/Ctrl para extender. El único gizmo 3D de este hito es el de traslación: tres tiradores X/Y/Z que al soltar envían **un único** `SetTransform` mediante `EditorApp::apply`. Rotar y escalar en 3D se hace desde el inspector; `W`/`E`/`R` siguen cambiando `TransformMode`, pero en modo 3D solo `Move` muestra tirador y los otros dos lo indican en la consola.
- La simulación de Play (ADR-020 posterior, hito E4) reutiliza `app.preview` también en 3D: las entidades dinámicas se dibujan en su posición simulada sin mutar el documento. El bloqueo `ensure_editable()` y la ruta única de mutación `EditorDocument::apply` se mantienen idénticos.
- No hay dependencias nuevas: `crates/lunar_editor_ui/Cargo.toml` sigue con `eframe`, `egui`, `lunar_editor`, `lunar_scenes` y `lunar_assets`. No se toca la API pública del motor ni la de los crates internos. Los gates de `cargo tree` del árbol `editor_ui` quedan sin cambios.
- Los tests son headless: proyección del objetivo al centro, descarte de puntos detrás de la cámara, límites de cámara, rotación y aristas de la caja, *hit-testing* de tiradores, cajas y cruces de marcadores, y un `SetTransform` único con undo. Dos `ctx.run_pass` comprueban el botón `2D`/`3D` y los controles de la vista 3D. Ninguno crea dispositivo, superficie ni ventana.

## Consecuencias

- El editor gana una vista 3D para inspeccionar y trasladar entidades sin exponer `wgpu`, sin `lunar_render` en su árbol y sin cambiar la fachada `lunar`.
- Se conservan todas las reglas de ADR-001, ADR-019, ADR-020, ADR-025 y ADR-031: GUI opcional, fuera de `default`, sin `rfd`, sin `glow` y sin dependencias nuevas.
- La vista es una ayuda de edición, no un render de juego: no hay z-buffer, iluminación, texturas ni materiales; en escenas grandes el coste de pintar en CPU puede ser alto.
- Rotar y escalar con gizmo en 3D queda fuera de este hito; ambas operaciones siguen disponibles desde el inspector.
- La presentación GPU real de mallas (pipeline con profundidad en `lunar_render`, adaptador al dispositivo de eframe y cambio del gate de CI) requiere una ADR-033 posterior y la autorización explícita del usuario antes de escribirse.
