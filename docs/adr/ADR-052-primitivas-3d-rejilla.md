# ADR-052: Primitivas 3D de prueba y rejilla finita del suelo

- Estado: Aceptada
- Fecha: 2026-11-28
- Relacionada: ADR-031 (preferencias del editor), ADR-032 (vista 3D CPU), ADR-047 (materiales), ADR-050 (luces), ADR-051 (entorno)

## Contexto

Para probar luces, materiales y el modo `Iluminado` hacen falta volúmenes colocados en la escena. El viewport CPU de Fase 3 dibuja cajas alámbricas y caras de caja; no carga mallas arbitrarias ni tiene `z-buffer`. Añadir primitivas al esquema v2 o a la ABI `0.2.0` rompería `ExactVersion` y quedaría fuera de fase según ADR-048.

La rejilla del suelo ya era finita (20×20 unidades), pero no se podía ocultar y su borde no se distinguía del resto de líneas, lo que daba la impresión de un plano infinito.

## Decisión

- Las primitivas son **plantillas del editor**, no assets nuevos ni componentes nuevos: `Cubo 1×1`, `Plano 8×8`, `Muro 4×3`, `Columna 0.5×3` y `Rampa inclinada`. Se crean con `EditorCommand::CreateEntity` y una `EditorCommand::SetTransform` inmediata; el resto del estado sigue siendo escena v2 normal.
- Cada plantilla fija una transformación inicial legible (apoyada en el suelo, con escala y rotación razonables) y un nombre propio. La rampa usa una rotación de unos 20° alrededor de X. No se añaden colisionadores, mallas ni materiales automáticos: eso corresponde a E51/E52 y a la asignación manual de ADR-049.
- El menú «+ Crear» de la jerarquía agrupa las primitivas en el submenú **Primitivas 3D**; el resto de plantillas siguen en la lista principal.
- La rejilla 3D se puede ocultar con la casilla **Rejilla** de la barra del viewport. El estado se persiste como `show_grid3d` en `settings.txt` (ADR-031) y por defecto está activo. El borde del cuadrado usa un trazo más claro para que su extensión finita sea visible.
- No hay dependencias nuevas, ni cambios en el esquema v2, en la ABI C++ `0.2.0` ni en la API pública del motor.

## Consecuencias

- Se pueden montar escenas de prueba de iluminación y materiales en segundos, sin salir del editor ni crear ficheros de malla.
- Las primitivas son cajas: no sustituyen a un pipeline de mallas, esqueletos o PBR GPU, que siguen fuera de Fase 3 (ADR-048).
- Ocultar la rejilla no afecta a la escena ni al guardado; solo cambia la preferencia del editor.
