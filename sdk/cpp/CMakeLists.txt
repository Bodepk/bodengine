cmake_minimum_required(VERSION 3.20)
project(lunar_bridge_sdk LANGUAGES CXX)

include(CMakePackageConfigHelpers)

get_filename_component(LUNAR_ROOT "${CMAKE_CURRENT_SOURCE_DIR}/../.." ABSOLUTE)
find_program(CARGO_EXECUTABLE cargo REQUIRED)

# La versión del paquete sigue a la del workspace; antes de 1.0 se usa
# `ExactVersion` porque cualquier cambio puede romper la ABI C++ (ADR-022).
file(STRINGS "${LUNAR_ROOT}/Cargo.toml" LUNAR_VERSION_LINES
     REGEX "^version = \"[0-9]+\\.[0-9]+\\.[0-9]+\"$")
if(NOT LUNAR_VERSION_LINES)
  message(FATAL_ERROR "no se encontró la versión del workspace en Cargo.toml")
endif()
list(GET LUNAR_VERSION_LINES 0 LUNAR_VERSION_LINE)
string(REGEX MATCH "^version = \"([0-9]+\\.[0-9]+\\.[0-9]+)\"" LUNAR_VERSION_MATCH "${LUNAR_VERSION_LINE}")
set(LUNAR_VERSION "${CMAKE_MATCH_1}")

# Patrón de ADR-004/021: Cargo publica el OUT_DIR del build script en JSON.
execute_process(
  COMMAND "${CARGO_EXECUTABLE}" build -p lunar_bridge --release --message-format=json
  WORKING_DIRECTORY "${LUNAR_ROOT}"
  OUTPUT_VARIABLE CARGO_JSON
  ERROR_VARIABLE CARGO_ERROR
  RESULT_VARIABLE CARGO_RESULT
)
if(NOT CARGO_RESULT EQUAL 0)
  message(FATAL_ERROR "cargo build -p lunar_bridge falló:\n${CARGO_ERROR}")
endif()

string(REPLACE "\r\n" "\n" CARGO_JSON "${CARGO_JSON}")
string(REPLACE "\n" ";" CARGO_LINES "${CARGO_JSON}")
set(LUNAR_BRIDGE_OUT_DIR "")
foreach(line IN LISTS CARGO_LINES)
  if(line MATCHES "build-script-executed" AND line MATCHES "lunar_bridge")
    string(JSON out_dir GET "${line}" out_dir)
    set(LUNAR_BRIDGE_OUT_DIR "${out_dir}")
  endif()
endforeach()
if(NOT LUNAR_BRIDGE_OUT_DIR)
  message(FATAL_ERROR "no se encontró el OUT_DIR de lunar_bridge en la salida de cargo")
endif()

set(LUNAR_BRIDGE_GENERATED_INCLUDE "${LUNAR_BRIDGE_OUT_DIR}/cxxbridge/include")
set(LUNAR_BRIDGE_PUBLIC_HEADER
    "${LUNAR_ROOT}/crates/lunar_bridge/include/lunar_bridge/game.h")
set(LUNAR_BRIDGE_BRIDGE_SOURCE "${LUNAR_ROOT}/crates/lunar_bridge/src/lib.rs")
set(LUNAR_BRIDGE_BUILD_SCRIPT "${LUNAR_ROOT}/crates/lunar_bridge/build.rs")
set(LUNAR_BRIDGE_GENERATED_HEADER
    "${LUNAR_BRIDGE_GENERATED_INCLUDE}/lunar_bridge/src/lib.rs.h")
set(LUNAR_BRIDGE_CXX_HEADER "${LUNAR_BRIDGE_GENERATED_INCLUDE}/rust/cxx.h")
set(LUNAR_BRIDGE_STATIC_NAME
    "${CMAKE_STATIC_LIBRARY_PREFIX}lunar_bridge${CMAKE_STATIC_LIBRARY_SUFFIX}")
set(LUNAR_BRIDGE_STATIC "${LUNAR_ROOT}/target/release/${LUNAR_BRIDGE_STATIC_NAME}")

foreach(required IN ITEMS
    LUNAR_BRIDGE_PUBLIC_HEADER
    LUNAR_BRIDGE_GENERATED_HEADER
    LUNAR_BRIDGE_CXX_HEADER
    LUNAR_BRIDGE_STATIC)
  if(NOT EXISTS "${${required}}")
    message(FATAL_ERROR "falta el artefacto del SDK: ${${required}}")
  endif()
