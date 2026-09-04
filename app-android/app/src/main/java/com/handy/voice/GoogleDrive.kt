package com.handy.voice

import java.io.File
import org.json.JSONObject

/**
 * Google Drive: вход по коду устройства и REST Drive v3. Один в один с
 * sync/google.rs на десктопе: папка «Sol Flow» в корне Диска, внутри
 * «meetings» и «audio»; разрешение только drive.file — приложение видит
 * лишь файлы, которые само создало.
 */
object GoogleDrive : Cloud.Provider {

    private const val OAUTH = "https://oauth2.googleapis.com"
    private const val DRIVE = "https://www.googleapis.com/drive/v3"
    private const val UPLOAD = "https://www.googleapis.com/upload/drive/v3/files"
    private const val SCOPE = "https://www.googleapis.com/auth/drive.file"
    private const val FOLDER_MIME = "application/vnd.google-apps.folder"
    private const val ROOT_NAME = "Sol Flow"

    override val id = "google"
    override val title = "Google Drive"
    override val configured: Boolean
        get() = BuildConfig.GOOGLE_CLIENT_ID.isNotEmpty() && BuildConfig.GOOGLE_CLIENT_SECRET.isNotEmpty()

    private fun auth(token: String) = "Bearer $token"

    private fun postForm(url: String, vararg fields: Pair<String, String>) =
        Cloud.http("POST", url, contentType = "application/x-www-form-urlencoded", body = Cloud.form(*fields))

    // --- OAuth ---------------------------------------------------------------

    override fun deviceCode(deviceName: String, deviceId: String): Cloud.DeviceCode {
        if (!configured) error("ключи Google OAuth не заданы в этой сборке")
        val reply = postForm(
            "$OAUTH/device/code",
            "client_id" to BuildConfig.GOOGLE_CLIENT_ID,
            "scope" to SCOPE,
        )
        if (!reply.ok) throw Cloud.describe(reply, "Google не дал код")
        val j = reply.json()
        return Cloud.DeviceCode(
            deviceCode = j.getString("device_code"),
            userCode = j.getString("user_code"),
            verificationUrl = j.optString("verification_url").ifBlank { "https://www.google.com/device" },
            interval = j.optLong("interval", 5).coerceAtLeast(2),
            expiresAt = System.currentTimeMillis() + j.optLong("expires_in", 1800) * 1000,
        )
    }

    override fun pollToken(code: Cloud.DeviceCode): Cloud.Poll {
        val reply = postForm(
            "$OAUTH/token",
            "client_id" to BuildConfig.GOOGLE_CLIENT_ID,
            "client_secret" to BuildConfig.GOOGLE_CLIENT_SECRET,
            "device_code" to code.deviceCode,
            "grant_type" to "urn:ietf:params:oauth:grant-type:device_code",
        )
        val j = runCatching { reply.json() }.getOrNull() ?: JSONObject()
        if (!reply.ok) {
            return when (j.optString("error")) {
                "authorization_pending", "slow_down" -> Cloud.Poll.Pending
                "expired_token" -> error("код устарел — запросите новый")
                "access_denied" -> error("вы отказали приложению в доступе")
                else -> throw Cloud.describe(reply, "вход не удался")
            }
        }
        return Cloud.Poll.Done(parseTokens(j, null))
    }

    override fun refresh(refreshToken: String): Cloud.Tokens {
        val reply = postForm(
            "$OAUTH/token",
            "client_id" to BuildConfig.GOOGLE_CLIENT_ID,
            "client_secret" to BuildConfig.GOOGLE_CLIENT_SECRET,
            "refresh_token" to refreshToken,
            "grant_type" to "refresh_token",
        )
        // Отозванный refresh-токен — это «войти заново», как 401.
        if (!reply.ok) throw Cloud.Unauthorized("${Cloud.describe(reply, "токен не продлился").message} (401)")
        return parseTokens(reply.json(), refreshToken)
    }

