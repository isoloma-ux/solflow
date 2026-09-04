package com.handy.voice

import android.content.Context
import android.os.Build
import android.util.Log
import androidx.work.Constraints
import androidx.work.CoroutineWorker
import androidx.work.ExistingPeriodicWorkPolicy
import androidx.work.ExistingWorkPolicy
import androidx.work.NetworkType
import androidx.work.OneTimeWorkRequestBuilder
import androidx.work.PeriodicWorkRequestBuilder
import androidx.work.WorkManager
import androidx.work.WorkerParameters
import java.io.File
import android.os.Handler
import android.os.Looper
import java.util.UUID
import java.util.concurrent.CopyOnWriteArraySet
import java.util.concurrent.TimeUnit
import kotlin.concurrent.thread

/**
 * Синхронизация как часть приложения: вход, выход, когда ходить на Диск.
 * Сам проход — в [SyncEngine], сеть — в [Yandex].
 *
 * Триггеры: после любого изменения — через WorkManager с задержкой (серия
 * правок уезжает одним разом), раз в час фоном, при открытии приложения и
 * по кнопке в настройках. Экран подписывается на [onStatus], чтобы
 * перерисовать строку «Яндекс.Диск».
 */
object SyncManager {

    private const val TAG = "HandyVoice"
    private const val WORK_SOON = "sync-soon"
    private const val WORK_PERIODIC = "sync-periodic"

    /** Сколько ждать после изменения, прежде чем идти на Диск. */
    private const val DEBOUNCE_SEC = 20L

    /** Не чаще этого — по открытию приложения. */
    private const val OPEN_THROTTLE_MS = 2 * 60 * 1000L

    /** За сколько до конца срока токен продлевается заранее. */
    private const val REFRESH_AHEAD_MS = 7L * 24 * 3600 * 1000

    @Volatile var running = false
        private set

    /** Последняя ошибка — строка под кнопкой. */
    @Volatile var message: String? = null
        private set

    /** Что делается прямо сейчас: «Отправляю звук: …». */
    @Volatile var progress: String? = null
        private set

    /** Вход идёт: код, который человек вводит на странице Яндекса. */
    @Volatile var code: Cloud.DeviceCode? = null
        private set

    /** В какое облако сейчас входят — пока код не подтверждён. */
    @Volatile var connecting: Cloud.Provider? = null
        private set

    @Volatile private var cancelConnect = false
    @Volatile private var lastRun = 0L
    private val lock = Any()

    /**
     * Кто хочет знать о ходе синхронизации: экран настроек (строка под
     * «Яндекс.Диск») и главный экран (крутилка списка, строка в шторке).
     */
    private val listeners = CopyOnWriteArraySet<() -> Unit>()

    fun addListener(listener: () -> Unit) { listeners += listener }

    fun removeListener(listener: () -> Unit) { listeners -= listener }

    fun connected(context: Context) = AppPrefs.yandexToken(context) != null

    fun lastSync(context: Context) = SyncEngine.State.load(context).lastSync

    private fun notifyStatus() { for (l in listeners) l() }

    // --- пока приложение на экране ------------------------------------------

    private val handler = Handler(Looper.getMainLooper())
    private var pollingContext: Context? = null

    /**
     * Опрос по расписанию из настроек, пока приложение открыто: WorkManager
     * не умеет чаще раза в четверть часа, а человек с Маком в соседней
     * комнате ждёт саммери через минуту. Проверяем каждые полминуты, идём на
     * Диск, когда с прошлого раза прошёл заданный интервал.
     */
    private val poll = object : Runnable {
        override fun run() {
            val ctx = pollingContext ?: return
            val interval = AppPrefs.syncIntervalMs(ctx)
            if (interval != null && connected(ctx) && !running &&
                System.currentTimeMillis() - lastRun >= interval
            ) {
                runNow(ctx)
            }
            handler.postDelayed(this, 30_000)
        }
    }

    fun startPolling(context: Context) {
        pollingContext = context.applicationContext
        handler.removeCallbacks(poll)
        handler.post(poll)
    }

    fun stopPolling() {
        handler.removeCallbacks(poll)
        pollingContext = null
    }

    // --- вход и выход ---------------------------------------------------------

