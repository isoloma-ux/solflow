package com.handy.voice

import android.app.Activity
import android.content.Context
import android.content.Intent
import android.net.Uri
import android.provider.Settings
import androidx.core.content.FileProvider
import org.json.JSONObject
import java.io.File
import java.net.HttpURLConnection
import java.net.URL

/**
 * Обновление приложения без магазина.
 *
 * Android не даёт приложению поставить APK молча — и правильно делает. Что
 * можно: скачать файл и открыть системный установщик, который спросит
 * человека. Разрешение «ставить неизвестные приложения» выдаётся один раз
 * и живёт дальше.
 *
 * Ссылку на файл берём из того же релиза, что и десктопные сборки: имя APK
 * меняется от версии к версии, поэтому ищем по расширению, а не угадываем.
 */
object AppUpdate {

    /** Что нашлось в последнем выпуске. */
    data class Release(val version: String, val apkUrl: String?)

    /** Спрашивает GitHub про последний выпуск. Null — не вышло. */
    fun latest(): Release? = runCatching {
        val conn = (URL(AboutActivity.RELEASES_API).openConnection() as HttpURLConnection).apply {
            instanceFollowRedirects = true
            connectTimeout = 8_000
            readTimeout = 10_000
            setRequestProperty("User-Agent", "SolFlow")
        }
        try {
            if (conn.responseCode !in 200..299) return null
            val json = JSONObject(conn.inputStream.bufferedReader().readText())
            val version = json.optString("tag_name").takeIf { it.isNotBlank() } ?: return null
            val assets = json.optJSONArray("assets")
            var apk: String? = null
            for (i in 0 until (assets?.length() ?: 0)) {
                val asset = assets!!.getJSONObject(i)
                if (asset.optString("name").endsWith(".apk")) {
                    apk = asset.optString("browser_download_url")
                    break
                }
            }
            Release(version, apk)
        } finally {
            conn.disconnect()
        }
    }.getOrNull()

    /**
     * Качает APK в свою папку. [onProgress] — доля от нуля до единицы;
     * когда размер неизвестен, приходит -1.
     */
    fun download(context: Context, url: String, onProgress: (Float) -> Unit): File? =
        runCatching {
            val dir = File(context.cacheDir, "updates").apply { mkdirs() }
            // Старые файлы не копим: обновление нужно ровно один раз.
            dir.listFiles()?.forEach { it.delete() }
            val target = File(dir, "solflow-update.apk")

            val conn = (URL(url).openConnection() as HttpURLConnection).apply {
                instanceFollowRedirects = true
                connectTimeout = 10_000
                readTimeout = 30_000
                setRequestProperty("User-Agent", "SolFlow")
            }
            try {
                if (conn.responseCode !in 200..299) return null
                val total = conn.contentLengthLong
                var done = 0L
                conn.inputStream.use { input ->
                    target.outputStream().use { output ->
                        val buffer = ByteArray(64 * 1024)
                        while (true) {
                            val read = input.read(buffer)
                            if (read <= 0) break
                            output.write(buffer, 0, read)
                            done += read
                            onProgress(if (total > 0) done.toFloat() / total else -1f)
                        }
                    }
                }
                // Оборванная закачка выглядит как обычный конец потока:
                // сверяем с обещанным размером, иначе установщик получит
                // обрезанный файл и скажет невнятное «пакет повреждён».
                if (total > 0 && done < total) {
                    target.delete()
                    return null
                }
                target
            } finally {
                conn.disconnect()
            }
        }.getOrNull()

    /** Новее ли версия с GitHub той, что стоит: сравнение по числам, не по строкам. */
    fun newer(latest: String, current: String): Boolean {
        fun parse(v: String) = v.trimStart('v').split('.', '-').mapNotNull { it.toIntOrNull() }
        val a = parse(latest)
        val b = parse(current)
        for (i in 0 until maxOf(a.size, b.size)) {
            val x = a.getOrElse(i) { 0 }
            val y = b.getOrElse(i) { 0 }
            if (x != y) return x > y
        }
        return false
    }

    /** Разрешено ли приложению ставить APK. */
    fun canInstall(context: Context): Boolean =
        context.packageManager.canRequestPackageInstalls()

    /** Открывает системный экран, где это разрешение выдают. */
    fun askInstallPermission(activity: Activity) {
        activity.startActivity(
            Intent(
                Settings.ACTION_MANAGE_UNKNOWN_APP_SOURCES,
                Uri.parse("package:${activity.packageName}"),
            )
        )
    }

    /** Открывает системный установщик для скачанного файла. */
    fun install(context: Context, apk: File) {
        val uri = FileProvider.getUriForFile(context, "${context.packageName}.files", apk)
        context.startActivity(
            Intent(Intent.ACTION_VIEW)
                .setDataAndType(uri, "application/vnd.android.package-archive")
                .addFlags(Intent.FLAG_GRANT_READ_URI_PERMISSION)
                .addFlags(Intent.FLAG_ACTIVITY_NEW_TASK)
        )
    }
}
