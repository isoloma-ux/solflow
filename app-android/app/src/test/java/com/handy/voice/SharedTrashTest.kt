package com.handy.voice
import org.junit.Test
import org.junit.Assert.*
import org.json.JSONObject
import java.io.File
import java.nio.file.Files

class SharedTrashTest {
    private class Memory: Cloud.Provider {
        var failDelete = false
        val files = mutableMapOf<Pair<Cloud.Folder,String>,ByteArray>()
        override val id="synthetic"; override val title="Synthetic"; override val configured=true
        override fun deviceCode(deviceName: String, deviceId: String): Cloud.DeviceCode = error("not used")
        override fun pollToken(code: Cloud.DeviceCode): Cloud.Poll = error("not used")
        override fun refresh(refreshToken: String): Cloud.Tokens = error("not used")
        override fun revoke(token: String) {}; override fun account(token: String)="synthetic"
        override fun prepare(token: String) {}
        override fun list(token: String,folder: Cloud.Folder)=files.filterKeys { it.first==folder }.map { (k,v)->Cloud.RemoteFile(k.second,SyncEngine.md5(v),1,v.size.toLong()) }
        override fun upload(token: String,folder: Cloud.Folder,name: String,bytes: ByteArray) { files[folder to name]=bytes }
        override fun uploadFile(token: String,folder: Cloud.Folder,name: String,file: File)=upload(token,folder,name,file.readBytes())
        override fun download(token: String,folder: Cloud.Folder,name: String)=files.getValue(folder to name)
        override fun downloadFile(token: String,folder: Cloud.Folder,name: String,target: File)=target.writeBytes(download(token,folder,name))
        override fun delete(token: String,folder: Cloud.Folder,name: String) { check(!failDelete) { "synthetic interruption" }; check(files.containsKey(Cloud.Folder.MEETINGS to "trash-v1-100.purged.json")); files.remove(folder to name) }
    }
    private fun fixture(): File {
        val dir=Files.createTempDirectory("solflow-trash-").toFile()
        File(dir,"archive.json").writeText("""{"schema":1,"id":"100","deleted_at":1000,"meta":{"title":"Keep","future":true},"transcript":[],"audio_md5":null,"audio_size":null}""")
        return dir
    }
    @Test fun identityBoundaries() {
        for(id in listOf("", "0", "01", "../1", "-1", "9223372036854775808")) assertFalse(SharedTrash.validId(id))
        assertTrue(SharedTrash.validId("100")); assertNotEquals(SharedTrash.restoreId("100"),SharedTrash.restoreId("101")); assertTrue(SharedTrash.restoreId("100")<9_007_199_254_740_992)
    }
    @Test fun cloudAudioPreservedEvenWithAudioSyncDisabled() {
        val dir=fixture(); try {
            val cloud=Memory(); cloud.upload("",Cloud.Folder.AUDIO,"100.wav","synthetic sound".toByteArray())
            SharedTrash.publishArchive(dir,100,cloud,"",false); SharedTrash.publishArchive(dir,100,cloud,"",false)
            val archive=JSONObject(String(cloud.download("",Cloud.Folder.MEETINGS,"trash-v1-100.json")))
            assertTrue(archive.getJSONObject("meta").getBoolean("future")); assertFalse(archive.isNull("audio_md5"))
            assertArrayEquals(cloud.download("",Cloud.Folder.AUDIO,"100.wav"),cloud.download("",Cloud.Folder.AUDIO,"trash-v1-100.wav"))
        } finally { dir.deleteRecursively() }
    }
    @Test fun badArchiveAudioBlocksDeletion() {
        val dir=fixture(); try {
            val cloud=Memory(); cloud.upload("",Cloud.Folder.AUDIO,"100.wav","sound".toByteArray())
            SharedTrash.publishArchive(dir,100,cloud,"",true)
            cloud.upload("",Cloud.Folder.AUDIO,"trash-v1-100.wav","corrupt".toByteArray())
            assertThrows(IllegalStateException::class.java) { SharedTrash.publishArchive(dir,100,cloud,"",true) }
            assertTrue(File(dir,"archive.json").exists())
        } finally { dir.deleteRecursively() }
    }
    @Test fun staleAudioReplacedWithoutLosingOriginal() {
        val dir=fixture(); try {
            val cloud=Memory(); val canonical="canonical cloud sound".toByteArray()
            File(dir,"local-original").mkdirs(); File(dir,"local-original/audio.wav").writeText("old local sound")
            File(dir,"audio.wav").writeText("stale cache")
            cloud.upload("",Cloud.Folder.AUDIO,"trash-v1-100.wav",canonical)
            val archive=SharedTrash.decode(File(dir,"archive.json").readBytes()).put("audio_md5",SyncEngine.md5(canonical)).put("audio_size",canonical.size)
            SharedTrash.ensureArchiveAudio(dir,archive,cloud,"")
            assertArrayEquals(canonical,File(dir,"audio.wav").readBytes())
            assertEquals("old local sound",File(dir,"local-original/audio.wav").readText())
            File(dir,"audio.wav").writeText("old cache again"); cloud.upload("",Cloud.Folder.AUDIO,"trash-v1-100.wav","broken".toByteArray())
            assertThrows(IllegalStateException::class.java) { SharedTrash.ensureArchiveAudio(dir,archive,cloud,"") }
            assertEquals("old cache again",File(dir,"audio.wav").readText())
        } finally { dir.deleteRecursively() }
    }

