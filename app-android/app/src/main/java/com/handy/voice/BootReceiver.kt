package com.handy.voice

import android.content.BroadcastReceiver
import android.content.Context
import android.content.Intent

/**
 * Поднимает плавающую кнопку после перезагрузки телефона.
 *
 * Основной путь всё же через службу спецвозможностей — систему её тоже
 * поднимает при загрузке. Этот приёмник нужен на случай, когда разрешение
 * спецвозможностей не выдано: тогда кнопка работает в запасном режиме
 * (видна всегда, текст уходит в буфер обмена), и поднять её больше некому.
 */
class BootReceiver : BroadcastReceiver() {

    override fun onReceive(context: Context, intent: Intent?) {
        if (intent?.action != Intent.ACTION_BOOT_COMPLETED) return
        if (!AppPrefs.bubbleEnabled(context)) return
        if (DictationService.running) return
        // Android может отказать в запуске сервиса сразу после загрузки —
        // тогда кнопку поднимет служба спецвозможностей или сам пользователь.
        runCatching { DictationService.start(context) }
    }
}
