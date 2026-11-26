# SDK C++ de Lunar

Paquete CMake descrito en [ADR-022](../../docs/adr/ADR-022-cpp-sdk.md). Construye la staticlib de `lunar_bridge` desde la raíz del workspace y deja un prefijo instalable con headers, librería y config de CMake.

```bash
cmake -S sdk/cpp -B build/cpp_sdk -DCMAKE_INSTALL_PREFIX="$PWD/build/sdk-prefix"
cmake --build build/cpp_sdk --config Release
cmake --install build/cpp_sdk --config Release
```

Desde cualquier proyecto C++ externo:

```cmake
find_package(lunar_bridge 0.2.0 EXACT CONFIG REQUIRED)
target_link_libraries(mi_juego PRIVATE lunar::bridge)
```

La versión es exacta porque, antes de 1.0, un cambio de versión puede romper la ABI C++. `lunar::bridge` propaga los includes y las librerías del sistema que la staticlib Rust no puede declarar por sí sola.

`examples/cpp_sdk_template` es la prueba mínima de instalación: compila sin referenciar el workspace, no abre ventana y verifica el reloj manual con 61 frames.
