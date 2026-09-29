package com.handy.voice

import java.text.Normalizer
import java.util.Locale

/** Display names and ISO codes share the same case/diacritic-insensitive search. */
object LanguageSearch {
    private fun normalize(value: String): String = Normalizer.normalize(value.trim(), Normalizer.Form.NFD)
        .replace(Regex("\\p{M}+"), "").lowercase(Locale.ROOT)

    fun matches(query: String, code: String, vararg names: String): Boolean {
        val needle = normalize(query)
        return needle.isEmpty() || normalize(code).startsWith(needle) || names.any { normalize(it).contains(needle) }
    }
}
