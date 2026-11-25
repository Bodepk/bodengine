# ADR-028: Diagnósticos, replay de input y hashes de estado

- Estado: Aceptada
- Fecha: 2026-11-25
- Supersede parcialmente: el `FrameStats` de ADR-021, que solo exponía substeps, alpha y contadores

## Contexto

H19 necesita observar el rendimiento, reproducir una sesión y comparar estados sin abrir ventanas ni depender de relojes reales. La bridge ya ofrece `FrameStats` con el resultado del paso fijo, pero no mide coste de CPU, no conserva el input por frames y no ofrece una huella comparable del estado. Los juegos C++ y el plataformas headless necesitan un mecanismo común y determinista.

## Decisiones

- No se añaden dependencias. `lunar_core` gana `Engine::reset_timing()` para reiniciar el reloj de paso fijo y los contadores; el resto de la instrumentación vive en `lunar_bridge`, sin tocar el bucle de `lunar_platform`.
- `FrameMetricsCpp` es aditivo y no sustituye a `FrameStats`: añade `cpu_ns`, `render_ns`, `draws_2d`, `draws_3d`, `invalid_draws`, `dropped_ns` y los contadores ya existentes. `cpu_ns` se mide alrededor del `update` con `Instant`, nunca para gobernar la simulación; `render_ns` rodea la preparación y presentación de órdenes en Rust.
- Logging estructurado: `LogEntryCpp { seq, level, code, a, b }` en un buffer circular de 256 entradas dentro de Rust. `log_event(level, code, a, b)` lo alimenta desde C++; `log_count` y `log_entry` lo leen. Niveles `0=error`, `1=warn`, `2=info`, `3=debug`, `4=trace`; códigos `1=render_invalid`, `2=replay_start`, `3=replay_end`, `4=game_error`. No hay `log`, `tracing` ni macros que escriban en la consola en nombre del juego.
- Replay de input: `ReplayFrameCpp` es POD con `elapsed_ns`, `keys: u32`, `mouse: u32`, cursor y scroll. La bridge registra automáticamente cada frame de reloj manual o sistema mientras `begin_replay_recording()` está activo; `end_replay_recording()` devuelve el número de frames y `recorded_frame(index)` los expone. Los bits de teclado siguen los códigos existentes (`bit = code - 1`) y los de ratón los botones `1..=5` (`bit = button - 1`).
- `load_replay_frames(frames)` permite reconstruir un replay externo desde C++ (por ejemplo leído de disco o copiado de otra engine); valida que cursor y scroll sean finitos y no puede usarse durante una reproducción. `start_replay_playback()` reinicia el reloj manual, los contadores del motor y el índice de reproducción; cada llamada posterior a `tick()` ignora su argumento y usa el `elapsed_ns` y el input del frame grabado. Es responsabilidad del juego grabar y reproducir desde estados equivalentes: la bridge no guarda física, assets ni escenas en el replay. `stop_replay_playback()` devuelve a C++ el control del reloj.
- Hash de estado: `state_hash()` combina FNV-1a 64 de contadores, input, física 2D/3D (ids públicos ordenados), deltas de frame y el JSON canónico de la escena activa, sin `HashMap` sin ordenar. Es una huella de determinismo, no un sistema de guardado: no cruza `World`, Rapier, `Scene`, `serde_json` ni `Renderer` por la FFI.
- `FrameStats` y todas las firmas existentes siguen intactas. El buffer de log y el replay están acotados y no asignan memoria desde hilos: solo se escriben desde el hilo del bucle que ya posee la bridge.

## Consecuencias

- CI comprueba los símbolos nuevos en el header generado, ejecuta smokes C++ con grabación/reproducción y verifica que dos ejecuciones del mismo replay desde motores equivalentes producen el mismo `state_hash`.
- El coste de `state_hash()` es proporcional al número de cuerpos y al tamaño de la escena; C++ decide cuándo calcularlo. No se calcula automáticamente cada frame.
- `editor_ui`, `egui`, `wgpu`, `rodio` y Rapier siguen sin aparecer en los tipos POD ni en los method de la bridge más allá de los handles opacos ya aprobados.
