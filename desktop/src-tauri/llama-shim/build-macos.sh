#!/bin/sh
# Собирает libsolflow_llama.dylib: llama.cpp статикой + наш узкий шим,
# всё в одном файле. Металлический шейдер вшивается в библиотеку
# (GGML_METAL_EMBED_LIBRARY), чтобы рядом ничего не возить.
#
# Ожидает клон llama.cpp в корне репозитория (../../..//llama.cpp).
set -e

here="$(cd "$(dirname "$0")" && pwd)"
repo="$(cd "$here/../../.." && pwd)"
llama="$repo/llama.cpp"
out="$here/../llama/lib"
cmake_bin="${CMAKE:-$(ls "$HOME"/Library/Python/*/bin/cmake 2>/dev/null | head -1)}"
[ -x "$cmake_bin" ] || cmake_bin=cmake

"$cmake_bin" -S "$llama" -B "$llama/build-static" \
    -DCMAKE_BUILD_TYPE=Release \
    -DBUILD_SHARED_LIBS=OFF \
    -DGGML_METAL_EMBED_LIBRARY=ON \
    -DLLAMA_CURL=OFF \
    -DLLAMA_BUILD_TESTS=OFF \
    -DLLAMA_BUILD_EXAMPLES=OFF \
    -DLLAMA_BUILD_TOOLS=OFF \
    -DLLAMA_BUILD_SERVER=OFF \
    -DLLAMA_BUILD_COMMON=OFF > /dev/null
# Только сама библиотека: cli-обвязка llama.cpp нам не нужна и без
# common не собирается.
"$cmake_bin" --build "$llama/build-static" --target llama -j 8 > /dev/null

mkdir -p "$out"
libs=""
for lib in \
    src/libllama.a \
    ggml/src/libggml.a \
    ggml/src/libggml-base.a \
    ggml/src/libggml-cpu.a \
    ggml/src/ggml-metal/libggml-metal.a \
    ggml/src/ggml-blas/libggml-blas.a
do
    [ -f "$llama/build-static/$lib" ] && libs="$libs -Wl,-force_load,$llama/build-static/$lib"
done

# shellcheck disable=SC2086
clang "$here/sf_llama.c" -dynamiclib -O2 \
    -I "$llama/include" -I "$llama/ggml/include" \
    -install_name @rpath/libsolflow_llama.dylib \
    $libs \
    -lc++ \
    -framework Foundation -framework Metal -framework MetalKit -framework Accelerate \
    -o "$out/libsolflow_llama.dylib"

echo "готово: $out/libsolflow_llama.dylib"
ls -la "$out/libsolflow_llama.dylib"
