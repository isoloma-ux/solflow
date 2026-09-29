package com.handy.voice

import android.content.Context
import org.json.JSONObject

/** Bundled UI text only; no recordings or generated content are translated here. */
object UiTranslations {
    private var dictionaries: JSONObject? = null
    fun english(context: Context, text: String): String {
        val lang = context.resources.configuration.locales[0].language
        if (lang == "en" || lang == "ru") return text
        val all = dictionaries ?: runCatching {
            JSONObject(context.assets.open("locales.json").bufferedReader().use { it.readText() })
        }.getOrElse { JSONObject() }.also { dictionaries = it }
        return all.optJSONObject(lang)?.optString(text)?.takeIf { it.isNotBlank() } ?: text
    }
}