    private fun parseTokens(j: JSONObject, oldRefresh: String?): Cloud.Tokens = Cloud.Tokens(
        access = j.optString("access_token").ifBlank { error("в ответе нет токена") },
        refresh = j.optString("refresh_token").ifBlank { oldRefresh.orEmpty() },
        expiresAt = System.currentTimeMillis() + j.optLong("expires_in", 3600) * 1000,
    )

    override fun revoke(token: String) {
        runCatching {
            Cloud.http(
                "POST", "$OAUTH/revoke?token=${Cloud.enc(token)}",
                contentType = "application/x-www-form-urlencoded", body = ByteArray(0),
            )
        }
    }

    override fun account(token: String): String {
        val reply = Cloud.http("GET", "$DRIVE/about?fields=user(emailAddress)", auth(token))
        if (!reply.ok) throw Cloud.describe(reply, "не удалось узнать аккаунт")
        return reply.json().optJSONObject("user")?.optString("emailAddress").orEmpty()
    }

    // --- папки и файлы ---------------------------------------------------------

    /** Идентификаторы папок и файлов по именам — чтобы не искать перед каждой операцией. */
    private class Ids(val meetings: String, val audio: String) {
        val files = HashMap<Pair<Cloud.Folder, String>, String>()
    }

    @Volatile private var ids: Ids? = null

    private fun escape(s: String) = s.replace("\\", "\\\\").replace("'", "\\'")

    private fun findChild(token: String, parent: String, name: String, folder: Boolean): String? {
        var q = "name = '${escape(name)}' and '${escape(parent)}' in parents and trashed = false"
        if (folder) q += " and mimeType = '$FOLDER_MIME'"
        val reply = Cloud.http("GET", "$DRIVE/files?q=${Cloud.enc(q)}&fields=files(id)&pageSize=5", auth(token))
        if (!reply.ok) throw Cloud.describe(reply, "не удалось найти «$name»")
        val files = reply.json().optJSONArray("files") ?: return null
        return if (files.length() > 0) files.getJSONObject(0).optString("id").ifBlank { null } else null
    }

    private fun createFolder(token: String, parent: String, name: String): String {
        val body = JSONObject().put("name", name).put("mimeType", FOLDER_MIME)
            .put("parents", org.json.JSONArray().put(parent))
        val reply = Cloud.http(
            "POST", "$DRIVE/files?fields=id", auth(token),
            contentType = "application/json", body = body.toString().toByteArray(),
        )
        if (!reply.ok) throw Cloud.describe(reply, "не удалось создать папку «$name»")
        return reply.json().optString("id").ifBlank { error("в ответе нет id папки") }
    }

    private fun ensureFolder(token: String, parent: String, name: String): String =
        findChild(token, parent, name, true) ?: createFolder(token, parent, name)

    private fun ensureIds(token: String): Ids {
        ids?.let { return it }
        val root = ensureFolder(token, "root", ROOT_NAME)
        val fresh = Ids(ensureFolder(token, root, "meetings"), ensureFolder(token, root, "audio"))
        ids = fresh
        return fresh
    }

    private fun folderId(ids: Ids, folder: Cloud.Folder) =
        if (folder == Cloud.Folder.MEETINGS) ids.meetings else ids.audio

    private fun fileId(token: String, folder: Cloud.Folder, name: String): String? {
        val ids = ensureIds(token)
        ids.files[folder to name]?.let { return it }
        val found = findChild(token, folderId(ids, folder), name, false)
        if (found != null) ids.files[folder to name] = found
        return found
    }

    override fun prepare(token: String) {
        // Новый аккаунт — кэш папок прежнего ни о чём.
        ids = null
        ensureIds(token)
    }

