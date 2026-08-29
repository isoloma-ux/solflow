package com.handy.voice

import android.content.Intent
import android.os.Bundle
import android.view.View
import android.widget.TextView
import androidx.appcompat.app.AppCompatActivity
import androidx.appcompat.app.AppCompatDelegate
import androidx.core.os.LocaleListCompat
import androidx.core.view.ViewCompat
import androidx.core.view.WindowInsetsCompat
import com.google.android.material.materialswitch.MaterialSwitch
import com.handy.voice.databinding.ActivitySettingsBinding

/**
 * Как приложение себя ведёт.
 *
 * Раньше настройки жили внизу вкладки диктовки, и туда помещалось ровно две
 * штуки. Перенос доделок с десктопной версии добавил ещё полтора десятка —
 * им нужен свой экран.
 *
 * Строки собираются кодом из трёх заготовок разметки: описание группы
 * читается сверху вниз одним куском, а не размазано по XML.
 */
class SettingsActivity : AppCompatActivity() {

    private lateinit var ui: ActivitySettingsBinding

    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        ui = ActivitySettingsBinding.inflate(layoutInflater)
        setContentView(ui.root)

        // Те же отступы и колонка 640dp, что на главном экране.
        val density = resources.displayMetrics.density
        val edge = (32 * density).toInt()
        ViewCompat.setOnApplyWindowInsetsListener(ui.root) { view, insets ->
            val bars = insets.getInsets(WindowInsetsCompat.Type.systemBars())
            val extra = ((view.width - (640 * density).toInt()) / 2).coerceAtLeast(0)
            view.setPadding(edge + extra, bars.top + edge, edge + extra, bars.bottom)
            insets
        }
        ui.root.addOnLayoutChangeListener { v, l, _, r, _, ol, _, or_, _ ->
            if (r - l != or_ - ol) v.requestApplyInsets()
        }

        ui.back.setOnClickListener { finish() }

        // Строки собираются из одной заготовки, поэтому у всех тумблеров
        // одинаковый id. Система сохраняет состояние вьюх по id и при
        // пересоздании (смена темы) раздала бы всем тумблерам состояние
        // последнего — а слушатель тут же записал бы это в настройки.
        // Состояние нам не нужно: экран строится из настроек заново.
        ui.settingsList.isSaveFromParentEnabled = false

