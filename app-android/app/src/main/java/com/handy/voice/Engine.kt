package com.handy.voice

import android.content.Context
import android.util.Log
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.Job
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.delay
import kotlinx.coroutines.launch
import kotlinx.coroutines.sync.Mutex
import kotlinx.coroutines.sync.withLock

/**
 * Одна загруженная модель на всё приложение.
 *
 * Экран диктовки и плавающая кнопка работают с одним и тем же движком:
 * модель весит под двести мегабайт, держать её в памяти дважды нельзя,
 * а грузить заново при каждом переключении — это секунды ожидания.
 */
object Engine {

    private const val TAG = "HandyVoice"

    private val lock = Mutex()
    private var transcriber: Transcriber? = null
    private var loadedFilename: String? = null

    private val scope = CoroutineScope(SupervisorJob() + Dispatchers.Default)
    private var unloadJob: Job? = null

    val currentModel: String? get() = loadedFilename

    /** Возвращает true, если активная модель загружена и готова. */
    suspend fun ensureLoaded(context: Context): Boolean {
        // Пока модель нужна, отложенная выгрузка отменяется — иначе она могла
        // бы сработать между загрузкой и распознаванием.
        cancelUnload()
        return load(context)
    }

    private suspend fun load(context: Context): Boolean = lock.withLock {
        val active = ModelStore.activeFile(context) ?: run {
            release()
            return@withLock false
        }
        if (active.name == loadedFilename && transcriber != null) return@withLock true

        release()
        val opened = Transcriber.open(active)
        if (opened == null) {
            Log.e(TAG, "не удалось открыть модель ${active.name}")
            return@withLock false
        }
        transcriber = opened
        loadedFilename = active.name
        true
    }

    /**
     * Распознаёт запись целиком, разбивая её по паузам, если она длиннее
     * окна модели. Куски склеиваются через пробел.
     */
    suspend fun transcribe(
        pcm: FloatArray,
        sampleRate: Int,
        dropParasites: Boolean = false,
    ): String = lock.withLock {
        val engine = transcriber ?: return@withLock ""
        val raw = Segmenter.split(pcm, sampleRate)
            .map { engine.transcribe(it).trim() }
            .filter { it.isNotEmpty() }
            .joinToString(" ")
        // Чистка после склейки, а не для каждого куска: повторы и лишние
        // пробелы возникают в том числе на стыках.
        TextCleanup.clean(raw, dropParasites)
    }

    /**
     * Распознаёт один готовый кусок без разбивки и чистки — для расшифровки
     * встреч, где куски нарезаются заранее по файлу. Замок берётся на каждый
     * кусок отдельно, чтобы диктовка могла вклиниться между ними и не ждать
     * все десять минут расшифровки.
     */
    suspend fun transcribeSegment(pcm: FloatArray): String = lock.withLock {
        transcriber?.transcribe(pcm)?.trim().orEmpty()
    }

    suspend fun unload() = lock.withLock { release() }

    /**
     * Заводит отложенную выгрузку по настройке «держать модель в памяти».
     * Вызывается после работы, а не после загрузки: загруженная модель
     * отвечает мгновенно, но держит под двести мегабайт, и решать, чем
     * жертвовать, — дело пользователя.
     */
    fun scheduleUnload(context: Context) {
        val after = AppPrefs.unloadAfterMs(context)
        cancelUnload()
        if (after == null) return
        unloadJob = scope.launch {
            if (after > 0) delay(after)
            unload()
        }
    }

    private fun cancelUnload() {
        unloadJob?.cancel()
        unloadJob = null
    }

    private fun release() {
        transcriber?.close()
        transcriber = null
        loadedFilename = null
    }
}