    override fun list(token: String, folder: Cloud.Folder): List<Cloud.RemoteFile> {
        val ids = ensureIds(token)
        val q = "'${escape(folderId(ids, folder))}' in parents and trashed = false"
        val out = mutableListOf<Cloud.RemoteFile>()
        var page: String? = null
        while (true) {
            var url = "$DRIVE/files?q=${Cloud.enc(q)}" +
                "&fields=nextPageToken,files(id,name,md5Checksum,modifiedTime,size)&pageSize=1000"
            if (page != null) url += "&pageToken=${Cloud.enc(page)}"
            val reply = Cloud.http("GET", url, auth(token))
            if (!reply.ok) throw Cloud.describe(reply, "не удалось прочитать папку")
            val j = reply.json()
            val files = j.optJSONArray("files")
            if (files != null) for (i in 0 until files.length()) {
                val item = files.getJSONObject(i)
                val name = item.optString("name")
                ids.files[folder to name] = item.optString("id")
                out += Cloud.RemoteFile(
                    name = name,
                    md5 = item.optString("md5Checksum"),
                    modified = Yandex.parseIso8601(item.optString("modifiedTime")),
                    size = item.optString("size").toLongOrNull() ?: 0L,
                )
            }
            page = j.optString("nextPageToken").ifBlank { null } ?: return out
        }
    }

    private fun putBytes(token: String, folder: Cloud.Folder, name: String, bytes: ByteArray) {
        fileId(token, folder, name)?.let { id ->
            val reply = Cloud.http(
                "PATCH", "$UPLOAD/$id?uploadType=media", auth(token),
                contentType = "application/octet-stream", body = bytes,
            )
            if (!reply.ok) throw Cloud.describe(reply, "не удалось загрузить $name")
            return
        }
        val ids = ensureIds(token)
        val meta = JSONObject().put("name", name)
            .put("parents", org.json.JSONArray().put(folderId(ids, folder)))
        val boundary = "solflow-multipart-boundary"
        val head = "--$boundary\r\nContent-Type: application/json; charset=UTF-8\r\n\r\n$meta\r\n" +
            "--$boundary\r\nContent-Type: application/octet-stream\r\n\r\n"
        val tail = "\r\n--$boundary--"
        val body = head.toByteArray() + bytes + tail.toByteArray()
        val reply = Cloud.http(
            "POST", "$UPLOAD?uploadType=multipart&fields=id", auth(token),
            contentType = "multipart/related; boundary=$boundary", body = body,
        )
        if (!reply.ok) throw Cloud.describe(reply, "не удалось загрузить $name")
        reply.json().optString("id").ifBlank { null }?.let { ids.files[folder to name] = it }
    }

    override fun upload(token: String, folder: Cloud.Folder, name: String, bytes: ByteArray) =
        putBytes(token, folder, name, bytes)

    /** Звук — сотни мегабайт; грузим целиком, возобновляемая загрузка пока не нужна. */
    override fun uploadFile(token: String, folder: Cloud.Folder, name: String, file: File) =
        putBytes(token, folder, name, file.readBytes())

    override fun download(token: String, folder: Cloud.Folder, name: String): ByteArray {
        val id = fileId(token, folder, name) ?: error("$name: файла нет (404)")
        val reply = Cloud.http("GET", "$DRIVE/files/$id?alt=media", auth(token))
        if (!reply.ok) throw Cloud.describe(reply, "не удалось скачать $name")
        return reply.body
    }

    override fun downloadFile(token: String, folder: Cloud.Folder, name: String, target: File) {
        val id = fileId(token, folder, name) ?: error("$name: файла нет (404)")
        target.parentFile?.mkdirs()
        val reply = Cloud.http("GET", "$DRIVE/files/$id?alt=media", auth(token), target = target)
        if (!reply.ok) throw Cloud.describe(reply, "не удалось скачать $name")
    }

    override fun delete(token: String, folder: Cloud.Folder, name: String) {
        val id = fileId(token, folder, name) ?: return
        val reply = Cloud.http("DELETE", "$DRIVE/files/$id", auth(token))
        ids?.files?.remove(folder to name)
        if (!reply.ok && reply.status != 404) throw Cloud.describe(reply, "не удалось удалить $name")
    }
}
