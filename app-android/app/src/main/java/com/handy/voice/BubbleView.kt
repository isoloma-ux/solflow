package com.handy.voice

import android.content.Context
import android.graphics.Canvas
import android.graphics.Paint
import android.graphics.RectF
import android.view.View
import kotlin.math.abs
import kotlin.math.sin

/**
 * Плавающая кнопка диктовки.
 *
 * Зелёная заливка здесь допустима: по правилу акцента это одна акцентная
 * кнопка на экран, и площадь у неё крошечная. Никаких теней и градиентов —
 * они в дизайн-системе запрещены.
 */
class BubbleView(context: Context) : View(context) {

    enum class State { IDLE, RECORDING, PROCESSING }

    var state: State = State.IDLE
        set(value) {
            field = value
            // В покое кнопка приглушена, чтобы не спорить с чужим интерфейсом,
            // а на записи «загорается» — это единственный индикатор того,
            // что микрофон открыт.
            animate()
                .alpha(if (value == State.IDLE) IDLE_ALPHA else 1f)
                .setDuration(140)
                .start()
            invalidate()
        }

    /** Громкость [0, 1] от AudioRecorder — задаёт высоту полосок. */
    var level: Float = 0f

    private val density = resources.displayMetrics.density
    private val fill = Paint(Paint.ANTI_ALIAS_FLAG).apply {
        color = context.getColor(R.color.accent_solid)
    }
    private val glyph = Paint(Paint.ANTI_ALIAS_FLAG).apply {
        color = context.getColor(R.color.on_accent)
    }
    private val rect = RectF()

    init {
        alpha = IDLE_ALPHA
    }

    /** Полоски сглажены между кадрами, иначе волна дёргается. */
    private val bars = FloatArray(BAR_COUNT)
    private var phase = 0f

    override fun onDraw(canvas: Canvas) {
        val cx = width / 2f
        val cy = height / 2f

        // Квадрат со скруглёнными углами, а не круг: так кнопка читается как
        // элемент интерфейса, а не как системный пузырь-«шарик».
        rect.set(0f, 0f, width.toFloat(), height.toFloat())
        val corner = minOf(width, height) * CORNER_RATIO
        canvas.drawRoundRect(rect, corner, corner, fill)

        when (state) {
            State.IDLE -> drawIdle(canvas, cx, cy)
            State.RECORDING -> drawWave(canvas, cx, cy)
            State.PROCESSING -> drawProcessing(canvas, cx, cy)
        }

        if (state != State.IDLE) postInvalidateOnAnimation()
    }

    /**
     * В покое — та же волна, что на иконке, но неподвижная. Раньше здесь был
     * квадрат, и он читался как «стоп», то есть как остановка записи, а не как
     * возможность её начать.
     */
    private fun drawIdle(canvas: Canvas, cx: Float, cy: Float) {
        drawBars(canvas, cx, cy) { i -> REST_HEIGHTS[i] * density }
    }

    private fun drawWave(canvas: Canvas, cx: Float, cy: Float) {
        phase += 0.22f
        val minHeight = 4f * density
        val maxHeight = 22f * density
        drawBars(canvas, cx, cy) { i ->
            // Центральные полоски выше краевых — так волна читается как голос.
            val shape = 1f - abs(i - (BAR_COUNT - 1) / 2f) / BAR_COUNT
            val wobble = 0.55f + 0.45f * sin(phase + i * 0.9f)
            val target = minHeight + (maxHeight - minHeight) * level * shape * wobble
            bars[i] += (target - bars[i]) * 0.35f
            bars[i].coerceAtLeast(minHeight)
        }
    }

    private inline fun drawBars(canvas: Canvas, cx: Float, cy: Float, height: (Int) -> Float) {
        val barWidth = 3f * density
        val gap = 2.5f * density
        val totalWidth = BAR_COUNT * barWidth + (BAR_COUNT - 1) * gap
        var x = cx - totalWidth / 2f
        for (i in 0 until BAR_COUNT) {
            val h = height(i)
            rect.set(x, cy - h / 2f, x + barWidth, cy + h / 2f)
            canvas.drawRoundRect(rect, barWidth / 2f, barWidth / 2f, glyph)
            x += barWidth + gap
        }
    }

    /** Три точки, гаснущие по очереди, пока движок считает. */
    private fun drawProcessing(canvas: Canvas, cx: Float, cy: Float) {
        phase += 0.12f
        val r = 2.5f * density
        val gap = 8f * density
        for (i in -1..1) {
            val a = (sin(phase + i * 1.1f) + 1f) / 2f
            glyph.alpha = (80 + 175 * a).toInt().coerceIn(0, 255)
            canvas.drawCircle(cx + i * gap, cy, r, glyph)
        }
        glyph.alpha = 255
    }

    private companion object {
        const val BAR_COUNT = 5
        const val IDLE_ALPHA = 0.55f

        /** Доля от стороны: даёт мягкий квадрат, но он остаётся квадратом. */
        const val CORNER_RATIO = 0.32f

        /** Высоты полосок в покое, dp — те же пропорции, что у иконки. */
        val REST_HEIGHTS = floatArrayOf(6f, 11f, 15f, 11f, 6f)
    }
}
