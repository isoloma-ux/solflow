import com.handy.voice.TextCleanup
import com.handy.voice.TranscriptFiles
import com.handy.voice.SpeakerAttribution
import java.io.File
import java.nio.file.Files

fun main(args: Array<String>) {
    var checks = 0
    File(File(args[0]).parentFile, "speakers.tsv").readLines().forEach { line ->
        val c = line.split('\t')
        val bounds = c[1].split(';').map { v -> v.split(',').let { it[0].toFloat() to it[1].toFloat() } }
        val turns = if (c[2] == "-") emptyList() else c[2].split(';').map { v ->
            v.split(',').let { SpeakerAttribution.Turn(it[0].toFloat(), it[1].toFloat(), it[2].toInt()) }
        }
        val (labels, count) = SpeakerAttribution.assign(bounds, turns)
        val encoded = labels.joinToString(";") {
            if (it.voices.size > 1) "mix:" + it.voices.joinToString(",") else it.speaker?.toString() ?: "?"
        }
        check(encoded == c[3] && count == c[4].toInt()) { "Speaker fixture ${c[0]}: $encoded, $count" }
        checks++
    }
    check(SpeakerAttribution.bigSpeakerThreshold(60f) == 3f)
    check(SpeakerAttribution.bigSpeakerThreshold(3600f) == 30f)
    checks += 2
    File(args[0]).readLines().forEachIndexed { i, line ->
        val cols = line.split('\t')
        check(cols.size == 3) { "Malformed fixture ${i + 1}" }
        val actual = TextCleanup.clean(cols[1], cols[0] == "1")
        val expected = if (cols[2] == "<EMPTY>") "" else cols[2]
        check(actual == expected) { "Fixture ${i + 1}: expected <$expected>, got <$actual>" }
        checks++
    }
    fun scenario(body: (File) -> Unit) {
        val temp = Files.createTempDirectory("solflow-kotlin-safeguards-").toFile()
        try { body(temp); checks++ } finally { temp.deleteRecursively() }
    }
    scenario { dir ->
        File(dir, "transcript.json").writeText("old complete text")
        File(dir, "meta.json").writeText("old metadata")
        val draft = TranscriptFiles.Draft(dir)
        draft.checkpoint("partial".toByteArray(), "raw including ООО".toByteArray())
        check(TranscriptFiles.readVisible(dir).decodeToString() == "old complete text")
        check(File(draft.version, "previous/meta.json").readText() == "old metadata")
        check(File(draft.version, "raw-transcript.json").readText() == "raw including ООО")
        // No commit = cancellation/crash: old text remains authoritative.
    }
    scenario { dir ->
        val draft = TranscriptFiles.Draft(dir)
        draft.checkpoint("partial".toByteArray(), "raw".toByteArray())
        check(!File(dir, "transcript.json").exists())
        check(TranscriptFiles.readVisible(dir).decodeToString() == "partial")
    }
    scenario { dir ->
        File(dir, "transcript.json").writeText("old")
        val draft = TranscriptFiles.Draft(dir)
        draft.checkpoint("new complete".toByteArray(), "raw complete".toByteArray())
        draft.commit()
        check(TranscriptFiles.readVisible(dir).decodeToString() == "new complete")
        check(File(draft.version, "previous/transcript.json").readText() == "old")
        check(File(draft.version, "raw-transcript.json").readText() == "raw complete")
        check(!File(dir, "transcript.partial.json").exists())
    }
    scenario { dir ->
        File(dir, "transcript.json").writeText("old")
        val draft = TranscriptFiles.Draft(dir)
        File(draft.version, "transcript.json").mkdir()
        check(runCatching { draft.checkpoint("new".toByteArray(), "raw".toByteArray()) }.isFailure)
        check(runCatching { draft.commit() }.isFailure)
        check(TranscriptFiles.readVisible(dir).decodeToString() == "old")
    }
    scenario { dir ->
        val target = File(dir, "transcript.json")
        target.mkdir()
        File(target, "keep").writeText("safe")
        check(runCatching { TranscriptFiles.atomicWrite(target, "new".toByteArray()) }.isFailure)
        check(File(target, "keep").readText() == "safe")
        check(dir.listFiles()!!.size == 1)
    }
    scenario { dir ->
        val missing = File(dir, "deleted")
        check(runCatching { TranscriptFiles.Draft(missing) }.isFailure)
        check(runCatching { TranscriptFiles.atomicWrite(File(missing, "meta.json"), byteArrayOf()) }.isFailure)
        check(!missing.exists())
    }
    println("Kotlin: $checks checks passed (shared cleanup fixtures + real filesystem scenarios)")
}
