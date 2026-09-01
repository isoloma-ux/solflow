package com.handy.voice

import java.io.File
import java.io.InputStream
import java.net.HttpURLConnection
import java.net.URL
import java.net.URLEncoder
import java.time.OffsetDateTime
import org.json.JSONObject

/**
 * Яндекс: вход по коду устройства и REST Диска. Один в один с yandex.rs на
 * десктопе — протокол общий, различаются только библиотеки.
 *
 * Вход — «как на телевизоре»: приложение получает короткий код, человек
 * вводит его на oauth.yandex.ru/device в любом браузере, приложение
 * опрашивает Яндекс и забирает токен. Ни редиректов, ни перехвата ссылок.
 *
 * Диск — только папка приложения (`app:/`, на Диске «Приложения/Sol Flow»):
 * остальные файлы человека приложению недоступны.
 */
object Yandex {

    private const val OAUTH = "https://oauth.yandex.ru"
    private const val DISK = "https://cloud-api.yandex.net/v1/disk"

    // Список разрешений в запросе не передаётся: Яндекс берёт те, что заданы
    // при регистрации приложения (папка приложения).

    /** Ключи заданы в сборке — без них вход невозможен, и об этом говорится словами. */
    val configured: Boolean
        get() = BuildConfig.YANDEX_CLIENT_ID.isNotEmpty() && BuildConfig.YANDEX_CLIENT_SECRET.isNotEmpty()

    /** Токен не подошёл — нужно войти заново, а не пробовать ещё раз. */
    class Unauthorized(message: String) : Exception(message)

    data class DeviceCode(
        val deviceCode: String,
        val userCode: String,
        val verificationUrl: String,
        /** Не чаще, чем раз в столько секунд, спрашивать про токен. */
        val interval: Long,
        /** Момент (millis), после которого код протухает. */
        val expiresAt: Long,
    )

    data class Tokens(val access: String, val refresh: String, val expiresAt: Long)

    data class RemoteFile(val name: String, val md5: String, val modified: Long, val size: Long)

    sealed class Poll {
        object Pending : Poll()
        class Done(val tokens: Tokens) : Poll()
    }

    // --- HTTP ---------------------------------------------------------------

    private class Reply(val status: Int, val body: ByteArray) {
        val ok get() = status in 200..299
        fun json() = JSONObject(String(body))
        fun text() = String(body)
    }

    private fun request(
        method: String,
        url: String,
        token: String? = null,
        contentType: String? = null,
        body: ByteArray? = null,
        stream: InputStream? = null,
        streamLength: Long = -1,
        target: File? = null,
    ): Reply {
        val conn = (URL(url).openConnection() as HttpURLConnection).apply {
            requestMethod = method
            instanceFollowRedirects = true
            connectTimeout = 15_000
            readTimeout = 60_000
            setRequestProperty("User-Agent", "SolFlow")
            if (token != null) setRequestProperty("Authorization", "OAuth $token")
            if (contentType != null) setRequestProperty("Content-Type", contentType)
        }
        try {
            if (body != null) {
                conn.doOutput = true
                conn.setFixedLengthStreamingMode(body.size)
                conn.outputStream.use { it.write(body) }
            } else if (stream != null) {
                conn.doOutput = true
                if (streamLength >= 0) conn.setFixedLengthStreamingMode(streamLength)
                else conn.setChunkedStreamingMode(1 shl 16)
                conn.outputStream.use { out -> stream.use { it.copyTo(out, 1 shl 16) } }
            }
            val status = conn.responseCode
            val input = if (status in 200..299) conn.inputStream else conn.errorStream
            if (target != null && status in 200..299) {
                // Пишем рядом и переименовываем по завершении: оборванная
                // загрузка не должна выглядеть готовым файлом.
                val part = File(target.path + ".part")
                try {
                    input.use { i -> part.outputStream().use { o -> i.copyTo(o, 1 shl 16) } }
                    if (!part.renameTo(target)) error("не удалось сохранить ${target.name}")
                } catch (e: Exception) {
                    part.delete()
                    throw e
                }
                return Reply(status, ByteArray(0))
            }
            val bytes = input?.use { it.readBytes() } ?: ByteArray(0)
            return Reply(status, bytes)
        } finally {
            conn.disconnect()
        }
    }

