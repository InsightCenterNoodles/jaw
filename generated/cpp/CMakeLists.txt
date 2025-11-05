cmake_minimum_required(VERSION 3.16 FATAL_ERROR)

# Project ======================================================================

project(
    jaw_cpp
    VERSION 0.1
    LANGUAGES C CXX
)

# Create BIN ===============================================================

add_executable(jaw_cpp
    src/basic.hpp
    src/codec.hpp
    src/main.cpp
)

set_target_properties(jaw_cpp PROPERTIES CXX_STANDARD 20)

if (CMAKE_BUILD_TYPE STREQUAL "Debug")
    message(STATUS "Enabling AddressSanitizer")
    target_compile_options(jaw_cpp PUBLIC -fsanitize=address,undefined -fno-omit-frame-pointer)
    target_link_options(jaw_cpp PUBLIC  -fsanitize=address,undefined -fno-omit-frame-pointer)
endif()
