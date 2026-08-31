package com.handy.voice

import android.app.Notification
import android.app.NotificationChannel
import android.app.NotificationManager
import android.app.PendingIntent
import android.app.Service
import android.content.Context
import android.content.Intent
import android.os.IBinder
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.Job
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.cancel
import kotlinx.coroutines.launch
import kotlinx.coroutines.withContext

/**
 * Скачивание моделей в фоне.
 *
 * Раньше загрузка жила в корутине экрана и умирала вместе с ним: стоило
 * свернуть приложение — и всё останавливалось. Теперь это foreground-сервис,
 * то есть система обязана дать ему доработать, а пользователь видит прогресс
 * в шторке и может отменить загрузку оттуда же.
 */
class DownloadService : Service() {

    private val scope = CoroutineScope(SupervisorJob() + Dispatchers.Main)
    private val jobs = mutableMapOf<String, Job>()

    override fun onBind(intent: Intent?): IBinder? = null

    override fun onCreate() {
        super.onCreate()
        createChannel()
        // Сводное уведомление нужно сразу: система требует показать его в
        // первые секунды жизни foreground-сервиса, иначе убьёт процесс.
        startForeground(SUMMARY_ID, summaryNotification())
    }

    override fun onStartCommand(intent: Intent?, flags: Int, startId: Int): Int {
        when (intent?.action) {
            ACTION_CANCEL -> intent.getStringExtra(EXTRA_FILE)?.let { stop(it) }
            ACTION_START -> {
                val modelId = intent.getStringExtra(EXTRA_MODEL).orEmpty()
                val filename = intent.getStringExtra(EXTRA_FILE).orEmpty()
                begin(modelId, filename)
            }
        }
        if (jobs.isEmpty()) stopSelf()
        return START_NOT_STICKY
    }

    override fun onDestroy() {
        jobs.values.forEach { it.cancel() }
        jobs.clear()
        progress.clear()
        scope.cancel()
        onChange?.invoke()
        super.onDestroy()
    }

    private fun begin(modelId: String, filename: String) {
        if (filename.isEmpty() || jobs.containsKey(filename)) return
        val model = Catalog.models(this).firstOrNull { it.id == modelId } ?: return
        val file = model.files.firstOrNull { it.filename == filename } ?: return

        progress[filename] = 0
        notifyChange()

        lateinit var job: Job
        job = scope.launch {
            val result = withContext(Dispatchers.IO) {
                ModelStore.download(
                    context = this@DownloadService,
                    model = model,
                    file = file,
                    onProgress = { pct ->
                        progress[filename] = pct
                        notify(filename, model, file, pct)
                        notifyChange()
                    },
                    isCancelled = { job.isCancelled },
                )
            }
            val cancelled = job.isCancelled
            jobs.remove(filename)
            progress.remove(filename)
            manager().cancel(filename.hashCode())

            if (!cancelled) {
                if (result.isSuccess) {
                    if (ModelStore.activeFilename(this@DownloadService) == null) {
                        ModelStore.setActive(this@DownloadService, filename)
                    }
                    finished(model, success = true, reason = "")
                } else {
                    finished(model, success = false, reason = failureText(model, file))
                }
            }
            notifyChange()
            if (jobs.isEmpty()) stopSelf()
        }
        jobs[filename] = job
        notify(filename, model, file, 0)
    }

    private fun stop(filename: String) {
        jobs.remove(filename)?.cancel()
        progress.remove(filename)
        manager().cancel(filename.hashCode())
        notifyChange()
        if (jobs.isEmpty()) stopSelf()
    }

    /**
     * Запасное зеркало хранит только основную версию модели, поэтому при
     * блокировке Hugging Face осмысленно предложить именно её.
     */
    private fun failureText(model: CatalogModel, file: ModelFile): String =
        if (file.quant != model.defaultQuant && model.defaultQuant.isNotEmpty()) {
            getString(R.string.download_blocked, model.defaultQuant)
        } else {
            getString(R.string.download_failed, "")
        }

    // --- уведомления -----------------------------------------------------

    private fun manager() = getSystemService(NotificationManager::class.java)

    private fun createChannel() {
        manager().createNotificationChannel(
            NotificationChannel(
                CHANNEL,
                getString(R.string.channel_downloads),
                NotificationManager.IMPORTANCE_LOW,
            )
        )
    }

    private fun openApp() = PendingIntent.getActivity(
        this, 0, Intent(this, MainActivity::class.java), PendingIntent.FLAG_IMMUTABLE,
    )

    private fun summaryNotification(): Notification =
        Notification.Builder(this, CHANNEL)
            .setContentTitle(getString(R.string.channel_downloads))
            .setSmallIcon(R.drawable.ic_notification)
            .setContentIntent(openApp())
            .setOngoing(true)
            .build()

    private fun notify(filename: String, model: CatalogModel, file: ModelFile, percent: Int) {
        val cancel = PendingIntent.getService(
            this, filename.hashCode(),
            Intent(this, DownloadService::class.java)
                .setAction(ACTION_CANCEL)
                .putExtra(EXTRA_FILE, filename),
            PendingIntent.FLAG_IMMUTABLE,
        )
        manager().notify(
            filename.hashCode(),
            Notification.Builder(this, CHANNEL)
                .setContentTitle(model.name)
                .setContentText(getString(R.string.quant_row, file.quant, formatSize(file.sizeBytes)))
                .setSmallIcon(R.drawable.ic_notification)
                .setProgress(100, percent, false)
                .setContentIntent(openApp())
                .setOngoing(true)
                .addAction(
                    Notification.Action.Builder(
                        null, getString(R.string.download_cancel), cancel,
                    ).build()
                )
                .build(),
        )
    }

    /** Итог остаётся в шторке обычным уведомлением, его можно смахнуть. */
    private fun finished(model: CatalogModel, success: Boolean, reason: String) {
        manager().notify(
            model.id.hashCode(),
            Notification.Builder(this, CHANNEL)
                .setContentTitle(model.name)
                .setContentText(
                    if (success) getString(R.string.download_ready) else reason
                )
                .setSmallIcon(R.drawable.ic_notification)
                .setContentIntent(openApp())
                .setAutoCancel(true)
                .build(),
        )
    }

    private fun notifyChange() = onChange?.invoke()

    companion object {
        private const val CHANNEL = "downloads"
        private const val SUMMARY_ID = 10
        private const val ACTION_START = "com.handy.voice.DOWNLOAD_START"
        private const val ACTION_CANCEL = "com.handy.voice.DOWNLOAD_CANCEL"
        private const val EXTRA_MODEL = "model"
        private const val EXTRA_FILE = "file"

        /** Проценты по имени файла. Экран читает это, чтобы нарисовать полосы. */
        val progress = mutableMapOf<String, Int>()

        /** Экран подписывается сюда, чтобы перерисоваться по ходу загрузки. */
        @Volatile
        var onChange: (() -> Unit)? = null

        fun start(context: Context, model: CatalogModel, file: ModelFile) {
            context.startForegroundService(
                Intent(context, DownloadService::class.java)
                    .setAction(ACTION_START)
                    .putExtra(EXTRA_MODEL, model.id)
                    .putExtra(EXTRA_FILE, file.filename)
            )
        }

        fun cancel(context: Context, filename: String) {
            context.startService(
                Intent(context, DownloadService::class.java)
                    .setAction(ACTION_CANCEL)
                    .putExtra(EXTRA_FILE, filename)
            )
        }
    }
}
