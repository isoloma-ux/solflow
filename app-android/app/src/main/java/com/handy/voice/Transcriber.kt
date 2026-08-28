package com.handy.voice

import java.io.File

/**
 * Обёртка над transcribe.cpp. Сессия держит загруженную модель, поэтому её
 * создают один раз и переиспользуют — повторная загрузка GGUF стоит секунды.
 */
class Transcriber private constructor(private var handle: Long) : AutoCloseable {

    fun transcribe(pcm: FloatArray): String = nativeRun(handle, pcm)

    override fun close() {
        if (handle != 0L) {
            nativeClose(handle)
            handle = 0L
        }
    }

    private external fun nativeOpen(modelPath: String): Long
    private external fun nativeRun(handle: Long, pcm: FloatArray): String
    private external fun nativeClose(handle: Long)

    companion object {
        init {
            System.loadLibrary("handyvoice")
        }

        @JvmStatic
        external fun nativeVersion(): String

        /** Возвращает null, если модель не загрузилась. */
        fun open(model: File): Transcriber? {
            val stub = Transcriber(0L)
            val h = stub.nativeOpen(model.absolutePath)
            return if (h == 0L) null else Transcriber(h)
        }
    }
}
