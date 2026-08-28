package com.handy.voice

import android.content.Context
import android.media.AudioFormat
import android.media.MediaCodec
import android.media.MediaExtractor
import android.media.MediaFormat
import android.net.Uri
import android.os.ParcelFileDescriptor
import java.nio.ByteOrder

/**
 * Приведение частоты к 16 кГц. Живёт между кусками декодера, поэтому хвост
 * входа хранится до следующего вызова.
 *
 * Вниз (44.1/48 → 16) каждый выходной отсчёт — среднее входных на своём
 * интервале: усреднение работает фильтром от алиасинга. Вверх (8 → 16) —
 * линейная интерполяция. Для распознавания речи этого достаточно, идеальный
 * полифазный фильтр здесь ничего бы не добавил.
 */
class Resampler(private val srcRate: Int, private val dstRate: Int) {

    private val ratio = srcRate.toDouble() / dstRate
    private var pending = FloatArray(0)
    private var position = 0.0

    fun process(input: FloatArray): FloatArray {
        if (srcRate == dstRate) return input

        val src = FloatArray(pending.size + input.size)
        pending.copyInto(src)
        input.copyInto(src, pending.size)

        val out = ArrayList<Float>(src.size * dstRate / srcRate + 2)
        if (ratio > 1.0) {
            // Вниз: среднее по [position, position + ratio).
            while (position + ratio <= src.size) {
                val from = position.toInt()
                val to = (position + ratio).toInt().coerceAtMost(src.size)
                var sum = 0f
                for (i in from until to) sum += src[i]
                out += sum / (to - from).coerceAtLeast(1)
                position += ratio
            }
        } else {
            // Вверх: линейная интерполяция между соседями.
            while (position + 1 < src.size) {
                val i = position.toInt()
                val frac = (position - i).toFloat()
                out += src[i] * (1 - frac) + src[i + 1] * frac
                position += ratio
            }
        }

        // Всё до целой части position уже отработано — оставляем хвост.
        val consumed = position.toInt().coerceAtMost(src.size)
        position -= consumed
        pending = src.copyOfRange(consumed, src.size)

        return out.toFloatArray()
    }
}

/**
 * Импорт чужого аудио: декодирование системным MediaCodec (mp3, m4a, ogg,
 * flac, wav — всё, что умеет телефон), сведение в моно и ресемплинг в 16 кГц.
 * Результат пишется потоково в тот же WAV-формат, что и живая запись, дальше
 * файл неотличим от записанной встречи.
 */
object AudioImport {

    private const val TIMEOUT_US = 10_000L