    /**
     * Начало входа: получить код и в фоне ждать, пока человек его введёт.
     * [onCode] зовётся с кодом (в фоновом потоке), ошибки уходят в [message].
     */
    fun startConnect(context: Context, cloud: Cloud.Provider, onCode: (Cloud.DeviceCode) -> Unit) {
        val app = context.applicationContext
        code?.let { if (connecting?.id == cloud.id && it.expiresAt > System.currentTimeMillis()) { onCode(it); return } }
        cancelConnect = false
        message = null
        connecting = cloud
        thread(name = "cloud-connect") {
            val flow = try {
                cloud.deviceCode(deviceName(), deviceId(app))
            } catch (e: Exception) {
                message = e.message ?: e.toString()
                notifyStatus()
                return@thread
            }
            code = flow
            notifyStatus()
            onCode(flow)

            var tokens: Cloud.Tokens? = null
            try {
                while (true) {
                    Thread.sleep(flow.interval * 1000)
                    if (cancelConnect) break
                    if (System.currentTimeMillis() > flow.expiresAt) error("код устарел — запросите новый")
                    when (val poll = cloud.pollToken(flow)) {
                        is Cloud.Poll.Pending -> continue
                        is Cloud.Poll.Done -> { tokens = poll.tokens; break }
                    }
                }
            } catch (e: Exception) {
                message = e.message ?: e.toString()
            }
            code = null
            connecting = null
            if (tokens != null) {
                val login = runCatching { cloud.account(tokens.access) }.getOrDefault("")
                AppPrefs.setSyncProvider(app, cloud.id)
                AppPrefs.setYandexTokens(app, tokens.access, tokens.refresh, tokens.expiresAt)
                AppPrefs.setYandexLogin(app, login)
                // Новый аккаунт — старые отметки о файлах ни о чём.
                SyncEngine.State.clear(app)
                message = null
                notifyStatus()
                schedulePeriodic(app)
                runNow(app)
            } else {
                notifyStatus()
            }
        }
    }

    fun cancelConnect() {
        cancelConnect = true
        code = null
        connecting = null
        notifyStatus()
    }

    /** Выход: токен отзывается и стирается. Встречи остаются и здесь, и на Диске. */
    fun disconnect(context: Context) {
        val app = context.applicationContext
        val cloud = Cloud.current(app)
        val token = AppPrefs.yandexToken(app)
        AppPrefs.setYandexTokens(app, null, null, 0)
        AppPrefs.setYandexLogin(app, "")
        SyncEngine.State.clear(app)
        message = null
        WorkManager.getInstance(app).cancelUniqueWork(WORK_PERIODIC)
        WorkManager.getInstance(app).cancelUniqueWork(WORK_SOON)
        if (token != null) thread { cloud.revoke(token) }
        notifyStatus()
    }

    private fun deviceName(): String = "Sol Flow · ${Build.MANUFACTURER} ${Build.MODEL}".trim()

    /**
     * Устойчивый идентификатор устройства для Яндекса: он ограничивает
     * число токенов на устройство, и каждый вход с этого телефона должен
     * приходить под одним именем.
     */
    private fun deviceId(context: Context): String {
        val f = File(context.filesDir, "device-id")
        if (f.exists()) f.readText().trim().takeIf { it.isNotEmpty() }?.let { return it }
        val id = UUID.randomUUID().toString().replace("-", "")
        f.writeText(id)
        return id
    }

    // --- триггеры ---------------------------------------------------------------

    /** Локально что-то поменялось — уедет с задержкой, когда правки утихнут. */
    fun touch(context: Context) {
        if (!connected(context)) return
        val request = OneTimeWorkRequestBuilder<SyncWorker>()
            .setInitialDelay(DEBOUNCE_SEC, TimeUnit.SECONDS)
            .setConstraints(Constraints.Builder().setRequiredNetworkType(NetworkType.CONNECTED).build())
            .build()
        WorkManager.getInstance(context.applicationContext)
            .enqueueUniqueWork(WORK_SOON, ExistingWorkPolicy.REPLACE, request)
    }

    /**
     * Встречу удалили: Диск узнает об этом надгробием при следующем заходе.
     * Записывается сразу — приложение могут закрыть раньше.
     */
    fun noteDeleted(context: Context, id: Long) {
        if (!connected(context)) return
        val state = SyncEngine.State.load(context)
        if (id !in state.pendingDeletes) {
            state.pendingDeletes += id
            state.save(context)
        }
        touch(context)
    }

