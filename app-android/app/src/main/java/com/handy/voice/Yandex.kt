package com.handy.voice

import java.io.File
import java.io.InputStream
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
object Yandex : Cloud.Provider {

    private const val OAUTH = "https://oauth.yandex.ru"
    private const val DISK = "https://cloud-api.yandex.net/v1/disk"

    // Список разрешений в запросе не передаётся: Яндекс берёт те, что заданы
    // при регистрации приложения (папка приложения).

    override val id = "yandex"
    override val title = "Яндекс.Диск"

    /** Ключи заданы в сборке — без них вход невозможен, и об этом говорится словами. */
    override val configured: Boolean
        get() = BuildConfig.YANDEX_CLIENT_ID.isNotEmpty() && BuildConfig.YANDEX_CLIENT_SECRET.isNotEmpty()

    private fun request(
        method: String,
        url: String,
        token: String? = null,
        contentType: String? = null,
        body: ByteArray? = null,
        stream: InputStream? = null,
        streamLength: Long = -1,
        target: File? = null,
    ): Cloud.Reply = Cloud.http(
        method, url, token?.let { "OAuth $it" }, contentType, body, stream, streamLength, target,
    )

    private fun enc(s: String) = Cloud.enc(s)

    private fun postForm(url: String, vararg fields: Pair<String, String>) =
        request("POST", url, contentType = "application/x-www-form-urlencoded", body = Cloud.form(*fields))

    private fun describe(reply: Cloud.Reply, what: String) = Cloud.describe(reply, what)

    // --- OAuth --------------------------------------------------------------

    override fun deviceCode(deviceName: String, deviceId: String): Cloud.DeviceCode {
        if (!configured) error("ключи Яндекс OAuth не заданы в этой сборке")
        val reply = postForm(
            "$OAUTH/device/code",
            "client_id" to BuildConfig.YANDEX_CLIENT_ID,
            "device_id" to deviceId,
            "device_name" to deviceName,
        )
        if (!reply.ok) throw describe(reply, "Яндекс не дал код")
        val j = reply.json()
        return Cloud.DeviceCode(
            deviceCode = j.getString("device_code"),
            userCode = j.getString("user_code"),
            verificationUrl = j.optString("verification_url").ifBlank { "https://oauth.yandex.ru/device" },
            interval = j.optLong("interval", 5).coerceAtLeast(2),
            expiresAt = System.currentTimeMillis() + j.optLong("expires_in", 300) * 1000,
        )
    }

    override fun pollToken(code: Cloud.DeviceCode): Cloud.Poll {
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
                "authorization_pending", "slow_down" -> Cloud.Poll.Pending
                "expired_token" -> error("код устарел — запросите новый")
                "access_denied" -> error("вы отказали приложению в доступе")
                else -> throw describe(reply, "вход не удался")
            }
        }
        return Cloud.Poll.Done(parseTokens(j, null))
    }

    override fun refresh(refreshToken: String): Cloud.Tokens {
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

    private fun parseTokens(j: JSONObject, oldRefresh: String?): Cloud.Tokens = Cloud.Tokens(
        access = j.optString("access_token").ifBlank { error("в ответе нет токена") },
        refresh = j.optString("refresh_token").ifBlank { oldRefresh.orEmpty() },
        expiresAt = System.currentTimeMillis() + j.optLong("expires_in", 365L * 86_400) * 1000,
    )

    /** Отзыв токена при выходе; ошибка не страшна — локально он всё равно стирается. */
    override fun revoke(token: String) {
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
    fun list(token: String, path: String): List<Cloud.RemoteFile> {
        val page = 500
        val out = mutableListOf<Cloud.RemoteFile>()
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
                out += Cloud.RemoteFile(
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

    // --- провайдер ------------------------------------------------------------

    private const val REMOTE_MEETINGS = "app:/meetings"
    private const val REMOTE_AUDIO = "app:/audio"

    private fun path(folder: Cloud.Folder, name: String) = when (folder) {
        Cloud.Folder.MEETINGS -> "$REMOTE_MEETINGS/$name"
        Cloud.Folder.AUDIO -> "$REMOTE_AUDIO/$name"
    }

    override fun account(token: String): String = diskLogin(token)

    override fun prepare(token: String) {
        mkdir(token, "app:/")
        mkdir(token, REMOTE_MEETINGS)
        mkdir(token, REMOTE_AUDIO)
    }

    override fun list(token: String, folder: Cloud.Folder): List<Cloud.RemoteFile> =
        list(token, if (folder == Cloud.Folder.MEETINGS) REMOTE_MEETINGS else REMOTE_AUDIO)

    override fun upload(token: String, folder: Cloud.Folder, name: String, bytes: ByteArray) =
        upload(token, path(folder, name), bytes)

    override fun uploadFile(token: String, folder: Cloud.Folder, name: String, file: File) =
        uploadFile(token, path(folder, name), file)

    override fun download(token: String, folder: Cloud.Folder, name: String): ByteArray =
        download(token, path(folder, name))

    override fun downloadFile(token: String, folder: Cloud.Folder, name: String, target: File) =
        downloadFile(token, path(folder, name), target)

    override fun delete(token: String, folder: Cloud.Folder, name: String) =
        delete(token, path(folder, name))
}
