package com.handy.voice

import android.content.Context
import android.util.AttributeSet
import android.view.MotionEvent
import android.view.ViewConfiguration
import android.widget.FrameLayout
import kotlin.math.abs

/**
 * Контейнер страниц, понимающий горизонтальный свайп: влево — следующая
 * вкладка, вправо — предыдущая. Вертикальная прокрутка списков не страдает:
 * жест перехватывается только когда движение явно горизонтальное — сдвиг по
 * X заметно больше порога и вдвое больше сдвига по Y.
 */
class SwipePagesLayout @JvmOverloads constructor(
    context: Context,
    attrs: AttributeSet? = null,
) : FrameLayout(context, attrs) {

    /** true — палец ушёл влево, пользователь листает вперёд. */
    var onSwipe: ((forward: Boolean) -> Unit)? = null

    private val slop = ViewConfiguration.get(context).scaledTouchSlop
    private val trigger = (56 * resources.displayMetrics.density)
    private var downX = 0f
    private var downY = 0f

    override fun onInterceptTouchEvent(ev: MotionEvent): Boolean {
        when (ev.actionMasked) {
            MotionEvent.ACTION_DOWN -> {
                downX = ev.x
                downY = ev.y
            }
            MotionEvent.ACTION_MOVE -> {
                val dx = ev.x - downX
                val dy = ev.y - downY
                if (abs(dx) > slop * 2 && abs(dx) > abs(dy) * 2) return true
            }
        }
        return false
    }

    override fun onTouchEvent(ev: MotionEvent): Boolean {
        if (ev.actionMasked == MotionEvent.ACTION_UP) {
            val dx = ev.x - downX
            if (abs(dx) >= trigger) onSwipe?.invoke(dx < 0)
        }
        return true
    }
}
