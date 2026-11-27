# ADR-035: Prefabs en el editor

- Estado: Aceptada
- Fecha: 2026-11-26
- Relacionada: ADR-027 (escenas v2 con `prefabs` y `Component::PrefabRef`), ADR-015 (núcleo headless)

## Contexto

La Fase 2 del editor necesita guardar una selección (con su jerarquía) como plantilla reutilizable e instanciarla varias veces, con undo/redo y sin tocar la API pública del motor. El esquema v2 de `lunar_scenes` ya define `Prefab { name, entities }`, el campo `Scene::prefabs` y `Component::PrefabRef`; también valida nombres duplicados, ids duplicados, padres desconocidos y ciclos entre prefabs. `Scene::instantiate_prefab` ya remapea ids al instanciar. Falta únicamente la capa de comandos del editor y el flujo de GUI.

No se puede añadir una biblioteca externa de prefabs ni cambiar el JSON sin romper ADR-008/ADR-027, y cualquier cambio en la bridge C++ exigiría una decisión nueva. La opción recomendada y aprobada en el plan de Fase 2 es reutilizar el formato existente.

## Decisión

- El formato de prefab no cambia: una plantilla es `Prefab { name, entities }` dentro de `Scene::prefabs` y se serializa con la escena. No hay ficheros separados ni GUIDs nuevos.
- `lunar_editor` añade dos comandos atómicos, validados por `EditorDocument::apply` como todos:
  - `SavePrefab { name, entities }`: guarda una copia de la selección. Los padres fuera de la selección ya se han convertido en raíces en la capa de GUI (`selection_entities`, E13). Un nombre vacío o duplicado y un id repetido dentro de la plantilla hacen fallar el comando sin mutar la escena.
  - `DeletePrefab { name }`: borra la plantilla y limpia todos los `PrefabRef { name }` de entidades de la escena y de otras plantillas, para que las instancias ya creadas sigan siendo válidas. El número de referencias limpiadas se informa en la barra de estado.
- Instanciar reutiliza `Scene::instantiate_prefab(name, next_id)`: la GUI calcula ids libres, separa cada instancia 2 unidades en X y añade un `PrefabRef { name }` a las entidades raíz como procedencia (sin sobrescribir un `PrefabRef` ya presente). La inserción se hace con `PasteEntities`, que ya es un único paso de undo y selecciona las entidades nuevas. No se añade un tercer comando.
- Los prefabs anidados se conservan: un `Prefab` puede contener entidades con `PrefabRef` a otro prefab; los ciclos siguen siendo rechazados por la validación existente de `Scene::validate`.
- Borrar una plantilla convierte sus instancias en entidades normales (copia instantánea), no las elimina ni las repara silenciosamente aparte de quitar el `PrefabRef`. El borrado es deshechable con `Ctrl+Z`.
- La GUI expone en la jerarquía un campo de nombre y un botón «Guardar prefab», un contador de instancias (1–10) y la lista de plantillas en el menú «+ Crear». No se añaden paneles nuevos.

## Consecuencias

- `lunar_scenes` y la bridge C++ no cambian: no hay nueva API pública del motor ni dependencias nuevas. El gate de `editor_ui` sigue prohibiendo `lunar_render`, `glow` y `rfd`.
- Toda mutación pasa por `EditorDocument::apply`, de modo que guardar, instanciar y borrar plantillas dejan exactamente una entrada de undo/redo.
- Los tests cubren: comandos atómicos y su undo, rechazo de nombres duplicados/desconocidos, limpieza de referencias al borrar, remapeo de ids y jerarquía al instanciar, separación determinista entre instancias y bloqueo durante Play.
- Las instancias no quedan vinculadas a futuras ediciones de la plantilla: «aplicar cambios del prefab a las instancias» queda fuera de E15 y necesitaría una decisión aparte.
