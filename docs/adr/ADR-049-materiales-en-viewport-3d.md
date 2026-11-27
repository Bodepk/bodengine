# ADR-049: Materiales del editor en el viewport 3D y asignación por entidad

- Estado: Aceptada
- Fecha: 2026-11-27
- Supersede parcialmente: el apartado «sin materiales» de ADR-037
- Relacionada: ADR-032 (vista 3D CPU), ADR-037 (modos de sombreado), ADR-047 (materiales editor-owned), ADR-048 (PBR GPU fuera de fase)

## Contexto

ADR-047 creó materiales editor-owned con un color de previsualización CPU determinista y dejó preparado que E48 lo usara como factor de las caras del cubo en el modo `Lit`. ADR-037 describía ese modo con el color de la entidad y una luz direccional fija, sin materiales. La Fase 3 pide además que la asignación de materiales sea visible y cómoda desde la vista 3D, donde hoy solo se aceptan mallas glTF arrastradas.

No se puede pasar a render PBR: ADR-048 lo deja fuera de fase y los gates prohíben `lunar_render`, `wgpu` y `glow` en `editor_ui`. Tampoco conviene tocar el esquema v2 ni la bridge C++: la asignación ya vive en `materials.map.txt` (ADR-047).

## Decisión

- En modo `Lit`, el color base de las caras de una entidad es `Material::preview_color()` del material asignado, si existe; si no hay asignación, se mantiene el color anterior de ADR-037. `Solid` y `Alámbrico` no consultan materiales.
- La luz direccional fija y el suelo de 0.35 de ADR-037 no cambian: el color del material se modula con el mismo `shade_factor`. No hay sombras, texturas, IBL ni presentación GPU.
- La selección deja de teñir el relleno en `Lit`: sigue indicada por aristas gruesas, gizmo, etiqueta y color de las aristas, para que el material asignado sea visible también en la entidad seleccionada.
- La vista 3D muestra en su barra el material de la primera entidad seleccionada («Sin material» si no hay asignación), y acepta arrastrar un `*.material.json` desde el content browser: al soltarlo sobre una entidad se le asigna mediante el mismo `EditorCommand::MaterialEdit` y el mismo historial que el inspector. Soltarlo sobre el vacío solo informa; soltar otras clases de asset mantiene la creación de entidades para mallas y rechaza el resto.
- Las funciones de color, picking y aplicación del arrastre siguen siendo puras sobre `egui::Painter`, `f32` y datos del editor. No hay dependencias nuevas, ni cambios en la fachada `lunar`, en la ABI C++ `0.2.0` ni en el esquema v2.

## Consecuencias

- El efecto de asignar o editar un material se ve de inmediato en `Lit` y es deshacible junto al resto del historial del editor.
- El color sigue siendo una aproximación CPU por cara, con artefactos de profundidad conocidos (ADR-032/037): es una ayuda de edición, no un render de juego.
- Luces reales, entorno/skybox y materiales del runtime requieren decisiones posteriores; esta ADR no las habilita.
