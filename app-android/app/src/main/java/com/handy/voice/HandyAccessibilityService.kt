package com.handy.voice

import android.accessibilityservice.AccessibilityService
import android.content.ClipData
import android.content.ClipboardManager
import android.content.Context
import android.graphics.Rect
import android.os.Build
import android.os.Bundle
import android.os.Handler
import android.os.Looper
import android.provider.Settings
import android.view.accessibility.AccessibilityEvent
import android.view.accessibility.AccessibilityNodeInfo
import android.view.accessibility.AccessibilityWindowInfo

/**
 * Вставка распознанного текста в поле ввода чужого приложения.
 *
 * Android намеренно не даёт обычного API для записи в чужие поля — это защита
 * от кейлоггеров. Легальных путей ровно два: стать клавиатурой или получить
 * разрешение специальных возможностей. Второй путь оставляет пользователю его
 * привычную клавиатуру, поэтому выбран он (так же устроен Wispr Flow).
 *
 * Служба ничего не читает и не сохраняет: она отвечает на вопрос «есть ли
 * сейчас поле ввода в фокусе» и пишет туда по команде пользователя.
 */
class HandyAccessibilityService : AccessibilityService() {

    private val handler = Handler(Looper.getMainLooper())

    /**
     * Клавиатура на экране и координата её верхнего края.
     *
     * Раньше кнопка привязывалась к фокусу поля ввода через `findFocus`, но
     * этот признак врал: в лаунчере срабатывал, а в мессенджерах нет. Окно
     * клавиатуры видно в системном списке окон однозначно и совпадает с тем,
     * как это воспринимает пользователь — «кнопка приходит с клавиатурой».
     *
     * Окно клавиатуры остаётся в списке и когда она скрыта, поэтому одного
     * его наличия мало: смотрим на высоту окна.
     */
    fun keyboardState(): Pair<Boolean, Int> = runCatching {
        val ime = windows.firstOrNull { it.type == AccessibilityWindowInfo.TYPE_INPUT_METHOD }
            ?: return@runCatching false to 0
        val bounds = Rect().also { ime.getBoundsInScreen(it) }

        // Сравнивать с высотой экрана нельзя: служба получает метрики без
        // системных панелей, они не совпадают с реальными координатами окна,
        // и клавиатура то определялась, то нет. Достаточно того, что окно
        // ощутимой высоты — скрытая клавиатура даёт нулевую или крошечную.
        val minHeight = (MIN_KEYBOARD_DP * resources.displayMetrics.density).toInt()
        val visible = bounds.height() >= minHeight
        visible to bounds.top
    }.getOrDefault(false to 0)

    /**
     * Служба спецвозможностей — якорь всего приложения.
     *
     * Система держит её сама и перезапускает после гибели процесса, смахивания
     * из недавних и перезагрузки телефона, пока разрешение выдано. Поэтому
     * плавающую кнопку поднимаем именно отсюда: иначе после закрытия
     * приложения она пропадала до следующего ручного запуска.
     */
    override fun onServiceConnected() {
        super.onServiceConnected()
        instance = this
        if (AppPrefs.bubbleEnabled(this) && !DictationService.running) {
            runCatching { DictationService.start(this) }
        }
    }

    override fun onDestroy() {
        if (instance === this) instance = null
        handler.removeCallbacksAndMessages(null)
        super.onDestroy()
    }

    override fun onInterrupt() = Unit

    // События нам не нужны: состояние клавиатуры плавающая кнопка опрашивает
    // сама. На событиях это уже пробовалось — система доставляет их не всегда,
    // и кнопка не появлялась в чужих приложениях.
    override fun onAccessibilityEvent(event: AccessibilityEvent?) = Unit

    /**
     * Поле ввода в фокусе.
     *
     * Ищем по всем окнам приложений, а не через `rootInActiveWindow`: с тех
     * пор как служба видит список окон, «активным» может оказаться наш
     * собственный оверлей с кнопкой, и поле ввода в нём, разумеется, не
     * находилось — текст уходил в буфер обмена вместо поля.
     */
    private fun focusedEditable(): AccessibilityNodeInfo? = runCatching {
        windows
            .filter { it.type == AccessibilityWindowInfo.TYPE_APPLICATION }
            .asSequence()
            .mapNotNull { it.root }
            .mapNotNull { it.findFocus(AccessibilityNodeInfo.FOCUS_INPUT) }
            .firstOrNull { it.isEditable }
            ?: rootInActiveWindow
                ?.findFocus(AccessibilityNodeInfo.FOCUS_INPUT)
                ?.takeIf { it.isEditable }
    }.getOrNull()

    /**
     * Вставляет текст в позицию курсора. Возвращает false, если поля в фокусе
     * нет — тогда вызывающий кладёт текст в буфер обмена.
     */
    fun insert(text: String): Boolean {
        val node = focusedEditable() ?: return false
        val done = pasteInto(node, text) || setTextInto(node, text)
        if (done && AppPrefs.autoSubmit(this)) submitLater(node)
        return done
    }

    /**
     * Нажимает ввод после вставки — сообщение уходит само.
     *
     * С задержкой: вставка асинхронная, и до неё поле ещё пустое, так что
     * ввод отправил бы пустоту. Действие появилось только в Android 11 —
     * на старых телефонах настройка просто ничего не делает.
     */
    private fun submitLater(node: AccessibilityNodeInfo) {
        if (Build.VERSION.SDK_INT < Build.VERSION_CODES.R) return
        handler.postDelayed(
            {
                runCatching {
                    node.performAction(
                        AccessibilityNodeInfo.AccessibilityAction.ACTION_IME_ENTER.id
                    )
                }
            },
            SUBMIT_DELAY_MS,
        )
    }

