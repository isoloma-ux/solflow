// Урезанная копия kotlin-api/Speaker.kt из sherpa-onnx: оставлен только
// SpeakerEmbeddingExtractor — он нужен для сшивки говорящих между окнами
// диаризации. SpeakerEmbeddingManager и обвязка под assets выброшены.
package com.k2fsa.sherpa.onnx

import android.content.res.AssetManager

class SpeakerEmbeddingExtractor(
    assetManager: AssetManager? = null,
    config: SpeakerEmbeddingExtractorConfig,
) {
    private var ptr: Long

    init {
        ptr = if (assetManager != null) {
            newFromAsset(assetManager, config)
        } else {
            newFromFile(config)
        }
        require(ptr != 0L) {
            "Invalid SpeakerEmbeddingExtractorConfig: failed to create native SpeakerEmbeddingExtractor"
        }
    }

    protected fun finalize() {
        if (ptr != 0L) {
            delete(ptr)
            ptr = 0
        }
    }

    fun release() = finalize()

    fun createStream(): OnlineStream {
        val p = createStream(ptr)
        return OnlineStream(p)
    }

    fun isReady(stream: OnlineStream) = isReady(ptr, stream.ptr)
    fun compute(stream: OnlineStream) = compute(ptr, stream.ptr)
    fun dim() = dim(ptr)

    private external fun newFromAsset(
        assetManager: AssetManager,
        config: SpeakerEmbeddingExtractorConfig,
    ): Long

    private external fun newFromFile(
        config: SpeakerEmbeddingExtractorConfig,
    ): Long

    private external fun delete(ptr: Long)

    private external fun createStream(ptr: Long): Long

    private external fun isReady(ptr: Long, streamPtr: Long): Boolean

    private external fun compute(ptr: Long, streamPtr: Long): FloatArray

    private external fun dim(ptr: Long): Int

    companion object {
        init {
            System.loadLibrary("sherpa-onnx-jni")
        }
    }
}
