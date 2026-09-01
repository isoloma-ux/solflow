package com.handy.voice

import android.content.Context
import android.util.Log
import java.io.File
import java.security.MessageDigest
import org.json.JSONArray
import org.json.JSONObject

/**
 * Один проход синхронизации с Яндекс.Диском — зеркало sync/mod.rs на
 * десктопе, тот же протокол и те же правила слияния.
 *
 * Раскладка на Диске плоская, чтобы одно чтение папки давало всю картину:
 * `app:/meetings/<id>.meta.json`, `<id>.transcript.json`, `<id>.deleted`
 * (надгробие), `projects.json`; звук отдельно — `app:/audio/<id>.wav`.
 *
 * Что менялось, видно по md5: у каждого файла помним, каким его видели на
 * Диске и каким — у себя. Разошлось с одной стороны — копируем, с обеих —
 * сливаем. Удаление помечается надгробием, иначе второе устройство
 * воскресило бы стёртое из своей копии.
 */
object SyncEngine {

    private const val TAG = "HandyVoice"
    private const val REMOTE_MEETINGS = "app:/meetings"
    private const val REMOTE_AUDIO = "app:/audio"

    // --- состояние на диске ----------------------------------------------------

    class FileState(var remote: String, var local: String)

    class State(
        val files: MutableMap<String, FileState> = mutableMapOf(),
        val pendingDeletes: MutableList<Long> = mutableListOf(),
        var projectsSnapshot: List<MeetingStore.Project> = emptyList(),
        var lastSync: Long = 0,
        var foldersReady: Boolean = false,
    ) {
        fun save(context: Context) {
            val files = JSONObject()
            for ((name, fs) in this.files) {
                files.put(name, JSONObject().put("remote", fs.remote).put("local", fs.local))
            }
            val snapshot = JSONArray()
            for (p in projectsSnapshot) snapshot.put(JSONObject().put("id", p.id).put("name", p.name))
            file(context).writeText(
                JSONObject()
                    .put("files", files)
                    .put("pending_deletes", JSONArray(pendingDeletes))
                    .put("projects_snapshot", snapshot)
                    .put("last_sync", lastSync)
                    .put("folders_ready", foldersReady)
                    .toString()
            )
        }

        companion object {
            fun file(context: Context) = File(context.filesDir, "sync.json")

            fun load(context: Context): State {
                val f = file(context)
                if (!f.exists()) return State()
                return runCatching {
                    val o = JSONObject(f.readText())
                    val files = mutableMapOf<String, FileState>()
                    o.optJSONObject("files")?.let { fo ->
                        for (name in fo.keys()) {
                            val e = fo.getJSONObject(name)
                            files[name] = FileState(e.optString("remote"), e.optString("local"))
                        }
                    }
                    val deletes = mutableListOf<Long>()
                    o.optJSONArray("pending_deletes")?.let { a ->
                        for (i in 0 until a.length()) deletes += a.getLong(i)
                    }
                    State(
                        files = files,
                        pendingDeletes = deletes,
                        projectsSnapshot = parseProjects(o.optJSONArray("projects_snapshot")),
                        lastSync = o.optLong("last_sync"),
                        foldersReady = o.optBoolean("folders_ready"),
                    )
                }.getOrDefault(State())
            }

            fun clear(context: Context) {
                file(context).delete()
            }
        }
    }

    // --- служебное -------------------------------------------------------------

    fun md5(bytes: ByteArray): String =
        MessageDigest.getInstance("MD5").digest(bytes).joinToString("") { "%02x".format(it) }

    private fun parseProjects(arr: JSONArray?): List<MeetingStore.Project> {
        if (arr == null) return emptyList()
        return (0 until arr.length()).mapNotNull { i ->
            val o = arr.optJSONObject(i) ?: return@mapNotNull null
            MeetingStore.Project(o.optString("id"), o.optString("name"))
        }
    }

    private fun projectsJson(list: List<MeetingStore.Project>): ByteArray {
        val arr = JSONArray()
        for (p in list) arr.put(JSONObject().put("id", p.id).put("name", p.name))
        return arr.toString().toByteArray()
    }

    private fun sameProjects(a: List<MeetingStore.Project>, b: List<MeetingStore.Project>) =
        a.size == b.size && a.zip(b).all { (x, y) -> x.id == y.id && x.name == y.name }