    private fun enc(s: String) = URLEncoder.encode(s, "UTF-8")

    private fun form(vararg fields: Pair<String, String>) =
        fields.joinToString("&") { (k, v) -> "${enc(k)}=${enc(v)}" }.toByteArray()

    private fun postForm(url: String, vararg fields: Pair<String, String>) =
        request("POST", url, contentType = "application/x-www-form-urlencoded", body = form(*fields))

    /** Ошибка Яндекса человеческим текстом: в теле обычно есть message. */
    private fun describe(reply: Reply, what: String): Exception {
        val detail = runCatching {
            val j = reply.json()
            listOf("message", "error_description", "description", "error")
                .firstNotNullOfOrNull { k -> j.optString(k).takeIf { it.isNotBlank() } }
        }.getOrNull() ?: reply.text().take(120)
        val text = "$what: ${detail.trim()} (${reply.status})"
        return if (reply.status == 401) Unauthorized(text) else Exception(text)
    }

    // --- OAuth --------------------------------------------------------------

    fun deviceCode(deviceName: String, deviceId: String): DeviceCode {
        if (!configured) error("ключи Яндекс OAuth не заданы в этой сборке")
        val reply = postForm(
            "$OAUTH/device/code",
            "client_id" to BuildConfig.YANDEX_CLIENT_ID,
            "device_id" to deviceId,
            "device_name" to deviceName,
        )
        if (!reply.ok) throw describe(reply, "Яндекс не дал код")
        val j = reply.json()
        return DeviceCode(
            deviceCode = j.getString("device_code"),
            userCode = j.getString("user_code"),
            verificationUrl = j.optString("verification_url").ifBlank { "https://oauth.yandex.ru/device" },
            interval = j.optLong("interval", 5).coerceAtLeast(2),
            expiresAt = System.currentTimeMillis() + j.optLong("expires_in", 300) * 1000,
        )
    }

    fun pollToken(code: DeviceCode): Poll {
        val reply = postForm(
            "$OAUTH/token",
            "grant_type" to "device_code",
            "code" to code.deviceCode,
            "client_id" to BuildConfig.YANDEX_CLIENT_ID,
            "client_secret" to BuildConfig.YANDEX_CLIENT_SECRET,
        )
        val j = runCatching { reply.json() }.getOrNull() ?: JSONObject()
        if (!reply.ok) {
            return when (j.optString("error")) {
                "authorization_pending", "slow_down" -> Poll.Pending
                "expired_token" -> error("код устарел — запросите новый")
                "access_denied" -> error("вы отказали приложению в доступе")
                else -> throw describe(reply, "вход не удался")
            }
        }
        return Poll.Done(parseTokens(j, null))
    }

    fun refresh(refreshToken: String): Tokens {
        val reply = postForm(
            "$OAUTH/token",
            "grant_type" to "refresh_token",
            "refresh_token" to refreshToken,
            "client_id" to BuildConfig.YANDEX_CLIENT_ID,
            "client_secret" to BuildConfig.YANDEX_CLIENT_SECRET,
        )
        if (!reply.ok) throw describe(reply, "не удалось продлить вход")
        return parseTokens(reply.json(), refreshToken)
    }

    private fun parseTokens(j: JSONObject, oldRefresh: String?): Tokens = Tokens(
        access = j.optString("access_token").ifBlank { error("в ответе нет токена") },
        refresh = j.optString("refresh_token").ifBlank { oldRefresh.orEmpty() },
        expiresAt = System.currentTimeMillis() + j.optLong("expires_in", 365L * 86_400) * 1000,
    )

    /** Отзыв токена при выходе; ошибка не страшна — локально он всё равно стирается. */
    fun revoke(token: String) {
        runCatching {
            postForm(
                "$OAUTH/revoke_token",
                "access_token" to token,
                "client_id" to BuildConfig.YANDEX_CLIENT_ID,
                "client_secret" to BuildConfig.YANDEX_CLIENT_SECRET,
            )
        }
    }

    // --- Диск ---------------------------------------------------------------

