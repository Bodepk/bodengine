# ADR-022: SDK C++ distribuible

- Estado: Aceptada
- Fecha: 2026-11-25
- Supersede parcialmente: nada; amplía ADR-004 y ADR-021

## Contexto

ADR-021 convirtió `lunar_bridge` en la API de juego y `examples/cpp_platformer` demostró el contrato dentro del workspace: CMake localiza el `OUT_DIR` de Cargo, incluye el header generado y enlaza la staticlib. Ese patrón no es suficiente para un tercero: un proyecto C++ externo no debe conocer la ruta del repositorio, ejecutar Cargo desde su propio `CMakeLists.txt` ni copiar headers a mano.

Además, una staticlib Rust no propaga los `#[link]` ni los `cargo:rustc-link-lib` de sus dependencias: el consumidor debe nombrar las librerías del sistema. El fallo de CI en macOS con símbolos de `Metal` y `Carbon` lo confirmó. La lista de enlazado es parte del contrato de distribución y debe vivir en un solo lugar versionado.

No se pueden añadir dependencias nuevas: CMake 3.20 y sus módulos `CMakePackageConfigHelpers` ya están disponibles en la toolchain actual.

## Decisión

- Se crea el proyecto CMake `sdk/cpp` como punto de empaquetado del SDK:
  - ejecuta `cargo build -p lunar_bridge --release --message-format=json` desde la raíz del workspace;
  - extrae el `OUT_DIR` del build script de `lunar_bridge`, igual que los ejemplos actuales;
  - instala bajo el prefijo elegido:
    - `include/lunar_bridge/game.h`, contrato público escrito a mano;
    - `include/lunar_bridge/src/lib.rs.h`, header generado por `cxx`;
    - `include/rust/cxx.h`;
    - `lib/<staticlib>` (`lunar_bridge.lib` o `liblunar_bridge.a`);
    - `lib/cmake/lunar_bridge/lunar_bridgeConfig.cmake` y `lunar_bridgeConfigVersion.cmake`.
  - El código `.cc` generado por `cxx-build` se compila dentro de la staticlib; los consumidores solo incluyen headers.
- El paquete expone un target importado `lunar::bridge` con:
  - `INTERFACE_INCLUDE_DIRECTORIES` apuntando a `include`;
  - `IMPORTED_LOCATION` apuntando a la staticlib instalada;
  - `INTERFACE_LINK_LIBRARIES` con las librerías del sistema ya usadas por los ejemplos: en MSVC `kernel32`, `ntdll`, `userenv`, `ws2_32`, `dbghelp`, `synchronization`, `imm32`, `uxtheme` y `dwmapi`; en Apple `pthread`, `m`, `objc`, `iconv`, `AppKit`, `ApplicationServices`, `AudioUnit`, `Carbon`, `CoreAudio`, `CoreFoundation`, `CoreGraphics`, `CoreMIDI`, `Foundation`, `IOKit`, `Metal`, `MetalKit`, `QuartzCore`, `Security`, `SystemConfiguration` y `UniformTypeIdentifiers`; en el resto `asound`, `dl`, `pthread`, `m` y `rt`. H15 amplió la lista con las librerías de audio y H20 alineó los tres consumidores antes de publicar `v0.2.0`.
- El consumidor usa `find_package(lunar_bridge CONFIG REQUIRED)` y enlaza `lunar::bridge`. El archivo de versión usa `ExactVersion`: antes de 1.0 cualquier cambio de versión puede romper la ABI C++ y no se debe aceptar silenciosamente. La versión del paquete se toma del workspace (`0.1.0`).
- Se añade `examples/cpp_sdk_template`, un ejecutable C++ sin dependencia del workspace que incluye `lunar_bridge/game.h` y `lunar_bridge/src/lib.rs.h`, define una clase `Game` mínima y comprueba el reloj manual con 61 frames. No abre ventana y sirve de prueba de instalación en CI.
- CI añade cuatro pasos: configurar/instalar el SDK con `CMAKE_INSTALL_PREFIX`, configurar/compilar la plantilla con `CMAKE_PREFIX_PATH`, ejecutarla en los tres sistemas operativos y verificar que la plantilla no referencia rutas del workspace.
- La frontera de ADR-021 no cambia: `Renderer`, `Surface`, `ImageData`, escenas completas y Rapier siguen sin cruzar la FFI; `wgpu` no aparece en los headers instalados. La bridge sigue siendo C++17 y `cxx` 1.0.205.

## Consecuencias

- `sdk/cpp` pasa a ser la fuente de verdad de los headers y las librerías del sistema que necesita un juego C++ externo. Los ejemplos en árbol mantienen su lógica actual por ahora; cualquier cambio de enlazado debe reflejarse primero aquí y luego en ellos.
- No se añaden dependencias: solo CMake y Cargo. No se introducen gestores de paquetes (`vcpkg`, `conan`) ni descargas de artefactos.
- `find_package` con `ExactVersion` obliga a regenerar/actualizar el SDK cuando cambie la versión del workspace; eso evita emparejar una staticlib vieja con headers nuevos.
- Quedan fuera de este hito el SDK 3D, hot reload, plugins en runtime, audio en C++ y cualquier cambio del esquema JSON v1.
