package com.handy.voice

import android.content.Context
import java.io.File
import java.io.InputStream
import java.net.HttpURLConnection
import java.net.URL
import java.net.URLEncoder

/**
 * Облако как набор операций: вход по коду, продление и отзыв токена, папки,
 * список, загрузка, скачивание, удаление. [SyncEngine] и [SyncManager]
 * ходят только через [Provider], поэтому Яндекс.Диск и Google Drive для
 * них неотличимы — раскладка файлов и слияние одни и те же. Один в один с
 * sync/provider.rs на десктопе.
 */
object Cloud {

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

    /** Две папки приложения в облаке. */
    enum class Folder { MEETINGS, AUDIO }

    interface Provider {
        /** "yandex" или "google" — так провайдер записан в настройках. */
        val id: String
        /** Название для экрана. */
        val title: String
        /** Ключи заданы в сборке — без них вход невозможен. */
        val configured: Boolean

        fun deviceCode(deviceName: String, deviceId: String): DeviceCode
        fun pollToken(code: DeviceCode): Poll
        fun refresh(refreshToken: String): Tokens
        fun revoke(token: String)
        /** Кто вошёл — логин или почта для настроек. */
        fun account(token: String): String

        /** Папки приложения на месте (создаются, если нет). */
        fun prepare(token: String)
        fun list(token: String, folder: Folder): List<RemoteFile>
        fun upload(token: String, folder: Folder, name: String, bytes: ByteArray)
        fun uploadFile(token: String, folder: Folder, name: String, file: File)
        fun download(token: String, folder: Folder, name: String): ByteArray
        fun downloadFile(token: String, folder: Folder, name: String, target: File)
        fun delete(token: String, folder: Folder, name: String)
    }

    val all: List<Provider> get() = listOf(Yandex, GoogleDrive)

    fun byId(id: String?): Provider = all.firstOrNull { it.id == id } ?: Yandex

    /** Подключённое облако; пустая настройка при токене — Яндекс (до Google). */
    fun current(context: Context): Provider = byId(AppPrefs.syncProvider(context))

    // --- HTTP, общий для обоих клиентов -----------------------------------------

    class Reply(val status: Int, val body: ByteArray) {
        val ok get() = status in 200..299
        fun json() = org.json.JSONObject(String(body))
        fun text() = String(body)
    }

    fun enc(s: String): String = URLEncoder.encode(s, "UTF-8")

    fun form(vararg fields: Pair<String, String>): ByteArray =
        fields.joinToString("&") { (k, v) -> "${enc(k)}=${enc(v)}" }.toByteArray()

    /**
     * Запрос как есть: код и тело. [auth] — готовое значение заголовка
     * Authorization («OAuth …» у Яндекса, «Bearer …» у Google). [target] —
     * писать тело в файл рядом и переименовать по завершении.
     */
    fun http(
        method: String,
        url: String,
        auth: String? = null,
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
            if (auth != null) setRequestProperty("Authorization", auth)
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

    /** Ошибка облака человеческим текстом; 401 — отдельный тип. */
    fun describe(reply: Reply, what: String): Exception {
        val detail = runCatching {
            val j = reply.json()
            listOf("message", "error_description", "description", "error")
                .firstNotNullOfOrNull { k -> j.optString(k).takeIf { it.isNotBlank() } }
        }.getOrNull() ?: reply.text().take(120)
        val text = "$what: ${detail.trim()} (${reply.status})"
        return if (reply.status == 401) Unauthorized(text) else Exception(text)
    }
}
