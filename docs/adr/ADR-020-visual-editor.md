# ADR-020: Editor visual con assets arrastrables

- Estado: Aceptada
- Fecha: 2026-11-24
- Supersede parcialmente: ADR-019 (panel central de diagrama, sin preview de sprites ni arrastre en la jerarquía)

## Contexto

ADR-019 entregó un frente `egui`/`eframe` con jerarquía, inspector y un diagrama de nodos en el panel central. El usuario pidió ahora un editor visual: cargar una carpeta de assets, ver los sprites, arrastrarlos al nivel, colocarlos, moverlos, escalarlos, rotarlos y reordenar la jerarquía, manteniendo deshacer/rehacer y guardado JSON.

ADR-019 delegaba explícitamente el preview de sprites, la cámara y el arrastre en la jerarquía a una ADR nueva, así que esta decisión se registra antes de tocar `lunar_editor_ui`, `lunar_editor` o el esquema de escenas. No se añaden dependencias: `egui`/`eframe` 0.37 ya están aprobados y `lunar_assets` ya decodifica PNG/JPEG. Un diálogo nativo exigiría `rfd`, que no está en el stack; la carpeta se configura con un campo de texto y un botón de recarga.

## Decisión

- El panel central pasa a ser una **vista 2D del nivel** con preview real de imágenes y se elimina el diagrama de `graph.rs`. Las texturas se crean con `egui::TextureHandle` a partir de `ImageData::as_rgba()` y `TextureOptions::NEAREST` (pixel art). La GUI no crea dispositivos GPU, no usa `lunar_render` y no expone `wgpu`.
- Se añade un **panel de assets** con la ruta de la carpeta configurada por texto y botón «Recargar». El escaneo es recursivo con `std::fs`, sin dependencias nuevas. Se listan `.png`/`.jpg`/`.jpeg` (con preview y arrastrables al lienzo), `.wav` y `.glb`/`.gltf` (arrastrables, sin preview). Las rutas guardadas en la escena son relativas a la carpeta y usan `/`, conforme a ADR-008.
- **Arrastrar un asset al lienzo** crea una entidad en la posición soltada mediante `CreateEntity`, `AddComponent` (`Image`, `Audio` o `Mesh`) y `SetTransform`, con un ID nuevo de `next_entity_id`. La vista 2D admite selections múltiples con Shift+clic, movimiento por arrastre, zoom con la rueda anclada al puntero, paneo con botón central/derecho, rejilla opcional y snap a 0.25 unidades.
- Los **gizmos** aparecen solo para una selección: tirador de esquina para escala uniforme y tirador superior para rotación. El arrastre muestra una vista previa temporal y al soltar se envía **un único** `SetTransform` por entidad, para no inundar el historial de undo. La rotación 2D se guarda como cuaternión `(0, 0, sin(ángulo/2), cos(ángulo/2))`, compatible con `Transform` de ADR-008.
- La **jerarquía es reordenable por arrastre**: soltar sobre otra entidad emite `SetParent`; soltar entre filas emite el nuevo comando `MoveEntity { id, to }`, que reordena el vector `entities` sin cambiar el esquema. Soltar en el área raíz emite `SetParent { parent: None }`. Se añade también `DuplicateEntity { id, new_id }` con atajo `Ctrl+D`.
- Los comandos nuevos (`MoveEntity`, `DuplicateEntity`) viven en `lunar_editor`, pasan por `EditorDocument::apply` y son validados y deshechos como los demás. El núcleo sigue sin depender de GPU ni de GUI.
- El formato JSON sigue siendo **versión 1**. `Component::Image` no gana campos: el recorte de fotogramas y las animaciones siguen siendo responsabilidad del juego, como en ADR-018. El editor previewa la imagen completa; metadatos de animación requerirían otra ADR.
- La GUI sigue siendo opcional: `editor_ui` implica `editor`, queda fuera de `default` y mantiene `eframe` sin backend `glow`. No se añaden features nuevas ni dependencias al workspace.
- Fuera de alcance: vista 3D, timeline de animación, reproducción de audio, diálogo nativo de carpetas, ejecutar el juego desde el editor y edición de fotogramas.

## Consecuencias

- ADR-019 permanece como decisión del MVP; sus puntos sobre «diagrama sin preview» y «sin arrastre en la jerarquía» quedan sustituidos por esta ADR.
- `crates/lunar_editor_ui` gana módulos de assets y lienzo; `graph.rs` desaparece. `lunar_editor` gana dos comandos con tests sin GPU.
- Las funciones puras de transformación de coordenadas, hit-testing, filtro de extensiones y conversión ángulo/cuaternión llevan tests unitarios; no necesitan ventana ni GPU.
- CI mantiene todos sus gates y además compila los nuevos comandos y tests. Los árboles `default`, `dim2`, `dim3`, `assets`, `scenes` y sin features siguen sin contener `egui`, `eframe` ni `lunar_editor_ui`.
