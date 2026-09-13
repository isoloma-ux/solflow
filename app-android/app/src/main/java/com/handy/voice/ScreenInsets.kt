package com.handy.voice

import android.app.Activity
import android.view.View
import androidx.core.graphics.Insets
import androidx.core.view.ViewCompat
import androidx.core.view.WindowCompat
import androidx.core.view.WindowInsetsCompat
import kotlin.math.roundToInt

/** Content stays readable even when an older firmware never dispatches insets. */
object ScreenInsets {
    fun extra(view: View): Int = if (AppPrefs.wideScreenEdges(view.context)) dp(view, 16) else 0
    fun dp(view: View, value: Int): Int = (value * view.resources.displayMetrics.density).roundToInt()

    fun safe(insets: WindowInsetsCompat, keyboard: Boolean = false): Insets {
        val types = WindowInsetsCompat.Type.systemBars() or WindowInsetsCompat.Type.displayCutout() or
            (if (keyboard) WindowInsetsCompat.Type.ime() else 0)
        return Insets.max(insets.getInsets(types), insets.displayCutout?.waterfallInsets ?: Insets.NONE)
    }

    fun pad(view: View, safe: Insets, edge: Int = 32, column: Boolean = true, top: Int = edge) {
        val extra = extra(view)
        val available = (view.width - safe.left - safe.right - 2 * extra).coerceAtLeast(0)
        val center = if (column) ((available - dp(view, 640)) / 2).coerceAtLeast(0) else 0
        view.setPadding(safe.left + dp(view, edge) + extra + center, safe.top + dp(view, top),
            safe.right + dp(view, edge) + extra + center, safe.bottom)
    }

    /** Listen on the window's content root, before containers can consume the insets. */
    fun install(activity: Activity, root: View, apply: (Insets) -> Unit) {
        WindowCompat.setDecorFitsSystemWindows(activity.window, false)
        var last = Insets.NONE
        apply(last) // baseline padding must not depend on an insets callback
        ViewCompat.setOnApplyWindowInsetsListener(root) { _, insets ->
            last = safe(insets, keyboard = true)
            apply(last)
            insets
        }
        root.addOnLayoutChangeListener { _, l, t, r, b, ol, ot, or_, ob ->
            if (r - l != or_ - ol || b - t != ob - ot) {
                apply(last)
                ViewCompat.requestApplyInsets(root)
            }
        }
        root.addOnAttachStateChangeListener(object : View.OnAttachStateChangeListener {
            override fun onViewAttachedToWindow(v: View) { ViewCompat.requestApplyInsets(v) }
            override fun onViewDetachedFromWindow(v: View) = Unit
        })
        ViewCompat.requestApplyInsets(root)
    }
}
