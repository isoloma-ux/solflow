package com.handy.voice

import android.content.Context
import java.io.File
import java.net.HttpURLConnection
import java.net.URL

/**
 * Саммери и названия встреч локальной языковой моделью — та же логика,
 * промпты и параметры, что на десктопе (desktop/src-tauri/src/summary.rs):
 * длинные расшифровки режутся на куски по бюджету токенов, куски сводятся
 * финальным проходом, модель грузится один раз на весь заход.
 */
object SummaryEngine {

    const val MODEL_FILENAME = "Qwen3-4B-Q4_K_M.gguf"
    private const val MODEL_URL =
        "https://huggingface.co/unsloth/Qwen3-4B-GGUF/resolve/main/Qwen3-4B-Q4_K_M.gguf"
    const val MODEL_MB = 2440L
    private const val MODEL_BYTES = 2_497_281_408L

    /** Контекст скромнее десктопного: телефону память дороже. */
    private const val N_CTX = 12288
    private const val PART_BUDGET = 9000
    private const val MAX_TOKENS = 2000
    private const val PART_TOKENS = 1600
    private const val MERGE_TOKENS = 3500
    private const val EXPECTED_ANSWER = 1500f

    private const val SYSTEM_PROMPT =
        "Ты помощник, который делает саммери рабочих встреч и интервью. Тебе дают " +
            "автоматическую расшифровку — в ней бывают ошибки распознавания и нет знаков " +
            "различия говорящих.\n\nСоставь саммери на русском строго в таком виде:\n\n" +
            "## О чем говорили\n- от четырех до восьми пунктов, пропорционально длине и " +
            "насыщенности разговора; каждый пункт — конкретная мысль или вывод, с именами и " +
            "цифрами, если они прозвучали\n\n## Решения\n- что решили; если явных решений не " +
            "было, напиши: «Явных решений не зафиксировано»\n\n## Задачи\n- кто что делает " +
            "дальше, если это прозвучало; если нет — «Задачи не проговаривались»\n\n" +
            "Правила: пиши только то, что есть в расшифровке; не выдумывай имена, цифры и " +
            "факты; не цитируй длинные куски; после раздела «Задачи» ничего не добавляй."

    private const val PART_PROMPT =
        "Тебе дают фрагмент автоматической расшифровки длинного разговора (встречи или " +
            "интервью); в тексте бывают ошибки распознавания. Выпиши ключевые пункты этого " +
            "фрагмента: от шести до десяти, каждый — конкретная мысль, факт, договорённость " +
            "или вывод, с именами и цифрами, если они прозвучали. Если во фрагменте были " +
            "решения или поставленные задачи — добавь их отдельными строками «Решение: …» и " +
            "«Задача: …». Пиши только по содержанию фрагмента, ничего не выдумывай и не " +
            "добавляй выводов от себя."

    private const val MERGE_PROMPT =
        "Ниже — ключевые пункты последовательных частей одного длинного разговора. Собери " +
            "из них подробный конспект на русском строго в таком виде:\n\n## Главное\n" +
            "- три-пять предложений: о чём разговор в целом и к чему пришли\n\n" +
            "## Ключевые пункты\nсгруппируй пункты по темам; каждая тема — подзаголовок " +
            "«### …» и от двух до пяти пунктов под ним; сохрани конкретику (имена, цифры, " +
            "договорённости) и не выбрасывай темы\n\n## Решения\n- все решения из частей; " +
            "если их нет — «Явных решений не зафиксировано»\n\n## Задачи\n- все задачи из " +
            "частей; если их нет — «Задачи не проговаривались»\n\nЭто подробный конспект, а " +
            "не краткая аннотация: не сжимай всё до трёх пунктов. Ничего не выдумывай и не " +
            "добавляй от себя."

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

    /** Полное саммери с нарезкой длинного текста на куски. */
    fun summarize(
        context: Context,
        transcript: String,
        onProgress: (Int) -> Unit,
        isCancelled: () -> Boolean,
    ): String {
        val model = modelFile(context)
        require(model.exists()) { "модель саммери не скачана" }
        val handle = SummaryNative.nativeLoad(model.absolutePath, N_CTX, 0)
        require(handle != 0L) { "модель саммери не загрузилась" }
        Llm(handle).use { llm ->
            val tokens = SummaryNative.nativeCountTokens(llm.handle, transcript)
            require(tokens >= 0) { "текст не токенизировался" }
            val partsN = ((tokens + PART_BUDGET - 1) / PART_BUDGET).coerceAtLeast(1)

            if (partsN == 1) {
                return generate(
                    llm, transcript, SYSTEM_PROMPT, MAX_TOKENS, 0..100,
                    onProgress, isCancelled,
                )
            }

            val parts = splitInto(transcript, partsN)
            val slice = 100 / (partsN + 1)
            val partials = parts.mapIndexed { i, part ->
                val from = slice * i
                generate(
                    llm, part, PART_PROMPT, PART_TOKENS, from..(from + slice),
                    onProgress, isCancelled,
                )
            }
            return generate(
                llm, partials.joinToString("\n\n---\n\n"), MERGE_PROMPT, MERGE_TOKENS,
                (slice * partsN)..100, onProgress, isCancelled,
            )
        }
    }

    /** Короткое название по началу расшифровки; пусто — не придумалось. */
    fun title(context: Context, transcriptHead: String): String {
        val model = modelFile(context)
        require(model.exists()) { "модель саммери не скачана" }
        val handle = SummaryNative.nativeLoad(model.absolutePath, 4096, 0)
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

    /** Режет текст на n примерно равных кусков по границам предложений. */
    private fun splitInto(text: String, n: Int): List<String> {
        val parts = mutableListOf<String>()
        val step = text.length / n
        var start = 0
        for (i in 1 until n) {
            var cut = (step * i).coerceAtLeast(start + 1).coerceAtMost(text.length)
            val dot = text.indexOf(". ", cut)
            if (dot >= 0) cut = dot + 1
            if (cut in (start + 1) until text.length) {
                parts.add(text.substring(start, cut))
                start = cut
            }
        }
        parts.add(text.substring(start))
        return parts
    }
}
