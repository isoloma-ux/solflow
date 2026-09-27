package com.handy.voice

import android.content.Context
import java.io.File
import java.security.MessageDigest
import org.json.JSONObject
import org.json.JSONArray

/** Versioned archives are ignored by old clients; a tombstone is published only after verification. */
object SharedTrash {
    data class Row(val id: Long, val title: String, val deletedAt: Long, val restoring: Boolean)
    fun validId(id: String) = id.isNotEmpty() && id.length <= 19 && id.all { it in '0'..'9' } && (id.toLongOrNull() ?: 0) > 0 && id.toLong().toString() == id
    fun restoreId(id: String): Long = 4_000_000_000_000_000L + MessageDigest.getInstance("SHA-256")
        .digest("solflow-trash-restore-v1:$id".toByteArray()).take(6).fold(0L) { a, b -> (a shl 8) or (b.toLong() and 255) }
    fun decode(bytes: ByteArray): JSONObject {
        require(bytes.size <= 20_000_000) { "Trash archive too large" }
        val a = JSONObject(String(bytes, Charsets.UTF_8))
        require(a.getInt("schema") == 1 && validId(a.getString("id")) && a.getLong("deleted_at") > 0)
        a.getJSONObject("meta"); a.getJSONArray("transcript")
        require(a.isNull("audio_md5") == a.isNull("audio_size"))
        if (!a.isNull("audio_md5")) { require(a.getString("audio_md5").matches(Regex("[a-fA-F0-9]{32}"))); require(a.getLong("audio_size") >= 0) }
        return a
    }
    fun root(context: Context): File {
        val scope = if (SyncManager.connected(context)) "${Cloud.current(context).id}:${AppPrefs.yandexLogin(context)}" else "local"
        val hash = MessageDigest.getInstance("SHA-256").digest(scope.toByteArray()).joinToString("") { "%02x".format(it) }
        return File(context.filesDir, "trash-v1/$hash")
    }
    private fun atomic(file: File, bytes: ByteArray) { file.parentFile!!.mkdirs(); TranscriptFiles.atomicWrite(file, bytes) }
    private fun audio(dir: File) = File(dir, "audio.wav").takeIf { it.exists() } ?: File(dir, "local-original/audio.wav")
    private fun md5(file: File): String {
        val hash = MessageDigest.getInstance("MD5")
        file.inputStream().use { input -> val block = ByteArray(65536); while (true) { val n = input.read(block); if (n < 0) break; hash.update(block, 0, n) } }
        return hash.digest().joinToString("") { "%02x".format(it) }
    }
    private fun matchesAudio(file: File, archive: JSONObject): Boolean = !archive.isNull("audio_md5") && file.exists() && file.length()==archive.getLong("audio_size") && md5(file)==archive.getString("audio_md5")
    internal fun ensureArchiveAudio(dir: File, archive: JSONObject, cloud: Cloud.Provider, token: String) {
        if(archive.isNull("audio_md5") || matchesAudio(audio(dir),archive)) return
        val temp=File(dir,"audio.download")
        cloud.downloadFile(token,Cloud.Folder.AUDIO,audioName(archive.getString("id")),temp)
        check(matchesAudio(temp,archive)) { "Trash audio checksum mismatch" }
        // Keep local-original intact; replace only the verified cloud cache.
        check(temp.renameTo(File(dir,"audio.wav"))) { "Could not save archived audio" }
    }
    private fun archiveName(id: String) = "trash-v1-$id.json"
    private fun audioName(id: String) = "trash-v1-$id.wav"
    private fun restoredName(id: String) = "trash-v1-$id.restored.json"
    fun retain(context: Context, id: Long, outgoing: Boolean) {
        require(id > 0)
        val source = MeetingStore.dir(context, id); val target = File(root(context), "$id")
        if (source.exists()) {
            val meta = JSONObject(File(source,"meta.json").readText())
            val text = File(source,"transcript.json").takeIf { it.exists() }?.readText()?.let(::JSONArray) ?: JSONArray()
            val sound = File(source,"audio.wav")
            val a = JSONObject().put("schema",1).put("id","$id").put("deleted_at",System.currentTimeMillis()).put("meta",meta).put("transcript",text)
                .put("audio_md5",if(sound.exists()) md5(sound) else JSONObject.NULL).put("audio_size",if(sound.exists()) sound.length() else JSONObject.NULL)
            val bytes = a.toString().toByteArray(); decode(bytes)
            target.mkdirs(); check(!File(target,"local-original").exists()) { "Another local copy is already archived" }
            if (!File(target,"archive.json").exists()) atomic(File(target,"archive.json"),bytes)
            check(source.renameTo(File(target,"local-original"))) { "Could not move recording to trash" }
        }
        if(outgoing) { check(File(target,"archive.json").exists()); atomic(File(target,"outgoing"),byteArrayOf(49)) }
    }
    fun rows(context: Context): List<Row> = (root(context).listFiles() ?: emptyArray()).mapNotNull { dir ->
        if(File(dir,"restored.json").exists()) null else runCatching {
            val a=decode(File(dir,"archive.json").readBytes())
            Row(a.getString("id").toLong(),a.getJSONObject("meta").optString("title",a.getString("id")),a.getLong("deleted_at"),File(dir,"restore-request").exists())
        }.getOrNull()
    }.sortedByDescending { it.deletedAt }
    fun pending(context: Context): List<Long> = (root(context).listFiles() ?: emptyArray()).filter { File(it,"outgoing").exists() && !File(it,"deleted-sent").exists() }.mapNotNull { it.name.toLongOrNull() }
    fun deletedSent(context: Context,id: Long) { val dir=File(root(context),"$id"); if(dir.exists()) atomic(File(dir,"deleted-sent"),byteArrayOf(49)) }
    fun restore(context: Context, id: Long): Long = SyncManager.localChange {
        val dir=File(root(context),"$id"); val a=decode((File(dir,"cloud-archive.json").takeIf { it.exists() } ?: File(dir,"archive.json")).readBytes()); val newId=restoreId(a.getString("id")); val target=MeetingStore.dir(context,newId)
        if(!target.exists()) {
            val stage=File(root(context),"restore-$newId"); stage.mkdirs()
            val meta=a.getJSONObject("meta").put("updated",a.getLong("deleted_at")).put("restored_from","$id")
            atomic(File(stage,"meta.json"),meta.toString().toByteArray()); atomic(File(stage,"transcript.json"),a.getJSONArray("transcript").toString().toByteArray())
            if(audio(dir).exists() && (a.isNull("audio_md5") || matchesAudio(audio(dir),a))) audio(dir).copyTo(File(stage,"audio.wav"),overwrite=true)
            else if(File(stage,"audio.wav").exists()) check(File(stage,"audio.wav").delete())
            atomic(File(dir,"restore-request"),"$newId".toByteArray()); target.parentFile!!.mkdirs()
            check(stage.renameTo(target)) { "Could not restore recording" }
        } else { check(File(dir,"restore-request").exists() || File(dir,"restored.json").exists()) { "Restore identity collision" } }
        atomic(File(dir,"restore-request"),"$newId".toByteArray()); if(!SyncManager.connected(context)) atomic(File(dir,"restored.json"),"{}".toByteArray()); SyncManager.touch(context); MeetingService.onChange?.invoke(); newId
    }
    private fun put(cloud: Cloud.Provider, token: String, folder: Cloud.Folder, name: String, bytes: ByteArray) {
        val hash=SyncEngine.md5(bytes); val existing=cloud.list(token,folder).find { it.name==name }
        if(existing!=null) check(existing.md5==hash) { "Concurrent archive differs" } else cloud.upload(token,folder,name,bytes)
        val check=cloud.list(token,folder).first { it.name==name }; check(check.md5==hash && check.size==bytes.size.toLong()) { "Archive verification failed" }
    }
    fun publish(context: Context,id: Long,cloud: Cloud.Provider,token: String,includeAudio: Boolean) {
        publishArchive(File(root(context),"$id"),id,cloud,token,includeAudio)
    }
    internal fun publishArchive(dir: File,id: Long,cloud: Cloud.Provider,token: String,includeAudio: Boolean) {
        val existing=cloud.list(token,Cloud.Folder.MEETINGS).find { it.name==archiveName("$id") }
        if(existing!=null) {
            val bytes=cloud.download(token,Cloud.Folder.MEETINGS,existing.name); check(SyncEngine.md5(bytes)==existing.md5)
            val a=decode(bytes); check(a.getString("id")=="$id")
            if(!a.isNull("audio_md5")) { val sound=cloud.list(token,Cloud.Folder.AUDIO).first { it.name==audioName("$id") }; check(sound.md5==a.getString("audio_md5") && sound.size==a.getLong("audio_size")) }
            atomic(File(dir,"cloud-archive.json"),bytes)
            return
        }
        val a=decode(File(dir,"archive.json").readBytes())
        val original=cloud.list(token,Cloud.Folder.AUDIO).find { it.name=="$id.wav" }
        if(original!=null) {
            val sound=audio(dir)
            if(!sound.exists() || md5(sound)!=original.md5) {
                val temp=File(dir,"audio.download"); cloud.downloadFile(token,Cloud.Folder.AUDIO,original.name,temp)
                check(md5(temp)==original.md5 && temp.length()==original.size); check(temp.renameTo(File(dir,"audio.wav")))
            }
            a.put("audio_md5",original.md5).put("audio_size",original.size)
        }
        if(includeAudio || original!=null) {
            if(!a.isNull("audio_md5")) {
                val hash=a.getString("audio_md5"); val sound=audio(dir); check(md5(sound)==hash)
                val old=cloud.list(token,Cloud.Folder.AUDIO).find { it.name==audioName("$id") }
                if(old!=null) check(old.md5==hash) else cloud.uploadFile(token,Cloud.Folder.AUDIO,audioName("$id"),sound)
                val check=cloud.list(token,Cloud.Folder.AUDIO).first { it.name==audioName("$id") }; check(check.md5==hash && check.size==a.getLong("audio_size"))
            }
        } else { a.put("audio_md5",JSONObject.NULL).put("audio_size",JSONObject.NULL) }
        val bytes=a.toString().toByteArray()
        put(cloud,token,Cloud.Folder.MEETINGS,archiveName("$id"),bytes); atomic(File(dir,"cloud-archive.json"),bytes)
    }
    fun sync(context: Context,cloud: Cloud.Provider,token: String,includeAudio: Boolean) {
        val listing=cloud.list(token,Cloud.Folder.MEETINGS)
        for(item in listing) {
            val match=Regex("trash-v1-([0-9]+)\\.json").matchEntire(item.name) ?: continue
            val id=match.groupValues[1]; if(!validId(id)) continue
            val bytes=cloud.download(token,Cloud.Folder.MEETINGS,item.name); check(SyncEngine.md5(bytes)==item.md5)
            val a=decode(bytes); check(a.getString("id")==id); val dir=File(root(context),id); dir.mkdirs()
            if(!File(dir,"archive.json").exists()) atomic(File(dir,"archive.json"),bytes)
            atomic(File(dir,"cloud-archive.json"),bytes)
            if(includeAudio || File(dir,"restore-request").exists()) ensureArchiveAudio(dir,a,cloud,token)
            listing.find { it.name==restoredName(id) }?.let { marker ->
                val data=cloud.download(token,Cloud.Folder.MEETINGS,marker.name); check(SyncEngine.md5(data)==marker.md5)
                val m=JSONObject(String(data)); check(m.getInt("schema")==1 && m.getString("id")==id && m.getString("restored_id")==restoreId(id).toString())
                atomic(File(dir,"restored.json"),data)
            }
        }
        for(row in rows(context).filter { it.restoring }) {
            val id=row.id.toString(); val restored=restoreId(id); val dir=File(root(context),id); val manifest=File(dir,"cloud-archive.json"); val a=decode((manifest.takeIf { it.exists() } ?: File(dir,"archive.json")).readBytes())
            if(!manifest.exists()) a.put("audio_md5",JSONObject.NULL).put("audio_size",JSONObject.NULL)
            if(!a.isNull("audio_md5")) {
                val source=audio(dir); check(source.exists() && md5(source)==a.getString("audio_md5"))
                val restoredAudio=File(MeetingStore.dir(context,restored),"audio.wav"); if(!restoredAudio.exists()) source.copyTo(restoredAudio)
                if(cloud.list(token,Cloud.Folder.AUDIO).none { it.name=="$restored.wav" }) cloud.uploadFile(token,Cloud.Folder.AUDIO,"$restored.wav",source)
            }
            val audioReady=a.isNull("audio_md5") || cloud.list(token,Cloud.Folder.AUDIO).any { it.name=="$restored.wav" && it.md5==a.getString("audio_md5") }
            if(audioReady && listing.any { it.name=="$restored.meta.json" } && listing.any { it.name=="$restored.transcript.json" }) {
                // Stable JSON key order is immaterial for decoding; existing markers are verified by content above.
                if(!File(dir,"restored.json").exists()) {
                    val marker="{\"id\":\"$id\",\"restored_id\":\"$restored\",\"schema\":1}".toByteArray()
                    put(cloud,token,Cloud.Folder.MEETINGS,restoredName(id),marker); atomic(File(dir,"restored.json"),marker)
                }
            }
        }
    }
}
