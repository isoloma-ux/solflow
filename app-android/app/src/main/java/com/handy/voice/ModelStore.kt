package com.handy.voice

import android.content.Context
import java.io.File
import java.net.HttpURLConnection
import java.net.URL
import java.security.MessageDigest

/**
 * Скачанные модели и выбор активной.
 *
 * Модели не кладутся в APK: даже одна весит больше сотни мегабайт, а каталог
 * даёт выбор из шести десятков. Качаем по требованию и проверяем sha256 —
 * оборванная закачка не должна выглядеть как готовая модель.
 */
object ModelStore {

    private const val PREFS = "handy"
    private const val KEY_ACTIVE = "active_model_file"

    fun dir(context: Context): File =
        File(context.filesDir, "models").apply { mkdirs() }

    fun fileFor(context: Context, file: ModelFile): File = File(dir(context), file.filename)

    fun isDownloaded(context: Context, file: ModelFile): Boolean {
        val f = fileFor(context, file)
        return f.exists() && f.length() == file.sizeBytes
    }

    fun downloadedFilenames(context: Context): Set<String> =
        dir(context).listFiles()?.filter { it.name.endsWith(".gguf") }?.map { it.name }?.toSet()
            ?: emptySet()

    fun activeFile(context: Context): File? {
        val name = prefs(context).getString(KEY_ACTIVE, null) ?: return null
        val f = File(dir(context), name)
        return if (f.exists()) f else null
    }

    fun activeFilename(context: Context): String? = activeFile(context)?.name

    fun setActive(context: Context, filename: String) {
        prefs(context).edit().putString(KEY_ACTIVE, filename).apply()
    }

    fun delete(context: Context, file: ModelFile) {
        fileFor(context, file).delete()
        if (prefs(context).getString(KEY_ACTIVE, null) == file.filename) {
            prefs(context).edit().remove(KEY_ACTIVE).apply()
        }
    }

    /**
     * Переносит модель, лежавшую в filesDir от ранних сборок, в models/.
     * Иначе пользователь качал бы уже скачанные 174 МБ заново.
     */
    fun migrateLegacyLayout(context: Context) {
        val legacy = context.filesDir.listFiles()?.filter { it.isFile && it.name.endsWith(".gguf") }
            ?: return
        for (f in legacy) {
            val target = File(dir(context), f.name)
            if (!target.exists() && f.renameTo(target) && activeFilename(context) == null) {
                setActive(context, target.name)
            }
        }
    }

    /**
     * Качает файл, проверяет sha256 и только потом делает его видимым.
     * Источники пробуются по очереди: Hugging Face, затем зеркало Handy.
     */
    fun download(
        context: Context,
        model: CatalogModel,
        file: ModelFile,
        onProgress: (Int) -> Unit,
        isCancelled: () -> Boolean = { false },
    ): Result<File> {
        val target = fileFor(context, file)
        val tmp = File(dir(context), "${file.filename}.part")

        var lastError: Throwable? = null
        for (url in Catalog.urlsFor(context, model, file)) {
            tmp.delete()
            val attempt = runCatching { fetch(url, tmp, file, onProgress, isCancelled) }
            if (attempt.isSuccess) {
                tmp.renameTo(target)
                return Result.success(target)
            }
            lastError = attempt.exceptionOrNull()
            if (isCancelled()) break
        }
        tmp.delete()
        return Result.failure(lastError ?: IllegalStateException("источники недоступны"))
    }

    private fun fetch(
        url: String,
        tmp: File,
        file: ModelFile,
        onProgress: (Int) -> Unit,
        isCancelled: () -> Boolean,
    ) {
        val conn = (URL(url).openConnection() as HttpURLConnection).apply {
            instanceFollowRedirects = true
            // Коротко: если Hugging Face заблокирован провайдером, соединение
            // просто висит, а нам надо быстро уйти на запасное зеркало.
            connectTimeout = 8_000
            readTimeout = 60_000
        }
        try {
            if (conn.responseCode !in 200..299) {
                error("сервер ответил ${conn.responseCode}")
            }
            val digest = MessageDigest.getInstance("SHA-256")
            val total = if (conn.contentLengthLong > 0) conn.contentLengthLong else file.sizeBytes

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
                        digest.update(buf, 0, n)
                        done += n
                        val pct = (done * 100 / total).toInt().coerceIn(0, 100)
                        if (pct != lastPct) {
                            lastPct = pct
                            onProgress(pct)
                        }
                    }
                }
            }

            val got = digest.digest().joinToString("") { "%02x".format(it) }
            if (!got.equals(file.sha256, ignoreCase = true)) {
                error("контрольная сумма не совпала")
            }
        } finally {
            conn.disconnect()
        }
    }

    private fun prefs(context: Context) =
        context.getSharedPreferences(PREFS, Context.MODE_PRIVATE)
}
