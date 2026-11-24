# ADR-008: Formato de escenas

- Estado: Aceptada
- Fecha: 2026-11-24

## Contexto

H9 necesita guardar y cargar escenas (entidades, componentes y referencias a assets). `serde`/`serde_json` están aprobados en ADR-001. El esquema concreto afecta a la estabilidad de la API pública y al editor futuro de H10, así que debe fijarse antes de escribir el primer `derive`.

## Decisión

- El formato canónico es **JSON UTF-8 legible**, con un objeto raíz `{ "version": 1, "name": ..., "entities": [...] }`. No hay formato binario propio en H9.
- `serde` = 1.0.229 (feature `derive`) y `serde_json` = 1.0.154 quedan fijados en H9 y confinados al crate **`lunar_scenes`**, expuesto por la feature `scenes` del crate raíz (`default` la activa).
- Las referencias a assets son **rutas relativas al archivo de escena**, con `/` como separador. Nunca se serializan handles numéricos ni UUIDs: los stores de H6/H7/H8 siguen entregando handles opacos locales. El resolvedor de rutas se decide en el hito que consuma las escenas.
- El esquema lleva `version: u32`. Solo se acepta `1`; cualquier otra versión se rechaza con un error propio antes de intentar migrar. Las migraciones futuras se añadirán como convertidores explícitos, nunca como campos opcionales silenciosos.
- Cada entidad tiene `id: u64`, `name`, `parent`, `transform` y una lista de componentes etiquetados por `kind`. Los IDs deben ser únicos; `parent` debe existir y no puede formar ciclos. Estas reglas se validan al cargar.
- Los componentes serializables son: `Mesh` (ruta glTF y primitiva opcional), `Image`, `Audio` (ruta WAV, volumen y `autoplay`), `Collider2D` y `Collider3D` (geometría y tipo de cuerpo en datos POD, sin tipos de Rapier).
- Se excluyen del formato: entidades transitorias de runtime, cámaras de editor, selección, gizmos y cualquier estado de UI. El editor de H10 no podrá inventar campos nuevos sin actualizar este ADR.
- `lunar_scenes` no depende de `winit`, `wgpu`, `rapier`, `rodio`, `image`, `gltf` ni `cxx`. Define solo datos y validación; la aplicación decide cómo materializarlos.

## Consecuencias

- Los builds `dim2`, `dim3` y `--no-default-features` no arrastran `serde_json` por escenas; se comprueba en CI con `cargo tree -e features`.
- `lunar_scenes` es consumido por el editor de H10 sin redefinir el esquema.
- Los errores de escena son propios y no exponen `serde_json` ni `std::io`.
- Cambiar el esquema obliga a subir `version` y a documentar la migración aquí.
