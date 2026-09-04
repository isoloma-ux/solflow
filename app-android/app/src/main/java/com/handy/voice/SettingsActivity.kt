package com.handy.voice

import android.content.ClipData
import android.content.ClipboardManager
import android.content.Intent
import android.net.Uri
import android.os.Bundle
import android.view.View
import android.widget.TextView
import android.widget.Toast
import androidx.appcompat.app.AppCompatActivity
import androidx.appcompat.app.AppCompatDelegate
import androidx.core.os.LocaleListCompat
import androidx.core.view.ViewCompat
import androidx.core.view.WindowInsetsCompat
import com.google.android.material.dialog.MaterialAlertDialogBuilder
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

    /** Системный выбор папки экспорта: доступ запоминается насовсем. */
    private val pickExportDir = registerForActivityResult(
        androidx.activity.result.contract.ActivityResultContracts.OpenDocumentTree(),
    ) { uri ->
        if (uri != null) {
            contentResolver.takePersistableUriPermission(
                uri,
                Intent.FLAG_GRANT_READ_URI_PERMISSION or Intent.FLAG_GRANT_WRITE_URI_PERMISSION,
            )
            AppPrefs.setExportDir(this, uri.toString())
        }
        refresh()
    }

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

    override fun onResume() {
        super.onResume()
        // Вход и синхронизация идут в фоне — строка «Яндекс.Диск» должна
        // меняться сама: код введён, синхронизация прошла, что-то не вышло.
        SyncManager.addListener(syncListener)
    }

    override fun onPause() {
        SyncManager.removeListener(syncListener)
        super.onPause()
    }

    private val syncListener: () -> Unit = { runOnUiThread { if (!isFinishing) refresh() } }

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
        switch(
            R.string.set_lock_screen, R.string.set_lock_screen_hint,
            AppPrefs.recordOnLockScreen(this),
        ) {
            AppPrefs.setRecordOnLockScreen(this, it)
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

        group(R.string.group_meetings)
        choice(
            R.string.set_meeting_audio, R.string.set_meeting_audio_hint,
            listOf(
                "keep" to getString(R.string.meeting_audio_keep),
                "delete_done" to getString(R.string.meeting_audio_delete),
            ),
            AppPrefs.meetingAudio(this),
        ) { value ->
            AppPrefs.setMeetingAudio(this, value)
            refresh()
        }
        val usage = MeetingStore.audioUsage(this)
        linkText(
            R.string.set_purge_audio,
            if (usage > 0) getString(R.string.set_purge_audio_hint, formatSize(usage))
            else getString(R.string.set_purge_audio_empty),
        ) {
            if (usage <= 0) return@linkText
            MaterialAlertDialogBuilder(this)
                .setTitle(R.string.set_purge_audio)
                .setMessage(getString(R.string.purge_audio_confirm, formatSize(usage)))
                .setNegativeButton(android.R.string.cancel, null)
                .setPositiveButton(R.string.purge_audio_do) { _, _ ->
                    MeetingStore.purgeAudio(this)
                    refresh()
                }
                .show()
        }
        choice(
            R.string.set_export_dir, R.string.set_export_dir_hint,
            listOf(
                "downloads" to getString(R.string.export_dir_downloads),
                "pick" to getString(R.string.export_dir_pick),
            ),
            if (AppPrefs.exportDir(this) == null) "downloads" else "pick",
            valueText = MeetingExport.exportDirName(this) ?: getString(R.string.export_dir_downloads),
        ) { value ->
            if (value == "pick") {
                pickExportDir.launch(null)
            } else {
                AppPrefs.setExportDir(this, null)
                refresh()
            }
        }

        group(R.string.group_sync)
        linkText(R.string.set_sync_account, syncHint()) { onSyncTap() }
        if (SyncManager.connected(this)) {
            choice(
                R.string.set_sync_interval, R.string.set_sync_interval_hint,
                intervalOptions(), AppPrefs.syncInterval(this),
            ) { value ->
                AppPrefs.setSyncInterval(this, value)
                SyncManager.schedulePeriodic(this)
                refresh()
            }
            switch(R.string.set_sync_audio, R.string.set_sync_audio_hint, AppPrefs.syncAudio(this)) {
                AppPrefs.setSyncAudio(this, it)
                // Включили звук — он должен поехать сейчас, а не через час.
                if (it) SyncManager.runNow(this)
            }
        }

        group(R.string.group_about)
        link(R.string.about_title, R.string.about_hint) {
            startActivity(Intent(this, AboutActivity::class.java))
        }
    }

    // --- синхронизация ----------------------------------------------------

    /** Строка состояния под «Яндекс.Диск» — по тому, что сейчас происходит. */
    private fun syncHint(): String {
        val code = SyncManager.code
        val message = SyncManager.message
        return when {
            !Yandex.configured -> getString(R.string.sync_hint_unconfigured)
            code != null -> getString(R.string.sync_hint_waiting, code.userCode)
            SyncManager.connected(this) -> {
                val who = AppPrefs.yandexLogin(this).ifBlank { getString(R.string.sync_account_unknown) }
                val last = SyncManager.lastSync(this)
                SyncManager.progress ?: when {
                    SyncManager.running -> getString(R.string.sync_hint_running, who)
                    message != null -> getString(R.string.sync_hint_error, who, message)
                    last > 0 -> getString(
                        R.string.sync_hint_connected, who,
                        "${TranscriptStore.dayLabel(this, last)}, ${TranscriptStore.timeLabel(last)}",
                    )
                    else -> getString(R.string.sync_hint_connected_never, who)
                }
            }
            message != null -> getString(R.string.sync_hint_error_short, message)
            else -> getString(R.string.sync_hint_off)
        }
    }

    private fun onSyncTap() {
        if (!Yandex.configured) return
        SyncManager.code?.let { showCode(it); return }
        if (SyncManager.connected(this)) {
            optionSheet(
                getString(R.string.set_sync_account),
                listOf(
                    "now" to getString(R.string.sync_action_now),
                    "disconnect" to getString(R.string.sync_action_disconnect),
                ),
                null,
            ) { value ->
                when (value) {
                    "now" -> SyncManager.runNow(this)
                    "disconnect" -> MaterialAlertDialogBuilder(this)
                        .setTitle(R.string.sync_action_disconnect)
                        .setMessage(R.string.sync_disconnect_confirm)
                        .setPositiveButton(R.string.sync_action_disconnect) { _, _ ->
                            SyncManager.disconnect(this)
                            refresh()
                        }
                        .setNegativeButton(R.string.sync_cancel, null)
                        .show()
                }
            }
            return
        }
        SyncManager.startConnect(this) { code ->
            runOnUiThread { if (!isFinishing) showCode(code) }
        }
    }

    /**
     * Код для страницы Яндекса. Кнопка «Открыть страницу» закрывает диалог —
     * это нормально: ожидание идёт в фоне, а строка настроек показывает код и
     * сама сменится на «подключено».
     */
    private fun showCode(code: Yandex.DeviceCode) {
        // Код сразу в буфере: на странице Яндекса остаётся только вставить.
        copyCode(code)
        MaterialAlertDialogBuilder(this)
            .setTitle(R.string.sync_code_title)
            .setMessage(getString(R.string.sync_code_message, code.userCode))
            .setPositiveButton(R.string.sync_open_page) { _, _ ->
                copyCode(code)
                startActivity(Intent(Intent.ACTION_VIEW, Uri.parse(code.verificationUrl)))
            }
            .setNeutralButton(R.string.sync_copy_code) { _, _ ->
                copyCode(code)
                Toast.makeText(this, R.string.sync_copied, Toast.LENGTH_SHORT).show()
            }
            .setNegativeButton(R.string.sync_cancel) { _, _ -> SyncManager.cancelConnect() }
            .show()
        refresh()
    }

    private fun copyCode(code: Yandex.DeviceCode) {
        getSystemService(ClipboardManager::class.java)
            .setPrimaryClip(ClipData.newPlainText("Sol Flow", code.userCode))
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

    private fun link(title: Int, hint: Int, onTap: () -> Unit) =
        linkText(title, getString(hint), onTap)

    /** Строка-ссылка с подсказкой, собранной на ходу. */
    private fun linkText(title: Int, hint: String, onTap: () -> Unit) {
        val view = layoutInflater.inflate(R.layout.item_setting_choice, ui.settingsList, false)
        view.findViewById<TextView>(R.id.title).setText(title)
        view.findViewById<TextView>(R.id.hint).text = hint
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

    private fun intervalOptions() = listOf(
        "min1" to getString(R.string.sync_interval_min1),
        "min2" to getString(R.string.sync_interval_min2),
        "min5" to getString(R.string.sync_interval_min5),
        "min15" to getString(R.string.sync_interval_min15),
        "hour1" to getString(R.string.sync_interval_hour1),
        "manual" to getString(R.string.sync_interval_manual),
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
