# ADR-027: Escenas v2 con sprites, animación y prefabs

- Estado: Aceptada
- Fecha: 2026-11-25
- Supersede parcialmente: el apartado «versión 1» de ADR-008 y la frase «el recorte y las animaciones siguen siendo responsabilidad del juego» de ADR-018/020/021

## Contexto

ADR-008 fijó JSON v1 con `deny_unknown_fields`, y ADR-018/020/021 prohibieron añadir campos de animación a `Component::Image`. Los juegos C++ y el editor han estado recortando fotogramas a mano, sin una forma serializada de describir hojas de sprites, clips ni plantillas de entidades. Añadir esos campos a `Image` rompería la promesa de compatibilidad de esas ADR, y hacerlo como campos opcionales silenciosos contradice la regla de ADR-008 de migraciones explícitas.

## Decisión

- El esquema canónico pasa a **versión 2**: `{ "version": 2, "name": ..., "prefabs": [...], "entities": [...] }`. `SCENE_VERSION = 2` y `SCENE_VERSION_V1 = 1` viven en `lunar_scenes`.
- La versión 1 **sigue siendo legible**, pero solo a través de un convertidor explícito `v1::upgrade`: se descodifica con tipos propios de v1 y se transforma en v2. Nunca se aceptan campos v2 pegados a un documento que diga `"version": 1`. `to_json` siempre emite v2.
- `Component::Image` **no cambia**: sigue siendo `{ "kind": "image", "asset": ... }`. Los metadatos viven en un componente nuevo y aditivo:
  - `Component::Sprite { asset, meta, animations }`, con `SpriteMeta { frame_width, frame_height, columns, rows, fps, looping }` y `AnimationClip { name, frames, fps, looping }`.
  - `Component::PrefabRef { name }`, que referencia una plantilla declarada en la escena.
  - `Prefab { name, entities }` en el array `prefabs` de la escena.
- El convertidor v1→v2 transforma cada `Component::Image` en `Component::Sprite` con una celda única (`columns = rows = 1`, `fps = 0`) y sin clips. El comportamiento visual es idéntico y la migración queda documentada y testeada, no implícita.
- `Scene::validate()` mantiene las reglas de ADR-008 para ids, jerarquía, rutas y volúmenes, y añade: dimensiones de celda positivas, `columns`/`rows` positivos, `fps` finito y no negativo, clips con al menos un fotograma y todos los índices dentro de `columns * rows`, nombres de prefab únicos, referencias resueltas y ausencia de ciclos entre prefabs.
- `Scene::instantiate_prefab(name, next_id)` devuelve entidades con ids locales remapeados a partir de `next_id`, rearmando `parent`. No muta la escena; el editor o el juego decidan dónde insertarlas.
- La bridge C++ sigue sin exponer `Scene`, `Component` ni tipos de animación: `entity()` rellena `image_asset` tanto desde `Image` como desde `Sprite`. No se añaden métodos C++ en este hito; la animación la sigue ejecutando Rust o C++ según el juego.
- No hay dependencias nuevas: `serde`/`serde_json` ya están aprobados y confinados en `lunar_scenes`. `lunar_editor`, el plataformas y los smokes siguen funcionando con documentos v1 migrados o v2 nativos.

## Consecuencias

- ADR-008 queda enmendada: la versión 2 es la canónica y la regla de «migraciones explícitas» se cumple con `v1::upgrade`. ADR-018, ADR-020 y ADR-021 dejan de excluir los metadatos de sprites: ahora residen en `Component::Sprite`, nunca en `Image`.
- Los assets de ejemplo pueden permanecer en v1: se cargan y se reescriben en v2 al guardar. Se añade una escena v2 de ejemplo con prefab y clip para cubrir el camino nativo.
- `deny_unknown_fields` se mantiene en las structs v2; el convertidor v1 ignora campos desconocidos de v1 igual que antes, pero no los propaga.
- CI comprueba el convertidor, la validación de clips/prefabs y que `lunar_scenes` sigue sin arrastrar GUI, GPU ni audio.
