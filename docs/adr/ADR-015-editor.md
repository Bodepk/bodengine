# ADR-015: Núcleo de editor H10

- Estado: Aceptada
- Fecha: 2026-11-24

## Contexto

ADR-007 reservó el editor como crate opcional y supeditó H10 a aprobación explícita; el usuario aprobó todos los hitos restantes. No hay dependencias GUI aprobadas y el esquema de escenas ya está fijado en ADR-008. Un editor con ventana own-way añadiría toolkit, input y presentación antes de tiempo.

## Decisión

- H10 entrega **`lunar_editor`**, un núcleo de edición headless: documento, selección, comandos, historial undo/redo y persistencia reutilizando `lunar_scenes`. No abre ventanas, no usa GPU y no define widgets.
- El crate se expone con la feature `editor` del crate raíz y **no entra en `default`**, cumpliendo ADR-007. Se comprueba en CI que `dim2`, `dim3`, `scenes` y el build sin features no lo arrastran.
- La única dependencia nueva del crate es `lunar_scenes`; no añade `serde`, `winit`, `wgpu`, `rapier`, `rodio`, `image`, `gltf` ni `cxx`.
- `EditorDocument` posee un `Scene`, una selección efímera de IDs y un flag `dirty`. La selección nunca se serializa: ADR-008 la excluye del esquema.
- Toda mutación pasa por `EditorCommand` (`CreateEntity`, `DeleteEntity`, `RenameEntity`, `SetParent`, `SetTransform`, `AddComponent`, `RemoveComponent`). `apply` valida el resultado con `Scene::validate`, descarta selection de entidades inexistentes y guarda una entrada de historial con `Scene` antes/después.
- Borrar una entidad rearma a sus hijos con el padre de la entidad borrada; `undo` restaura la entidad y los padres anteriores. No se permiten comandos que dejen el documento inválido: se rechazan sin historial ni cambios.
- `undo`/`redo` restauran también la selección, así que el historial es determinista y testeable sin GUI. `load` limpia el historial; `save` limpia `dirty`.
- La bridge C++ no gana API de editor en H10: el consumidor previsto es el propio editor/Rust y no hay aún comandos C++ que deban congelarse.

## Consecuencias

- Los builds por defecto y parciales no pagan el tamaño del editor; `--features editor` lo incluye.
- Los tests del editor se ejecutan sin dispositivos ni ventanas, igual que los de escenas.
- Un futuro frente GUI podrá consumir este documento sin redefinir comandos ni esquema; cualquier toolkit nuevo exigirá un ADR.
