package com.handy.voice

import android.content.Context
import android.util.Log
import com.k2fsa.sherpa.onnx.FastClusteringConfig
import com.k2fsa.sherpa.onnx.OfflineSpeakerDiarization
import com.k2fsa.sherpa.onnx.OfflineSpeakerDiarizationConfig
import com.k2fsa.sherpa.onnx.OfflineSpeakerDiarizationSegment
import com.k2fsa.sherpa.onnx.OfflineSpeakerSegmentationModelConfig
import com.k2fsa.sherpa.onnx.OfflineSpeakerSegmentationPyannoteModelConfig
import com.k2fsa.sherpa.onnx.SpeakerEmbeddingExtractor
import com.k2fsa.sherpa.onnx.SpeakerEmbeddingExtractorConfig
import java.io.File
import java.net.HttpURLConnection
import java.net.URL
import kotlin.math.sqrt

/**
 * Разделение говорящих. Языка оно не знает: pyannote находит границы речи
 * и смен голоса, CAM++ считает «отпечаток» тембра, кластеризация собирает
 * отпечатки в говорящих. Проверено на этом телефоне на русской речи:
 * границы двух чтецов нашлись с точностью до долей секунды, порог 0.7 дал
 * правильное число голосов и на двух, и на четырёх (int8-сегментация при
 * этом ошибалась и была медленнее — поэтому float).
 *
 * Двухчасовой файл в память не влезает, поэтому диаризация идёт окнами по
 * десять минут. Внутри окна говорящих считает sherpa-onnx, а между окнами
 * они сшиваются вручную: у каждого локального говорящего берётся до десяти
 * секунд его речи, по ним считается эмбеддинг, и близкие эмбеддинги из
 * разных окон объявляются одним человеком.
 */
object Diarizer {

    private const val TAG = "HandyVoice"

    /** Порог кластеризации внутри окна — подобран на тестах, см. выше. */
    private const val CLUSTER_THRESHOLD = 0.7f

    /**
     * Если даже самое последнее, самое дорогое слияние дешевле этого порога,
     * вся запись — один голос: скачка дистанций в таких данных не бывает.
     * Замер на двух чтецах: свои пары 0.13–0.20, чужие от 0.25.
     */
    private const val SINGLE_VOICE_DISTANCE = 0.3f

    private const val WINDOW_SEC = 600
    private const val EMBED_SPEECH_SEC = 10f
    private const val THREADS = 4

    /** Сколько речи делает локального говорящего «крупным» — см. [autoTarget]. */
    private const val BIG_SPEAKER_SEC = 30f

    // --- модели -----------------------------------------------------------

    /**
     * Сегментация маленькая (6 МБ) и едет прямо в APK как asset — качать
     * надо только эмбеддинг: его 28 МБ раздували бы APK зря.
     */
    private const val SEG_ASSET = "segmentation.onnx"
    private const val EMB_FILENAME = "embedding.onnx"
    private const val EMB_URL =
        "https://github.com/k2-fsa/sherpa-onnx/releases/download/speaker-recongition-models/3dspeaker_speech_campplus_sv_zh_en_16k-common_advanced.onnx"
    private const val EMB_BYTES = 28_281_164L

    /** Размер докачки — для текста на кнопке и в сообщениях. */
    const val DOWNLOAD_MB = 27

    private fun dir(context: Context) = File(context.filesDir, "diarization").apply { mkdirs() }
    private fun segFile(context: Context) = File(dir(context), SEG_ASSET)
    private fun embFile(context: Context) = File(dir(context), EMB_FILENAME)

    fun modelsReady(context: Context): Boolean = embFile(context).length() == EMB_BYTES

    /** ONNX читает модель по пути файла, из asset-потока не умеет. */
    private fun ensureSegmentation(context: Context): File {
        val f = segFile(context)
        val expected = context.assets.open(SEG_ASSET).use { input ->
            var total = 0L
            val buf = ByteArray(1 shl 16)
            while (true) {
                val n = input.read(buf)
                if (n < 0) break
                total += n
            }
            total
        }
        if (f.length() != expected) {
            context.assets.open(SEG_ASSET).use { input ->
                f.outputStream().use { input.copyTo(it) }
            }
        }
        return f
    }