    /**
     * Возвращает длительность в секундах или бросает исключение.
     *
     * [fd] — уже открытый файл. Он нужен для «Поделиться»: разрешение на
     * чужой `content://` живёт вместе с намерением, а импорт идёт в сервисе
     * и переживает уход из приложения — к этому времени читать по ссылке
     * уже нельзя, а по дескриптору можно.
     */
    fun run(
        context: Context,
        uri: Uri,
        out: WavWriter,
        onProgress: (Int) -> Unit,
        isCancelled: () -> Boolean,
        fd: ParcelFileDescriptor? = null,
    ): Float {
        val extractor = MediaExtractor()
        if (fd != null) extractor.setDataSource(fd.fileDescriptor)
        else extractor.setDataSource(context, uri, null)

        var trackIndex = -1
        var format: MediaFormat? = null
        for (i in 0 until extractor.trackCount) {
            val f = extractor.getTrackFormat(i)
            if (f.getString(MediaFormat.KEY_MIME).orEmpty().startsWith("audio/")) {
                trackIndex = i
                format = f
                break
            }
        }
        val trackFormat = format ?: run {
            extractor.release()
            error("аудиодорожки нет")
        }
        extractor.selectTrack(trackIndex)

        val durationUs = runCatching { trackFormat.getLong(MediaFormat.KEY_DURATION) }
            .getOrDefault(0L)

        val codec = MediaCodec.createDecoderByType(
            trackFormat.getString(MediaFormat.KEY_MIME)!!
        )
        codec.configure(trackFormat, null, null, 0)
        codec.start()

        var srcRate = trackFormat.getInteger(MediaFormat.KEY_SAMPLE_RATE)
        var channels = trackFormat.getInteger(MediaFormat.KEY_CHANNEL_COUNT)
        var floatPcm = false
        var resampler = Resampler(srcRate, AudioRecorder.SAMPLE_RATE)

        val info = MediaCodec.BufferInfo()
        var inputDone = false
        var outputDone = false
        var lastPercent = -1

        try {
            while (!outputDone) {
                if (isCancelled()) error("отменено")

                if (!inputDone) {
                    val inIndex = codec.dequeueInputBuffer(TIMEOUT_US)
                    if (inIndex >= 0) {
                        val buf = codec.getInputBuffer(inIndex)!!
                        val size = extractor.readSampleData(buf, 0)
                        if (size < 0) {
                            codec.queueInputBuffer(
                                inIndex, 0, 0, 0, MediaCodec.BUFFER_FLAG_END_OF_STREAM
                            )
                            inputDone = true
                        } else {
                            codec.queueInputBuffer(inIndex, 0, size, extractor.sampleTime, 0)
                            if (durationUs > 0) {
                                val pct = (extractor.sampleTime * 100 / durationUs).toInt()
                                if (pct != lastPercent) {
                                    lastPercent = pct
                                    onProgress(pct.coerceIn(0, 100))
                                }
                            }
                            extractor.advance()
                        }
                    }
                }

                when (val outIndex = codec.dequeueOutputBuffer(info, TIMEOUT_US)) {
                    MediaCodec.INFO_OUTPUT_FORMAT_CHANGED -> {
                        val f = codec.outputFormat
                        srcRate = f.getInteger(MediaFormat.KEY_SAMPLE_RATE)
                        channels = f.getInteger(MediaFormat.KEY_CHANNEL_COUNT)
                        floatPcm = runCatching { f.getInteger(MediaFormat.KEY_PCM_ENCODING) }
                            .getOrDefault(AudioFormat.ENCODING_PCM_16BIT) ==
                            AudioFormat.ENCODING_PCM_FLOAT
                        resampler = Resampler(srcRate, AudioRecorder.SAMPLE_RATE)
                    }
                    in 0..Int.MAX_VALUE -> {
                        val buf = codec.getOutputBuffer(outIndex)!!
                        if (info.size > 0) {
                            buf.position(info.offset)
                            buf.limit(info.offset + info.size)
                            val mono = downmix(buf.order(ByteOrder.nativeOrder()), channels, floatPcm)
                            writePcm16(out, resampler.process(mono))
                        }
                        codec.releaseOutputBuffer(outIndex, false)
                        if (info.flags and MediaCodec.BUFFER_FLAG_END_OF_STREAM != 0) {
                            outputDone = true
                        }
                    }
                }
            }
        } finally {
            runCatching { codec.stop() }
            codec.release()
            extractor.release()
        }

        return out.samplesWritten.toFloat() / AudioRecorder.SAMPLE_RATE
    }

    /** Сведение каналов в моно усреднением. */
    private fun downmix(
        buf: java.nio.ByteBuffer,
        channels: Int,
        floatPcm: Boolean,
    ): FloatArray {
        return if (floatPcm) {
            val fb = buf.asFloatBuffer()
            val frames = fb.remaining() / channels
            FloatArray(frames) { i ->
                var sum = 0f
                for (c in 0 until channels) sum += fb.get(i * channels + c)
                sum / channels
            }
        } else {
            val sb = buf.asShortBuffer()
            val frames = sb.remaining() / channels
            FloatArray(frames) { i ->
                var sum = 0f
                for (c in 0 until channels) sum += sb.get(i * channels + c) / 32768.0f
                sum / channels
            }
        }
    }

    private fun writePcm16(out: WavWriter, pcm: FloatArray) {
        if (pcm.isEmpty()) return
        val bytes = ByteArray(pcm.size * 2)
        for (i in pcm.indices) {
            val v = (pcm[i].coerceIn(-1f, 1f) * 32767).toInt()
            bytes[i * 2] = (v and 0xFF).toByte()
            bytes[i * 2 + 1] = (v shr 8 and 0xFF).toByte()
        }
        out.write(bytes, bytes.size)
    }
}
