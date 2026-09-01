/*
 * JNI-мост к узкому интерфейсу sf_llama.h — для Android. Вкомпилируется
 * в ту же libsolflow_llama.so, что и llama.cpp с шимом.
 *
 * Kotlin-сторона: object SummaryNative c external-функциями и интерфейсом
 * Callback { onPiece(String); onProgress(Int); isCancelled(): Boolean }.
 * Генерация синхронна и идёт в потоке вызвавшего — JNIEnv валиден в
 * колбэках без плясок с attach.
 */
#include <jni.h>
#include <stdlib.h>
#include <string.h>

#include "sf_llama.h"

typedef struct {
    JNIEnv *  env;
    jobject   cb;
    jmethodID on_piece;
    jmethodID on_progress;
    jmethodID is_cancelled;
} jni_ctx;

static void jni_on_piece(const char * piece, int len, void * ud) {
    jni_ctx * c = ud;
    char * copy = malloc(len + 1);
    memcpy(copy, piece, len);
    copy[len] = 0;
    jstring s = (*c->env)->NewStringUTF(c->env, copy);
    free(copy);
    if (s) {
        (*c->env)->CallVoidMethod(c->env, c->cb, c->on_piece, s);
        (*c->env)->DeleteLocalRef(c->env, s);
    }
}

static void jni_on_progress(int percent, void * ud) {
    jni_ctx * c = ud;
    (*c->env)->CallVoidMethod(c->env, c->cb, c->on_progress, (jint) percent);
}

static bool jni_should_stop(void * ud) {
    jni_ctx * c = ud;
    return (*c->env)->CallBooleanMethod(c->env, c->cb, c->is_cancelled);
}

JNIEXPORT jlong JNICALL Java_com_handy_voice_SummaryNative_nativeLoad(
    JNIEnv * env, jclass cls, jstring path, jint n_ctx, jint n_threads) {
    (void) cls;
    const char * p = (*env)->GetStringUTFChars(env, path, NULL);
    void * h = sf_llm_load(p, n_ctx, n_threads);
    (*env)->ReleaseStringUTFChars(env, path, p);
    return (jlong) h;
}

JNIEXPORT void JNICALL Java_com_handy_voice_SummaryNative_nativeFree(
    JNIEnv * env, jclass cls, jlong handle) {
    (void) env; (void) cls;
    sf_llm_free((void *) handle);
}

JNIEXPORT jint JNICALL Java_com_handy_voice_SummaryNative_nativeCountTokens(
    JNIEnv * env, jclass cls, jlong handle, jstring text) {
    (void) cls;
    const char * t = (*env)->GetStringUTFChars(env, text, NULL);
    int n = sf_llm_count_tokens((void *) handle, t);
    (*env)->ReleaseStringUTFChars(env, text, t);
    return n;
}

JNIEXPORT jint JNICALL Java_com_handy_voice_SummaryNative_nativeGenerate(
    JNIEnv * env, jclass cls, jlong handle, jstring system, jstring user,
    jint max_tokens, jfloat temperature, jfloat repeat_penalty, jobject callback) {
    (void) cls;
    jclass cb_cls = (*env)->GetObjectClass(env, callback);
    jni_ctx ctx = {
        .env = env,
        .cb = callback,
        .on_piece = (*env)->GetMethodID(env, cb_cls, "onPiece", "(Ljava/lang/String;)V"),
        .on_progress = (*env)->GetMethodID(env, cb_cls, "onProgress", "(I)V"),
        .is_cancelled = (*env)->GetMethodID(env, cb_cls, "isCancelled", "()Z"),
    };

    const char * sys = (*env)->GetStringUTFChars(env, system, NULL);
    const char * usr = (*env)->GetStringUTFChars(env, user, NULL);
    int rc = sf_llm_generate(
        (void *) handle, sys, usr, max_tokens, temperature, repeat_penalty,
        jni_on_piece, jni_on_progress, jni_should_stop, &ctx);
    (*env)->ReleaseStringUTFChars(env, system, sys);
    (*env)->ReleaseStringUTFChars(env, user, usr);
    return rc;
}
