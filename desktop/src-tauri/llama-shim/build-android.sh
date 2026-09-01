#!/bin/sh
# Собирает libsolflow_llama.so для Android arm64: llama.cpp статикой + шим
# + JNI-мост, всё в одном файле. Кладёт в jniLibs Android-приложения.
#
# Гонять через симлинк без пробелов (/Users/isoloma/handy-android) — как и
# остальные нативные сборки проекта.
set -e

here="$(cd "$(dirname "$0")" && pwd)"
repo="$(cd "$here/../../.." && pwd)"
llama="$repo/llama.cpp"
out="$repo/app-android/app/src/main/jniLibs/arm64-v8a"

toolchain="$HOME/android-toolchain/sdk/ndk/28.2.13676358"
cmake_bin="$HOME/android-toolchain/sdk/cmake/3.31.6/bin/cmake"
[ -x "$cmake_bin" ] || cmake_bin=cmake

# armv8.2+dotprod+fp16: быстро на современных телефонах и не падает на
# чипах без i8mm (он есть только с 2021 года).
"$cmake_bin" -S "$llama" -B "$llama/build-android" \
    -DCMAKE_TOOLCHAIN_FILE="$toolchain/build/cmake/android.toolchain.cmake" \
    -DANDROID_ABI=arm64-v8a \
    -DANDROID_PLATFORM=android-30 \
    -DCMAKE_BUILD_TYPE=Release \
    -DBUILD_SHARED_LIBS=OFF \
    -DGGML_NATIVE=OFF \
    -DGGML_CPU_ARM_ARCH=armv8.2-a+dotprod+fp16 \
    -DGGML_CPU_KLEIDIAI=ON \
    -DGGML_OPENMP=OFF \
    -DLLAMA_CURL=OFF \
    -DLLAMA_BUILD_TESTS=OFF \
    -DLLAMA_BUILD_EXAMPLES=OFF \
    -DLLAMA_BUILD_TOOLS=OFF \
    -DLLAMA_BUILD_SERVER=OFF \
    -DLLAMA_BUILD_COMMON=OFF > /dev/null
"$cmake_bin" --build "$llama/build-android" --target llama -j 8 > /dev/null

mkdir -p "$out"
cc="$toolchain/toolchains/llvm/prebuilt/darwin-x86_64/bin/aarch64-linux-android30-clang"

libs=""
for lib in \
    src/libllama.a \
    ggml/src/libggml.a \
    ggml/src/libggml-base.a \
    ggml/src/libggml-cpu.a
do
    [ -f "$llama/build-android/$lib" ] && libs="$libs $llama/build-android/$lib"
done

# shellcheck disable=SC2086
"$cc" "$here/sf_llama.c" "$here/sf_llama_jni.c" -shared -O2 -fPIC \
    -I "$llama/include" -I "$llama/ggml/include" \
    -Wl,--whole-archive $libs -Wl,--no-whole-archive \
    -static-libstdc++ -lc++_static -lc++abi -llog -landroid \
    -o "$out/libsolflow_llama.so"

"$toolchain/toolchains/llvm/prebuilt/darwin-x86_64/bin/llvm-strip" "$out/libsolflow_llama.so"
echo "готово: $out/libsolflow_llama.so"
ls -la "$out/libsolflow_llama.so"
