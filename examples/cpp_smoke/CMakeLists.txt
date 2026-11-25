cmake_minimum_required(VERSION 3.20)
project(lunar_cpp_smoke LANGUAGES CXX)

set(CMAKE_CXX_STANDARD 17)
set(CMAKE_CXX_STANDARD_REQUIRED ON)
set(CMAKE_CXX_EXTENSIONS OFF)

get_filename_component(LUNAR_ROOT "${CMAKE_CURRENT_SOURCE_DIR}/../.." ABSOLUTE)
find_program(CARGO_EXECUTABLE cargo REQUIRED)

# Cargo publica el directorio OUT_DIR del build script en JSON; ahí cxx-build
# deja el header generado y `rust/cxx.h`. Construir en release mantiene el
# smoke C++ independiente del perfil debug del workspace.
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

set(LUNAR_BRIDGE_INCLUDE "${LUNAR_BRIDGE_OUT_DIR}/cxxbridge/include")
if(NOT EXISTS "${LUNAR_BRIDGE_INCLUDE}/rust/cxx.h")
  message(FATAL_ERROR "falta el header de cxx en ${LUNAR_BRIDGE_INCLUDE}")
endif()

set(LUNAR_BRIDGE_STATIC
    "${LUNAR_ROOT}/target/release/${CMAKE_STATIC_LIBRARY_PREFIX}lunar_bridge${CMAKE_STATIC_LIBRARY_SUFFIX}"
)

add_custom_target(lunar_bridge_rust ALL
  COMMAND "${CARGO_EXECUTABLE}" build -p lunar_bridge --release
  WORKING_DIRECTORY "${LUNAR_ROOT}"
  COMMENT "Construyendo la staticlib Rust de lunar_bridge"
  VERBATIM
)

add_executable(cpp_smoke src/main.cpp)
add_dependencies(cpp_smoke lunar_bridge_rust)
target_include_directories(cpp_smoke PRIVATE
  "${LUNAR_BRIDGE_INCLUDE}"
  "${LUNAR_ROOT}/crates/lunar_bridge/include"
)
target_link_libraries(cpp_smoke PRIVATE "${LUNAR_BRIDGE_STATIC}")

if(MSVC)
  target_link_libraries(cpp_smoke PRIVATE
    kernel32 ntdll userenv ws2_32 dbghelp synchronization imm32 uxtheme dwmapi
  )
elseif(APPLE)
  # Una staticlib Rust no propaga los `#[link]` de sus dependencias: CMake
  # debe nombrar los frameworks que usan winit, wgpu y security-framework.
  target_link_libraries(cpp_smoke PRIVATE pthread m objc iconv
    "-framework AppKit"
    "-framework CoreFoundation"
    "-framework CoreGraphics"
    "-framework Foundation"
    "-framework IOKit"
    "-framework Metal"
    "-framework MetalKit"
    "-framework QuartzCore"
    "-framework Security"
    "-framework SystemConfiguration"
    "-framework UniformTypeIdentifiers"
  )
else()
  target_link_libraries(cpp_smoke PRIVATE dl pthread m rt)
endif()