    fun download(
        context: Context,
        onProgress: (Int) -> Unit,
        isCancelled: () -> Boolean,
    ) {
        val conn = (URL(EMB_URL).openConnection() as HttpURLConnection).apply {
            instanceFollowRedirects = true
            connectTimeout = 8_000
            readTimeout = 60_000
        }
        try {
            if (conn.responseCode !in 200..299) error("сервер ответил ${conn.responseCode}")
            val tmp = File(dir(context), "$EMB_FILENAME.part")
            conn.inputStream.use { input ->
                tmp.outputStream().use { output ->
                    val buf = ByteArray(1 shl 16)
                    var done = 0L
                    var lastPct = -1
                    while (true) {
                        if (isCancelled()) error("отменено")
                        val n = input.read(buf)
                        if (n < 0) break
                        output.write(buf, 0, n)
                        done += n
                        val pct = (done * 100 / EMB_BYTES).toInt().coerceIn(0, 100)
                        if (pct != lastPct) {
                            lastPct = pct
                            onProgress(pct)
                        }
                    }
                }
            }
            if (tmp.length() != EMB_BYTES) {
                tmp.delete()
                error("размер не сошёлся: ${tmp.length()}")
            }
            if (!tmp.renameTo(embFile(context))) error("не удалось сохранить модель")
        } finally {
            conn.disconnect()
        }
    }

    // --- сам разбор -------------------------------------------------------

    /** Отрезок «этот человек говорил тут», в секундах всей записи. */
    private data class Turn(val start: Float, val end: Float, val speaker: Int)

    /**
     * Прогоняет диаризацию по WAV встречи и возвращает найденное число
     * говорящих. [numSpeakers] — сколько людей было (0 — определить самой).
     * Разметка пишется прямо в transcript.json встречи.
     */
    fun run(
        context: Context,
        meetingId: Long,
        numSpeakers: Int,
        onProgress: (Int) -> Unit,
        isCancelled: () -> Boolean,
    ): Int {
        val segments = MeetingStore.loadTranscript(context, meetingId)
        if (segments.isEmpty()) error("расшифровки нет")

        val segModel = ensureSegmentation(context).absolutePath
        val embModel = embFile(context).absolutePath

        val sd = OfflineSpeakerDiarization(
            config = OfflineSpeakerDiarizationConfig(
                segmentation = OfflineSpeakerSegmentationModelConfig(
                    pyannote = OfflineSpeakerSegmentationPyannoteModelConfig(model = segModel),
                    numThreads = THREADS,
                ),
                embedding = SpeakerEmbeddingExtractorConfig(
                    model = embModel,
                    numThreads = THREADS,
                ),
                clustering = FastClusteringConfig(
                    numClusters = if (numSpeakers > 0) numSpeakers else -1,
                    threshold = CLUSTER_THRESHOLD,
                ),
            ),
        )

        try {
            val turns = WavReader(MeetingStore.audioFile(context, meetingId)).use { wav ->
                diarizeWindowed(embModel, wav, sd, numSpeakers, onProgress, isCancelled)
            }
            if (isCancelled()) error("отменено")

            val labelled = assignSpeakers(segments, turns)
            val speakerCount = (labelled.mapNotNull { it.speaker }.maxOrNull() ?: -1) + 1
            Log.i(
                TAG,
                "диаризация: отрезков ${turns.size} " +
                    "(голосов в отрезках ${turns.map { it.speaker }.distinct().size}), " +
                    "реплик ${labelled.size}, говорящих в репликах $speakerCount",
            )
            MeetingStore.saveTranscript(context, meetingId, labelled)
            return speakerCount
        } finally {
            sd.release()
        }
    }

