cmake_minimum_required(VERSION 3.22)
project(handyvoice CXX)

# transcribe.cpp живёт рядом с проектом приложения, а не внутри него —
# это тот же клон, который собирает build-engine-android.sh.
set(TCPP_DIR "${CMAKE_CURRENT_SOURCE_DIR}/../../../../../tcpp")

if(NOT EXISTS "${TCPP_DIR}/CMakeLists.txt")
    message(FATAL_ERROR "Не найден transcribe.cpp по пути ${TCPP_DIR}")
endif()

# Ставим до add_subdirectory, иначе option() в transcribe.cpp возьмёт свои
# значения. BLAS выключен намеренно: в NDK нет системной BLAS.
set(TRANSCRIBE_BUILD_TESTS    OFF CACHE BOOL "" FORCE)
set(TRANSCRIBE_BUILD_EXAMPLES OFF CACHE BOOL "" FORCE)
set(TRANSCRIBE_USE_SYSTEM_BLAS OFF CACHE BOOL "" FORCE)
set(TRANSCRIBE_METAL          OFF CACHE BOOL "" FORCE)
set(TRANSCRIBE_INSTALL        OFF CACHE BOOL "" FORCE)
set(GGML_OPENMP               OFF CACHE BOOL "" FORCE)
set(GGML_NATIVE               OFF CACHE BOOL "" FORCE)

# transcribe.cpp объявляет свои заголовки как ${CMAKE_SOURCE_DIR}/include, что
# верно только когда он собирается как корневой проект. Через add_subdirectory
# CMAKE_SOURCE_DIR указывает уже сюда, поэтому путь добавляем сами — это
# дешевле, чем патчить апстрим.
include_directories(BEFORE "${TCPP_DIR}/include")

add_subdirectory("${TCPP_DIR}" "${CMAKE_CURRENT_BINARY_DIR}/tcpp" EXCLUDE_FROM_ALL)

add_library(handyvoice SHARED handy_jni.cpp)

target_include_directories(handyvoice PRIVATE "${TCPP_DIR}/include")

# Состав берётся из lib/transcribe-link.json, который генерирует сборка.
target_link_libraries(handyvoice
    PRIVATE
        transcribe
        ggml
        ggml-cpu
        ggml-base
        android
        log
)
