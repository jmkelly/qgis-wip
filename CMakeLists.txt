cmake_minimum_required(VERSION 3.16)
project(qgis_oracle_harness LANGUAGES CXX)

# Minimal C++ stand-in for a real QGIS binary (bead qgis-uic.7).
# A full QGIS build is impractical on most machines (no system cmake,
# 30-60+ minute compile), so this harness plays the role of
# $QGIS_CPP_BINARY at E0: it reads a fixture file and emits the same
# canonical output format as the Rust oracle backend
# (oracle/src/rust_backend.rs) with the backend
# self-identification line set to "cpp".
#
# Build (out of tree):
#   cmake -S rust/cpp_harness -B rust/cpp_harness/build
#   cmake --build rust/cpp_harness/build
# Binary: rust/cpp_harness/build/qgis_oracle_harness

set(CMAKE_CXX_STANDARD 17)
set(CMAKE_CXX_STANDARD_REQUIRED ON)
set(CMAKE_CXX_EXTENSIONS OFF)

add_executable(qgis_oracle_harness main.cpp)