    /** Окна по десять минут; одно окно — если запись короче двенадцати. */
    private fun diarizeWindowed(
        embModel: String,
        wav: WavReader,
        sd: OfflineSpeakerDiarization,
        numSpeakers: Int,
        onProgress: (Int) -> Unit,
        isCancelled: () -> Boolean,
    ): List<Turn> {
        val sr = AudioRecorder.SAMPLE_RATE
        val window = WINDOW_SEC * sr
        val total = wav.totalSamples
        val singleWindow = total <= window.toLong() * 12 / 10
        val windowCount =
            if (singleWindow) 1 else ((total + window - 1) / window).toInt()

        // Экстрактор нужен только для сшивки — на одном окне не создаём.
        val extractor = if (windowCount > 1) {
            SpeakerEmbeddingExtractor(
                config = SpeakerEmbeddingExtractorConfig(
                    model = embModel,
                    numThreads = THREADS,
                ),
            )
        } else null

        // Сшивка идёт после всех окон: сначала собираем по эмбеддингу на
        // каждого локального говорящего каждого окна, потом кластеризуем их
        // разом. Инкрементальное прикрепление к первому похожему центроиду
        // здесь не работало: имея все точки сразу, ошибиться труднее.
        val localTurns = mutableListOf<Triple<Int, Int, Turn>>() // окно, локальный, отрезок
        val embeddings = mutableListOf<Pair<Pair<Int, Int>, FloatArray>>()
        val speechSec = mutableMapOf<Pair<Int, Int>, Float>()

        try {
            for (w in 0 until windowCount) {
                val offset = w.toLong() * window
                val count =
                    if (singleWindow) total.toInt() else minOf(window, (total - offset).toInt())
                val samples = wav.read(offset, count)

                val local = sd.processWithCallback(samples, { done, chunkTotal, _ ->
                    val share = if (chunkTotal > 0) done.toFloat() / chunkTotal else 0f
                    onProgress(((w + share) * 100 / windowCount).toInt().coerceIn(0, 99))
                    if (isCancelled()) 1 else 0
                })
                if (isCancelled()) return emptyList()

                for (s in local) {
                    localTurns += Triple(
                        w, s.speaker,
                        Turn(
                            start = offset.toFloat() / sr + s.start,
                            end = offset.toFloat() / sr + s.end,
                            speaker = 0,
                        ),
                    )
                    val key = w to s.speaker
                    speechSec[key] = (speechSec[key] ?: 0f) + (s.end - s.start)
                }
                if (extractor != null) {
                    for (speaker in local.map { it.speaker }.distinct()) {
                        embeddings += (w to speaker) to
                            speakerEmbedding(extractor, samples, local, speaker)
                    }
                }
            }
        } finally {
            extractor?.release()
        }

        onProgress(99)
        if (extractor == null) {
            return localTurns.map { (_, local, turn) -> turn.copy(speaker = local) }
        }

        val clusterOf = cluster(
            embeddings.map { it.second },
            embeddings.map { speechSec[it.first] ?: 0f },
            numSpeakers,
        )
        val globalOf = embeddings.mapIndexed { i, (key, _) -> key to clusterOf[i] }.toMap()
        return localTurns.map { (w, local, turn) ->
            turn.copy(speaker = globalOf[w to local] ?: 0)
        }
    }

    /** Эмбеддинг говорящего по его самым длинным репликам, до десяти секунд. */
    private fun speakerEmbedding(
        extractor: SpeakerEmbeddingExtractor,
        samples: FloatArray,
        local: Array<OfflineSpeakerDiarizationSegment>,
        speaker: Int,
    ): FloatArray {
        val sr = AudioRecorder.SAMPLE_RATE
        val speech = local.filter { it.speaker == speaker }
            .sortedByDescending { it.end - it.start }
        var need = (EMBED_SPEECH_SEC * sr).toInt()

        val stream = extractor.createStream()
        for (s in speech) {
            if (need <= 0) break
            val from = (s.start * sr).toInt().coerceIn(0, samples.size)
            val to = (s.end * sr).toInt().coerceIn(from, samples.size)
            val take = minOf(to - from, need)
            if (take > 0) {
                stream.acceptWaveform(samples.copyOfRange(from, from + take), sr)
                need -= take
            }
        }
        stream.inputFinished()
        val embedding = extractor.compute(stream)
        stream.release()
        return embedding
    }

    /**
     * Агломеративная кластеризация эмбеддингов (average linkage, косинусная
     * дистанция). Точек мало — пар «окно, говорящий» даже у двухчасовой
     * записи десятки, так что квадратичность не страшна.
     *
     * С заданным числом говорящих сливаем до ровно K кластеров. В авторежиме
     * жёсткий порог не работает: дистанции «свой—чужой» гуляют от записи к
     * записи. Вместо него — самый большой скачок в цене слияний: пока
     * склеиваются куски одного голоса, слияния дешёвые, а первое склеивание
     * двух разных людей заметно дороже предыдущего.
     */
    private fun cluster(
        points: List<FloatArray>,
        speechSec: List<Float>,
        numSpeakers: Int,
    ): IntArray {
        // Матрица дистанций в лог: по ней калибруются пороги.
        for (i in points.indices) {
            val row = points.indices.joinToString(" ") {
                "%.2f".format(1f - cosine(points[i], points[it]))
            }
            Log.i(TAG, "сшивка: дистанции[$i] (%.0f с): $row".format(speechSec[i]))
        }

        // Кластеризуются только крупные говорящие: осколки с парой секунд
        // речи шумные и сцепляют чужие кластеры друг с другом — на них
        // сшивка уже склеивала двух людей в одного. Осколки прикрепляются
        // к готовым кластерам в конце.
        val big = points.indices.filter { speechSec[it] >= BIG_SPEAKER_SEC }
            .ifEmpty { points.indices.toList() }
        val bigPoints = big.map { points[it] }

        val target = if (numSpeakers > 0) {
            numSpeakers.coerceAtMost(big.size)
        } else {
            autoTarget(bigPoints)
        }
        Log.i(
            TAG,
            "сшивка: целевое число говорящих $target, крупных ${big.size} из ${points.size}",
        )

        val clusters = agglomerate(bigPoints, target, null)
        val labels = IntArray(points.size) { -1 }
        for ((index, members) in clusters.withIndex()) {
            for (p in members) labels[big[p]] = index
        }
        for (i in points.indices) {
            if (labels[i] >= 0) continue
            labels[i] = clusters.indices.minByOrNull { c ->
                clusters[c].map { 1.0 - cosine(points[i], bigPoints[it]) }.average()
            } ?: 0
        }
        return labels
    }

