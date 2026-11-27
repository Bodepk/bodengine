# ADR-047: Materiales PBR editor-owned y previsualización CPU

- Estado: Aceptada
- Fecha: 2026-11-27
- Relacionada: ADR-019/020 (GUI), ADR-025/032/037 (vista 3D CPU), ADR-034 (GUIDs), ADR-044 (preferencias), ADR-048 (fuera de fase)

## Contexto

La Fase 3 pide editar parámetros PBR, previsualizarlos y guardar materiales como assets. Hoy no existe `Material` en el motor: `lunar_assets::MaterialData` guarda factores PBR del glTF pero `lunar_render3d` solo lee `base_color_factor`, `lunar_render` tiene dos shaders WGSL hardcodeados sin profundidad, y el viewport 3D del editor es una ayuda CPU con `egui::Painter` (ADR-032/037). Añadir PBR real exigiría pipeline 3D con z-buffer, texturas y shaders, además de cambiar el gate de CI que prohíbe `lunar_render` en el editor (ADR-032:32) y la API pública: una decisión que ADR-025 y ADR-037 dejan para otra ADR con autorización explícita.

## Decisión

- Los materiales de la Fase 3 son **datos del editor**, no del runtime: archivos `.material.json` en la raíz de assets con GUID estable mediante el `.meta` de ADR-034. El esquema v2 de `lunar_scenes` y la bridge C++ no cambian.
- `Material` guarda `albedo: [f32;4]`, `metallic`, `roughness`, `occlusion` (los tres clampeados a `[0,1]`), `emissive: [f32;3]`, `normal: Option<String>` y `shader: Option<String>` como **referencia** a otros assets. Codificación y decodificación son puras y tolerantes: campos ausentes o no finitos caen al default por campo.
- La asignación material↔entidad vive en un sidecar `materials.map.txt` junto a la escena, con el mismo patrón de escritura atómica e historial paralelo que `scripts.map.txt` (ADR-040). Toda edición se empareja con un comando de `lunar_editor` y se restaura con `undo`/`redo` junto al resto de sidecars.
- El content browser clasifica `.material.json` como `AssetKind::Material` y permite crearlo, selector y arrastrarlo. El inspector ofrece color pickers y sliders y dibuja una **previsualización cuadrada de 64×64 px** con `egui::Painter`: color = `albedo.rgb * (0.55 + 0.45 * (1.0 - roughness)) * occlusion + emissive`, con alfa de `albedo`; es una aproximación CPU determinista, no PBR.
- En E48 el mismo color se usa como factor de las caras del cubo en la vista `Lit`, conservando la luz direccional fija de ADR-037 hasta E49.
- No se añaden dependencias, ni dispositivos GPU, ni tipos `wgpu` en el editor. `lunar_render`/`glow`/`rfd` siguen prohibidos por el gate de CI.

## Consecuencias

- Se puede editar, asignar, guardar, cargar y deshacer materiales sin tocar la API pública del motor ni romper la ABI C++ `0.2.0`.
- La previsualización es coherente y estable, pero **no es render PBR**: no hay texturas, normal maps, sombras ni presentación GPU. La UI lo indica así.
- Cuando el motor gane materiales reales, hará falta una ADR de migración explícita (ADR-027) desde estos ficheros al esquema de escena; no se hará de forma silenciosa.
