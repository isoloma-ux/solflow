/* Реализация узкого интерфейса — см. sf_llama.h. */
#include "sf_llama.h"

#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#if defined(_WIN32)
#    include <windows.h>
#else
#    include <unistd.h>
#endif

#include "llama.h"

typedef struct {
    struct llama_model *   model;
    struct llama_context * ctx;
    const struct llama_vocab * vocab;
    int n_ctx;
} sf_llm;

/* Одна попытка загрузки: gpu_layers > 0 — модель и кэш едут на видеокарту
 * (если бэкенд собран: Metal на Маке, Vulkan на Windows). */
static sf_llm * load_once(const char * model_path, int n_ctx, int n_threads, int gpu_layers);

void * sf_llm_load(const char * model_path, int n_ctx, int n_threads) {
    llama_backend_init();
    /* Сначала видеокарта; не вышло (нет её или не хватило видеопамяти) —
     * честный запасной путь на процессоре. */
    sf_llm * h = load_once(model_path, n_ctx, n_threads, 99);
    if (!h) h = load_once(model_path, n_ctx, n_threads, 0);
    return h;
}

static sf_llm * load_once(const char * model_path, int n_ctx, int n_threads, int gpu_layers) {
    struct llama_model_params mp = llama_model_default_params();
    mp.n_gpu_layers = gpu_layers;
    struct llama_model * model = llama_model_load_from_file(model_path, mp);
    if (!model) return NULL;

    struct llama_context_params cp = llama_context_default_params();
    cp.n_ctx = (unsigned) n_ctx;
    /* Промпт кормится кусками этого размера — так виден прогресс. */
    cp.n_batch = 1024;
    if (n_threads <= 0) {
        /* Умолчание llama — 4 потока, и на восьмиядерном телефоне это
         * оставляет половину процессора без дела. Берём все ядра минус
         * два — системе и интерфейсу тоже надо дышать. */
#if defined(_WIN32)
        SYSTEM_INFO si;
        GetSystemInfo(&si);
        long cores = (long) si.dwNumberOfProcessors;
#else
        long cores = sysconf(_SC_NPROCESSORS_ONLN);
#endif
        n_threads = (int) cores - 2;
        if (n_threads < 4) n_threads = 4;
    }
    cp.n_threads = n_threads;
    cp.n_threads_batch = n_threads;
    struct llama_context * ctx = llama_init_from_model(model, cp);
    if (!ctx) {
        llama_model_free(model);
        return NULL;
    }

    sf_llm * h = calloc(1, sizeof(sf_llm));
    h->model = model;
    h->ctx = ctx;
    h->vocab = llama_model_get_vocab(model);
    h->n_ctx = n_ctx;
    return h;
}

void sf_llm_free(void * handle) {
    sf_llm * h = handle;
    if (!h) return;
    if (h->ctx) llama_free(h->ctx);
    if (h->model) llama_model_free(h->model);
    free(h);
}

int sf_llm_count_tokens(void * handle, const char * text) {
    sf_llm * h = handle;
    if (!h || !text) return -1;
    int n = -llama_tokenize(h->vocab, text, (int) strlen(text), NULL, 0, true, true);
    return n;
}

/* Промпт по чат-шаблону модели; буфер растет, пока не влезет. */
static char * format_chat(sf_llm * h, const char * system, const char * user) {
    const char * tmpl = llama_model_chat_template(h->model, NULL);
    struct llama_chat_message msgs[2] = {
        { "system", system },
        { "user", user },
    };
    int size = (int) (strlen(system) + strlen(user)) + 1024;
    char * buf = malloc(size);
    int need = llama_chat_apply_template(tmpl, msgs, 2, true, buf, size);
    if (need > size) {
        buf = realloc(buf, need + 1);
        need = llama_chat_apply_template(tmpl, msgs, 2, true, buf, need + 1);
    }
    if (need < 0) {
        free(buf);
        return NULL;
    }
    buf[need] = 0;
    return buf;
}

