package com.handy.voice

import android.content.Context
import android.util.AtomicFile
import org.json.JSONObject
import java.io.File

/** Permission metadata only. The phone never runs an MCP server or exports audio. */
object AiAccess {
    const val FILE = "ai-access-v1.json"
    private fun file(context: Context) = AtomicFile(File(context.filesDir, FILE))
    fun decode(bytes: ByteArray): Map<String, AiAccessEntry> {
        require(bytes.size <= 4 * 1024 * 1024)
        val root = JSONObject(String(bytes, Charsets.UTF_8))
        require(root.getInt("schema") == 1 && root.keys().asSequence().toSet() == setOf("schema", "projects"))
        val projects = root.getJSONObject("projects")
        require(projects.length() <= 10000)
        return projects.keys().asSequence().associateWith { id ->
            require(id.matches(Regex("[0-9]{1,24}")))
            val entry = projects.getJSONObject(id)
            require(entry.keys().asSequence().toSet() == setOf("revision", "grant"))
            val rev = entry.get("revision")
            require(rev is Int || rev is Long)
            val revision = entry.getLong("revision")
            require(revision in 1..AiAccessRules.MAX_REV)
            val g = entry.getJSONObject("grant")
            val keys = listOf("transcript", "summary", "map", "analyses")
            require(g.keys().asSequence().toSet() == keys.toSet() && keys.all { g.get(it) is Boolean })
            AiAccessEntry(revision, AiGrant(g.getBoolean("transcript"), g.getBoolean("summary"), g.getBoolean("map"), g.getBoolean("analyses")))
        }.toSortedMap()
    }
    fun encode(entries: Map<String, AiAccessEntry>): ByteArray {
        val projects = JSONObject()
        for ((id,e) in entries.toSortedMap()) projects.put(id, JSONObject().put("revision",e.revision).put("grant",JSONObject()
            .put("transcript",e.grant.transcript).put("summary",e.grant.summary).put("map",e.grant.map).put("analyses",e.grant.analyses)))
        return JSONObject().put("schema",1).put("projects",projects).toString().toByteArray(Charsets.UTF_8)
    }
    @Synchronized fun load(context: Context): Map<String, AiAccessEntry> {
        val f = file(context)
        if (!f.baseFile.exists() && !File(f.baseFile.path + ".bak").exists()) return emptyMap()
        return decode(f.readFully())
    }
    private fun save(context: Context, entries: Map<String, AiAccessEntry>) {
        val f = file(context); val stream = f.startWrite()
        try { stream.write(encode(entries)); f.finishWrite(stream) }
        catch (e: Exception) { f.failWrite(stream); throw e }
    }
    @Synchronized fun set(context: Context, id: String, grant: AiGrant) {
        require(MeetingStore.projects(context).any { it.id == id })
        val entries = load(context).toMutableMap()
        val revision = (entries[id]?.revision ?: 0) + 1
        require(revision <= AiAccessRules.MAX_REV)
        entries[id] = AiAccessEntry(revision,grant)
        save(context,entries)
        SyncManager.touch(context)
    }
    @Synchronized fun merge(context: Context, remote: Map<String, AiAccessEntry>): Map<String, AiAccessEntry> {
        val merged = AiAccessRules.merge(load(context),remote)
        save(context,merged)
        return merged
    }
    fun grant(context: Context, id: String): AiGrant = runCatching { load(context)[id]?.grant }.getOrNull() ?: AiGrant()
}
