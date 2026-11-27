# ADR-034: GUIDs de assets en el editor

- Estado: Aceptada
- Fecha: 2026-11-26
- Relacionada: ADR-008 (rutas relativas de assets), ADR-020 (content browser), ADR-027 (escenas v2)

## Contexto

Las escenas v2 guardan referencias a assets como rutas relativas (`Component::Sprite`, `Image`, `Audio`, `Mesh`). Mover o renombrar un archivo rompe esas referencias. El plan de Fase 2 pedía GUIDs estables y proponía guardar el GUID dentro de la escena con la ruta como fallback; el usuario ha decidido expresamente **no cambiar la API pública del motor**. Por tanto el esquema de `lunar_scenes`, el JSON v2 y la bridge C++ se mantienen intactos.

## Decisión

- Cada asset soportado recibe un archivo lateral `<asset>.meta` junto al archivo, en texto plano `clave=valor`. La clave obligatoria es `guid=<32 hexadecimales>`. El formato ignora claves desconocidas para poder añadir opciones de importación en E17 sin cambiar el formato.
- El GUID se genera una sola vez, sin dependencias nuevas, y se guarda en el `.meta`. Si el archivo se mueve junto con su `.meta`, conserva su GUID; si no hay `.meta`, se crea uno nuevo. Los archivos `.meta` no se cuentan como assets.
- En la raíz de assets se mantiene `guid_index.txt`, también en texto plano, con líneas `guid=<ruta relativa con />`. Al recargar, el editor:
  1. lee los `.meta` presentes y construye el mapa GUID → ruta actual;
  2. compara con `guid_index.txt` y guarda las rutas anteriores como alias de ese GUID;
  3. reescribe el índice con las rutas actuales.
- Al abrir, recargar assets o guardar una escena, el editor sustituye en memoria las rutas antiguas por las actuales usando esos alias. La reescritura se aplica como un único comando atómico `SetComponents` de `lunar_editor`, de modo que cuenta como un solo paso de undo y toda mutación sigue pasando por `EditorDocument::apply`.
- Una ruta que no aparece ni en el índice ni en los alias se deja tal cual y se reporta como referencia rota en la barra de estado. La detección completa en consola corresponde a E17.
- El content browser muestra el GUID del asset seleccionado para que la estabilidad sea verificable sin leer ficheros.

## Consecuencias

- `lunar_scenes`, el formato de escena y la bridge C++ no cambian: no hay nueva API pública del motor, ni `serde_json`, ni dependencias nuevas en `editor_ui`.
- El motor puede seguir cargando directamente un JSON ya guardado: al guardar, el editor ha reescrito las rutas a su ubicación actual.
- Si se mueve un asset sin su `.meta`, se le asigna un GUID nuevo y la referencia antigua se reporta como rota: mover el lateral es parte del flujo.
- Los GUIDs son locales a una raíz de assets; colisiones entre proyectos distintos quedan fuera de alcance.
- Guardar el GUID dentro de la escena seguiría siendo una decisión que afecta a la API pública y exigiría una autorización explícita y un ADR nuevo que supere este.
