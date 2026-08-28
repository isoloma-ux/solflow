package com.handy.voice

import android.content.Context

/**
 * Настройки приложения. Отдельно от [ModelStore], потому что переживают
 * перезагрузку телефона и читаются из служб, а не только с экрана.
 *
 * Хранилище одно на всё: службы поднимаются раньше экрана и должны читать
 * то же самое, что показывает [SettingsActivity].
 */
object AppPrefs {

    private const val PREFS = "handy"

    // --- плавающая кнопка -------------------------------------------------

    /**
     * Пользователь включил плавающую кнопку. Флаг живёт до тех пор, пока её
     * не выключат в приложении: гибель процесса, смахивание из недавних и
     * перезагрузка телефона его не сбрасывают.
     */
    fun bubbleEnabled(context: Context): Boolean = flag(context, "bubble_enabled", false)

    fun setBubbleEnabled(context: Context, enabled: Boolean) =
        setFlag(context, "bubble_enabled", enabled)

    /**
     * Притягивать кнопку к краю экрана. Кому-то так удобнее, кому-то нужно
     * ставить её ровно там, где отпустил, — поэтому решает пользователь.
     */
    fun snapToEdge(context: Context): Boolean = flag(context, "snap_to_edge", true)

    fun setSnapToEdge(context: Context, snap: Boolean) = setFlag(context, "snap_to_edge", snap)

    // --- звук и микрофон --------------------------------------------------

    /**
     * Режим «комната»: микрофон без подавления дальних звуков.
     *
     * По умолчанию запись идёт источником для распознавания речи, а он
     * заточен под голос у самого микрофона и режет всё остальное — из-за
     * этого речь собеседников в стороне не попадала в запись.
     */
    fun roomMode(context: Context): Boolean = flag(context, "room_mode", false)

    fun setRoomMode(context: Context, room: Boolean) = setFlag(context, "room_mode", room)

    /** Короткий сигнал в начале записи — чтобы не гадать, слышит ли она. */
    fun startSound(context: Context): Boolean = flag(context, "start_sound", true)

    fun setStartSound(context: Context, on: Boolean) = setFlag(context, "start_sound", on)

    /**
     * Приглушать чужой звук на время диктовки: музыка из динамика телефона
     * иначе попадает в микрофон и лезет в расшифровку.
     */
    fun muteWhileRecording(context: Context): Boolean = flag(context, "mute_recording", false)

    fun setMuteWhileRecording(context: Context, on: Boolean) =
        setFlag(context, "mute_recording", on)

    /**
     * Микрофон, закреплённый пользователем: «тип:название» из
     * [MicDevices]. null — какой выберет система (с наушниками это может
     * оказаться не тот, который нужен).
     */
    fun inputDevice(context: Context): String? = text(context, "input_device", null)

    fun setInputDevice(context: Context, id: String?) = setText(context, "input_device", id)

    // --- поведение диктовки -----------------------------------------------

    /**
     * Оставлять распознанный текст в буфере обмена. По умолчанию буфер
     * возвращается к тому, что в нём было: вставка идёт через него, и
     * терять скопированное раньше пользователь не подписывался.
     */
    fun clipboardKeep(context: Context): Boolean = flag(context, "clipboard_keep", false)

    fun setClipboardKeep(context: Context, keep: Boolean) =
        setFlag(context, "clipboard_keep", keep)

    /** Нажимать ввод после вставки — сообщение уходит само. */
    fun autoSubmit(context: Context): Boolean = flag(context, "auto_submit", false)

    fun setAutoSubmit(context: Context, on: Boolean) = setFlag(context, "auto_submit", on)

    /** Убирать слова-паразиты: «типа», «как бы», «короче». */
    fun removeFillers(context: Context): Boolean = flag(context, "remove_fillers", false)

    fun setRemoveFillers(context: Context, on: Boolean) = setFlag(context, "remove_fillers", on)

    // --- модель и память --------------------------------------------------

    /**
     * Когда выгружать модель из памяти: «never», «immediately», «min2»,
     * «min5», «min10», «min15», «hour1».
     */
    fun modelUnload(context: Context): String = text(context, "model_unload", "min5")!!

    fun setModelUnload(context: Context, value: String) = setText(context, "model_unload", value)

    /** Через сколько миллисекунд простоя выгружать модель; null — никогда. */
    fun unloadAfterMs(context: Context): Long? = when (modelUnload(context)) {
        "never" -> null
        "immediately" -> 0L
        "min2" -> 120_000L
        "min10" -> 600_000L
        "min15" -> 900_000L
        "hour1" -> 3_600_000L
        else -> 300_000L
    }

    // --- история ----------------------------------------------------------

    /**
     * Сколько последних диктовок держать. По умолчанию двести — столько
     * приложение хранило до появления настройки, и обновление не должно
     * молча стирать чужую историю.
     */
    fun historyLimit(context: Context): Int = prefs(context).getInt("history_limit", 200)

    fun setHistoryLimit(context: Context, limit: Int) {
        prefs(context).edit().putInt("history_limit", limit).apply()
    }

    /**
     * Когда чистить историю: «keep_limit» — только по числу, «never» — не
     * хранить вовсе, «days3», «weeks2», «months3».
     */
    fun historyRetention(context: Context): String = text(context, "history_retention", "keep_limit")!!

    fun setHistoryRetention(context: Context, value: String) =
        setText(context, "history_retention", value)

    /** Сколько миллисекунд держать историю; null — без срока. */
    fun retentionMs(context: Context): Long? = when (historyRetention(context)) {
        "days3" -> 3L * 24 * 3600 * 1000
        "weeks2" -> 14L * 24 * 3600 * 1000
        "months3" -> 90L * 24 * 3600 * 1000
        else -> null
    }

    /** Хранить ли звук диктовок — чтобы можно было переслушать. */
    fun keepAudio(context: Context): Boolean = flag(context, "keep_audio", true)

    fun setKeepAudio(context: Context, on: Boolean) = setFlag(context, "keep_audio", on)

    // --- оформление -------------------------------------------------------

    /** Тема окна: «system», «light» или «dark». */
    fun theme(context: Context): String = text(context, "theme", "system")!!

    fun setTheme(context: Context, value: String) = setText(context, "theme", value)

    // --- вводный экран ----------------------------------------------------

    /** Показывали ли уже вводный экран при первом запуске. */
    fun introShown(context: Context): Boolean = flag(context, "intro_shown", false)

    fun setIntroShown(context: Context, shown: Boolean) = setFlag(context, "intro_shown", shown)

    // --- служебное --------------------------------------------------------

    private fun flag(context: Context, key: String, default: Boolean) =
        prefs(context).getBoolean(key, default)

    private fun setFlag(context: Context, key: String, value: Boolean) {
        prefs(context).edit().putBoolean(key, value).apply()
    }

    private fun text(context: Context, key: String, default: String?) =
        prefs(context).getString(key, default)

    private fun setText(context: Context, key: String, value: String?) {
        prefs(context).edit().putString(key, value).apply()
    }

    private fun prefs(context: Context) =
        context.getSharedPreferences(PREFS, Context.MODE_PRIVATE)
}
