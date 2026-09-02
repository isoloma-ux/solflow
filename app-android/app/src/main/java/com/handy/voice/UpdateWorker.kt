package com.handy.voice

import android.app.Notification
import android.app.NotificationChannel
import android.app.NotificationManager
import android.app.PendingIntent
import android.content.Context
import android.content.Intent
import androidx.work.Constraints
import androidx.work.CoroutineWorker
import androidx.work.ExistingPeriodicWorkPolicy
import androidx.work.NetworkType
import androidx.work.PeriodicWorkRequestBuilder
import androidx.work.WorkManager
import androidx.work.WorkerParameters
import java.util.concurrent.TimeUnit

/**
 * Раз в день спрашивает GitHub про новую версию и, если она вышла,
 * показывает уведомление. До этого о новой версии узнавали только те, кто
 * заходил в «О проекте» или смотрел на строку версии в шторке — то есть
 * почти никто. Тап по уведомлению ведёт в «О проекте», где обновление
 * ставится в одно нажатие.
 */
class UpdateWorker(context: Context, params: WorkerParameters) : CoroutineWorker(context, params) {

    override suspend fun doWork(): Result {
        val release = AppUpdate.latest() ?: return Result.retry()
        if (!AppUpdate.newer(release.version, BuildConfig.VERSION_NAME)) return Result.success()
        // Об одной версии — один раз: уведомление раз в день про то же
        // самое быстро научило бы смахивать не глядя.
        if (AppPrefs.notifiedVersion(applicationContext) == release.version) return Result.success()
        AppPrefs.setNotifiedVersion(applicationContext, release.version)
        notify(applicationContext, release.version)
        return Result.success()
    }

    private fun notify(context: Context, version: String) {
        val manager = context.getSystemService(NotificationManager::class.java)
        manager.createNotificationChannel(
            NotificationChannel(
                CHANNEL,
                context.getString(R.string.channel_updates),
                NotificationManager.IMPORTANCE_DEFAULT,
            )
        )
        val open = PendingIntent.getActivity(
            context, 0,
            Intent(context, AboutActivity::class.java)
                .putExtra(AboutActivity.EXTRA_UPDATE, true)
                .addFlags(Intent.FLAG_ACTIVITY_NEW_TASK),
            PendingIntent.FLAG_IMMUTABLE or PendingIntent.FLAG_UPDATE_CURRENT,
        )
        val notification = Notification.Builder(context, CHANNEL)
            .setSmallIcon(R.drawable.ic_notification)
            .setContentTitle(context.getString(R.string.update_notify_title, version))
            .setContentText(context.getString(R.string.update_notify_text))
            .setContentIntent(open)
            .setAutoCancel(true)
            .build()
        runCatching { manager.notify(NOTIF_ID, notification) }
    }

    companion object {
        private const val CHANNEL = "updates"
        private const val NOTIF_ID = 30
        private const val WORK = "update-check"

        fun schedule(context: Context) {
            val request = PeriodicWorkRequestBuilder<UpdateWorker>(1, TimeUnit.DAYS)
                .setConstraints(
                    Constraints.Builder().setRequiredNetworkType(NetworkType.CONNECTED).build()
                )
                .build()
            WorkManager.getInstance(context.applicationContext)
                .enqueueUniquePeriodicWork(WORK, ExistingPeriodicWorkPolicy.KEEP, request)
        }
    }
}
