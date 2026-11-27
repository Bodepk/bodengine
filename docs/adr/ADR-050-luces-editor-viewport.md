# ADR-050: Luces del editor en el viewport 3D

- Estado: Aceptada
- Fecha: 2026-11-27
- Supersede parcialmente: la luz direccional fija de ADR-037/049
- Relacionada: ADR-032 (vista 3D CPU), ADR-037 (modos de sombreado), ADR-045 (historial), ADR-047 (materiales), ADR-049 (materiales en el viewport), ADR-048 (PBR GPU fuera de fase)

## Contexto

ADR-049 dejó el color de los materiales en el modo `Lit` modulado por una luz direccional fija «hasta E49». El esquema v2 de `lunar_scenes` no tiene componentes de luz y la bridge C++ es `ExactVersion`, así que añadir `Light` al JSON de escena o a la fachada pública rompería la compatibilidad. Tampoco se puede usar `lunar_render`, sombras, IBL/HDRI ni texturas: ADR-048 los deja fuera de Fase 3 y los gates prohíben esas dependencias en `editor_ui`.

## Decisión

- Las luces son datos del editor, no del runtime: un sidecar `lights.map.txt` junto a la escena guarda, por id de entidad, tipo (`Direccional` o `Puntual`), color RGB, intensidad, dirección y alcance. Usa la misma escritura atómica y el mismo historial paralelo que `materials.map.txt` y `scripts.map.txt`.
- La plantilla «Luz 3D (marcador)» crea una entidad marcadora; el inspector permite darle parámetros, quitarlos y editarlos. Cada cambio se empareja con `EditorCommand::LightEdit` y se restaura con `undo`/`redo` junto al resto de sidecars.
- En `Lit`, el sombreado de cada cara suma la respuesta de todas las luces del editor: suelo ambiental de 0.35 (ADR-037), producto escalar de la normal con la dirección hacia la luz, atenuación lineal para luces puntuales y color de la luz. El resultado multiplica el color base del material o de la entidad y se acota a `[0,1]`. Varias luces se acumulan; no hay sombras, texturas, normal maps ni ordenación por píxel.
- Si la escena no tiene luces definidas, se mantiene la luz direccional fija y blanca de ADR-037 como fallback, de modo que las escenas existentes no cambian de aspecto.
- Ni `Solid` ni `Alámbrico` consultan luces; la cruz de una entidad con luz se pinta con el color de esa luz para poder localizarla.
- No hay dependencias nuevas, ni cambios en el esquema v2, en la ABI C++ `0.2.0` ni en la API pública del motor. Las funciones de respuesta lumínica son puras y se prueban sin GPU.

## Consecuencias

- Se pueden crear, configurar y deshacer luces de edición y ver su efecto inmediato sobre los materiales del viewport.
- El resultado es una aproximación por cara con artefactos de profundidad conocidos (ADR-032/037): sirve para inspeccionar, no para render de juego.
- Sombras, IBL/HDRI, lightmapping y materiales PBR reales siguen fuera de fase y exigirían una decisión nueva con autorización explícita.
