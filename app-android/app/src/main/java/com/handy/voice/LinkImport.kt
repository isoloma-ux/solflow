package com.handy.voice

import android.net.Uri
import java.io.File
import java.net.HttpURLConnection
import java.net.URL
import java.net.URLEncoder

/**
 * Расшифровка по ссылке.
 *
 * На Mac этим занимается yt-dlp — отдельная программа на Python, которой на
 * телефоне нет и не будет. Поэтому здесь честно поддержаны два случая,
 * которые работают без чужих зависимостей: прямая ссылка на файл и
 * публичная ссылка Яндекс.Диска (у него открытый API, авторизация не нужна).
 *
 * YouTube и VK отдают видео только через свои внутренние протоколы —
 * говорим об этом прямо, а не роняем закачку с непонятной ошибкой.
 */
object LinkImport {

    private const val YANDEX_API =
        "https://cloud-api.yandex.net/v1/disk/public/resources/download?public_key="

    /** Сообщение о том, почему ссылка не годится; null — годится. */
    fun unsupportedReason(context: android.content.Context, url: String): String? {
        val host = runCatching { Uri.parse(url.trim()).host.orEmpty() }.getOrDefault("")
        return when {
            !url.trim().startsWith("http") -> context.getString(R.string.link_not_url)
            host.contains("youtube.") || host.contains("youtu.be") || host.contains("vk.com") ||
                host.contains("vkvideo.") || host.contains("rutube.") ->
                context.getString(R.string.link_unsupported_host)
            else -> null
        }
    }

    private fun isYandexDisk(url: String): Boolean {
        val host = runCatching { Uri.parse(url).host.orEmpty() }.getOrDefault("")
        return host.contains("disk.yandex") || host.contains("yadi.sk")
    }

    /**
     * Превращает публичную ссылку Яндекс.Диска в прямую. Остальные ссылки
     * возвращаются как есть.
     */
    private fun resolve(url: String): String {
        if (!isYandexDisk(url)) return url
        val api = YANDEX_API + URLEncoder.encode(url, "UTF-8")
        val conn = (URL(api).openConnection() as HttpURLConnection).apply {
            instanceFollowRedirects = true
            connectTimeout = 10_000
            readTimeout = 15_000
            setRequestProperty("User-Agent", "SolFlow")
        }
        try {
            if (conn.responseCode !in 200..299) error("Яндекс.Диск ответил ${conn.responseCode}")
            val json = org.json.JSONObject(conn.inputStream.bufferedReader().readText())
            return json.optString("href").takeIf { it.isNotBlank() }
                ?: error("Яндекс.Диск не дал ссылку на файл")
        } finally {
            conn.disconnect()
        }
    }

    /**
     * Качает файл во временный [target]. Бросает исключение с человеческим
     * текстом: сообщение уходит в уведомление, и «HTTP 403» там бесполезно.
     */
    fun download(
        url: String,
        target: File,
        onProgress: (Int) -> Unit,
        isCancelled: () -> Boolean,
    ) {
        val direct = resolve(url.trim())
        val conn = (URL(direct).openConnection() as HttpURLConnection).apply {
            instanceFollowRedirects = true
            connectTimeout = 10_000
            readTimeout = 30_000
            setRequestProperty("User-Agent", "SolFlow")
        }
        try {
            if (conn.responseCode !in 200..299) error("сервер ответил ${conn.responseCode}")
            // По ссылке нередко лежит страница, а не файл: качать её незачем,
            // декодер всё равно ничего в ней не найдёт.
            val type = conn.contentType.orEmpty()
            if (type.startsWith("text/html")) error("по ссылке страница, а не файл")

            val total = conn.contentLengthLong
            conn.inputStream.use { input ->
                target.outputStream().use { output ->
                    val buf = ByteArray(1 shl 16)
                    var done = 0L
                    var lastPct = -1
                    while (true) {
                        if (isCancelled()) error("отменено")
                        val n = input.read(buf)
                        if (n < 0) break
                        output.write(buf, 0, n)
                        done += n
                        if (total > 0) {
                            val pct = (done * 100 / total).toInt().coerceIn(0, 100)
                            if (pct != lastPct) {
                                lastPct = pct
                                onProgress(pct)
                            }
                        }
                    }
                }
            }
            if (target.length() == 0L) error("файл оказался пустым")
        } finally {
            conn.disconnect()
        }
    }
}
