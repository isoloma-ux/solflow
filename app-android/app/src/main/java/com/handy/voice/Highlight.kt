package com.handy.voice

import android.content.Context
import android.text.SpannableString
import android.text.Spanned
import android.text.style.ForegroundColorSpan
import androidx.core.content.ContextCompat

/**
 * Подсветка найденного слова в тексте.
 *
 * Цветом, а не заливкой: жёлтый маркер из браузеров в эту палитру не лезет,
 * а зелёный акцент уже значит «то, что вы искали» по всему приложению.
 */
object Highlight {

    fun of(context: Context, text: String, needle: String): CharSequence {
        val query = needle.trim()
        if (query.isBlank()) return text

        val span = SpannableString(text)
        val lower = text.lowercase()
        val target = query.lowercase()
        var from = 0
        while (true) {
            val at = lower.indexOf(target, from)
            if (at < 0) break
            span.setSpan(
                ForegroundColorSpan(ContextCompat.getColor(context, R.color.accent)),
                at,
                at + target.length,
                Spanned.SPAN_EXCLUSIVE_EXCLUSIVE,
            )
            from = at + target.length
        }
        return span
    }
}
