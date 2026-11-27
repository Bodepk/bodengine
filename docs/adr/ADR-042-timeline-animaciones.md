# ADR-042: Timeline de animaciones en el editor

- Estado: Aceptada
- Fecha: 2026-11-27
- Relacionada: ADR-027 (escenas v2 con clips), ADR-030 (inspector de componentes), ADR-019/020 (GUI opcional)

## Contexto

El esquema v2 serializa `AnimationClip { name, frames, fps, looping }` dentro de `Component::Sprite` y el inspector permite editar clips y fotogramas, pero no hay forma de reproducirlos ni de ver qué celda corresponde a un instante. ADR-020 excluyó la timeline del MVP y ADR-030 constata que sigue sin existir. Añadirla no debe convertir al editor en runtime ni tocar el esquema.

## Decisión

- Se añade un módulo puro `timeline.rs` que, dado un clip y un tiempo, calcula la posición y el fotograma de la hoja, sanea `fps`/tiempos no finitos y aplica `looping` o truncamiento. No depende de `wgpu`, `lunar_render` ni de relojes propios.
- `EditorApp` guarda un estado de reproducción en memoria (`clip`, `elapsed`, `playing`), avanzado con el `stable_dt` que ya recibe `eframe::App::ui`. No se persiste en `settings.txt` ni entra en la escena: es una ayuda de inspección, como la cámara 3D.
- La ventana «Timeline de animación» se abre desde el inspector cuando la entidad seleccionada tiene un `Sprite` con clips. Muestra selector de clip, reproducción/pausa, reinicio, barra de tiempo y una rejilla de celdas con el fotograma actual resaltado. No muta la escena: todos los controles cambian solo el estado de la timeline.
- Los clips con índices fuera de `SpriteMeta::frame_count()` se marcan como inválidos en la ventana, pero no se corrigen automáticamente; su edición sigue pasando por el inspector y `EditorCommand::SetComponent`.
- No hay dependencias nuevas, cambios de esquema v2 ni cambios en la API pública de los crates del motor. Los tests son headless.

## Consecuencias

- Se puede comprobar visualmente la secuencia de fotogramas de un sprite sin abrir el juego ni iniciar el Play de física.
- La reproducción usa el reloj de la GUI, no el bucle de 60 Hz: es una vista previa y no garantiza sincronía con el runtime de C++.
- La reproducción de vídeo/audio, el scrub con arrastre sobre la rejilla y la edición de clips desde la timeline quedan fuera de alcance.