    private fun agglomerate(
        points: List<FloatArray>,
        target: Int,
        merges: MutableList<Float>?,
    ): List<List<Int>> {
        val clusters = points.indices.map { mutableListOf(it) }.toMutableList()

        fun linkage(a: List<Int>, b: List<Int>): Float {
            var sum = 0f
            for (i in a) for (j in b) sum += 1f - cosine(points[i], points[j])
            return sum / (a.size * b.size)
        }

        while (clusters.size > target) {
            var bi = -1
            var bj = -1
            var bestDist = Float.MAX_VALUE
            for (i in clusters.indices) {
                for (j in i + 1 until clusters.size) {
                    val d = linkage(clusters[i], clusters[j])
                    if (d < bestDist) {
                        bestDist = d
                        bi = i
                        bj = j
                    }
                }
            }
            merges?.add(bestDist)
            clusters[bi].addAll(clusters[bj])
            clusters.removeAt(bj)
        }
        return clusters
    }

    /**
     * Сколько людей в записи — по следу слияний полной агломерации крупных
     * говорящих: пока склеиваются куски одного голоса, слияния дешёвые, а
     * первое склеивание двух разных людей заметно дороже предыдущего.
     */
    private fun autoTarget(bigPoints: List<FloatArray>): Int {
        if (bigPoints.size <= 1) return 1

        val merges = mutableListOf<Float>()
        agglomerate(bigPoints, 1, merges)
        Log.i(TAG, "сшивка: след слияний крупных ${merges.joinToString { "%.3f".format(it) }}")

        // Все слияния дешёвые — один голос; все дорогие — все голоса разные.
        if (merges.last() < SINGLE_VOICE_DISTANCE) return 1
        if (merges.first() >= SINGLE_VOICE_DISTANCE) return bigPoints.size

        // Есть переход от дешёвых слияний к дорогим: режем по самому
        // большому скачку, выполняются только слияния до него.
        var cut = 0
        var bestGap = 0f
        for (i in 0 until merges.size - 1) {
            val gap = merges[i + 1] - merges[i]
            if (gap >= bestGap) {
                bestGap = gap
                cut = i
            }
        }
        return bigPoints.size - (cut + 1)
    }

    private fun cosine(a: FloatArray, b: FloatArray): Float {
        var dot = 0f
        var na = 0f
        var nb = 0f
        for (i in a.indices) {
            dot += a[i] * b[i]
            na += a[i] * a[i]
            nb += b[i] * b[i]
        }
        val d = sqrt(na) * sqrt(nb)
        return if (d > 0) dot / d else 0f
    }

    /**
     * Каждой реплике таймлайна — говорящий с наибольшим пересечением по
     * времени. Реплика без пересечений наследует говорящего предыдущей:
     * это обычно короткий хвост на паузе.
     *
     * Номера идут по порядку появления в разговоре: «говорящий 1» — тот,
     * кто заговорил первым, а не кого кластеризация посчитала первым.
     */
    private fun assignSpeakers(
        segments: List<MeetingSegment>,
        turns: List<Turn>,
    ): List<MeetingSegment> {
        var previous = 0
        val raw = segments.map { seg ->
            val speaker = turns
                .map { it.speaker to (minOf(seg.end, it.end) - maxOf(seg.start, it.start)) }
                .filter { it.second > 0f }
                .groupBy({ it.first }, { it.second })
                .maxByOrNull { (_, spans) -> spans.sum() }
                ?.key ?: previous
            previous = speaker
            seg.copy(speaker = speaker)
        }

        val order = mutableListOf<Int>()
        for (s in raw) {
            val spk = s.speaker ?: continue
            if (spk !in order) order += spk
        }
        return raw.map { it.copy(speaker = order.indexOf(it.speaker)) }
    }
}
