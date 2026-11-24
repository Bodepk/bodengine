# ADR-004: Frontera cxx y firma de `EngineConfig`

- Estado: Aceptada (validada en H3)
- Fecha: 2026-11-24

## Contexto

La API pública es C++. `cxx` permite exponer structs POD y handles opacos, pero la firma exacta de una configuración con texto debía validarse contra la versión real de `cxx` antes de escribir el ejemplo C++.

## Decisión prevista

- Structs POD y handles opacos (`Engine`, `WorldHandle`, `AssetHandle`) cruzan la frontera.
- Callbacks C++ se modelan como `fn* + void* ctx` si la versión de `cxx` no soporta limpiamente `Box<clase abstracta C++>`.
- Fijar la versión de `cxx` y validar la firma real en H3.

## Punto abierto resuelto en H3

`EngineConfig.title` se propuso como `String`, pero `String` no es POD y no es fiable en `#[repr(C)]`. Se validó con `cxx`/`cxx-build` 1.0.205:

- `String` sí está permitido como campo de un *shared struct*; `cxx` lo traduce a `rust::String`, un tipo con layout y destructores gestionados por la bridge.
- `&str` (`rust::Str`) está permitido como argumento o retorno de función, pero **no** como campo de shared struct.
- Por tanto, la shared struct `EngineConfig { title: String, width: u32, height: u32 }` es la firma elegida. C++ la construye con `EngineConfig{"...", 800, 600}` y `cxx` genera los static assertions de layout.

## Decisión

- `cxx` y `cxx-build` quedan fijados en 1.0.205 (MSRV 1.88), compatibles con la toolchain 1.100.0.
- `EngineConfig` y `FrameStats` son shared structs. `FrameStats` sustituye `Duration` por `dropped_ns: u64` para mantener la ABI POD.
- `Engine` es un tipo Rust opaco entregado a C++ como `Box<Engine>` (`rust::Box<Engine>` en el header generado). El smoke C++ crea el motor, consulta la configuración y ejecuta 61 frames de tiempo inyectado.
- El reloj del bridge es `ManualClock`: la prueba C++ es determinista y no depende de dormir ni de GPU/ventana.
- No se definen callbacks en H3; la alternativa `fn* + void* ctx` queda registrada para cuando aparezca un caso de uso real.
- `WorldHandle` y `AssetHandle` se posponen hasta que existan `World` y assets públicos (H4/H6). No se crean handles vacíos solo para rellenar la ABI.

## Consecuencias

- H1 y H2 no escribieron código C++ ni definieron la ABI pública.
- El smoke de H2 era solo Rust; la frontera se prueba en H3 con `examples/cpp_smoke`.
- `cargo test -p lunar_bridge` cubre la configuración compartida y el bucle de paso fijo sin ventana.
- CMake localiza el header generado parseando el `OUT_DIR` que Cargo publica en `--message-format=json`; no se copia código generado al árbol de fuentes.
