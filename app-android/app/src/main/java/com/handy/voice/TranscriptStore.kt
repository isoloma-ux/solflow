package com.handy.voice

import android.content.Context
import org.json.JSONArray
import org.json.JSONObject
import java.io.File
import java.nio.ByteBuffer
import java.nio.ByteOrder
import java.text.SimpleDateFormat
import java.util.Date
import java.util.Locale

/**
 * Одна диктовка. [audio] — есть ли рядом WAV, который можно переслушать
 * и расшифровать заново другой моделью.
 */
data class Transcript(
    val text: String,
    val at: Long,
    val seconds: Float,
    val audio: Boolean = false,
)

/**
 * История расшифровок. Лежит одним JSON-файлом: записей мало, они короткие,
 * и заводить ради них базу данных было бы лишним. Звук — отдельными WAV в
 * подпапке `history/`, имя файла равно моменту диктовки.
 *
 * Сколько записей держать и как долго — решают настройки: звук занимает
 * место, и человек, диктующий каждый день, не должен молча забивать телефон.
 */
object TranscriptStore {

    private fun file(context: Context) = File(context.filesDir, "history.json")

    private fun audioDir(context: Context) =
        File(context.filesDir, "history").also { it.mkdirs() }

    fun audioFile(context: Context, at: Long) = File(audioDir(context), "$at.wav")

    /**
     * Новая запись идёт наверх. Звук сохраняем, только если пользователь
     * этого хочет: минута диктовки — около двух мегабайт.
     */
    fun add(context: Context, text: String, seconds: Float, pcm: FloatArray? = null) {
        if (text.isBlank()) return
        if (AppPrefs.historyRetention(context) == "never") return

        val at = System.currentTimeMillis()
        var hasAudio = false
        if (pcm != null && AppPrefs.keepAudio(context)) {
            hasAudio = runCatching { writeWav(audioFile(context, at), pcm) }.isSuccess
        }

        val items = listOf(Transcript(text, at, seconds, hasAudio)) + all(context)
        save(context, prune(context, items))
    }

    fun all(context: Context): List<Transcript> {
        val f = file(context)
        if (!f.exists()) return emptyList()
        return runCatching {
            val arr = JSONArray(f.readText())
            (0 until arr.length()).map { i ->
                val o = arr.getJSONObject(i)
                Transcript(
                    text = o.getString("text"),
                    at = o.getLong("at"),
                    seconds = o.optDouble("seconds").toFloat(),
                    audio = o.optBoolean("audio"),
                )
            }
        }.getOrDefault(emptyList())
    }

    fun remove(context: Context, at: Long) {
        audioFile(context, at).delete()
        save(context, all(context).filterNot { it.at == at })
    }

    fun clear(context: Context) {
        audioDir(context).deleteRecursively()
        file(context).delete()
    }

    /** Заменяет текст записи — после повторной расшифровки другой моделью. */
    fun updateText(context: Context, at: Long, text: String) {
        save(context, all(context).map { if (it.at == at) it.copy(text = text) else it })
    }

    /**
     * Применяет текущие правила к тому, что уже лежит. Вызывается после
     * смены настроек: иначе новый лимит подействовал бы только на новые
     * записи, а выключенный звук не освободил бы место.
     */
    fun applyLimits(context: Context) {
        if (AppPrefs.historyRetention(context) == "never") {
            clear(context)
            return
        }
        var items = prune(context, all(context))
        if (!AppPrefs.keepAudio(context)) {
            for (item in items) audioFile(context, item.at).delete()
            items = items.map { it.copy(audio = false) }
        }
        save(context, items)
    }

    /** Чистка по правилам настроек: сначала по сроку, потом по количеству. */
    private fun prune(context: Context, items: List<Transcript>): List<Transcript> {
        val ttl = AppPrefs.retentionMs(context)
        val now = System.currentTimeMillis()
        val alive = if (ttl == null) items else items.filter { now - it.at <= ttl }
        val kept = alive.take(AppPrefs.historyLimit(context))
        // Файлы со звуком уходят вместе с записями, иначе папка растёт молча.
        for (gone in items - kept.toSet()) audioFile(context, gone.at).delete()
        return kept
    }

    private fun save(context: Context, items: List<Transcript>) {
        val arr = JSONArray()
        for (t in items) {
            arr.put(JSONObject().apply {
                put("text", t.text)
                put("at", t.at)
                put("seconds", t.seconds.toDouble())
                put("audio", t.audio)
            })
        }
        file(context).writeText(arr.toString())
    }

    /** float32 из движка обратно в PCM16 — формат, который читает плеер. */
    private fun writeWav(target: File, pcm: FloatArray) {
        val bytes = ByteArray(pcm.size * 2)
        val buffer = ByteBuffer.wrap(bytes).order(ByteOrder.LITTLE_ENDIAN)
        for (sample in pcm) {
            buffer.putShort((sample.coerceIn(-1f, 1f) * 32767f).toInt().toShort())
        }
        WavWriter(target).use { it.write(bytes, bytes.size) }
    }

    // --- подписи ----------------------------------------------------------

    /** «Сегодня», «Вчера» или дата — для группировки списка по дням. */
    fun dayLabel(context: Context, at: Long): String {
        val day = 24 * 60 * 60 * 1000L
        val startOfToday = startOfDay(System.currentTimeMillis())
        return when {
            at >= startOfToday -> context.getString(R.string.today)
            at >= startOfToday - day -> context.getString(R.string.yesterday)
            else -> SimpleDateFormat("d MMMM", Locale("ru")).format(Date(at))
        }
    }

    fun timeLabel(at: Long): String =
        SimpleDateFormat("HH:mm", Locale("ru")).format(Date(at))

    private fun startOfDay(ts: Long): Long {
        val cal = java.util.Calendar.getInstance()
        cal.timeInMillis = ts
        cal.set(java.util.Calendar.HOUR_OF_DAY, 0)
        cal.set(java.util.Calendar.MINUTE, 0)
        cal.set(java.util.Calendar.SECOND, 0)
        cal.set(java.util.Calendar.MILLISECOND, 0)
        return cal.timeInMillis
    }
}
