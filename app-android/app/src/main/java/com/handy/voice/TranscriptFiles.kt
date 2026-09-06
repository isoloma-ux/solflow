package com.handy.voice

import java.io.File
import java.io.FileOutputStream
import java.nio.file.Files
import java.nio.file.StandardCopyOption
import java.util.UUID

/** Same local layout and safety rules as desktop transcript_store.rs. */
object TranscriptFiles {
    /** Flush a sibling, then replace in one atomic move. Never unlink the old file.
     * If the filesystem cannot do an atomic replacement, fail without a fallback
     * to truncate/copy. Android minSdk 26 supports java.nio.file.
     */
    fun atomicWrite(file: File, bytes: ByteArray) {
        val temp = File(file.parentFile, ".solflow-${UUID.randomUUID()}.tmp")
        check(temp.createNewFile()) { "Cannot create temporary file" }
        try {
            FileOutputStream(temp).use { output ->
                output.write(bytes)
                output.fd.sync()
            }
            Files.move(temp.toPath(), file.toPath(),
                StandardCopyOption.ATOMIC_MOVE, StandardCopyOption.REPLACE_EXISTING)
        } finally {
            temp.delete()
        }
    }

    fun readVisible(meeting: File): ByteArray {
        val canonical = File(meeting, "transcript.json")
        return try {
            Files.readAllBytes(canonical.toPath())
        } catch (e: java.nio.file.NoSuchFileException) {
            File(meeting, "transcript.partial.json").readBytes()
        }
    }

    class Draft(private val meeting: File) {
        val version = File(meeting, "transcript-versions/${System.currentTimeMillis()}-${UUID.randomUUID()}")
        init {
            check(meeting.isDirectory) { "Meeting disappeared" }
            check(version.mkdirs()) { "Cannot create transcript version" }
            val previous = File(version, "previous")
            check(previous.mkdir()) { "Cannot create transcript backup" }
            val entries = meeting.listFiles() ?: error("Cannot list meeting files")
            for (file in entries) {
                if (file.isFile && file.name.endsWith(".json")) {
                    atomicWrite(File(previous, file.name), file.readBytes())
                }
            }
        }

        fun checkpoint(clean: ByteArray, raw: ByteArray) {
            atomicWrite(File(version, "raw-transcript.json"), raw)
            atomicWrite(File(version, "transcript.json"), clean)
            atomicWrite(File(meeting, "transcript.partial.json"), clean)
        }

        /** Caller must check cancellation and nonempty output first. */
        fun commit() {
            atomicWrite(File(meeting, "transcript.json"), File(version, "transcript.json").readBytes())
            File(meeting, "transcript.partial.json").delete()
        }
    }
}
