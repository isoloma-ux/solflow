package com.handy.voice

import java.io.BufferedOutputStream
import java.io.Closeable
import java.io.File
import java.io.FileOutputStream
import java.io.RandomAccessFile
import java.nio.ByteBuffer
import java.nio.ByteOrder

/**
 * Потоковая запись WAV: звук уходит на диск по мере поступления, в памяти
 * держится только текущий буфер. Двухчасовая встреча — это ~230 МБ, копить
 * её в ByteArrayOutputStream, как делает диктовка, нельзя.
 *
 * Формат фиксированный — PCM16 моно 16 кГц, тот же, что ждёт движок.
 * Размеры в заголовке дописываются при закрытии; если процесс погиб до
 * finish(), файл чинится по фактической длине при чтении.
 */
class WavWriter(private val file: File) : Closeable {

    private val out = BufferedOutputStream(FileOutputStream(file), 1 shl 16)
    private var dataBytes = 0L

    init {
        out.write(header(0))
    }

    val samplesWritten: Long get() = dataBytes / 2

    fun write(pcm16: ByteArray, count: Int) {
        out.write(pcm16, 0, count)
        dataBytes += count
    }

    /** Закрывает поток и вписывает настоящие размеры в заголовок. */
    fun finish() {
        out.flush()
        out.close()
        RandomAccessFile(file, "rw").use { raf ->
            raf.write(header(dataBytes))
        }
    }

    override fun close() = finish()

    private fun header(data: Long): ByteArray {
        val b = ByteBuffer.allocate(HEADER_BYTES).order(ByteOrder.LITTLE_ENDIAN)
        b.put("RIFF".toByteArray())
        b.putInt((data + HEADER_BYTES - 8).toInt())
        b.put("WAVE".toByteArray())
        b.put("fmt ".toByteArray())
        b.putInt(16)
        b.putShort(1) // PCM
        b.putShort(1) // моно
        b.putInt(AudioRecorder.SAMPLE_RATE)
        b.putInt(AudioRecorder.SAMPLE_RATE * 2) // байт в секунду
        b.putShort(2) // байт на кадр
        b.putShort(16) // бит на отсчёт
        b.put("data".toByteArray())
        b.putInt(data.toInt())
        return b.array()
    }

    companion object {
        const val HEADER_BYTES = 44
    }
}

/**
 * Чтение своих же WAV кусками через RandomAccessFile — файл целиком в память
 * не поднимается. Заголовку не верим на слово: если запись оборвалась,
 * размер в нём нулевой, поэтому длину берём по факту.
 */
class WavReader(file: File) : Closeable {

    private val raf = RandomAccessFile(file, "r")

    val totalSamples: Long = maxOf(0L, (file.length() - WavWriter.HEADER_BYTES) / 2)

    /** Читает [count] отсчётов начиная с [from], как float32 в [-1, 1]. */
    fun read(from: Long, count: Int): FloatArray {
        val n = minOf(count.toLong(), totalSamples - from).toInt()
        if (n <= 0) return FloatArray(0)
        val bytes = ByteArray(n * 2)
        raf.seek(WavWriter.HEADER_BYTES + from * 2)
        raf.readFully(bytes)
        val shorts = ByteBuffer.wrap(bytes).order(ByteOrder.LITTLE_ENDIAN).asShortBuffer()
        return FloatArray(n) { shorts.get(it) / 32768.0f }
    }

    override fun close() = raf.close()
}