    /** Кто вошёл — для строки в настройках. */
    fun diskLogin(token: String): String {
        val reply = request("GET", "$DISK/?fields=user", token)
        if (!reply.ok) throw describe(reply, "Диск не ответил")
        val user = reply.json().optJSONObject("user") ?: return ""
        return user.optString("display_name").ifBlank { user.optString("login") }
    }

    /** Создать папку; «уже есть» — не ошибка. */
    fun mkdir(token: String, path: String) {
        val reply = request("PUT", "$DISK/resources?path=${enc(path)}", token, body = ByteArray(0))
        if (!reply.ok && reply.status != 409) throw describe(reply, "не удалось создать папку $path")
    }

    /** Все файлы папки, постранично. Нет папки — пустой список. */
    fun list(token: String, path: String): List<RemoteFile> {
        val page = 500
        val out = mutableListOf<RemoteFile>()
        var offset = 0
        while (true) {
            val url = "$DISK/resources?path=${enc(path)}&limit=$page&offset=$offset" +
                "&fields=_embedded.total,_embedded.items.name,_embedded.items.type," +
                "_embedded.items.md5,_embedded.items.modified,_embedded.items.size"
            val reply = request("GET", url, token)
            if (reply.status == 404) return out
            if (!reply.ok) throw describe(reply, "не удалось прочитать $path")
            val embedded = reply.json().optJSONObject("_embedded") ?: return out
            val total = embedded.optInt("total")
            val items = embedded.optJSONArray("items") ?: return out
            for (i in 0 until items.length()) {
                val item = items.getJSONObject(i)
                if (item.optString("type") != "file") continue
                out += RemoteFile(
                    name = item.optString("name"),
                    md5 = item.optString("md5"),
                    modified = parseIso8601(item.optString("modified")),
                    size = item.optLong("size"),
                )
            }
            offset += items.length()
            if (items.length() == 0 || offset >= total) return out
        }
    }

    private fun uploadHref(token: String, path: String): String {
        val reply = request("GET", "$DISK/resources/upload?path=${enc(path)}&overwrite=true", token)
        if (!reply.ok) throw describe(reply, "Диск не принял $path")
        return reply.json().optString("href").ifBlank { error("Диск не дал адрес для загрузки") }
    }

    private fun downloadHref(token: String, path: String): String {
        val reply = request("GET", "$DISK/resources/download?path=${enc(path)}", token)
        if (!reply.ok) throw describe(reply, "Диск не отдал $path")
        return reply.json().optString("href").ifBlank { error("Диск не дал адрес для скачивания") }
    }

    fun upload(token: String, path: String, bytes: ByteArray) {
        val reply = request(
            "PUT", uploadHref(token, path),
            contentType = "application/octet-stream", body = bytes,
        )
        if (!reply.ok) throw describe(reply, "не удалось загрузить $path")
    }

    /** Большой файл — потоком с диска, в память не поднимаем. */
    fun uploadFile(token: String, path: String, file: File) {
        val reply = request(
            "PUT", uploadHref(token, path),
            contentType = "application/octet-stream",
            stream = file.inputStream(), streamLength = file.length(),
        )
        if (!reply.ok) throw describe(reply, "не удалось загрузить $path")
    }

    fun download(token: String, path: String): ByteArray {
        val reply = request("GET", downloadHref(token, path))
        if (!reply.ok) throw describe(reply, "не удалось скачать $path")
        return reply.body
    }

    fun downloadFile(token: String, path: String, target: File) {
        val reply = request("GET", downloadHref(token, path), target = target)
        if (!reply.ok) throw describe(reply, "не удалось скачать $path")
    }

    /** Удаление без корзины; «уже нет» — не ошибка. */
    fun delete(token: String, path: String) {
        val reply = request("DELETE", "$DISK/resources?path=${enc(path)}&permanently=true", token)
        if (!reply.ok && reply.status != 404) throw describe(reply, "не удалось удалить $path")
    }

    /** «2024-05-01T12:34:56+00:00» → millis; мусор — ноль. */
    fun parseIso8601(s: String): Long =
        runCatching { OffsetDateTime.parse(s).toInstant().toEpochMilli() }.getOrDefault(0L)
}
