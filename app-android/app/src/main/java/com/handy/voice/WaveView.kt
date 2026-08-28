package com.handy.voice

import android.content.Context
import android.graphics.Canvas
import android.graphics.Paint
import android.graphics.RectF
import android.util.AttributeSet
import android.view.View

/**
 * Живая волна уровня звука при записи: бегущая лента скруглённых столбиков,
 * тот же характер, что у плавающей кнопки. Смысл не декоративный — по волне
 * видно, что звук действительно пишется и микрофон слышит комнату.
 */
class WaveView @JvmOverloads constructor(
    context: Context,
    attrs: AttributeSet? = null,
) : View(context, attrs) {

    private val levels = FloatArray(BARS)
    private var head = 0

    private val paint = Paint(Paint.ANTI_ALIAS_FLAG).apply {
        color = context.getColor(R.color.accent_solid)
    }
    private val rect = RectF()

    /** Новый замер уровня [0..1] — лента сдвигается на один столбик. */
    fun push(level: Float) {
        levels[head] = level.coerceIn(0f, 1f)
        head = (head + 1) % BARS
        invalidate()
    }

    fun reset() {
        levels.fill(0f)
        invalidate()
    }

    override fun onDraw(canvas: Canvas) {
        val barWidth = width.toFloat() / (BARS * 2 - 1)
        val minH = MIN_FRACTION * height
        val cy = height / 2f

        for (i in 0 until BARS) {
            // Свежие замеры справа, старые уезжают влево.
            val value = levels[(head + i) % BARS]
            val h = (minH + (height - minH) * value).coerceAtLeast(minH)
            val left = i * barWidth * 2
            rect.set(left, cy - h / 2, left + barWidth, cy + h / 2)
            canvas.drawRoundRect(rect, barWidth / 2, barWidth / 2, paint)
        }
    }

    private companion object {
        const val BARS = 36
        const val MIN_FRACTION = 0.08f
    }
}
