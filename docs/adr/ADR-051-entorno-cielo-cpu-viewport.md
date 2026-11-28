# ADR-051: Entorno y cielo CPU del viewport 3D

- Estado: Aceptada
- Fecha: 2026-11-28
- Relacionada: ADR-032 (vista 3D CPU), ADR-037 (modos de sombreado), ADR-048 (PBR GPU fuera de fase), ADR-050 (luces del editor)

## Contexto

La Fase 3 pide entorno/skybox en el editor. El esquema v2 de `lunar_scenes` no tiene componente de entorno y la bridge C++ usa `ExactVersion`, así que añadirlo al JSON de la escena o a la API pública rompería la compatibilidad. ADR-048 deja fuera de fase las texturas, el PBR GPU y la compilación de shaders; ADR-050 ya recordó que IBL/HDRI y lightmapping siguen sin autorización. El viewport se dibuja con `egui::Painter` y no tiene `z-buffer`, solo la profundidad aproximada por cara de ADR-037.

## Decisión

- El entorno es dato del editor en un sidecar `environment.txt` junto a la escena: modo de cielo (`Sólido` o `Degradado`), colores de cima/medio/base, niebla opcional con color, inicio y fin. Usa escritura atómica, recarga desde disco e historial paralelo como `lights.map.txt` (ADR-050). Cada cambio se empareja con `EditorCommand::EnvironmentEdit` y se restaura con `undo`/`redo`.
- El fondo se pinta con 48 bandas horizontales calculadas por una función pura `sky_color(env, t)` sobre `egui::Painter`. `Sólido` usa el color medio en toda la vista; `Degradado` interpola cima→medio→base. Los botones «Despejado», «Atardecer» y «Noche» son presets de esos tres colores, no texturas.
- La niebla es opcional y solo se aplica al modo `Iluminado`: factor lineal según la profundidad de la cara, mezclando su color con el color de niebla. `Sólido` y `Alámbrico` no la consultan. La profundidad es la del centroide de la cara (ADR-037): es una ayuda de edición, no un render con `z-buffer`.
- El valor por defecto es un degradado oscuro neutro (cima `16,20,30`, medio `28,33,46`, base `10,12,18`) con niebla desactivada: mantiene la vista oscura pero deja ver el horizonte y el efecto del cielo sin editar nada. Sigue siendo dato del editor y no altera la escena.
- No hay dependencias nuevas, ni cambios en el esquema v2, en la ABI C++ `0.2.0` ni en la fachada pública del motor. No se cargan HDRI, cubemaps ni imágenes; el cielo no contribuye a la iluminación de los materiales.

## Consecuencias

- Se puede configurar el fondo y la niebla de cada escena y ver el efecto de inmediato en 3D, con deshacer/rehacer y persistencia junto a la escena.
- El cielo es una aproximación por bandas con 48 pasos y la niebla usa profundidad por cara: sirve para inspeccionar la composición, no para render de juego.
- Skybox con textura, IBL/HDRI, lightmapping y entorno del runtime requieren una ADR nueva con autorización explícita.
