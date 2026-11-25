# ADR-025: Servicios 3D en la bridge C++

- Estado: Aceptada
- Fecha: 2026-11-25
- Supersede parcialmente: la frase de ADR-021 que excluía 3D de la bridge; amplía ADR-022 y ADR-009

## Contexto

`lunar_physics3d` y el cargador glTF de `lunar_assets` están completos desde H5 y H7, pero `lunar_render3d` sigue siendo un marcador vacío y la bridge C++ solo ofrece física 2D y sprites. Un juego 3D en C++ no puede crear cuerpos, cargar una malla ni enviar órdenes de dibujo 3D.

ADR-009 confina `wgpu` al render, ADR-021 prohíbe que `Renderer`, `wgpu`, `ImageData` o Rapier crucen la FFI, y ADR-022 distribuye los headers con versión exacta. Añadir 3D a la bridge no debe arrastrar GUI ni meter `lunar_bridge` en los gates `dim2`, `dim3` o bare.

## Decisión

- `lunar_bridge` depende de `lunar_physics3d`, `lunar_render3d` y de `lunar_assets` con la feature `gltf`, todas de forma incondicional, igual que el resto de servicios de la bridge. No se añaden features nuevas ni dependencias externas.
- `lunar_render3d` deja de ser un marcador y pasa a poseer la superficie CPU de mallas, materiales y órdenes 3D:
  - `MeshId` opaco y `MeshStore` con `define_mesh(vertices, indices, color)` y `from_gltf(document, mesh, primitive)`; los atributos se convierten a vértices `(posición, normal, UV)` y los índices a `u32`, validando accessors, longitud y modo de primitiva.
  - `MeshDraw` POD de Rust con `mesh: u64`, `model: [f32; 16]` (columna mayor) y `color: [f32; 4]`.
  - `Renderer3D` con cámara `view_projection`, `submit(&[MeshDraw]) -> bool` y `begin_frame()`; rechaza handles desconocidos, matrices no finitas y colores no finitos antes de encolar nada.
  - No aparece `wgpu` en este crate: `lunar_render3d` produce órdenes validadas, no una pass GPU. La presentación 3D en pantalla requiere una decisión posterior y no forma parte de este hito.
- La bridge expone servicios 3D nuevos, sin cambiar struct ni método existente:
  - Física: `set_gravity3d`, `add_body3d`, `add_collider3d`, `body_state3d`, `apply_impulse3d`, `cast_ray3d` y `step_physics3d`, con los mismos ids públicos `u64` y flags `valid`/`hit` que la API 2D.
  - Assets y mallas: `load_gltf(asset) -> u64`, `define_mesh(gltf, mesh, primitive) -> u64` y `mesh_vertex_count(mesh)`.
  - Cámara y órdenes: `set_camera3d(eye, target, up, fov, aspect, near, far)`, `view_projection3d() -> [f32;16]`, `submit_mesh_draws(&[MeshDrawCpp]) -> bool`, `mesh_draw_count()` y `mesh_draw(index) -> MeshDrawCpp`.
- `MeshDrawCpp` es un shared struct nuevo con escalares y un array fijo de 16 `f32`; `BodyState3DCpp` y `RayHit3DCpp` usan los mismos patrones POD con flag que sus equivalentes 2D. Ningún tipo de Rapier, glTF ni `GltfDocument` cruza la frontera.
- Las rutas de glTF se resuelven con `safe_asset_path` dentro de `GameConfig.assets_dir`, igual que imágenes y sonidos. `new_engine` mantiene `assets_dir` vacío.
- La cola de dibujo 3D se vacía en `begin_render()`, que ya inicia el frame C++ actual; `Game` no gana nuevos métodos virtuales.

## Consecuencias

- El árbol de `lunar_bridge` pasa a incluir `rapier3d`, `lunar_physics3d`, `lunar_render3d` y `gltf`; el gate de CI se renombra y verifica esos paquetes sin GUI. Los builds `dim2`, `dim3`, bare y 2D/3D-only siguen sin incluir la bridge.
- Los tests son deterministas y no necesitan GPU: física 3D, carga del glTF de `scene_demo`, extracción de mallas, validación de órdenes y cámara.
- El SDK regenera headers con los nuevos símbolos; la plantilla externa comprueba que compilan. ADR-022 mantiene `ExactVersion`: la versión se subirá a `0.2.0` en H20.
- El esquema JSON v1 no cambia: `Component::Mesh` y `Component::Collider3D` siguen siendo metadatos que el juego C++ interpreta.
- La presentación GPU de mallas, materiales con textura y cámara integrada en el bucle quedan fuera de este hito y necesitarán ADR propia antes de implementarse.
