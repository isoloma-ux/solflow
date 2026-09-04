package com.handy.voice

import android.content.Context
import org.json.JSONArray
import org.json.JSONObject
import java.io.File
import java.text.SimpleDateFormat
import java.util.Date
import java.util.Locale

/**
 * Одна реплика таймлайна: границы в секундах от начала записи.
 * [speaker] появляется после разделения говорящих; 0 — «говорящий 1».
 */
data class MeetingSegment(
    val start: Float,
    val end: Float,
    val text: String,
    val speaker: Int? = null,
)

data class Meeting(
    val id: Long,
    val title: String,
    val at: Long,
    val seconds: Float,
    val state: String,
    val imported: Boolean,
    /** Сколько говорящих нашла диаризация; 0 — ещё не запускалась. */
    val speakers: Int = 0,
    /** Имена, которые пользователь дал говорящим, по их номерам. */
    val speakerNames: Map<Int, String> = emptyMap(),
    /** Проект, в котором лежит встреча; null — вне проектов. */
    val project: String? = null,
    /** Саммери от локальной языковой модели; пусто — его ещё не делали. */
    val summary: String = "",
    /**
     * Момент последнего сохранения (millis) — по нему синхронизация решает,
     * чья правка новее, когда встречу поменяли на двух устройствах.
     */
    val updated: Long = 0,
) {
    val isDone: Boolean get() = state == STATE_DONE

    companion object {
        const val STATE_RECORDED = "recorded"
        const val STATE_TRANSCRIBING = "transcribing"
        const val STATE_DONE = "done"
        const val STATE_FAILED = "failed"
    }
}

/**
 * Хранилище встреч: по каталогу на встречу в `files/meetings/<id>/`.
 * Внутри `audio.wav`, `meta.json` и, после расшифровки, `transcript.json`.
 * Аудио остаётся и после расшифровки — его можно переслушать или
 * расшифровать заново другой моделью.
 */
object MeetingStore {

    private fun root(context: Context) = File(context.filesDir, "meetings")

    /** Каталог встречи — синхронизация пишет в него файлы с Диска как есть. */
    fun dir(context: Context, id: Long) = File(root(context), id.toString())

    /** Все встречи на диске — по каталогам с meta.json. */
    fun ids(context: Context): List<Long> =
        (root(context).listFiles() ?: emptyArray())
            .mapNotNull { it.name.toLongOrNull() }
            .filter { metaFile(context, it).exists() }

    fun audioFile(context: Context, id: Long) = File(dir(context, id), "audio.wav")

    /** Сколько байт занимает звук готовых записей — для настройки. */
    fun audioUsage(context: Context): Long =
        all(context).filter { it.isDone }.sumOf { audioFile(context, it.id).length() }

    /** Удалить звук у готовых записей, кроме той, что пишется. Освобождено байт. */
    fun purgeAudio(context: Context): Long {
        var freed = 0L
        for (m in all(context)) {
            if (!m.isDone || m.id == MeetingService.recordingId) continue
            val f = audioFile(context, m.id)
            val size = f.length()
            if (size > 0 && f.delete()) freed += size
        }
        return freed
    }

    private fun metaFile(context: Context, id: Long) = File(dir(context, id), "meta.json")

    private fun transcriptFile(context: Context, id: Long) =
        File(dir(context, id), "transcript.json")

    /** Создаёт каталог новой встречи и возвращает её. Id — момент старта. */
    fun create(context: Context, imported: Boolean): Meeting {
        val now = System.currentTimeMillis()
        val meeting = Meeting(
            id = now,
            title = "",
            at = now,
            seconds = 0f,
            state = Meeting.STATE_RECORDED,
            imported = imported,
        )
        dir(context, now).mkdirs()
        save(context, meeting)
        return meeting
    }

    /**
     * Каждое сохранение — правка: штамп `updated` ставится здесь, а не у
     * вызывающих, чтобы его нельзя было забыть. Синхронизация узнаёт об
     * изменении и уезжает на Диск с задержкой.
     */
    fun save(context: Context, meeting: Meeting) {
        metaFile(context, meeting.id).writeText(
            JSONObject().apply {
                put("title", meeting.title)
                put("at", meeting.at)
                put("seconds", meeting.seconds.toDouble())
                put("state", meeting.state)
                put("imported", meeting.imported)
                put("speakers", meeting.speakers)
                put("names", JSONObject().apply {
                    for ((index, name) in meeting.speakerNames) put(index.toString(), name)
                })
                meeting.project?.let { put("project", it) }
                if (meeting.summary.isNotEmpty()) put("summary", meeting.summary)
                put("updated", System.currentTimeMillis())
            }.toString()
        )
        SyncManager.touch(context)
    }

