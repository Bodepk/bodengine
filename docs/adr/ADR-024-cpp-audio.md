# ADR-024: Audio desde C++

- Estado: Aceptada
- Fecha: 2026-11-25
- Supersede parcialmente: la frase de ADR-014 que excluía audio de la bridge C++; amplía ADR-021 y ADR-022

## Contexto

`lunar_audio` ya carga WAV, controla volumen por reproducción y volumen maestro con `rodio` confinado en Rust, pero la bridge C++ no puede usarlo: `examples/scene_demo` hace todo desde Rust y ADR-021/022 dejaron audio fuera de alcance. Para que un juego C++ pueda reproducir sonidos propios sin abrir dispositivos en tests ni exponer tipos de `rodio`, hace falta una superficie pequeña y determinista.

La API existente usa `0` como handle inválido, `false` como operación fallida y rutas relativas validadas por `safe_asset_path`. `AudioPlayer::play` abre un dispositivo de forma perezosa, así que en CI headless puede devolver `NoDevice`; las pruebas no deben depender de que exista hardware de audio.

## Decisión

- `lunar_bridge` depende de `lunar_audio` de forma incondicional, igual que de `lunar_assets` o `lunar_physics2d`. No se añaden features ni dependencias externas: `rodio` sigue dentro de `lunar_audio`.
- `Engine` posee un `AudioStore` y un `AudioPlayer` privados, con mapas `u64 ↔ SoundHandle` y `u64 ↔ AudioHandle`. Los identificadores públicos empiezan en `1`; `0` significa error o handle desconocido. `SoundData`, `AudioHandle` de `rodio` y cualquier tipo de `cpal` no cruzan la FFI.
- La API C++ añade métodos nuevos, sin cambiar structs existentes:
  - `load_sound(asset: &str) -> u64`: valida la ruta con `safe_asset_path` y devuelve `0` ante Io, Decode, Empty u overflow.
  - `play_sound(sound: u64) -> u64`: devuelve `0` si el handle de sonido es desconocido o si no hay dispositivo; en success devuelve un handle de reproducción.
  - `stop_sound(play: u64) -> bool` y `stop_all_sounds()`: detienen una reproducción o todas.
  - `set_volume(play: u64, volume: f32) -> bool`: aplica `clamp_volume` en Rust y devuelve `false` si el handle no existe.
  - `set_master_volume(volume: f32)` y `master_volume() -> f32`: exponen el volumen global usado por `scene_demo`.
  - `is_sound_playing(play: u64) -> bool`: consulta sin abrir dispositivo.
- Las rutas se resuelven dentro de `GameConfig.assets_dir`, con la misma política de ADR-021: absolutos, `..` y unidades de Windows se rechazan antes de tocar el filesystem. `new_engine` mantiene `assets_dir` vacío, así que el smoke de reloj no puede cargar sonidos; el juego C++ usa `new_game_engine`.
- Los componentes `Audio` de una escena siguen siendo metadatos gestionados por Rust; C++ solicita la reproducción pasando la ruta del asset o un handle previamente cargado. `EntityInfo` no gana campos en este hito, para no romper la ABI de ADR-023.
- La ausencia de dispositivo es un resultado normal en headless y no un error fatal: `play_sound` devuelve `0` y el juego puede seguir ejecutándose. La detención y el volumen sobre handles inexistentes son verificables sin audio hardware.

## Consecuencias

- La bridge pasa a arrastrar `lunar_audio`, `rodio` y `cpal` en los árboles `bridge` y `default`; los gates `dim2`, `dim3`, bare y 2D/3D-only siguen sin incluirlos porque la bridge no entra en esos árboles. CI refleja el nuevo árbol de la bridge.
- El SDK regenera sus headers con los nuevos métodos y la plantilla externa comprueba que compilan sin abrir dispositivo de audio.
- No se modifica el esquema JSON v1 ni el componente `Audio`; un hito posterior decidirá si el editor debe mostrar o editar esos campos.
- Gamepad, HUD, hot reload y bridge 3D siguen fuera de este hito.
