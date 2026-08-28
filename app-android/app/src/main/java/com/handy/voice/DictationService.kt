package com.handy.voice

import android.app.Notification
import android.app.NotificationChannel
import android.app.NotificationManager
import android.app.PendingIntent
import android.app.Service
import android.content.ClipData
import android.content.ClipboardManager
import android.content.Context
import android.content.Intent
import android.graphics.PixelFormat
import android.os.Build
import android.os.IBinder
import android.provider.Settings
import android.view.Gravity
import android.view.MotionEvent
import android.view.View
import android.view.WindowManager
import android.widget.FrameLayout
import android.widget.TextView
import android.widget.Toast
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.Job
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.cancel
import kotlinx.coroutines.delay
import kotlinx.coroutines.launch
import kotlinx.coroutines.withContext
import kotlin.math.abs
import kotlin.math.roundToInt

/**
 * Плавающая кнопка диктовки поверх других приложений.
 *
 * Живёт в foreground-сервисе: без него Android оборвёт доступ к микрофону,
 * как только пользователь уйдёт из приложения, а вся смысловая нагрузка тут
 * именно в работе поверх чужих экранов.
 */
class DictationService : Service() {

    private lateinit var windows: WindowManager
    private lateinit var bubble: BubbleView
    private lateinit var params: WindowManager.LayoutParams

    private var snoozeZone: View? = null
    private var snoozedUntil = 0L
    private var keyboardShown = false
    private var lastDegraded: Boolean? = null

    private var recorder = AudioRecorder()
    private val scope = CoroutineScope(SupervisorJob() + Dispatchers.Main)
    private var levelJob: Job? = null
    private var keyboardJob: Job? = null

    override fun onBind(intent: Intent?): IBinder? = null

    override fun onCreate() {
        super.onCreate()
        windows = getSystemService(WINDOW_SERVICE) as WindowManager
        startForeground(NOTIFICATION_ID, notification())
        addBubble()

        // Кнопка приходит вместе с клавиатурой — как в Wispr Flow.
        watchKeyboard()
        updateVisibility()
        running = true
    }

    override fun onStartCommand(intent: Intent?, flags: Int, startId: Int): Int {
        if (intent?.action == ACTION_STOP) {
            stopSelf()
            return START_NOT_STICKY
        }
        return START_STICKY
    }

    override fun onDestroy() {
        running = false
        keyboardJob?.cancel()
        levelJob?.cancel()
        if (recorder.isRecording) recorder.stop()
        runCatching { windows.removeView(bubble) }
        hideSnoozeZone()
        scope.cancel()
        super.onDestroy()
    }

    // --- оверлей ---------------------------------------------------------

    private fun addBubble() {
        bubble = BubbleView(this)
        val size = dp(56)
        params = WindowManager.LayoutParams(
            size, size,
            WindowManager.LayoutParams.TYPE_APPLICATION_OVERLAY,
            WindowManager.LayoutParams.FLAG_NOT_FOCUSABLE or
                WindowManager.LayoutParams.FLAG_LAYOUT_NO_LIMITS,
            PixelFormat.TRANSLUCENT,
        ).apply {
            gravity = Gravity.TOP or Gravity.START
            x = resources.displayMetrics.widthPixels - size - dp(16)
            y = resources.displayMetrics.heightPixels / 2
        }
        bubble.setOnTouchListener(DragHandler())
        windows.addView(bubble, params)
    }

