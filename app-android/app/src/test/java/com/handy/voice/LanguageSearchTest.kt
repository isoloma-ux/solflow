package com.handy.voice

import org.junit.Assert.*
import org.junit.Test

class LanguageSearchTest {
    @Test fun displayedNamesRemainFoundWhileTyping() {
        for ((code, name) in listOf("zh" to "Chinese", "ko" to "Korean")) {
            for (length in 1..name.length) {
                val query = name.take(length)
                assertTrue(query, LanguageSearch.matches(query, code, name))
                assertTrue(query, LanguageSearch.matches(query.uppercase(), code, name))
            }
        }
    }
    @Test fun namesCodesAndAccentsAreNormalized() {
        assertTrue(LanguageSearch.matches(" KO ", "ko", "Korean"))
        assertTrue(LanguageSearch.matches("francais", "fr", "Français"))
        assertTrue(LanguageSearch.matches("中", "zh", "中文"))
        assertTrue(LanguageSearch.matches("한국", "ko", "한국어"))
        assertTrue(LanguageSearch.matches("", "ja", "日本語"))
        assertFalse(LanguageSearch.matches("xyz", "ko", "Korean"))
    }
}