    fun load(context: Context, id: Long): Meeting? {
        val f = metaFile(context, id)
        if (!f.exists()) return null
        return runCatching {
            val o = JSONObject(f.readText())
            Meeting(
                id = id,
                title = if (o.isNull("title")) "" else o.optString("title"),
                at = o.optLong("at", id),
                seconds = o.optDouble("seconds", 0.0).toFloat(),
                state = o.optString("state", Meeting.STATE_RECORDED),
                imported = o.optBoolean("imported"),
                speakers = o.optInt("speakers", 0),
                speakerNames = o.optJSONObject("names")?.let { names ->
                    names.keys().asSequence()
                        .mapNotNull { k -> k.toIntOrNull()?.let { it to names.getString(k) } }
                        .toMap()
                } ?: emptyMap(),
                // Мета с компьютера пишет отсутствующий проект как JSON null,
                // а optString на null отдаёт строку «null» — и встреча уезжала
                // бы в несуществующий проект. Поэтому null проверяется явно.
                project = if (o.isNull("project")) null
                else o.optString("project").takeIf { it.isNotBlank() },
                summary = if (o.isNull("summary")) "" else o.optString("summary"),
                updated = o.optLong("updated", 0),
            )
        }.getOrNull()
    }

    /** Все встречи, новые сверху. Каталоги без meta.json — мусор, пропускаем. */
    fun all(context: Context): List<Meeting> =
        (root(context).listFiles() ?: emptyArray())
            .mapNotNull { it.name.toLongOrNull() }
            .mapNotNull { load(context, it) }
            .sortedByDescending { it.at }

    fun delete(context: Context, id: Long) {
        // Сначала отметка для синхронизации, потом файлы: иначе следующее
        // устройство привезло бы встречу обратно.
        SyncManager.noteDeleted(context, id)
        dir(context, id).deleteRecursively()
    }

    fun saveTranscript(context: Context, id: Long, segments: List<MeetingSegment>) {
        val arr = JSONArray()
        for (s in segments) {
            arr.put(JSONObject().apply {
                put("s", s.start.toDouble())
                put("e", s.end.toDouble())
                put("text", s.text)
                s.speaker?.let { put("spk", it) }
            })
        }
        transcriptFile(context, id).writeText(arr.toString())
        SyncManager.touch(context)
    }

    fun loadTranscript(context: Context, id: Long): List<MeetingSegment> {
        val f = transcriptFile(context, id)
        if (!f.exists()) return emptyList()
        return runCatching {
            val arr = JSONArray(f.readText())
            (0 until arr.length()).map { i ->
                val o = arr.getJSONObject(i)
                MeetingSegment(
                    start = o.getDouble("s").toFloat(),
                    end = o.getDouble("e").toFloat(),
                    text = o.getString("text"),
                    speaker = if (o.has("spk")) o.getInt("spk") else null,
                )
            }
        }.getOrDefault(emptyList())
    }

    // --- проекты ----------------------------------------------------------

    /**
     * Проекты — просто папки для встреч: имя и идентификатор. Живут одним
     * JSON рядом со встречами, потому что список короткий и меняется редко.
     */
    data class Project(val id: String, val name: String)

    private fun projectsFile(context: Context) = File(context.filesDir, "projects.json")

    fun projects(context: Context): List<Project> {
        val f = projectsFile(context)
        if (!f.exists()) return emptyList()
        return runCatching {
            val arr = JSONArray(f.readText())
            (0 until arr.length()).map { i ->
                val o = arr.getJSONObject(i)
                Project(o.getString("id"), o.getString("name"))
            }
        }.getOrDefault(emptyList())
    }

    private fun saveProjects(context: Context, list: List<Project>) {
        val arr = JSONArray()
        for (p in list) {
            arr.put(JSONObject().apply {
                put("id", p.id)
                put("name", p.name)
            })
        }
        projectsFile(context).writeText(arr.toString())
        SyncManager.touch(context)
    }

    /**
     * Список проектов целиком — так его приносит синхронизация. Встречи из
     * пропавших проектов остаются, просто выходят из них.
     */
    fun replaceProjects(context: Context, list: List<Project>) {
        val alive = list.map { it.id }.toSet()
        for (meeting in all(context)) {
            if (meeting.project != null && meeting.project !in alive) {
                save(context, meeting.copy(project = null))
            }
        }
        saveProjects(context, list)
    }

    fun createProject(context: Context, name: String): Project {
        val project = Project(System.currentTimeMillis().toString(), name.trim())
        saveProjects(context, projects(context) + project)
        return project
    }

    fun renameProject(context: Context, id: String, name: String) {
        saveProjects(
            context,
            projects(context).map { if (it.id == id) it.copy(name = name.trim()) else it },
        )
    }

