# ADR-019: Interfaz gráfica del editor

- Estado: Aceptada
- Fecha: 2026-11-24

## Contexto

H10 entregó `lunar_editor` como núcleo headless con comandos, selección, undo/redo y persistencia JSON. El usuario pidió pasar a un editor con interfaz gráfica y aprobó explícitamente tres decisiones: usar `egui`/`eframe`, crear un crate `lunar_editor_ui` separado y limitar el primer alcance a jerarquía + inspector.

ADR-001 no incluía toolkits GUI en el stack y ADR-015 exige una ADR nueva antes de añadir uno. Por tanto esta decisión se registra antes de modificar `Cargo.toml` o escribir widgets.

## Decisión

- Se fijan `egui` y `eframe` en **0.37.0**. `eframe` se configura sin features por defecto y con el backend `wgpu` (`default-features = false`, `wgpu`, `default_fonts`, `x11`, `wayland`). Las features `x11`/`wayland` activan las de `winit` a través de `egui-winit`, así que no hace falta una feature `winit/default` directa. No se habilita el backend `glow`/`glutin` de `eframe`; `wgpu` puede arrastrar crates GL de forma transitiva, pero no se usan como backend de la GUI.
- `egui-wgpu` 0.37 depende de `wgpu` 30, la misma serie fijada en ADR-009, así que no aparece un segundo renderer ni una API `wgpu` nueva en el motor.
- El frente vive en **`crates/lunar_editor_ui`**, un crate opcional del workspace. El crate raíz gana la feature `editor_ui`, que implica `editor` y `dep:lunar_editor_ui`. Ni `editor_ui` ni `editor` entran en `default`.
- La GUI no reimplementa operaciones: toda mutación pasa por `EditorDocument::apply` con los comandos existentes de ADR-015 (`CreateEntity`, `DeleteEntity`, `RenameEntity`, `SetParent`, `SetTransform`, `AddComponent`, `RemoveComponent`), y deshacer/rehacer usa el historial del núcleo. La GUI solo envía comandos y muestra el estado.
- El MVP contiene: barra superior con ruta, abrir, guardar, nuevo, deshacer y rehacer; panel izquierdo de jerarquía con selección y borrado; panel derecho de inspector con nombre, transformación, padre y lista de componentes; y un panel central con un diagrama de nodos dibujado con `egui::Painter`. Atajos: `Ctrl+S`, `Ctrl+Z`, `Ctrl+Shift+Z`/`Ctrl+Y` y `Supr`, ignorados mientras un campo de texto tiene el foco.
- No hay texturas, preview de sprites, cámara ni integración con `lunar_render`: el panel central es un diagrama, no una vista de juego. Un futuro preview con sprites requerirá su propia ADR.
- `lunar_platform`, la bridge C++ y los gates 2D/3D/no-render no cambian. Se ejecuta con `cargo run -p lunar_editor_ui [ruta.json]`.

## Consecuencias

- Se añaden `egui` y `eframe` al stack aprobado por el usuario junto con sus dependencias transitivas; no se añade `rfd`, `imgui-rs` ni ningún otro toolkit.
- CI verifica que los árboles `default`, `dim2`, `dim3`, `assets`, `scenes` y sin features **no** contienen `lunar_editor_ui` ni `egui`, y que `--features editor_ui` sí los incluye. `--all-features` compila la GUI en los tres sistemas operativos.
- El núcleo `lunar_editor` sigue siendo testeable sin GPU y no adquiere dependencias nuevas.
- Cualquier extensión más allá de este MVP (file dialog nativo, preview con sprites, arrastrar en la jerarquía) se decide en una ADR nueva.