    private fun purgeFixture(): Pair<File,Memory> {
        val base=fixture(); val dir=File(base,"100"); dir.mkdirs()
        check(File(base,"archive.json").renameTo(File(dir,"archive.json"))); File(dir,"purge-request").writeText("1")
        val cloud=Memory(); SharedTrash.publishArchive(dir,100,cloud,"",false); return base to cloud
    }
    @Test fun durablePurgePreservesUnselectedAndRepairsStaleUpload() {
        val (base,cloud)=purgeFixture(); try {
            cloud.upload("",Cloud.Folder.MEETINGS,"trash-v1-101.json","unselected".toByteArray())
            cloud.upload("",Cloud.Folder.AUDIO,"100.wav","original sound".toByteArray())
            SharedTrash.purgeAt(base,cloud,"")
            assertFalse(File(base,"100").exists()); assertTrue(File(base,"purged/100.json").exists())
            assertEquals("""{"id":"100","schema":1}""",String(cloud.download("",Cloud.Folder.MEETINGS,"trash-v1-100.purged.json")))
            assertFalse(cloud.files.containsKey(Cloud.Folder.AUDIO to "100.wav"))
            cloud.upload("",Cloud.Folder.MEETINGS,"trash-v1-100.json","stale".toByteArray()); SharedTrash.purgeAt(base,cloud,"")
            assertFalse(cloud.files.containsKey(Cloud.Folder.MEETINGS to "trash-v1-100.json"))
            assertEquals("unselected",String(cloud.download("",Cloud.Folder.MEETINGS,"trash-v1-101.json")))
        } finally { base.deleteRecursively() }
    }
    @Test fun interruptedPurgeKeepsLocalPayloadAndRetries() {
        val (base,cloud)=purgeFixture(); try {
            cloud.failDelete=true; assertThrows(IllegalStateException::class.java) { SharedTrash.purgeAt(base,cloud,"") }
            assertTrue(File(base,"100/archive.json").exists()); assertTrue(cloud.files.containsKey(Cloud.Folder.MEETINGS to "trash-v1-100.purged.json"))
            cloud.failDelete=false; SharedTrash.purgeAt(base,cloud,""); assertFalse(File(base,"100").exists())
        } finally { base.deleteRecursively() }
    }
    @Test fun invalidPurgeMarkerCannotRemovePayload() {
        val (base,cloud)=purgeFixture(); try {
            cloud.upload("",Cloud.Folder.MEETINGS,"trash-v1-100.purged.json","""{"id":"101","schema":1}""".toByteArray())
            assertThrows(IllegalStateException::class.java) { SharedTrash.purgeAt(base,cloud,"") }
            assertTrue(File(base,"100/archive.json").exists()); assertTrue(cloud.files.containsKey(Cloud.Folder.MEETINGS to "trash-v1-100.json"))
        } finally { base.deleteRecursively() }
    }
    @Test fun concurrentRestoreIsPreserved() {
        val (base,cloud)=purgeFixture(); try {
            val name="${SharedTrash.restoreId("100")}.meta.json"; cloud.upload("",Cloud.Folder.MEETINGS,name,"restored".toByteArray())
            SharedTrash.purgeAt(base,cloud,""); assertFalse(File(base,"purged/100.json").exists()); assertFalse(File(base,"100/purge-request").exists())
            assertEquals("restored",String(cloud.download("",Cloud.Folder.MEETINGS,name)))
        } finally { base.deleteRecursively() }
    }
    @Test fun receivedPurgeClearsWithoutLocalRequest() {
        val (base,cloud)=purgeFixture(); try {
            File(base,"100/purge-request").delete(); cloud.upload("",Cloud.Folder.MEETINGS,"trash-v1-100.purged.json","""{"id":"100","schema":1}""".toByteArray())
            SharedTrash.purgeAt(base,cloud,""); assertFalse(File(base,"100").exists())
        } finally { base.deleteRecursively() }
    }

}
