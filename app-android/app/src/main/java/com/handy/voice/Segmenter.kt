package com.handy.voice

import kotlin.math.sqrt

/**
 * Режет длинную запись на куски по паузам.
 *
 * GigaAM обучен на репликах примерно до 25 секунд, дальше точность падает —
 * движок сам предупреждает об этом в логе. Поэтому длинную диктовку надо
 * разбивать, и разбивать по тишине, а не по таймеру: разрез посреди слова
 * стоит дороже, чем лишний кусок.
 *
 * Детектор простой, по энергии кадра. Для диктовки в телефон у рта этого
 * достаточно: голос там на порядок громче фона. Для записи через комнату
 * понадобился бы Silero, как в десктопном Handy.
 *
 * Логика разрезов работает на энергиях кадров, а не на самом звуке — так ей
 * может пользоваться и расшифровка встреч, где двухчасовой файл в память не
 * влезает: энергии считаются потоково, звук читается с диска по кускам.
 */
object Segmenter {

    const val FRAME_MS = 20
    private const val MIN_PAUSE_MS = 350
    private const val MIN_SEGMENT_SEC = 4f
    private const val MAX_SEGMENT_SEC = 24f

    /** Порог тишины относительно шумового фона записи. */
    private const val NOISE_FACTOR = 3.0f

    /** Ниже этого уровня считаем тишиной даже при очень тихом фоне. */
    private const val ABSOLUTE_FLOOR = 0.004f

    fun frameSamples(sampleRate: Int) = FRAME_MS * sampleRate / 1000

    fun split(pcm: FloatArray, sampleRate: Int): List<FloatArray> {
        val maxSamples = (MAX_SEGMENT_SEC * sampleRate).toInt()
        if (pcm.size <= maxSamples) return listOf(pcm)

        val frame = frameSamples(sampleRate)
        val loud = energyPerFrame(pcm, frame)
        val cuts = cutFrames(loud)

        val bounds = (listOf(0) + cuts + listOf(loud.size)).map { it * frame }
        return bounds.zipWithNext()
            .map { (from, to) -> pcm.copyOfRange(from, minOf(to, pcm.size)) }
            .filter { it.size > frame * 5 }
    }

    /**
     * Кадры, по которым надо резать. Вход — громкость каждого 20-мс кадра
     * записи целиком: порог тишины считается от общего шумового фона.
     */
    fun cutFrames(loud: FloatArray): List<Int> {
        val threshold = threshold(loud)
        val isSpeech = BooleanArray(loud.size) { loud[it] > threshold }

        val minPauseFrames = MIN_PAUSE_MS / FRAME_MS
        val minSegmentFrames = (MIN_SEGMENT_SEC * 1000 / FRAME_MS).toInt()
        val maxSegmentFrames = (MAX_SEGMENT_SEC * 1000 / FRAME_MS).toInt()

        val cuts = mutableListOf<Int>()
        var segmentStart = 0
        var silenceRun = 0
        var index = 0

        while (index < isSpeech.size) {
            silenceRun = if (isSpeech[index]) 0 else silenceRun + 1
            val length = index - segmentStart

            val pauseIsLongEnough = silenceRun >= minPauseFrames
            val segmentIsLongEnough = length >= minSegmentFrames

            if (pauseIsLongEnough && segmentIsLongEnough) {
                // Режем в середине паузы: так ни одна фраза не теряет края.
                val cut = index - silenceRun / 2
                cuts += cut
                segmentStart = cut
                silenceRun = 0
            } else if (length >= maxSegmentFrames) {
                // Пауз не нашлось — режем по самой тихой точке в хвосте куска,
                // это меньшее зло, чем резать на полуслове.
                val from = segmentStart + minSegmentFrames
                val quietest = (from until index).minByOrNull { loud[it] } ?: index
                cuts += quietest
                segmentStart = quietest
                silenceRun = 0
            }
            index++
        }
        return cuts
    }

    /** Громкость одного кадра — для потокового первого прохода по файлу. */
    fun frameEnergy(pcm: FloatArray, offset: Int, frame: Int): Float {
        var sum = 0.0
        for (i in offset until offset + frame) {
            sum += pcm[i].toDouble() * pcm[i]
        }
        return sqrt(sum / frame).toFloat()
    }

    private fun energyPerFrame(pcm: FloatArray, frame: Int): FloatArray {
        val count = pcm.size / frame
        return FloatArray(count) { f -> frameEnergy(pcm, f * frame, frame) }
    }

    /** Шумовой фон берём как 10-й процентиль громкости кадров. */
    private fun threshold(loud: FloatArray): Float {
        val sorted = loud.sortedArray()
        val floor = sorted[(sorted.size * 0.1f).toInt().coerceIn(0, sorted.lastIndex)]
        return maxOf(floor * NOISE_FACTOR, ABSOLUTE_FLOOR)
    }
}