    /**
     * Три жеста на одной кнопке:
     *   короткий тап   — включить запись, следующий тап выключает;
     *   удержание      — пишем, пока палец на кнопке, отпустил — отправили;
     *   перетаскивание — двигаем кнопку, сброс вниз откладывает её.
     */
    private inner class DragHandler : View.OnTouchListener {
        private var downX = 0f
        private var downY = 0f
        private var startX = 0
        private var startY = 0
        private var dragging = false
        private var holdMode = false

        private val startHold = Runnable {
            if (!dragging && !recorder.isRecording) {
                holdMode = true
                beginRecording()
            }
        }

        override fun onTouch(v: View, e: MotionEvent): Boolean {
            when (e.action) {
                MotionEvent.ACTION_DOWN -> {
                    downX = e.rawX; downY = e.rawY
                    startX = params.x; startY = params.y
                    dragging = false
                    holdMode = false
                    v.postDelayed(startHold, HOLD_MS)
                }

                MotionEvent.ACTION_MOVE -> {
                    // На удержании кнопку не двигаем: палец там держат ради
                    // записи, а мелкое дрожание не должно её растаскивать.
                    if (holdMode) return true

                    val dx = e.rawX - downX
                    val dy = e.rawY - downY
                    if (!dragging && (abs(dx) > touchSlop || abs(dy) > touchSlop)) {
                        dragging = true
                        v.removeCallbacks(startHold)
                        showSnoozeZone()
                    }
                    if (dragging) {
                        params.x = startX + dx.roundToInt()
                        params.y = startY + dy.roundToInt()
                        runCatching { windows.updateViewLayout(bubble, params) }
                    }
                }

                MotionEvent.ACTION_UP, MotionEvent.ACTION_CANCEL -> {
                    v.removeCallbacks(startHold)
                    when {
                        dragging -> {
                            val droppedInZone =
                                e.rawY > resources.displayMetrics.heightPixels - dp(120)
                            hideSnoozeZone()
                            if (droppedInZone) snooze() else snapToEdge()
                        }

                        holdMode -> {
                            holdMode = false
                            finishRecording()
                        }

                        else -> {
                            v.performClick()
                            toggle()
                        }
                    }
                }
            }
            return true
        }
    }

    /** Пузырь липнет к ближайшему боку, если пользователь этого хочет. */
    private fun snapToEdge() {
        if (!AppPrefs.snapToEdge(this)) return
        val width = resources.displayMetrics.widthPixels
        params.x = if (params.x + dp(28) < width / 2) dp(16) else width - dp(56) - dp(16)
        runCatching { windows.updateViewLayout(bubble, params) }
    }

    private fun showSnoozeZone() {
        if (snoozeZone != null) return
        val label = TextView(this).apply {
            text = getString(R.string.snooze_hint)
            setTextColor(getColor(R.color.fog))
            textSize = 14f
            typeface = resources.getFont(R.font.inter_medium)
            gravity = Gravity.CENTER
            setBackgroundResource(R.drawable.bg_snooze)
        }
        val holder = FrameLayout(this).apply {
            addView(label, FrameLayout.LayoutParams(dp(240), dp(52), Gravity.CENTER))
        }
        val zoneParams = WindowManager.LayoutParams(
            WindowManager.LayoutParams.MATCH_PARENT, dp(120),
            WindowManager.LayoutParams.TYPE_APPLICATION_OVERLAY,
            WindowManager.LayoutParams.FLAG_NOT_FOCUSABLE or
                WindowManager.LayoutParams.FLAG_NOT_TOUCHABLE,
            PixelFormat.TRANSLUCENT,
        ).apply { gravity = Gravity.BOTTOM }
        runCatching { windows.addView(holder, zoneParams) }
        snoozeZone = holder
    }

    private fun hideSnoozeZone() {
        snoozeZone?.let { runCatching { windows.removeView(it) } }
        snoozeZone = null
    }

    private fun snooze() {
        snoozedUntil = System.currentTimeMillis() + SNOOZE_MINUTES * 60_000L
        updateVisibility()
        toast(getString(R.string.snoozed, SNOOZE_MINUTES))
        scope.launch {
            delay(SNOOZE_MINUTES * 60_000L)
            snoozedUntil = 0L
            updateVisibility()
        }
    }

    /**
     * Кнопка видна, пока на экране клавиатура. Два исключения: во время
     * записи и распознавания она остаётся на месте (клавиатура часто
     * закрывается по ходу диктовки), а без службы спецвозможностей список
     * окон недоступен — тогда показываем всегда, иначе кнопка не появится
     * вовсе.
     */
    private fun updateVisibility() {
        val snoozing = System.currentTimeMillis() < snoozedUntil
        val busy = recorder.isRecording || bubble.state != BubbleView.State.IDLE
        val canSeeKeyboard = HandyAccessibilityService.isConnected
        val show = !snoozing && (busy || !canSeeKeyboard || keyboardShown)
        bubble.visibility = if (show) View.VISIBLE else View.GONE
    }

