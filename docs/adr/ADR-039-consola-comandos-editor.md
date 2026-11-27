# ADR-039: Consola de comandos del editor

- Estado: Aceptada
- Fecha: 2026-11-26
- Relacionada: ADR-019/020 (editor visual), ADR-028 (diagnósticos), ADR-026 (documentos)

## Contexto

La consola del editor solo mostraba mensajes de `set_status` con filtros y búsqueda. Para editar sin ratón y para repetir acciones de forma reproducible hacía falta aceptar comandos de texto con historial y autocompletado, sin abrir un intérprete de scripting ni exponer nueva API pública.

## Decisión

- Se añade un módulo puro `console` con tokenizer (comillas simples/dobles), comandos `spawn`, `destroy`, `set`, `get`, `save`, `load` y `help`, y autocompletado por prefijo único. Los errores de parseo son mensajes legibles, no panics.
- `spawn`, `destroy` y `set` se ejecutan exclusivamente mediante `EditorCommand` y `EditorApp::apply`; `get` solo lee el documento. No se permite mutar la escena desde el parser ni desde la capa de dibujo.
- `save`/`load` reutilizan las acciones existentes del editor: guardar en la ruta del documento o en una ruta explícita, y abrir con el aviso de cambios sin guardar. No se crea un formato nuevo.
- El historial es en memoria, con navegación Arriba/Abajo y repetición con Enter; el autocompletado solo actúa sobre el nombre del comando cuando el prefijo es inequívoco. Los comandos y sus resultados se registran en la consola con severidad explícita.
- No se añaden dependencias, hilos, features ni cambios en el esquema v2, la bridge C++ ni la API pública del motor.

## Consecuencias

- Las pruebas son headless: parseo y tokenizer, autocompletado, historial, y ejecución de cada comando contra un `EditorApp` con documento en memoria.
- Los comandos respetan el bloqueo durante Play porque pasan por `ensure_editable` antes de `apply`.
- `get` y `help` son de solo lectura; `save`/`load` siguen siendo las únicas operaciones que tocan ficheros.
