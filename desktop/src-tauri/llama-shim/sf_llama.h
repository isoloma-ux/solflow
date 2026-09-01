/*
 * Узкий C-интерфейс поверх llama.cpp для Sol Flow: загрузка модели и
 * генерация по чат-шаблону. Собирается в одну динамическую библиотеку
 * вместе с самим llama.cpp (libsolflow_llama), чтобы её ggml не дрался
 * со статическим ggml движка расшифровки — проверено спайком coexist.c.
 *
 * Интерфейс нарочно из простых типов: его дергают и Rust (десктоп),
 * и JNI (Android), и хрупких структур по значению здесь нет.
 */
#ifndef SF_LLAMA_H
#define SF_LLAMA_H

#include <stdbool.h>

#ifdef __cplusplus
extern "C" {
#endif

#if defined(_WIN32)
#    define SF_API __declspec(dllexport)
#else
#    define SF_API __attribute__((visibility("default")))
#endif

/* Загрузка модели: путь к GGUF, размер контекста, число потоков
 * (0 — выбрать самим). NULL при ошибке. */
SF_API void * sf_llm_load(const char * model_path, int n_ctx, int n_threads);

SF_API void sf_llm_free(void * handle);

/* Список вычислителей через запятую («NVIDIA ... (Vulkan), CPU») — в буфер;
 * возвращает длину или -1. Для диагностики «кто считает». */
SF_API int sf_llm_devices(char * out, int cap);

/* Сколько токенов займет текст — чтобы решить, влезает ли в контекст. */
SF_API int sf_llm_count_tokens(void * handle, const char * text);

/*
 * Одна генерация: system + user → текст ответа кусками в on_piece.
 * Блок размышлений (<think>…</think>) в on_piece не попадает.
 * on_progress получает проценты обработки промпта (0–100), может быть NULL.
 * should_stop опрашивается по ходу; вернет true — генерация обрывается.
 * Возвращает 0 при успехе, отрицательный код при ошибке.
 */
SF_API int sf_llm_generate(
    void *       handle,
    const char * system_prompt,
    const char * user_prompt,
    int          max_tokens,
    float        temperature,
    float        repeat_penalty,
    void (*on_piece)(const char * piece, int len, void * userdata),
    void (*on_progress)(int percent, void * userdata),
    bool (*should_stop)(void * userdata),
    void * userdata);

#ifdef __cplusplus
}
#endif

#endif
