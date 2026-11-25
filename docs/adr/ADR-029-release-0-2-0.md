# ADR-029: Publicación de `v0.2.0`

- Estado: Aceptada
- Fecha: 2026-11-25
- Supersede parcialmente: la versión `0.1.0` fijada en ADR-022 y la nota «hasta H20» de ADR-023/025/026

## Contexto

H14–H19 añadieron GUI opcional, sprite platformer, API C++ con bucle propio, SDK instalable, input de ratón, audio, servicios 3D, documentos de escena, escenas v2 y diagnósticos con replay. ADR-022 fijó `ExactVersion = 0.1.0` precisamente para impedir que un consumidor empareje headers nuevos con una staticlib antigua. Llegados a H20 es necesario publicar una versión nueva que represente la ABI ampliada y mantener la regla estricta.

## Decisión

- La versión del workspace y de todos los crates pasa a **`0.2.0`**. El tag de Git y el release de GitHub se denominan **`v0.2.0`**.
- `sdk/cpp/CMakeLists.txt` mantiene `COMPATIBILITY ExactVersion`; la plantilla externa y `sdk/cpp/README.md` pasan a `find_package(lunar_bridge 0.2.0 EXACT CONFIG REQUIRED)`. No se relaja la compatibilidad exacta.
- `CHANGELOG.md` transforma la sección `[No publicado]` en `## [0.2.0] — 2026-11-25`, conserva `0.1.0` como versión anterior con su enlace y añade el enlace de comparación `v0.1.0...v0.2.0`. `README.md` deja de anunciar la demo como `v0.1.0` y pasa a `v0.2.0`.
- La release incluye el SDK regenerado con los símbolos de H14–H19, la plantilla externa y los smokes C++ en verde en los tres sistemas operativos. El tag se crea sobre el commit de implementación de H20, nunca sobre el commit de la ADR.
- No se rompe deliberadamente ninguna firma C++ existente: `FrameStats`, `Game`, los handles opacos y los structs POD anteriores siguen presentes. La subida de versión es una señal de ABI ampliada, no de incompatible changes.

## Consecuencias

- Los consumidores con `find_package(lunar_bridge 0.1.0 EXACT ...)` dejan de resolver: deben actualizar la versión solicitada y reinstalar el SDK `0.2.0`. Esto evita mezclar la staticlib nueva con headers viejos.
- CI verifica que no queden referencias a `0.1.0` en manifiestos del workspace, SDK, plantilla ni documentación de versión vigente, salvo el histórico de `CHANGELOG.md` y las ADR que lo mencionan como pasado.
- `v0.1.0` permanece disponible para comparación y no se reescribe.
