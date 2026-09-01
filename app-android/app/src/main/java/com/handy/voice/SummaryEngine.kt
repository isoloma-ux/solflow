package com.handy.voice

import android.content.Context
import java.io.File
import java.net.HttpURLConnection
import java.net.URL

/**
 * Названия встреч локальной языковой моделью. Саммери на телефоне НЕТ —
 * решение пользователя 2026-09-01: счёт 4B-модели по длинной расшифровке
 * занимал минуты и вешал телефон (вплоть до перезагрузки). Саммери живёт
 * на десктопе, где считает GPU; сюда его принесёт синхронизация. Название
 * же короткое: маленький контекст, четыре потока, десятки секунд.
 */
object SummaryEngine {

    const val MODEL_FILENAME = "Qwen3-4B-Q4_K_M.gguf"
    private const val MODEL_URL =
        "https://huggingface.co/unsloth/Qwen3-4B-GGUF/resolve/main/Qwen3-4B-Q4_K_M.gguf"
    const val MODEL_MB = 2440L
    private const val MODEL_BYTES = 2_497_281_408L

    private const val EXPECTED_ANSWER = 1500f

    private const val TITLE_PROMPT =
        "Тебе дают начало автоматической расшифровки записи (встреча, интервью, лекция или " +
            "заметка); в тексте бывают ошибки распознавания. Придумай короткое название " +
            "этой записи на русском: от двух до пяти слов, по сути разговора. Ответь только " +
            "самим названием — без кавычек, точки в конце и пояснений. /no_think"

    fun modelFile(context: Context): File =
        File(ModelStore.dir(context), MODEL_FILENAME)

    fun modelReady(context: Context): Boolean = modelFile(context).exists()

    /** Скачивание модели — тем же способом, что эмбеддинги диаризации. */
    fun download(context: Context, onProgress: (Int) -> Unit, isCancelled: () -> Boolean) {
        val target = modelFile(context)
        target.parentFile?.mkdirs()
        val tmp = File(target.parentFile, "$MODEL_FILENAME.part")
        val conn = (URL(MODEL_URL).openConnection() as HttpURLConnection).apply {
            instanceFollowRedirects = true
            connectTimeout = 8_000
            readTimeout = 60_000
        }
        try {
            if (conn.responseCode !in 200..299) error("сервер ответил ${conn.responseCode}")
            val total = conn.contentLengthLong.takeIf { it > 0 } ?: MODEL_BYTES
            conn.inputStream.use { input ->
                tmp.outputStream().use { output ->
                    val buf = ByteArray(1 shl 16)
                    var done = 0L
                    var lastPct = -1
                    while (true) {
                        if (isCancelled()) error("отменено")
                        val n = input.read(buf)
                        if (n < 0) break
                        output.write(buf, 0, n)
                        done += n
                        val pct = (done * 100 / total).toInt().coerceIn(0, 99)
                        if (pct != lastPct) {
                            lastPct = pct
                            onProgress(pct)
                        }
                    }
                }
            }
            if (!tmp.renameTo(target)) error("файл не переименовался")
        } finally {
            conn.disconnect()
            tmp.delete()
        }
    }

    /** Загруженная модель на время одного захода. */
    private class Llm(val handle: Long) : AutoCloseable {
        override fun close() = SummaryNative.nativeFree(handle)
    }

    private fun generate(
        llm: Llm,
        text: String,
        system: String,
        maxTokens: Int,
        slice: IntRange,
        onProgress: (Int) -> Unit,
        isCancelled: () -> Boolean,
    ): String {
        val out = StringBuilder()
        val from = slice.first
        val span = (slice.last - slice.first).toFloat()
        val rc = SummaryNative.nativeGenerate(
            llm.handle,
            system,
            "Текст:\n\n$text",
            maxTokens,
            0.4f,
            1.15f,
            object : SummaryNative.Callback {
                override fun onPiece(piece: String) {
                    out.append(piece)
                }

                override fun onProgress(percent: Int) {
                    // 0–100 — чтение текста (60% доли), 100+N — токены ответа.
                    val pct = if (percent > 100) {
                        val g = ((percent - 100) / EXPECTED_ANSWER).coerceAtMost(1f)
                        from + (span * (0.6f + 0.4f * g)).toInt()
                    } else {
                        from + (percent.coerceIn(0, 100) / 100f * span * 0.6f).toInt()
                    }
                    onProgress(pct.coerceAtMost(99))
                }

                override fun isCancelled(): Boolean = isCancelled()
            },
        )
        when (rc) {
            0 -> return out.toString().trim()
            -4 -> error("расшифровка не влезла в контекст")
            -5 -> error("отменено")
            else -> error("генерация не удалась ($rc)")
        }
    }

    /** Короткое название по началу расшифровки; пусто — не придумалось. */
    fun title(context: Context, transcriptHead: String): String {
        val model = modelFile(context)
        require(model.exists()) { "модель саммери не скачана" }
        // Потоков — четыре, а не все ядра: название считается недолго, а
        // телефон при полном заносе процессора замирал (замер пользователя).
        val handle = SummaryNative.nativeLoad(model.absolutePath, 4096, 4)
        require(handle != 0L) { "модель саммери не загрузилась" }
        val raw = Llm(handle).use { llm ->
            generate(llm, transcriptHead, TITLE_PROMPT, 96, 0..100, {}, { false })
        }
        val title = raw.lineSequence().firstOrNull().orEmpty()
            .trim()
            .trim { it in "«»\"'“”." }
            .trim()
        return if (title.length > 60) "" else title
    }

}