    /** Удаляет папку, но не встречи: они просто выходят из проекта. */
    fun deleteProject(context: Context, id: String) {
        saveProjects(context, projects(context).filterNot { it.id == id })
        for (meeting in all(context).filter { it.project == id }) {
            save(context, meeting.copy(project = null))
        }
    }

    fun setProject(context: Context, id: Long, project: String?) {
        load(context, id)?.let { save(context, it.copy(project = project)) }
    }

    fun projectName(context: Context, id: String?): String? =
        projects(context).firstOrNull { it.id == id }?.name

    // --- поиск ------------------------------------------------------------

    /**
     * Найденное место: номер реплики (по нему прокручивается таймлайн),
     * её время и кусок текста вокруг совпадения.
     */
    data class Quote(val index: Int, val start: Float, val text: String)

    /**
     * Встреча, в которой что-то нашлось: сколько совпадений всего и первые
     * три из них. По одному совпадению без места пролистывать расшифровку
     * на два часа невозможно — поэтому цитаты знают свой номер реплики.
     */
    data class Hit(val meeting: Meeting, val count: Int, val quotes: List<Quote>)

    /** Поиск по названию и тексту расшифровок; регистр не важен. */
    fun search(context: Context, query: String): List<Hit> {
        val needle = query.trim().lowercase()
        if (needle.isBlank()) return emptyList()
        return all(context).mapNotNull { meeting ->
            val inTitle = displayTitle(context, meeting).lowercase().contains(needle)
            val segments = loadTranscript(context, meeting.id)
            val found = segments.indices
                .filter { segments[it].text.lowercase().contains(needle) }
            when {
                found.isNotEmpty() -> Hit(
                    meeting = meeting,
                    count = found.size,
                    quotes = found.take(3).map { index ->
                        val segment = segments[index]
                        Quote(index, segment.start, quoteAround(segment.text, needle))
                    },
                )
                inTitle -> Hit(meeting, 0, emptyList())
                else -> null
            }
        }
    }

    /** Номера реплик, в которых встречается слово, — для перехода по ним. */
    fun matches(segments: List<MeetingSegment>, query: String): List<Int> {
        val needle = query.trim().lowercase()
        if (needle.isBlank()) return emptyList()
        return segments.indices.filter { segments[it].text.lowercase().contains(needle) }
    }

    /**
     * Кусок реплики вокруг найденного слова: целую реплику в карточку не
     * поместить, а без контекста непонятно, та ли это встреча.
     */
    private fun quoteAround(text: String, needle: String): String {
        val around = 60
        val at = text.lowercase().indexOf(needle)
        if (at < 0) return text.take(around * 2)
        val before = text.substring(0, at).length
        val start = (before - around).coerceAtLeast(0)
        val end = (before + needle.length + around).coerceAtMost(text.length)
        return buildString {
            if (start > 0) append('…')
            append(text, start, end)
            if (end < text.length) append('…')
        }
    }

    // --- подписи ----------------------------------------------------------

    /** «Встреча 27 августа, 14:30» — если пользователь не переименовал. */
    fun displayTitle(context: Context, meeting: Meeting): String {
        if (meeting.title.isNotBlank()) return meeting.title
        val date = SimpleDateFormat("d MMMM, HH:mm", Locale("ru")).format(Date(meeting.at))
        return context.getString(
            if (meeting.imported) R.string.meeting_imported_title else R.string.meeting_default_title,
            date,
        )
    }

    /** Имя говорящего: как назвал пользователь, иначе «Говорящий N». */
    fun speakerLabel(context: Context, meeting: Meeting, index: Int): String =
        meeting.speakerNames[index]?.takeIf { it.isNotBlank() }
            ?: context.getString(R.string.speaker_label, index + 1)

    /** «1 ч 42 мин» или «12 мин» — длительность для списка. */
    fun durationLabel(context: Context, seconds: Float): String {
        val total = seconds.toInt()
        val h = total / 3600
        val m = total % 3600 / 60
        return when {
            h > 0 -> context.getString(R.string.duration_hours, h, m)
            m > 0 -> context.getString(R.string.duration_minutes, m)
            else -> context.getString(R.string.duration_seconds, total)
        }
    }

    /** Метка времени в таймлайне: «12:34» или «1:02:34» после часа. */
    fun clockLabel(seconds: Float): String {
        val total = seconds.toInt()
        val h = total / 3600
        val m = total % 3600 / 60
        val s = total % 60
        return if (h > 0) {
            String.format(Locale.US, "%d:%02d:%02d", h, m, s)
        } else {
            String.format(Locale.US, "%02d:%02d", m, s)
        }
    }
}
