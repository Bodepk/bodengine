cmake_minimum_required(VERSION 3.20)
project(lunar_cpp_platformer LANGUAGES CXX)

set(CMAKE_CXX_STANDARD 17)
set(CMAKE_CXX_STANDARD_REQUIRED ON)
set(CMAKE_CXX_EXTENSIONS OFF)

get_filename_component(LUNAR_ROOT "${CMAKE_CURRENT_SOURCE_DIR}/../.." ABSOLUTE)
find_program(CARGO_EXECUTABLE cargo REQUIRED)

# Patrón de examples/cpp_smoke (ADR-004/021): Cargo publica OUT_DIR en JSON y
# cxx-build deja ahí el header generado y `rust/cxx.h`.
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

file(TO_CMAKE_PATH "${LUNAR_ROOT}/examples/sprite_platformer/assets" LUNAR_ASSETS_PATH)

add_executable(cpp_platformer src/main.cpp src/game.cpp)
add_dependencies(cpp_platformer lunar_bridge_rust)
target_include_directories(cpp_platformer PRIVATE
  "${LUNAR_BRIDGE_INCLUDE}"
  "${LUNAR_ROOT}/crates/lunar_bridge/include"
)
target_compile_definitions(cpp_platformer PRIVATE LUNAR_ASSETS_DIR="${LUNAR_ASSETS_PATH}")
target_link_libraries(cpp_platformer PRIVATE "${LUNAR_BRIDGE_STATIC}")

if(MSVC)
  target_link_libraries(cpp_platformer PRIVATE
    kernel32 ntdll userenv ws2_32 dbghelp synchronization imm32 uxtheme dwmapi
  )
elseif(APPLE)
  # Una staticlib Rust no propaga los `#[link]` de sus dependencias: CMake
  # debe nombrar los frameworks que usan winit, wgpu, cpal y security-framework.
  target_link_libraries(cpp_platformer PRIVATE pthread m objc iconv
    "-framework AppKit"
    "-framework ApplicationServices"
    "-framework AudioUnit"
    "-framework Carbon"
    "-framework CoreAudio"
    "-framework CoreFoundation"
    "-framework CoreGraphics"
    "-framework CoreMIDI"
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
  target_link_libraries(cpp_platformer PRIVATE asound dl pthread m rt)
endif()