    /**
     * Фоном, со свёрнутым приложением — по интервалу из настроек, но не чаще
     * четверти часа: меньше WorkManager не даёт. «Только вручную» — фона нет.
     */
    fun schedulePeriodic(context: Context) {
        val app = context.applicationContext
        val interval = AppPrefs.syncIntervalMs(app)
        if (!connected(app) || interval == null) {
            WorkManager.getInstance(app).cancelUniqueWork(WORK_PERIODIC)
            return
        }
        val minutes = (interval / 60_000).coerceAtLeast(15)
        val request = PeriodicWorkRequestBuilder<SyncWorker>(minutes, TimeUnit.MINUTES)
            .setConstraints(Constraints.Builder().setRequiredNetworkType(NetworkType.CONNECTED).build())
            .build()
        WorkManager.getInstance(app)
            .enqueueUniquePeriodicWork(WORK_PERIODIC, ExistingPeriodicWorkPolicy.UPDATE, request)
    }

    /**
     * Открыли приложение или вкладку встреч — проверим Диск, если давно не
     * смотрели. При «только вручную» ничего не делаем: человек так решил.
     */
    fun onAppOpened(context: Context) {
        if (!connected(context)) return
        schedulePeriodic(context)
        val interval = AppPrefs.syncIntervalMs(context) ?: return
        if (System.currentTimeMillis() - lastRun > minOf(interval, OPEN_THROTTLE_MS)) runNow(context)
    }

    /** Синхронизация сейчас, в фоновом потоке. */
    fun runNow(context: Context) {
        val app = context.applicationContext
        thread(name = "cloud-sync") { runBlockingSync(app) }
    }

    /**
     * Сам проход, синхронно и под замком: вторая просьба ждёт первую.
     * Возвращает текст ошибки или null.
     */
    fun runBlockingSync(context: Context): String? {
        val app = context.applicationContext
        if (!connected(app)) return null
        synchronized(lock) {
            running = true
            notifyStatus()
            val result = try {
                var token = freshToken(app)
                try {
                    pass(app, token)
                } catch (e: Cloud.Unauthorized) {
                    token = refreshTokens(app) ?: throw Exception("нужно войти заново: ${e.message}")
                    pass(app, token)
                }
            } catch (e: Exception) {
                Log.w(TAG, "синхронизация", e)
                e.message ?: e.toString()
            }
            lastRun = System.currentTimeMillis()
            running = false
            progress = null
            message = result
            notifyStatus()
            return result
        }
    }

    private fun pass(app: Context, token: String): String? {
        val busy = MeetingService.phase.keys.toMutableSet()
        MeetingService.recordingId?.let { busy += it }
        val outcome = SyncEngine.run(app, Cloud.current(app), token, AppPrefs.syncAudio(app), busy) { text ->
            progress = text
            notifyStatus()
        }
        if (outcome.changedLocal) MeetingService.onChange?.invoke()
        return outcome.error
    }

    // --- токены -----------------------------------------------------------------

    private fun freshToken(app: Context): String {
        val token = AppPrefs.yandexToken(app) ?: error("облако не подключено")
        val expiresAt = AppPrefs.yandexExpiresAt(app)
        if (AppPrefs.yandexRefresh(app) != null && expiresAt > 0 &&
            expiresAt - System.currentTimeMillis() < REFRESH_AHEAD_MS
        ) {
            return refreshTokens(app) ?: token
        }
        return token
    }

    private fun refreshTokens(app: Context): String? {
        val refresh = AppPrefs.yandexRefresh(app) ?: return null
        return runCatching {
            val t = Cloud.current(app).refresh(refresh)
            AppPrefs.setYandexTokens(app, t.access, t.refresh.ifBlank { refresh }, t.expiresAt)
            t.access
        }.onFailure { Log.w(TAG, "продление токена", it) }.getOrNull()
    }
}

/** Фоновый проход по расписанию WorkManager — тот же [SyncManager.runBlockingSync]. */
class SyncWorker(context: Context, params: WorkerParameters) : CoroutineWorker(context, params) {
    override suspend fun doWork(): Result {
        val error = SyncManager.runBlockingSync(applicationContext)
        // Ошибка сети — попробуем позже; своя ошибка данных повтором не лечится.
        return if (error == null) Result.success() else Result.retry()
    }
}
