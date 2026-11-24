# ADR-004: Frontera cxx y firma de `EngineConfig`

- Estado: Pendiente de validar en H3
- Fecha: 2026-11-24

## Contexto

La API pública es C++. `cxx` permite exponer structs POD y handles opacos, pero la firma exacta de una configuración con texto debe validarse contra la versión real de `cxx` antes de escribir el ejemplo C++.

## Decisión prevista

- Structs POD y handles opacos (`Engine`, `WorldHandle`, `AssetHandle`) cruzan la frontera.
- Callbacks C++ se modelan como `fn* + void* ctx` si la versión de `cxx` no soporta limpiamente `Box<clase abstracta C++>`.
- Fijar la versión de `cxx` y validar la firma real en H3.

## Punto abierto (bloqueante para el ejemplo C++)

`EngineConfig.title` se propuso como `String`, pero `String` no es POD y no es fiable en `#[repr(C)]`. No se asumirá su traducción. En H3 se validará si `cxx` soporta `&str`, `CxxString` u otra forma limpia; el resultado se escribirá aquí **antes** de crear el ejemplo C++. Si ninguna opción es limpia, se pregunta al usuario.

## Consecuencias

- H1 y H2 no escriben código C++ ni definen la ABI pública.
- El smoke de H1 es solo Rust; la frontera se prueba en H3.
