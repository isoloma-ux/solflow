# Сборка solflow_llama для Windows (MSVC): llama.cpp статикой + шим одной
# DLL, импортная библиотека появляется сама. На macOS то же делает
# build-macos.sh (там нужен force_load и вшитый Metal-шейдер).
#
#   cmake -S llama-shim -B shim-build -DLLAMA_DIR=путь/к/llama.cpp
#   cmake --build shim-build --config Release
cmake_minimum_required(VERSION 3.20)
project(solflow_llama C CXX)

set(LLAMA_DIR "" CACHE PATH "Клон llama.cpp")
if(NOT LLAMA_DIR)
    message(FATAL_ERROR "нужен -DLLAMA_DIR=путь/к/llama.cpp")
endif()

set(BUILD_SHARED_LIBS OFF CACHE BOOL "" FORCE)
set(LLAMA_BUILD_TESTS OFF CACHE BOOL "" FORCE)
set(LLAMA_BUILD_EXAMPLES OFF CACHE BOOL "" FORCE)
set(LLAMA_BUILD_TOOLS OFF CACHE BOOL "" FORCE)
set(LLAMA_BUILD_SERVER OFF CACHE BOOL "" FORCE)
set(LLAMA_BUILD_COMMON OFF CACHE BOOL "" FORCE)
set(LLAMA_CURL OFF CACHE BOOL "" FORCE)
# Статические куски пойдут в общую DLL.
set(CMAKE_POSITION_INDEPENDENT_CODE ON)

add_subdirectory(${LLAMA_DIR} llama-build)

add_library(solflow_llama SHARED sf_llama.c)
target_include_directories(solflow_llama PRIVATE
    ${LLAMA_DIR}/include
    ${LLAMA_DIR}/ggml/include)
target_link_libraries(solflow_llama PRIVATE llama)