    /**
     * Основной путь — системная вставка.
     *
     * Читать текущий текст поля нельзя: у пустого поля `text` возвращает
     * подсказку («Message» в Telegram), и надиктованное дописывалось к ней.
     * Признаки подсказки (`isShowingHintText`, `hintText`) заполняют не все
     * приложения, поэтому надёжнее не читать поле вообще, а поручить вставку
     * самому приложению — оно точно знает, где курсор и что в поле.
     *
     * Буфер обмена возвращаем на место: пользователь не должен терять то,
     * что копировал до диктовки.
     */
    private fun pasteInto(node: AccessibilityNodeInfo, text: String): Boolean {
        val clipboard = getSystemService(Context.CLIPBOARD_SERVICE) as? ClipboardManager
            ?: return false
        val previous = runCatching { clipboard.primaryClip }.getOrNull()

        clipboard.setPrimaryClip(ClipData.newPlainText("dictation", text))
        val pasted = node.performAction(AccessibilityNodeInfo.ACTION_PASTE)

        // Пользователь мог попросить оставить надиктованное в буфере — тогда
        // прежнее содержимое не возвращаем.
        if (previous != null && !AppPrefs.clipboardKeep(this)) {
            // С запасом: вставка асинхронная, вернуть буфер сразу нельзя.
            handler.postDelayed(
                { runCatching { clipboard.setPrimaryClip(previous) } },
                CLIPBOARD_RESTORE_MS,
            )
        }
        return pasted
    }

    /**
     * Запасной путь, если приложение не умеет ACTION_PASTE. Здесь текст поля
     * читать приходится, поэтому подсказку отсекаем по двум признакам.
     */
    private fun setTextInto(node: AccessibilityNodeInfo, text: String): Boolean {
        val existing = currentText(node)
        val start = node.textSelectionStart.takeIf { it in 0..existing.length } ?: existing.length
        val end = node.textSelectionEnd.takeIf { it in start..existing.length } ?: start

        val prefix = existing.substring(0, start)
        val glue = if (prefix.isNotEmpty() && !prefix.last().isWhitespace()) " " else ""
        val updated = prefix + glue + text + existing.substring(end)

        val args = Bundle().apply {
            putCharSequence(AccessibilityNodeInfo.ACTION_ARGUMENT_SET_TEXT_CHARSEQUENCE, updated)
        }
        if (!node.performAction(AccessibilityNodeInfo.ACTION_SET_TEXT, args)) return false

        val caret = prefix.length + glue.length + text.length
        node.performAction(
            AccessibilityNodeInfo.ACTION_SET_SELECTION,
            Bundle().apply {
                putInt(AccessibilityNodeInfo.ACTION_ARGUMENT_SELECTION_START_INT, caret)
                putInt(AccessibilityNodeInfo.ACTION_ARGUMENT_SELECTION_END_INT, caret)
            },
        )
        return true
    }

    private fun currentText(node: AccessibilityNodeInfo): String {
        val raw = node.text?.toString().orEmpty()
        if (node.isShowingHintText) return ""
        val hint = node.hintText?.toString()
        if (!hint.isNullOrEmpty() && raw == hint) return ""
        return raw
    }

    companion object {
        private const val CLIPBOARD_RESTORE_MS = 1200L

        /** Вставка успевает дойти до поля раньше, чем мы жмём ввод. */
        private const val SUBMIT_DELAY_MS = 400L

        /** Реальная клавиатура заметно выше; скрытая даёт почти нулевое окно. */
        private const val MIN_KEYBOARD_DP = 120

        @Volatile
        var instance: HandyAccessibilityService? = null
            private set


        /**
         * Служба живёт и готова вставлять текст. После остановки приложения
         * объект гибнет, даже если разрешение осталось выданным, — поэтому
         * для карточки разрешений это не подходит.
         */
        val isConnected: Boolean get() = instance != null

        /**
         * Служба выдана и реально работает.
         *
         * Проверять один только список `ENABLED_ACCESSIBILITY_SERVICES` нельзя:
         * после обновления приложения имя службы в нём остаётся, но главный
         * переключатель `ACCESSIBILITY_ENABLED` система сбрасывает в ноль, и
         * служба не запускается. Приложение при этом показывало «выдано», хотя
         * текст уходил в буфер обмена — то есть врало о своём состоянии.
         *
         * Живое подключение важнее настроек: если объект службы есть, она точно
         * работает, что бы ни было записано в Settings.
         */
        fun isGranted(context: Context): Boolean {
            if (isConnected) return true

            val master = Settings.Secure.getInt(
                context.contentResolver,
                Settings.Secure.ACCESSIBILITY_ENABLED,
                0,
            )
            if (master == 0) return false

            val enabled = Settings.Secure.getString(
                context.contentResolver,
                Settings.Secure.ENABLED_ACCESSIBILITY_SERVICES,
            ).orEmpty()
            val name = "${context.packageName}/${HandyAccessibilityService::class.java.name}"
            val short = "${context.packageName}/.${HandyAccessibilityService::class.java.simpleName}"
            return enabled.split(':').any { it.equals(name, true) || it.equals(short, true) }
        }
    }
}