    private fun idOf(name: String): Long? = name.substringBefore('.').toLongOrNull()

    // --- слияние ---------------------------------------------------------------

    /**
     * Мета встречи: побеждает сохранённая позже (`updated`), но пустое поле
     * победителя не затирает заполненное у проигравшего — иначе саммери,
     * посчитанное на компьютере, пропадало бы от переименования на телефоне.
     * Проект — исключение: «убрать из проекта» — осознанное действие.
     */
    fun mergeMeta(local: JSONObject, remote: JSONObject): JSONObject {
        val (newer, older) =
            if (remote.optLong("updated") >= local.optLong("updated")) remote to local else local to remote
        val m = JSONObject(newer.toString())
        if (m.optString("title").isBlank() && older.optString("title").isNotBlank()) {
            m.put("title", older.optString("title"))
        }
        if (m.optString("summary").isEmpty() && older.optString("summary").isNotEmpty()) {
            m.put("summary", older.optString("summary"))
        }
        val names = m.optJSONObject("names")
        val olderNames = older.optJSONObject("names")
        if ((names == null || names.length() == 0) && olderNames != null && olderNames.length() > 0) {
            m.put("names", olderNames)
        }
        if (m.optInt("speakers") == 0 && older.optInt("speakers") > 0) {
            m.put("speakers", older.optInt("speakers"))
        }
        if (m.optString("state") != Meeting.STATE_DONE && older.optString("state") == Meeting.STATE_DONE) {
            m.put("state", Meeting.STATE_DONE)
            m.remove("error")
        }
        if (m.optDouble("seconds", 0.0) < older.optDouble("seconds", 0.0)) {
            m.put("seconds", older.optDouble("seconds"))
        }
        m.put("updated", maxOf(newer.optLong("updated"), older.optLong("updated")))
        return m
    }

    /**
     * Проекты — трёхстороннее слияние со снимком после прошлой
     * синхронизации: чего нет на одной стороне, но было в снимке, — удалено;
     * чего не было в снимке — добавлено. Порядок: как на Диске, новые
     * местные — в конец.
     */
    fun mergeProjects(
        local: List<MeetingStore.Project>,
        remote: List<MeetingStore.Project>,
        snapshot: List<MeetingStore.Project>,
    ): List<MeetingStore.Project> {
        fun find(list: List<MeetingStore.Project>, id: String) = list.firstOrNull { it.id == id }
        val out = mutableListOf<MeetingStore.Project>()
        for (r in remote) {
            val l = find(local, r.id)
            if (l != null) {
                val name = when {
                    l.name == r.name -> l.name
                    find(snapshot, r.id)?.name == l.name -> r.name
                    else -> l.name
                }
                out += MeetingStore.Project(r.id, name)
            } else if (find(snapshot, r.id) == null) {
                out += r
            }
        }
        for (l in local) {
            if (find(remote, l.id) == null && find(snapshot, l.id) == null) out += l
        }
        return out
    }

    // --- проход ----------------------------------------------------------------

    private enum class Plan { NOTHING, UPLOAD, DOWNLOAD, CONFLICT, FORGET }

    private fun plan(local: String?, remote: String?, seen: FileState?): Plan = when {
        local == null && remote == null -> Plan.FORGET
        remote == null -> Plan.UPLOAD
        local == null -> Plan.DOWNLOAD
        else -> {
            val localChanged = seen == null || seen.local != local
            val remoteChanged = seen == null || seen.remote != remote
            when {
                !localChanged && !remoteChanged -> Plan.NOTHING
                localChanged && !remoteChanged -> Plan.UPLOAD
                !localChanged -> Plan.DOWNLOAD
                else -> Plan.CONFLICT
            }
        }
    }

    /** Итог прохода: что-то поменялось локально — экрану надо перечитать список. */
    class Outcome(val changedLocal: Boolean, val error: String?)