    /**
     * Раз в 300 мс спрашиваем систему, показана ли клавиатура.
     *
     * Опрос, а не подписка на события: события об изменении окон система
     * доставляет не всегда, из-за чего кнопка не появлялась в чужих
     * приложениях. Проверка дешёвая — чтение списка окон.
     */
    private fun watchKeyboard() {
        keyboardJob?.cancel()
        keyboardJob = scope.launch {
            while (true) {
                val service = HandyAccessibilityService.instance

                // Спецвозможности могут отключиться на ходу — Android делает
                // это при каждом обновлении приложения. Уведомление должно
                // честно показывать, что кнопка работает в урезанном режиме.
                val degraded = service == null
                if (degraded != lastDegraded) {
                    val wasWorking = lastDegraded == false
                    lastDegraded = degraded
                    getSystemService(NotificationManager::class.java)
                        .notify(NOTIFICATION_ID, notification(degraded))
                    // Права отваливаются сами: система выгружает приложение,
                    // и служба гаснет. Пользователь узнавал об этом только
                    // когда текст уходил в буфер — поэтому говорим сразу.
                    if (degraded && wasWorking) warnAccessibilityLost()
                    updateVisibility()
                }

                val (shown, top) = service?.keyboardState() ?: (false to 0)
                if (shown != keyboardShown) {
                    keyboardShown = shown
                    if (shown) placeAboveKeyboard(top)
                    updateVisibility()
                }
                delay(KEYBOARD_POLL_MS)
            }
        }
    }

    /** Поднимает кнопку над клавиатурой, если та её накрыла. */
    private fun placeAboveKeyboard(keyboardTop: Int) {
        if (keyboardTop <= 0) return
        val highest = dp(80)
        val limit = keyboardTop - dp(56) - dp(12)
        if (params.y > limit) {
            params.y = limit.coerceAtLeast(highest)
            runCatching { windows.updateViewLayout(bubble, params) }
        }
    }

    // --- диктовка --------------------------------------------------------

    private fun toggle() {
        if (recorder.isRecording) finishRecording() else beginRecording()
    }

    private fun beginRecording() {
        // Режим микрофона и выбранное устройство могли поменяться в
        // настройках, пока сервис жил.
        recorder = AudioRecorder(AppPrefs.roomMode(this), MicDevices.preferred(this))
        if (!recorder.start()) {
            toast(getString(R.string.mic_failed))
            return
        }
        AudioSession.playStart(this)
        AudioSession.mute(this)
        bubble.state = BubbleView.State.RECORDING
        updateVisibility()
        levelJob = scope.launch {
            while (recorder.isRecording) {
                bubble.level = recorder.level
                delay(40)
            }
        }
    }

    private fun finishRecording() {
        val pcm = recorder.stop()
        AudioSession.unmute(this)
        levelJob?.cancel()
        bubble.state = BubbleView.State.PROCESSING

        val seconds = pcm.size.toFloat() / AudioRecorder.SAMPLE_RATE
        if (seconds < 0.3f) {
            bubble.state = BubbleView.State.IDLE
            updateVisibility()
            return
        }

        scope.launch {
            val ready = Engine.ensureLoaded(this@DictationService)
            val text = if (ready) {
                withContext(Dispatchers.Default) {
                    Engine.transcribe(
                        pcm,
                        AudioRecorder.SAMPLE_RATE,
                        AppPrefs.removeFillers(this@DictationService),
                    )
                }
            } else ""
            Engine.scheduleUnload(this@DictationService)

            bubble.state = BubbleView.State.IDLE
            updateVisibility()

            if (text.isBlank()) {
                toast(getString(if (ready) R.string.nothing_heard else R.string.no_model))
                return@launch
            }

            TranscriptStore.add(this@DictationService, text, seconds, pcm)
            deliver(text)
        }
    }

