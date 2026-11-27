# ADR-041: Snap de rotación y escala en el editor

- Estado: Aceptada
- Fecha: 2026-11-27
- Relacionada: ADR-014 (W/E/R y gizmos de grupo), ADR-030 (inspector), ADR-031 (preferencias)

## Contexto

El editor ya tiene un interruptor `Snap 0.25` persistido en `LunarEditor/settings.txt` que cuantiza la traslación del lienzo 2D y del gizmo 3D. La rotación y la escala, tanto con gizmo como desde el inspector, siguen siendo continuas. Eso dificulta alinear ángulos y tamaños con las rejillas y los valores declarativos usados por los comportamientos de ADR-033/040.

## Decisión

- El mismo interruptor `snap` pasa a cuantizar también la **rotación resultante a 15°** y la **escala a 0.25**. La etiqueta de la casilla pasa a `Snap 0.25 / 15°`; no se añade una preferencia nueva ni cambia el formato de `settings.txt`.
- Las tres reglas viven en `helpers.rs` como funciones puras: `snap_to_grid` (0.25, ya existente), `snap_degrees`/`snap_angle` (15°) y `snap_scale` (0.25 con mínimo 0.25 para no producir escalas degeneradas). Los valores no finitos se sanean.
- `rotate_group` y `scale_group` reciben el flag `snap` y cuantizan la rotación absoluta y la escala de cada entidad del grupo. `translate_group` conserva su comportamiento actual. El gizmo 2D y el inspector pasan el flag; en 3D solo existe el gizmo de traslación, así que rotación y escala se cuantizan desde el inspector.
- Al editar con snap activo, el inspector cuantiza sus buffers antes de enviar el comando, de modo que el número escrito y el guardado coinciden. Cada cambio sigue enviando un único `SetTransform`/`SetTransforms` a través de `EditorApp::apply`, sin añadir entradas al historial.
- No hay dependencias nuevas ni cambios de esquema v2, de la API pública de los crates ni de la bridge C++. Los tests son headless y comprueban los tres pasos, el grupo y el flujo del inspector.

## Consecuencias

- Al activar snap, arrastrar rotación o escala produce saltos de 15° y 0.25; al desactivarlo se recupera el comportamiento continuo anterior.
- La cuantización afecta a la entidad, no a los componentes: las mitades de colisionadores y los parámetros de sprites siguen editándose con su paso propio.
- El valor mínimo de escala con snap es 0.25; para escalas menores hay que desactivar snap.
