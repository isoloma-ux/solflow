package com.handy.voice

/**
 * JNI-мост к libsolflow_llama — llama.cpp с узким шимом, той же сборки,
 * что на десктопе. Генерация синхронна и зовется из фонового потока
 * [MeetingService]; колбэки приходят в том же потоке.
 */
object SummaryNative {

    init {
        System.loadLibrary("solflow_llama")
    }

    interface Callback {
        fun onPiece(piece: String)
        fun onProgress(percent: Int)
        fun isCancelled(): Boolean
    }

    external fun nativeLoad(path: String, nCtx: Int, nThreads: Int): Long
    external fun nativeFree(handle: Long)
    external fun nativeCountTokens(handle: Long, text: String): Int
    external fun nativeGenerate(
        handle: Long,
        system: String,
        user: String,
        maxTokens: Int,
        temperature: Float,
        repeatPenalty: Float,
        callback: Callback,
    ): Int
}