    /** Пишем прямо в поле ввода, а если его нет — кладём в буфер обмена. */
    private fun deliver(text: String) {
        val service = HandyAccessibilityService.instance
        if (service != null && service.insert(text)) return

        val cm = getSystemService(Context.CLIPBOARD_SERVICE) as ClipboardManager
        cm.setPrimaryClip(ClipData.newPlainText("transcript", text))
        // Причины две, и они требуют разных действий от пользователя, поэтому
        // сообщения разные: одно про выключенное разрешение, другое про
        // отсутствие поля в фокусе.
        toast(
            getString(
                if (service == null) R.string.copied_no_accessibility
                else R.string.copied_no_field
            )
        )
    }

    // --- служебное -------------------------------------------------------

    private fun notification(degraded: Boolean = !HandyAccessibilityService.isConnected): Notification {
        val manager = getSystemService(NotificationManager::class.java)
        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.O) {
            manager.createNotificationChannel(
                NotificationChannel(
                    CHANNEL, getString(R.string.channel_name),
                    NotificationManager.IMPORTANCE_LOW,
                )
            )
        }
        val open = PendingIntent.getActivity(
            this, 0, Intent(this, MainActivity::class.java),
            PendingIntent.FLAG_IMMUTABLE,
        )
        val stop = PendingIntent.getService(
            this, 1, Intent(this, DictationService::class.java).setAction(ACTION_STOP),
            PendingIntent.FLAG_IMMUTABLE,
        )
        return Notification.Builder(this, CHANNEL)
            .setContentTitle(getString(R.string.bubble_running))
            .setContentText(
                getString(
                    if (degraded) R.string.bubble_running_degraded
                    else R.string.bubble_running_hint
                )
            )
            .setSmallIcon(android.R.drawable.presence_audio_online)
            .setContentIntent(open)
            .addAction(
                Notification.Action.Builder(null, getString(R.string.turn_off), stop).build()
            )
            .setOngoing(true)
            .build()
    }

    /** Отдельное уведомление, ведущее прямо в настройки спецвозможностей. */
    private fun warnAccessibilityLost() {
        val manager = getSystemService(NotificationManager::class.java)
        manager.createNotificationChannel(
            NotificationChannel(
                CHANNEL_ALERT,
                getString(R.string.accessibility_lost),
                NotificationManager.IMPORTANCE_HIGH,
            )
        )
        val open = PendingIntent.getActivity(
            this, 2,
            Intent(Settings.ACTION_ACCESSIBILITY_SETTINGS)
                .addFlags(Intent.FLAG_ACTIVITY_NEW_TASK),
            PendingIntent.FLAG_IMMUTABLE,
        )
        manager.notify(
            ALERT_ID,
            Notification.Builder(this, CHANNEL_ALERT)
                .setContentTitle(getString(R.string.accessibility_lost))
                .setContentText(getString(R.string.accessibility_lost_hint))
                .setSmallIcon(android.R.drawable.stat_sys_warning)
                .setContentIntent(open)
                .setAutoCancel(true)
                .build(),
        )
    }

    private fun toast(text: String) = Toast.makeText(this, text, Toast.LENGTH_SHORT).show()

    private fun dp(value: Int) = (value * resources.displayMetrics.density).toInt()

    private val touchSlop by lazy {
        android.view.ViewConfiguration.get(this).scaledTouchSlop.toFloat()
    }

    companion object {
        private const val CHANNEL = "dictation"
        private const val CHANNEL_ALERT = "accessibility"
        private const val NOTIFICATION_ID = 1
        private const val ALERT_ID = 2
        private const val SNOOZE_MINUTES = 10

        /** Чуть меньше системного long press: диктовку хочется начинать сразу. */
        private const val HOLD_MS = 320L

        /** Достаточно часто, чтобы кнопка появлялась вместе с клавиатурой. */
        private const val KEYBOARD_POLL_MS = 300L
        const val ACTION_STOP = "com.handy.voice.STOP"

        @Volatile
        var running = false
            private set

        fun canDrawOverlay(context: Context): Boolean = Settings.canDrawOverlays(context)

        fun start(context: Context) {
            AppPrefs.setBubbleEnabled(context, true)
            context.startForegroundService(Intent(context, DictationService::class.java))
        }

        fun stop(context: Context) {
            AppPrefs.setBubbleEnabled(context, false)
            context.startService(
                Intent(context, DictationService::class.java).setAction(ACTION_STOP)
            )
        }
    }
}
