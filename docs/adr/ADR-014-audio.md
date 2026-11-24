# ADR-014: Audio con `rodio` y crate `lunar_audio`

- Estado: Aceptada
- Fecha: 2026-11-24

## Contexto

H8 necesita reproducir sonido sin acoplar el bucle a ventanas, GPU ni reloj. `rodio` está aprobado en ADR-001 pero sin versión fijada. Los tests de CI no pueden depender de un dispositivo de audio real, y los builds 2D/3D/bare no deben arrastrar `cpal`.

## Decisión

- `rodio` queda fijado en **0.23.0** (MSRV 1.87), con `default-features = false` y las features `playback` y `wav`. Se excluyen grabación, mp3, mp4, flac, vorbis y dither: H8 solo necesita reproducción de WAV.
- `rodio` queda confinado en `lunar_audio`; ningún otro crate del motor lo menciona. La feature `audio` del crate raíz sigue activando solo este crate.
- `AudioStore` no abre dispositivos: guarda bytes WAV y entrega `SoundHandle(u64)` opaco, monótono por instancia. `load_from_slice` valida el WAV decodificándolo en memoria con `rodio::Decoder` sobre un `Cursor`; `load_from_path` lee el archivo y reutiliza la misma validación.
- `SoundData` es una instantánea poseída de bytes, sin tipos de `rodio` en la API pública. Cada reproducción crea un decoder nuevo a partir de una copia, de modo que un sonido se puede reproducir varias veces y en paralelo.
- `AudioPlayer` posee perezosamente el `MixerDeviceSink` de rodio. `play` devuelve `AudioHandle(u64)`; `stop`, `set_volume`, `stop_all` y `empty` actúan sobre ese handle. El volumen se limita a `[0.0, 1.0]` antes de pasarlo a rodio.
- Los errores son propios (`AudioLoadError`, `AudioError`) y no exponen `rodio`, `cpal` ni `std::io`. Si no hay dispositivo de salida, `play` devuelve `NoDevice` en lugar de hacer `unwrap`.
- La bridge C++ no gana APIs de audio en H8: no hay consumidor C++ real todavía, igual que ocurrió con `WorldHandle` y `AssetHandle`.
- H8 no decide cómo referenciar sonidos en escenas serializadas: eso depende de ADR-008 antes de H9.

## Consecuencias

- `dim2`, `dim3` y `--no-default-features` no arrastran `rodio` ni `cpal`; se comprueba en CI con `cargo tree -e features`.
- `--features audio` incluye `rodio` con `playback` y `wav`, pero sin `recording`.
- Los tests de `lunar_audio` no abren dispositivos: validan handles, volumen y decodificación en memoria.
- Añadir más formatos de audio no obliga a tocar render, física ni FFI.