int sf_llm_generate(
    void *       handle,
    const char * system_prompt,
    const char * user_prompt,
    int          max_tokens,
    float        temperature,
    float        repeat_penalty,
    void (*on_piece)(const char * piece, int len, void * userdata),
    void (*on_progress)(int percent, void * userdata),
    bool (*should_stop)(void * userdata),
    void * userdata) {
    sf_llm * h = handle;
    if (!h || !system_prompt || !user_prompt || !on_piece) return -1;

    char * prompt = format_chat(h, system_prompt, user_prompt);
    if (!prompt) return -2;

    /* Каждая генерация — с чистого листа: одна встреча — один вызов. */
    llama_memory_clear(llama_get_memory(h->ctx), true);

    int cap = (int) strlen(prompt) + 16;
    llama_token * tokens = malloc(cap * sizeof(llama_token));
    int n_tok = llama_tokenize(h->vocab, prompt, (int) strlen(prompt), tokens, cap, true, true);
    free(prompt);
    if (n_tok <= 0) {
        free(tokens);
        return -3;
    }
    if (n_tok >= h->n_ctx - max_tokens) {
        /* Не влезает — пусть вызывающий режет текст на куски. */
        free(tokens);
        return -4;
    }

    /* Сэмплер как в подобранном на замерах режиме: штраф повторов,
     * температура, случайный выбор. */
    struct llama_sampler_chain_params scp = llama_sampler_chain_default_params();
    struct llama_sampler * chain = llama_sampler_chain_init(scp);
    llama_sampler_chain_add(chain, llama_sampler_init_penalties(
        llama_vocab_n_tokens(h->vocab), 64, repeat_penalty, 0.0f, 0.0f));
    llama_sampler_chain_add(chain, llama_sampler_init_temp(temperature));
    llama_sampler_chain_add(chain, llama_sampler_init_dist(LLAMA_DEFAULT_SEED));

    int rc = 0;

    /* Промпт — кусками, с прогрессом. */
    int done = 0;
    while (done < n_tok) {
        if (should_stop && should_stop(userdata)) {
            rc = -5;
            goto out;
        }
        int chunk = n_tok - done;
        if (chunk > 1024) chunk = 1024;
        struct llama_batch batch = llama_batch_get_one(tokens + done, chunk);
        if (llama_decode(h->ctx, batch) != 0) {
            rc = -6;
            goto out;
        }
        done += chunk;
        if (on_progress) on_progress(done * 100 / n_tok, userdata);
    }

    /* Генерация. Блок размышлений придерживаем и наружу не отдаём.
     * Прогресс генерации уходит тем же колбэком со сдвигом: 100 + номер
     * токена — вызывающий сам решает, как это показать. */
    bool in_think = false;
    bool think_checked = false;
    char hold[64];
    int held = 0;

    for (int i = 0; i < max_tokens; i++) {
        if (should_stop && should_stop(userdata)) {
            rc = -5;
            goto out;
        }
        if (on_progress && i % 8 == 0) on_progress(100 + i, userdata);
        llama_token next = llama_sampler_sample(chain, h->ctx, -1);
        if (llama_vocab_is_eog(h->vocab, next)) break;

        char piece[256];
        int len = llama_token_to_piece(h->vocab, next, piece, sizeof(piece), 0, true);
        if (len > 0) {
            if (!think_checked) {
                /* Начало ответа: копим немного и смотрим, не <think> ли это. */
                int copy = len;
                if (held + copy > (int) sizeof(hold)) copy = (int) sizeof(hold) - held;
                memcpy(hold + held, piece, copy);
                held += copy;
                if (held >= 7 || hold[0] != '<') {
                    think_checked = true;
                    if (held >= 7 && strncmp(hold, "<think>", 7) == 0) {
                        in_think = true;
                    } else {
                        on_piece(hold, held, userdata);
                    }
                    held = 0;
                }
            } else if (in_think) {
                /* Ждём закрывающий тег; хвосты держим в hold на случай
                 * тега, разрезанного границей токена. */
                int total = held + len;
                char * merged = malloc(total + 1);
                memcpy(merged, hold, held);
                memcpy(merged + held, piece, len);
                merged[total] = 0;
                char * end = strstr(merged, "</think>");
                if (end) {
                    in_think = false;
                    const char * rest = end + 8;
                    while (*rest == '\n' || *rest == ' ') rest++;
                    int rest_len = (int) (total - (rest - merged));
                    if (rest_len > 0) on_piece(rest, rest_len, userdata);
                    held = 0;
                } else {
                    held = total < 16 ? total : 16;
                    memcpy(hold, merged + total - held, held);
                }
                free(merged);
            } else {
                on_piece(piece, len, userdata);
            }
        }

        struct llama_batch batch = llama_batch_get_one(&next, 1);
        if (llama_decode(h->ctx, batch) != 0) {
            rc = -6;
            goto out;
        }
    }

out:
    llama_sampler_free(chain);
    free(tokens);
    return rc;
}
