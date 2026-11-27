# ADR-037: Modos de sombreado del viewport 3D

- Estado: Aceptada
- Fecha: 2026-11-27
- Supersede parcialmente: el apartado «sin relleno, sin texturas y sin materiales» de ADR-032
- Relacionada: ADR-032 (vista 3D CPU), ADR-019/ADR-020 (GUI opcional, sin `lunar_render`)

## Contexto

La vista 3D de ADR-032 dibuja solo cajas alámbricas con `egui::Painter`, sin relleno ni ordenación por profundidad. Eso basta para inspeccionar jerarquía y trasladar entidades, pero no permite distinguir caras, entender la orientación de una malla ni apreciar la forma de un `Collider3D` cuando varias cajas se solapan. El siguiente hito del editor debe añadir modos de sombreado sin convertir el editor en un render de juego.

No se puede usar `lunar_render` ni `wgpu`: el gate del árbol `editor_ui` los prohíbe y ADR-001 exige autorización para dependencias nuevas. Tampoco conviene tocar `lunar_render3d` ni la API pública de los crates del motor.

## Decisión

- Se añade `ShadingMode { Wireframe, Solid, Lit }` en `view3d.rs`, con selector en la barra de la vista 3D. El valor inicial sigue siendo `Wireframe`, así que el comportamiento previo no cambia. El modo no se persiste en `LunarEditor/settings.txt` (misma regla que la cámara en ADR-032).
- Las caras de la caja unitaria se derivan de las ocho esquinas existentes. Cada cara se proyecta con la cámara de ADR-032; si algún vértice queda detrás del plano cercano, la cara se descarta. Las caras visibles se ordenan por profundidad media (de lejos a cerca) antes de pintarse con `Shape::convex_polygon` del mismo `Painter`.
- `Solid` rellena con el color de la entidad. `Lit` modula ese color con un producto escalar contra una luz direccional fija y un suelo de 0.35, sin sombras, texturas ni materiales. `Wireframe` mantiene solo las aristas. En los tres modos se conservan aristas finas, etiquetas, rejilla, ejes, gizmo y selección.
- La geometría, la normal por cara, el factor de luz, la profundidad y el orden son funciones puras con `f32` y arrays; ninguna crea dispositivo, superficie ni hilo. Los tests son headless.
- No hay dependencias nuevas (`eframe`, `egui`, `lunar_editor`, `lunar_scenes`, `lunar_assets` siguen igual) ni cambios de esquema v2, de la fachada `lunar` ni de la bridge C++. Los gates de `cargo tree` quedan intactos.

## Consecuencias

- El editor puede inspeccionar orientación y solapamiento de mallas/colisionadores 3D en CPU, con un coste acotado a seis polígonos por entidad.
- La ordenación es por cara y no por píxel: en geometría que se interpenetra pueden aparecer artefactos. Esto es una ayuda de edición, no un render de juego (ADR-032).
- La persistencia del modo, los materiales reales y la presentación GPU siguen fuera de alcance y exigirían una decisión nueva.
