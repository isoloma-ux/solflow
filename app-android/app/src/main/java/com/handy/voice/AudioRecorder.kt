package com.handy.voice

import android.annotation.SuppressLint
import android.media.AudioDeviceInfo
import android.media.AudioFormat
import android.media.AudioRecord
import android.media.MediaRecorder
import java.io.ByteArrayOutputStream
import java.nio.ByteBuffer
import java.nio.ByteOrder

/**
 * Запись с микрофона сразу в 16 кГц моно — той частоте, которую ждёт
 * transcribe.cpp. Ресемплера в библиотеке нет, поэтому просить у Android
 * другую частоту нельзя.
 */
class AudioRecorder(
    private val roomMode: Boolean = false,
    /** Закреплённый в настройках микрофон; null — какой выберет система. */
    private val preferredDevice: AudioDeviceInfo? = null,
) {

    private var record: AudioRecord? = null
    @Volatile private var recording = false
    private val collected = ByteArrayOutputStream()

    val isRecording: Boolean get() = recording

    /** Текущая громкость в диапазоне [0, 1] — для волны на плавающей кнопке. */
    @Volatile
    var level: Float = 0f
        private set

    @SuppressLint("MissingPermission")
    fun start(): Boolean {
        val minBuffer = AudioRecord.getMinBufferSize(SAMPLE_RATE, CHANNEL, ENCODING)
        if (minBuffer <= 0) return false

        val r = AudioRecord(
            // VOICE_RECOGNITION заточен под голос у самого микрофона и давит
            // всё дальнее — речь собеседников в стороне до записи не доходила.
            // В режиме «комната» берём необработанный поток с микрофона.
            if (roomMode) MediaRecorder.AudioSource.MIC
            else MediaRecorder.AudioSource.VOICE_RECOGNITION,
            SAMPLE_RATE,
            CHANNEL,
            ENCODING,
            minBuffer * 4,
        )
        // Система выбирает микрофон сама и с наушниками нередко берёт не тот;
        // просьба необязательная — если устройство отвалилось, запись просто
        // пойдёт со штатного.
        preferredDevice?.let { runCatching { r.setPreferredDevice(it) } }

        if (r.state != AudioRecord.STATE_INITIALIZED) {
            r.release()
            return false
        }

        collected.reset()
        record = r
        recording = true
        r.startRecording()

        Thread {
            val buf = ByteArray(minBuffer)
            while (recording) {
                val n = r.read(buf, 0, buf.size)
                if (n > 0) {
                    synchronized(collected) { collected.write(buf, 0, n) }
                    level = levelOf(buf, n)
                }
            }
            level = 0f
        }.start()

        return true
    }

    /** Останавливает запись и отдаёт float32 PCM в диапазоне [-1, 1]. */
    fun stop(): FloatArray {
        recording = false
        record?.let {
            runCatching { it.stop() }
            it.release()
        }
        record = null

        val bytes = synchronized(collected) { collected.toByteArray() }
        val shorts = ByteBuffer.wrap(bytes).order(ByteOrder.LITTLE_ENDIAN).asShortBuffer()
        val out = FloatArray(shorts.remaining())
        for (i in out.indices) {
            out[i] = shorts.get(i) / 32768.0f
        }
        return out
    }

    /** Громкость кадра, поджатая корнем — иначе тихая речь почти не видна. */
    private fun levelOf(bytes: ByteArray, count: Int): Float {
        val shorts = ByteBuffer.wrap(bytes, 0, count).order(ByteOrder.LITTLE_ENDIAN).asShortBuffer()
        var sum = 0.0
        val n = shorts.remaining()
        if (n == 0) return 0f
        for (i in 0 until n) {
            val v = shorts.get(i) / 32768.0
            sum += v * v
        }
        val rms = kotlin.math.sqrt(sum / n)
        return (kotlin.math.sqrt(rms) * 2.2).toFloat().coerceIn(0f, 1f)
    }

    companion object {
        const val SAMPLE_RATE = 16_000
        private const val CHANNEL = AudioFormat.CHANNEL_IN_MONO
        private const val ENCODING = AudioFormat.ENCODING_PCM_16BIT
    }
}