    /**
     * Один проход. Бросает [Yandex.Unauthorized], если токен не подошёл;
     * остальные ошибки собираются и возвращаются текстом, чтобы одна
     * неудачная встреча не останавливала остальные.
     */
    fun run(
        context: Context,
        token: String,
        syncAudio: Boolean,
        busy: Set<Long>,
        onProgress: (String?) -> Unit,
    ): Outcome {
        val state = State.load(context)
        if (!state.foldersReady) {
            Yandex.mkdir(token, "app:/")
            Yandex.mkdir(token, REMOTE_MEETINGS)
            Yandex.mkdir(token, REMOTE_AUDIO)
            state.foldersReady = true
            state.save(context)
        }

        val remote = Yandex.list(token, REMOTE_MEETINGS).associateBy { it.name }
        val ids = sortedSetOf<Long>()
        ids += MeetingStore.ids(context)
        ids += remote.keys.mapNotNull(::idOf)
        ids += state.pendingDeletes

        var changed = false
        var firstError: String? = null

        fun upload(name: String, bytes: ByteArray) {
            Yandex.upload(token, "$REMOTE_MEETINGS/$name", bytes)
            val h = md5(bytes)
            state.files[name] = FileState(h, h)
        }

        fun download(name: String): ByteArray = Yandex.download(token, "$REMOTE_MEETINGS/$name")

        fun markDownloaded(name: String, bytes: ByteArray) {
            state.files[name] = FileState(remote[name]?.md5 ?: md5(bytes), md5(bytes))
        }

        fun writeLocal(id: Long, file: String, bytes: ByteArray) {
            val dir = MeetingStore.dir(context, id)
            dir.mkdirs()
            File(dir, file).writeBytes(bytes)
            changed = true
        }

        fun syncMeta(id: Long) {
            val name = "$id.meta.json"
            val path = File(MeetingStore.dir(context, id), "meta.json")
            val localBytes = if (path.exists()) path.readBytes() else null
            val seen = state.files[name]
            when (plan(localBytes?.let(::md5), remote[name]?.md5, seen)) {
                Plan.NOTHING -> {}
                Plan.FORGET -> state.files.remove(name)
                Plan.UPLOAD -> {
                    val bytes = localBytes ?: return
                    // Встречу посреди расшифровки не отправляем: второе
                    // устройство показывало бы «расшифровываю» без конца.
                    val stateNow = runCatching { JSONObject(String(bytes)).optString("state") }.getOrNull()
                    if (stateNow == null || stateNow == Meeting.STATE_TRANSCRIBING) return
                    upload(name, bytes)
                }
                Plan.DOWNLOAD -> {
                    val bytes = download(name)
                    val meta = runCatching { JSONObject(String(bytes)) }.getOrNull()
                        ?: error("мета $id на Диске нечитаема")
                    if (meta.optString("state") == Meeting.STATE_TRANSCRIBING) return
                    writeLocal(id, "meta.json", bytes)
                    markDownloaded(name, bytes)
                }
                Plan.CONFLICT -> {
                    val local = runCatching { JSONObject(String(localBytes!!)) }.getOrDefault(JSONObject())
                    val remoteMeta = runCatching { JSONObject(String(download(name))) }.getOrDefault(JSONObject())
                    val bytes = mergeMeta(local, remoteMeta).toString().toByteArray()
                    writeLocal(id, "meta.json", bytes)
                    upload(name, bytes)
                }
            }
        }

        fun syncTranscript(id: Long) {
            val name = "$id.transcript.json"
            val dir = MeetingStore.dir(context, id)
            // Расшифровка без меты — осколок: подождём, пока приедет мета.
            if (!File(dir, "meta.json").exists()) return
            val path = File(dir, "transcript.json")
            val localBytes = if (path.exists()) path.readBytes() else null
            val seen = state.files[name]
            when (plan(localBytes?.let(::md5), remote[name]?.md5, seen)) {
                Plan.NOTHING -> {}
                Plan.FORGET -> state.files.remove(name)
                Plan.UPLOAD -> upload(name, localBytes ?: return)
                Plan.DOWNLOAD -> {
                    val bytes = download(name)
                    writeLocal(id, "transcript.json", bytes)
                    markDownloaded(name, bytes)
                }
                Plan.CONFLICT -> {
                    // Две разных расшифровки одной записи — берём более свежую.
                    if ((remote[name]?.modified ?: 0) > path.lastModified()) {
                        val bytes = download(name)
                        writeLocal(id, "transcript.json", bytes)
                        markDownloaded(name, bytes)
                    } else {
                        upload(name, localBytes ?: return)
                    }
                }
            }
        }

        fun tombstone(id: Long) {
            upload("$id.deleted", "{}".toByteArray())
            for (name in listOf("$id.meta.json", "$id.transcript.json")) {
                if (remote.containsKey(name)) {
                    Yandex.delete(token, "$REMOTE_MEETINGS/$name")
                    state.files.remove(name)
                }
            }
            runCatching { Yandex.delete(token, "$REMOTE_AUDIO/$id.wav") }
            state.pendingDeletes.remove(id)
        }

        fun applyTombstone(id: Long) {
            val dir = MeetingStore.dir(context, id)
            if (dir.exists()) {
                dir.deleteRecursively()
                changed = true
            }
            state.files.remove("$id.meta.json")
            state.files.remove("$id.transcript.json")
            state.pendingDeletes.remove(id)
        }

        for (id in ids) {
            if (id in busy) continue
            try {
                when {
                    remote.containsKey("$id.deleted") -> applyTombstone(id)
                    id in state.pendingDeletes -> tombstone(id)
                    else -> {
                        syncMeta(id)
                        syncTranscript(id)
                    }
                }
            } catch (e: Yandex.Unauthorized) {
                state.save(context)
                throw e
            } catch (e: Exception) {
                Log.w(TAG, "синхронизация встречи $id", e)
                if (firstError == null) firstError = e.message ?: e.toString()
            }
            // Состояние пишется по ходу: обрыв посреди длинного списка не
            // заставит начинать с нуля.
            state.save(context)
        }

        // --- проекты ---
        try {
            val name = "projects.json"
            val local = MeetingStore.projects(context)
            val localMd5 = md5(projectsJson(local))
            val seen = state.files[name]
            val remoteMd5 = remote[name]?.md5
            val localChanged = seen == null || seen.local != localMd5
            val remoteChanged = if (seen == null) remoteMd5 != null else seen.remote != remoteMd5
            if (localChanged || remoteChanged) {
                val remoteList = if (remoteMd5 != null) {
                    runCatching { parseProjects(JSONArray(String(download(name)))) }.getOrDefault(emptyList())
                } else emptyList()
                val merged = if (remoteMd5 == null && state.projectsSnapshot.isEmpty()) local
                else mergeProjects(local, remoteList, state.projectsSnapshot)
                if (!sameProjects(merged, local)) {
                    MeetingStore.replaceProjects(context, merged)
                    changed = true
                }
                val bytes = projectsJson(merged)
                if (remoteMd5 == null || !sameProjects(merged, remoteList)) {
                    upload(name, bytes)
                } else {
                    state.files[name] = FileState(remoteMd5, md5(bytes))
                }
                state.projectsSnapshot = merged
            }
        } catch (e: Yandex.Unauthorized) {
            state.save(context)
            throw e
        } catch (e: Exception) {
            Log.w(TAG, "синхронизация проектов", e)
            if (firstError == null) firstError = e.message ?: e.toString()
        }

        // --- звук ---
        if (syncAudio) {
            try {
                val remoteAudio = Yandex.list(token, REMOTE_AUDIO).map { it.name }.toSet()
                for (id in ids) {
                    if (id in busy || id in state.pendingDeletes) continue
                    val name = "$id.wav"
                    val local = MeetingStore.audioFile(context, id)
                    val hasMeta = File(MeetingStore.dir(context, id), "meta.json").exists()
                    val title = MeetingStore.load(context, id)
                        ?.let { MeetingStore.displayTitle(context, it) } ?: id.toString()
                    if (local.exists() && name !in remoteAudio && remote.containsKey("$id.meta.json")) {
                        onProgress(context.getString(R.string.sync_progress_upload_audio, title))
                        Yandex.uploadFile(token, "$REMOTE_AUDIO/$name", local)
                    } else if (!local.exists() && name in remoteAudio && hasMeta) {
                        onProgress(context.getString(R.string.sync_progress_download_audio, title))
                        Yandex.downloadFile(token, "$REMOTE_AUDIO/$name", local)
                        changed = true
                    }
                }
                onProgress(null)
            } catch (e: Yandex.Unauthorized) {
                state.save(context)
                throw e
            } catch (e: Exception) {
                onProgress(null)
                Log.w(TAG, "синхронизация звука", e)
                if (firstError == null) firstError = e.message ?: e.toString()
            }
        }

        if (firstError == null) state.lastSync = System.currentTimeMillis()
        state.save(context)
        return Outcome(changed, firstError)
    }
}