        build()
    }

    /** Перерисовываем целиком: строк мало, а зависимости между ними есть. */
    private fun refresh() = build()

    private fun build() {
        ui.settingsList.removeAllViews()

        group(R.string.group_look)
        choice(
            R.string.set_language, R.string.set_language_hint,
            languageOptions(), currentLanguage(),
        ) { value ->
            // Язык меняет система: она сама пересоздаёт экраны и запоминает
            // выбор — своего хранения не нужно.
            AppCompatDelegate.setApplicationLocales(
                if (value == LANGUAGE_SYSTEM) LocaleListCompat.getEmptyLocaleList()
                else LocaleListCompat.forLanguageTags(value)
            )
        }
        choice(
            R.string.set_theme, R.string.set_theme_hint,
            themeOptions(), AppPrefs.theme(this),
        ) { value ->
            AppPrefs.setTheme(this, value)
            applyTheme(value)
            refresh()
        }

        group(R.string.group_sound)
        choice(
            R.string.set_mic, R.string.set_mic_hint,
            micOptions(), AppPrefs.inputDevice(this) ?: SYSTEM_MIC,
            valueText = MicDevices.title(this, AppPrefs.inputDevice(this)),
        ) { value ->
            AppPrefs.setInputDevice(this, value.takeIf { it != SYSTEM_MIC })
            refresh()
        }
        switch(R.string.set_start_sound, R.string.set_start_sound_hint, AppPrefs.startSound(this)) {
            AppPrefs.setStartSound(this, it)
        }
        switch(R.string.set_mute, R.string.set_mute_hint, AppPrefs.muteWhileRecording(this)) {
            AppPrefs.setMuteWhileRecording(this, it)
        }
        switch(R.string.setting_room, R.string.setting_room_hint, AppPrefs.roomMode(this)) {
            AppPrefs.setRoomMode(this, it)
        }

        group(R.string.group_dictation)
        choice(
            R.string.set_clipboard, R.string.set_clipboard_hint,
            listOf(
                "restore" to getString(R.string.set_clipboard_restore),
                "keep" to getString(R.string.set_clipboard_keep),
            ),
            if (AppPrefs.clipboardKeep(this)) "keep" else "restore",
        ) { value ->
            AppPrefs.setClipboardKeep(this, value == "keep")
            refresh()
        }
        switch(R.string.set_submit, R.string.set_submit_hint, AppPrefs.autoSubmit(this)) {
            AppPrefs.setAutoSubmit(this, it)
        }
        switch(R.string.set_fillers, R.string.set_fillers_hint, AppPrefs.removeFillers(this)) {
            AppPrefs.setRemoveFillers(this, it)
        }
        switch(R.string.setting_snap, R.string.setting_snap_hint, AppPrefs.snapToEdge(this)) {
            AppPrefs.setSnapToEdge(this, it)
        }

        group(R.string.group_model)
        choice(
            R.string.set_unload, R.string.set_unload_hint,
            unloadOptions(), AppPrefs.modelUnload(this),
        ) { value ->
            AppPrefs.setModelUnload(this, value)
            // Новый срок должен подействовать на уже загруженную модель.
            Engine.scheduleUnload(this)
            refresh()
        }

        group(R.string.group_history)
        choice(
            R.string.set_history_limit, R.string.set_history_limit_hint,
            limitOptions(), AppPrefs.historyLimit(this).toString(),
        ) { value ->
            AppPrefs.setHistoryLimit(this, value.toInt())
            TranscriptStore.applyLimits(this)
            refresh()
        }
        choice(
            R.string.set_history_retention, R.string.set_history_retention_hint,
            retentionOptions(), AppPrefs.historyRetention(this),
        ) { value ->
            AppPrefs.setHistoryRetention(this, value)
            TranscriptStore.applyLimits(this)
            refresh()
        }
        switch(R.string.set_keep_audio, R.string.set_keep_audio_hint, AppPrefs.keepAudio(this)) {
            AppPrefs.setKeepAudio(this, it)
            // Выключили — звук прошлых диктовок должен уйти с диска сразу,
            // иначе настройка не освобождает ничего.
            TranscriptStore.applyLimits(this)
        }

        group(R.string.group_about)
        link(R.string.about_title, R.string.about_hint) {
            startActivity(Intent(this, AboutActivity::class.java))
        }
    }

    // --- заготовки строк --------------------------------------------------

    private fun group(title: Int) {
        val view = layoutInflater.inflate(R.layout.item_setting_group, ui.settingsList, false)
        (view as TextView).setText(title)
        ui.settingsList.addView(view)
    }

    private fun switch(title: Int, hint: Int, checked: Boolean, onChange: (Boolean) -> Unit) {
        val view = layoutInflater.inflate(R.layout.item_setting_switch, ui.settingsList, false)
        view.findViewById<TextView>(R.id.title).setText(title)
        view.findViewById<TextView>(R.id.hint).setText(hint)
        val toggle = view.findViewById<MaterialSwitch>(R.id.toggle)
        toggle.isChecked = checked
        toggle.setOnCheckedChangeListener { _, value -> onChange(value) }
        // Тап по всей строке, а не только по тумблеру: попасть в него пальцем
        // на ходу тяжело.
        view.setOnClickListener { toggle.toggle() }
        view.isClickable = true
        ui.settingsList.addView(view)
    }

    private fun choice(
        title: Int,
        hint: Int,
        options: List<Pair<String, String>>,
        selected: String,
        valueText: String? = null,
        onPick: (String) -> Unit,
    ) {
        val view = layoutInflater.inflate(R.layout.item_setting_choice, ui.settingsList, false)
        view.findViewById<TextView>(R.id.title).setText(title)
        view.findViewById<TextView>(R.id.hint).setText(hint)
        view.findViewById<TextView>(R.id.value).text =
            valueText ?: options.firstOrNull { it.first == selected }?.second.orEmpty()
        view.setOnClickListener {
            optionSheet(getString(title), options, selected, onPick = onPick)
        }
        ui.settingsList.addView(view)
    }

    private fun link(title: Int, hint: Int, onTap: () -> Unit) {
        val view = layoutInflater.inflate(R.layout.item_setting_choice, ui.settingsList, false)
        view.findViewById<TextView>(R.id.title).setText(title)
        view.findViewById<TextView>(R.id.hint).setText(hint)
        view.findViewById<TextView>(R.id.value).visibility = View.GONE
        view.setOnClickListener { onTap() }
        ui.settingsList.addView(view)
    }

    // --- наборы вариантов -------------------------------------------------

    /** Выбранный язык: пустой список — «как в системе». */
    private fun currentLanguage(): String =
        AppCompatDelegate.getApplicationLocales().toLanguageTags()
            .split(",").firstOrNull()?.take(2)?.takeIf { it.isNotBlank() }
            ?: LANGUAGE_SYSTEM

    /** Названия языков — на них самих: так их узнают и те, кто не читает
     *  по-русски, и те, кто не читает по-английски. */
    private fun languageOptions() = listOf(
        LANGUAGE_SYSTEM to getString(R.string.language_system),
        "ru" to "Русский",
        "en" to "English",
    )

    private fun themeOptions() = listOf(
        "system" to getString(R.string.theme_system),
        "light" to getString(R.string.theme_light),
        "dark" to getString(R.string.theme_dark),
    )

    private fun micOptions() =
        listOf(SYSTEM_MIC to getString(R.string.mic_system)) +
            MicDevices.list(this).map { it.id to it.title }

    private fun unloadOptions() = listOf(
        "never" to getString(R.string.unload_never),
        "hour1" to getString(R.string.unload_hour),
        "min15" to getString(R.string.unload_min15),
        "min10" to getString(R.string.unload_min10),
        "min5" to getString(R.string.unload_min5),
        "min2" to getString(R.string.unload_min2),
        "immediately" to getString(R.string.unload_now),
    )

    private fun limitOptions() =
        listOf(20, 50, 100, 200, 300).map { it.toString() to it.toString() }

    private fun retentionOptions() = listOf(
        "keep_limit" to getString(R.string.retention_forever),
        "days3" to getString(R.string.retention_days3),
        "weeks2" to getString(R.string.retention_weeks2),
        "months3" to getString(R.string.retention_months3),
        "never" to getString(R.string.retention_never),
    )

    companion object {
        private const val SYSTEM_MIC = "system"

        /** «Как в системе» — не код языка, поэтому отдельным значением. */
        private const val LANGUAGE_SYSTEM = "system"

        /**
         * Тема из настроек. Вызывается при старте приложения и при смене:
         * система пересоздаёт активити сама.
         */
        fun applyTheme(value: String) {
            AppCompatDelegate.setDefaultNightMode(
                when (value) {
                    "light" -> AppCompatDelegate.MODE_NIGHT_NO
                    "dark" -> AppCompatDelegate.MODE_NIGHT_YES
                    else -> AppCompatDelegate.MODE_NIGHT_FOLLOW_SYSTEM
                }
            )
        }
    }
}
