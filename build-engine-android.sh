#!/bin/sh
# Кросс-компиляция transcribe.cpp под Android arm64-v8a.
# Результат: build-android-opt/src/libtranscribe.a + bin/transcribe-cli
set -e

. "$HOME/android-toolchain/env.sh"

NDK="$ANDROID_HOME/ndk/28.2.13676358"
CMAKE_BIN="$ANDROID_HOME/cmake/3.31.6/bin/cmake"
SRC="$(cd "$(dirname "$0")" && pwd)/tcpp"

# armv8.2-a+dotprod+fp16 — базовая линия для телефонов примерно с 2018 года
# (Snapdragon 845 и новее). Даёт ~34% к скорости против дефолтных флагов NDK.
# SVE/i8mm/SME намеренно не включаем: они сузили бы совместимость.
"$CMAKE_BIN" -B "$SRC/build-android-opt" -S "$SRC" \
  -DCMAKE_TOOLCHAIN_FILE="$NDK/build/cmake/android.toolchain.cmake" \
  -DANDROID_ABI=arm64-v8a \
  -DANDROID_PLATFORM=android-26 \
  -DCMAKE_BUILD_TYPE=Release \
  -DTRANSCRIBE_BUILD_TESTS=OFF \
  -DTRANSCRIBE_BUILD_EXAMPLES=ON \
  -DTRANSCRIBE_USE_SYSTEM_BLAS=OFF \
  -DTRANSCRIBE_METAL=OFF \
  -DGGML_OPENMP=OFF \
  -DGGML_NATIVE=OFF \
  -DGGML_CPU_ARM_ARCH="armv8.2-a+dotprod+fp16"

"$CMAKE_BIN" --build "$SRC/build-android-opt" -j8