endforeach()

# Se stagea todo antes de instalar para que `cmake --install` copie exactamente
# los artefactos construidos por el último `cmake --build`.
set(SDK_STAGE "${CMAKE_CURRENT_BINARY_DIR}/stage")
file(MAKE_DIRECTORY
  "${SDK_STAGE}/include/lunar_bridge/src"
  "${SDK_STAGE}/include/rust"
  "${SDK_STAGE}/lib"
)

set(SDK_OUTPUTS
  "${SDK_STAGE}/include/lunar_bridge/game.h"
  "${SDK_STAGE}/include/lunar_bridge/src/lib.rs.h"
  "${SDK_STAGE}/include/rust/cxx.h"
  "${SDK_STAGE}/lib/${LUNAR_BRIDGE_STATIC_NAME}"
)

add_custom_command(
  OUTPUT ${SDK_OUTPUTS}
  COMMAND "${CMAKE_COMMAND}" -E make_directory
          "${SDK_STAGE}/include/lunar_bridge/src"
          "${SDK_STAGE}/include/rust"
          "${SDK_STAGE}/lib"
  COMMAND "${CARGO_EXECUTABLE}" build -p lunar_bridge --release
          --manifest-path "${LUNAR_ROOT}/Cargo.toml"
  COMMAND "${CMAKE_COMMAND}" -E copy_if_different
          "${LUNAR_BRIDGE_PUBLIC_HEADER}" "${SDK_STAGE}/include/lunar_bridge/game.h"
  COMMAND "${CMAKE_COMMAND}" -E copy_if_different
          "${LUNAR_BRIDGE_GENERATED_HEADER}" "${SDK_STAGE}/include/lunar_bridge/src/lib.rs.h"
  COMMAND "${CMAKE_COMMAND}" -E copy_if_different
          "${LUNAR_BRIDGE_CXX_HEADER}" "${SDK_STAGE}/include/rust/cxx.h"
  COMMAND "${CMAKE_COMMAND}" -E copy_if_different
          "${LUNAR_BRIDGE_STATIC}" "${SDK_STAGE}/lib/${LUNAR_BRIDGE_STATIC_NAME}"
  DEPENDS
    "${LUNAR_BRIDGE_PUBLIC_HEADER}"
    "${LUNAR_BRIDGE_BRIDGE_SOURCE}"
    "${LUNAR_BRIDGE_BUILD_SCRIPT}"
  WORKING_DIRECTORY "${LUNAR_ROOT}"
  COMMENT "Construyendo y empaquetando la staticlib Rust de lunar_bridge"
  VERBATIM
)
add_custom_target(lunar_bridge_sdk ALL DEPENDS ${SDK_OUTPUTS})

install(DIRECTORY "${SDK_STAGE}/include/" DESTINATION include)
install(FILES "${SDK_STAGE}/lib/${LUNAR_BRIDGE_STATIC_NAME}" DESTINATION lib)

set(LUNAR_BRIDGE_INSTALL_INCLUDE_DIR include)
set(LUNAR_BRIDGE_INSTALL_LIB_DIR lib)

configure_package_config_file(
  "${CMAKE_CURRENT_SOURCE_DIR}/cmake/lunar_bridgeConfig.cmake.in"
  "${CMAKE_CURRENT_BINARY_DIR}/lunar_bridgeConfig.cmake"
  INSTALL_DESTINATION lib/cmake/lunar_bridge
  PATH_VARS LUNAR_BRIDGE_INSTALL_INCLUDE_DIR LUNAR_BRIDGE_INSTALL_LIB_DIR
)
write_basic_package_version_file(
  "${CMAKE_CURRENT_BINARY_DIR}/lunar_bridgeConfigVersion.cmake"
  VERSION "${LUNAR_VERSION}"
  COMPATIBILITY ExactVersion
)
install(FILES
  "${CMAKE_CURRENT_BINARY_DIR}/lunar_bridgeConfig.cmake"
  "${CMAKE_CURRENT_BINARY_DIR}/lunar_bridgeConfigVersion.cmake"
  DESTINATION lib/cmake/lunar_bridge
)
