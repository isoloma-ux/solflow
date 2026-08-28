// JNI-мост между Kotlin и transcribe.cpp.
//
// Весь счастливый путь библиотеки — пять вызовов:
//   transcribe_open -> transcribe_run -> transcribe_full_text -> transcribe_close
// Передача NULL вместо структур параметров — это версионно-устойчивый способ
// запросить умолчания (см. комментарий в transcribe.h).

#include <jni.h>
#include <android/log.h>

#include <string>
#include <vector>

#include "transcribe.h"

#define LOG_TAG "HandyVoice"
#define LOGI(...) __android_log_print(ANDROID_LOG_INFO, LOG_TAG, __VA_ARGS__)
#define LOGE(...) __android_log_print(ANDROID_LOG_ERROR, LOG_TAG, __VA_ARGS__)

namespace {

// Логи ggml/transcribe уходят в logcat, иначе они теряются.
void forward_log(transcribe_log_level level, const char * text, void * /*userdata*/) {
    if (text == nullptr) {
        return;
    }
    int prio;
    switch (level) {
        case TRANSCRIBE_LOG_LEVEL_ERROR: prio = ANDROID_LOG_ERROR; break;
        case TRANSCRIBE_LOG_LEVEL_WARN:  prio = ANDROID_LOG_WARN;  break;
        case TRANSCRIBE_LOG_LEVEL_DEBUG: prio = ANDROID_LOG_DEBUG; break;
        default:                         prio = ANDROID_LOG_INFO;  break;
    }
    __android_log_print(prio, LOG_TAG, "%s", text);
}

jstring to_jstring(JNIEnv * env, const char * s) {
    return env->NewStringUTF(s == nullptr ? "" : s);
}

}  // namespace

extern "C" {

JNIEXPORT jstring JNICALL
Java_com_handy_voice_Transcriber_nativeVersion(JNIEnv * env, jclass) {
    return to_jstring(env, transcribe_version());
}

// Возвращает указатель на сессию как jlong, или 0 при ошибке.
JNIEXPORT jlong JNICALL
Java_com_handy_voice_Transcriber_nativeOpen(JNIEnv * env, jobject, jstring model_path) {
    static bool backends_ready = false;
    if (!backends_ready) {
        transcribe_log_set(forward_log, nullptr);
        const transcribe_status st = transcribe_init_backends_default();
        if (st != TRANSCRIBE_OK) {
            LOGE("transcribe_init_backends_default: %s", transcribe_status_string(st));
            return 0;
        }
        backends_ready = true;
    }

    const char * path = env->GetStringUTFChars(model_path, nullptr);
    if (path == nullptr) {
        return 0;
    }

    struct transcribe_session * session = nullptr;
    const transcribe_status     st      = transcribe_open(path, nullptr, nullptr, &session);
    env->ReleaseStringUTFChars(model_path, path);

    if (st != TRANSCRIBE_OK || session == nullptr) {
        LOGE("transcribe_open: %s", transcribe_status_string(st));
        return 0;
    }

    LOGI("модель загружена, arch=%s", transcribe_model_arch_string(transcribe_get_model(session)));
    return reinterpret_cast<jlong>(session);
}

// pcm — 16 кГц моно float32 в диапазоне [-1, 1]. Библиотека не содержит
// ресемплера, приводить частоту обязан вызывающий.
JNIEXPORT jstring JNICALL
Java_com_handy_voice_Transcriber_nativeRun(JNIEnv * env, jobject, jlong handle, jfloatArray pcm) {
    auto * session = reinterpret_cast<struct transcribe_session *>(handle);
    if (session == nullptr || pcm == nullptr) {
        return to_jstring(env, "");
    }

    const jsize n       = env->GetArrayLength(pcm);
    jfloat *    samples = env->GetFloatArrayElements(pcm, nullptr);
    if (samples == nullptr) {
        return to_jstring(env, "");
    }

    const transcribe_status st = transcribe_run(session, samples, static_cast<int>(n), nullptr);
    env->ReleaseFloatArrayElements(pcm, samples, JNI_ABORT);

    if (st != TRANSCRIBE_OK) {
        LOGE("transcribe_run: %s", transcribe_status_string(st));
        return to_jstring(env, "");
    }

    return to_jstring(env, transcribe_full_text(session));
}

JNIEXPORT void JNICALL
Java_com_handy_voice_Transcriber_nativeClose(JNIEnv *, jobject, jlong handle) {
    auto * session = reinterpret_cast<struct transcribe_session *>(handle);
    if (session != nullptr) {
        transcribe_close(session);
    }
}

}  // extern "C"
