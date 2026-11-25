cmake_minimum_required(VERSION 3.20)
project(lunar_cpp_sdk_template LANGUAGES CXX)

set(CMAKE_CXX_STANDARD 17)
set(CMAKE_CXX_STANDARD_REQUIRED ON)
set(CMAKE_CXX_EXTENSIONS OFF)

# Proyecto externo: no conoce la ruta del workspace ni ejecuta Cargo (ADR-022).
find_package(lunar_bridge 0.1.0 EXACT CONFIG REQUIRED)

add_executable(cpp_sdk_template src/main.cpp)
target_link_libraries(cpp_sdk_template PRIVATE lunar::bridge)
