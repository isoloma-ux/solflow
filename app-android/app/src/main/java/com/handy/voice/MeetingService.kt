package com.handy.voice

import android.annotation.SuppressLint
import android.app.Notification
import android.app.NotificationChannel
import android.app.NotificationManager
import android.app.PendingIntent
import android.app.Service
import android.content.Context
import android.content.Intent
import android.content.pm.ServiceInfo
import android.media.AudioFormat
import android.media.AudioRecord
import android.media.MediaRecorder
import android.net.Uri
import android.os.IBinder
import android.os.ParcelFileDescriptor
import java.io.File
import android.os.PowerManager
import android.util.Log
import androidx.core.app.ServiceCompat
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.Job
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.cancel
import kotlinx.coroutines.launch

/**
 * Всё долгое по встречам живёт здесь: запись, импорт чужого файла и
 * расшифровка. Это foreground-сервис по той же причине, что и загрузка
 * моделей: встреча идёт часами со свёрнутым приложением, а расшифровка
 * двух часов — десять минут счёта, и системе нельзя дать убить процесс
 * на полпути.
 *
 * Запись пишется на диск по мере поступления — см. [WavWriter]. Расшифровка
 * идёт в два прохода по файлу: сначала по энергиям кадров ищутся паузы для
 * разрезов, потом куски по очереди читаются с диска и распознаются. Целиком
 * файл в память не поднимается никогда.
 */
class MeetingService : Service() {

    private val scope = CoroutineScope(SupervisorJob() + Dispatchers.Default)
    private val jobs = mutableMapOf<Long, Job>()

    private var recorder: AudioRecord? = null
    private var recordThread: Thread? = null
    private var wakeLock: PowerManager.WakeLock? = null

    override fun onBind(intent: Intent?): IBinder? = null

    override fun onCreate() {
        super.onCreate()
        manager().createNotificationChannel(
            NotificationChannel(
                CHANNEL,
                getString(R.string.channel_meetings),
                NotificationManager.IMPORTANCE_LOW,
            )
        )
    }

    override fun onStartCommand(intent: Intent?, flags: Int, startId: Int): Int {
        when (intent?.action) {
            ACTION_RECORD_START -> startRecording()
            ACTION_RECORD_STOP -> stopRecording()
            // startForeground — сразу, до проверок: сервис подняли через
            // startForegroundService, и не показать уведомление нельзя.
            ACTION_TRANSCRIBE -> {
                foregroundForWork()
                beginTranscribe(intent.getLongExtra(EXTRA_ID, 0))
            }
            ACTION_IMPORT -> {
                foregroundForWork()
                beginImport(intent.getStringExtra(EXTRA_URI).orEmpty())
            }
            ACTION_IMPORT_URL -> {
                foregroundForWork()
                beginImportUrl(intent.getStringExtra(EXTRA_URL).orEmpty())
            }
            ACTION_DIARIZE -> {
                foregroundForWork()
                beginDiarize(
                    intent.getLongExtra(EXTRA_ID, 0),
                    intent.getIntExtra(EXTRA_SPEAKERS, 0),
                )
            }
        }
        stopIfIdle()
        return START_NOT_STICKY
    }

    override fun onDestroy() {
        // Гибель сервиса посреди записи не теряет звук: он уже на диске,
        // а заголовок WAV чинится по длине файла при чтении.
        finishRecording()
        jobs.values.forEach { it.cancel() }
        jobs.clear()
        progress.clear()
        phase.clear()
        scope.cancel()
        releaseWakeLock()
        notifyChange()
        super.onDestroy()
    }

    // --- запись -----------------------------------------------------------

    @SuppressLint("MissingPermission")
    private fun startRecording() {
        if (recordingId != null) return

        // Сервис подняли через startForegroundService, поэтому уведомление
        // обязано появиться даже если микрофон не открылся — иначе система
        // убьёт процесс за неявку.
        ServiceCompat.startForeground(
            this,
            NOTIF_RECORDING,
            recordingNotification(System.currentTimeMillis()),
            ServiceInfo.FOREGROUND_SERVICE_TYPE_MICROPHONE,
        )

        val minBuffer = AudioRecord.getMinBufferSize(
            AudioRecorder.SAMPLE_RATE, AudioFormat.CHANNEL_IN_MONO,
            AudioFormat.ENCODING_PCM_16BIT,
        )
        if (minBuffer <= 0) return

        // Всегда «комната»: источник для распознавания давит дальние голоса,
        // а на встрече они и есть главное.
        val r = AudioRecord(
            MediaRecorder.AudioSource.MIC,
            AudioRecorder.SAMPLE_RATE,
            AudioFormat.CHANNEL_IN_MONO,
            AudioFormat.ENCODING_PCM_16BIT,
            minBuffer * 4,
        )
        // Микрофон, закреплённый в настройках: на встрече чаще всего это
        // как раз внешний, а система по своей воле берёт встроенный.
        MicDevices.preferred(this)?.let { runCatching { r.setPreferredDevice(it) } }

        if (r.state != AudioRecord.STATE_INITIALIZED) {
            r.release()
            return
        }

        val meeting = MeetingStore.create(this, imported = false)
        val wav = WavWriter(MeetingStore.audioFile(this, meeting.id))

        recordingId = meeting.id
        recorder = r
        holdWakeLock()
        manager().notify(NOTIF_RECORDING, recordingNotification(meeting.at))
        r.startRecording()

        recordThread = Thread {
            val buf = ByteArray(minBuffer)
            while (recordingId != null) {
                val n = r.read(buf, 0, buf.size)
                if (n > 0) {
                    wav.write(buf, n)
                    recordingSeconds = (wav.samplesWritten / AudioRecorder.SAMPLE_RATE).toInt()
                    recordingLevel = levelOf(buf, n)
                }
            }
            recordingLevel = 0f
            runCatching { wav.finish() }
            val seconds = wav.samplesWritten.toFloat() / AudioRecorder.SAMPLE_RATE
            MeetingStore.save(this, meeting.copy(seconds = seconds))
            notifyChange()
            // Расшифровка стартует сама: пользователь просил результат,
            // а не промежуточное состояние «записано, нажмите ещё раз».
            beginTranscribe(meeting.id)
            stopIfIdle()
        }.also { it.start() }

        notifyChange()
    }

    private fun stopRecording() {
        finishRecording()
        notifyChange()
    }

    /** Громкость кадра, поджатая корнем, как в [AudioRecorder.levelOf]. */
    private fun levelOf(bytes: ByteArray, count: Int): Float {
        val shorts = java.nio.ByteBuffer.wrap(bytes, 0, count)
            .order(java.nio.ByteOrder.LITTLE_ENDIAN).asShortBuffer()
        var sum = 0.0
        val n = shorts.remaining()
        if (n == 0) return 0f
        for (i in 0 until n) {
            val v = shorts.get(i) / 32768.0
            sum += v * v
        }
        val rms = kotlin.math.sqrt(sum / n)
        return (kotlin.math.sqrt(rms) * 2.2).toFloat().coerceIn(0f, 1f)
    }

    /** Останавливает железо; хвост записи дописывает поток записи. */
    private fun finishRecording() {
        if (recordingId == null) return
        recordingId = null
        recordingSeconds = 0
        recorder?.let {
            runCatching { it.stop() }
            it.release()
        }
        recorder = null
        recordThread?.join(2000)
        recordThread = null
        manager().cancel(NOTIF_RECORDING)
    }

    // --- расшифровка ------------------------------------------------------

    private fun beginTranscribe(id: Long) {
        if (id == 0L || jobs.containsKey(id)) return
        val meeting = MeetingStore.load(this, id) ?: return

        progress[id] = 0
        phase[id] = R.string.meeting_state_transcribing
        notifyChange()
        foregroundForWork()

        jobs[id] = scope.launch {
            val result = runCatching { transcribe(meeting) }
            jobs.remove(id)
            progress.remove(id)
            phase.remove(id)
            manager().cancel(id.toInt())

            result.exceptionOrNull()?.let { e ->
                Log.e(TAG, "расшифровка не удалась", e)
                MeetingStore.save(
                    this@MeetingService,
                    meeting.copy(state = Meeting.STATE_FAILED),
                )
            }
            notifyChange()
            stopIfIdle()
        }
    }

    private suspend fun transcribe(meeting: Meeting) {
        if (!Engine.ensureLoaded(this)) error("модель не загрузилась")

        WavReader(MeetingStore.audioFile(this, meeting.id)).use { wav ->
            val sr = AudioRecorder.SAMPLE_RATE
            val frame = Segmenter.frameSamples(sr)
            val total = wav.totalSamples
            if (total < sr / 2) error("запись пустая")

            MeetingStore.save(this, meeting.copy(state = Meeting.STATE_TRANSCRIBING))
            notifyChange()

            // Первый проход: энергии кадров всего файла. Для двух часов это
            // 360 тысяч float — копейки по сравнению с самим звуком.
            val frames = (total / frame).toInt()
            val loud = FloatArray(frames)
            val block = frame * 500
            var f = 0
            var offset = 0L
            while (f < frames) {
                val pcm = wav.read(offset, block)
                var o = 0
                while (o + frame <= pcm.size && f < frames) {
                    loud[f++] = Segmenter.frameEnergy(pcm, o, frame)
                    o += frame
                }
                offset += o
            }

            val cuts = if (total <= (24f * sr).toLong()) emptyList() else Segmenter.cutFrames(loud)
            val bounds = (listOf(0) + cuts + listOf(frames)).map { it.toLong() * frame }
            val ranges = bounds.zipWithNext().filter { (a, b) -> b - a > frame * 5 }

            // Второй проход: куски читаются с диска и распознаются по одному.
            // Частичный результат сохраняется после каждого куска — обрыв
            // на середине не выбрасывает уже готовый текст.
            val segments = mutableListOf<MeetingSegment>()
            for ((index, range) in ranges.withIndex()) {
                val (from, to) = range
                if (jobs[meeting.id]?.isCancelled == true) return
                val pcm = wav.read(from, (to - from).toInt())
                val text = TextCleanup.clean(
                    Engine.transcribeSegment(pcm),
                    AppPrefs.removeFillers(this),
                )
                if (text.isNotBlank()) {
                    segments += MeetingSegment(
                        start = from.toFloat() / sr,
                        end = to.toFloat() / sr,
                        text = text,
                    )
                    MeetingStore.saveTranscript(this, meeting.id, segments)
                }

                val pct = ((index + 1) * 100 / ranges.size).coerceIn(0, 100)
                if (progress[meeting.id] != pct) {
                    progress[meeting.id] = pct
                    notifyProgress(meeting, pct)
                    notifyChange()
                }
            }

            MeetingStore.save(this, meeting.copy(state = Meeting.STATE_DONE))
            // Модель отработала пачку — дальше её судьбу решает настройка
            // «держать модель в памяти».
            Engine.scheduleUnload(this)
            notifyDone(meeting)
        }
    }

    /**
     * Расшифровка по ссылке: сначала качаем файл во временную папку, дальше
     * он идёт тем же путём, что и выбранный руками, — декодер, ресемплинг,
     * расшифровка. Временный файл удаляется сразу после разбора: приложению
     * нужен только звук, а исходник может весить гигабайты.
     */
    private fun beginImportUrl(url: String) {
        if (url.isEmpty()) return
        val meeting = MeetingStore.create(this, imported = true)

        progress[meeting.id] = 0
        phase[meeting.id] = R.string.meeting_state_fetching
        notifyChange()
        foregroundForWork()

        jobs[meeting.id] = scope.launch {
            val temp = File(cacheDir, "link-${meeting.id}")
            val wav = WavWriter(MeetingStore.audioFile(this@MeetingService, meeting.id))
            val result = runCatching {
                LinkImport.download(
                    url, temp,
                    onProgress = { pct ->
                        progress[meeting.id] = pct
                        notifyChange()
                    },
                    isCancelled = { jobs[meeting.id]?.isCancelled == true },
                )
                phase[meeting.id] = R.string.meeting_state_importing
                progress[meeting.id] = 0
                notifyChange()

                ParcelFileDescriptor.open(temp, ParcelFileDescriptor.MODE_READ_ONLY).use { fd ->
                    AudioImport.run(
                        this@MeetingService, Uri.fromFile(temp), wav,
                        onProgress = { pct ->
                            progress[meeting.id] = pct
                            notifyChange()
                        },
                        isCancelled = { jobs[meeting.id]?.isCancelled == true },
                        fd = fd,
                    )
                }
            }
            runCatching { wav.finish() }
            temp.delete()
            jobs.remove(meeting.id)
            progress.remove(meeting.id)
            phase.remove(meeting.id)

            result.fold(
                onSuccess = { seconds ->
                    MeetingStore.save(this@MeetingService, meeting.copy(seconds = seconds))
                    notifyChange()
                    beginTranscribe(meeting.id)
                },
                onFailure = { e ->
                    Log.e(TAG, "ссылка не скачалась", e)
                    MeetingStore.delete(this@MeetingService, meeting.id)
                    notifyImportFailed(e.message)
                    notifyChange()
                    stopIfIdle()
                },
            )
        }
    }

    // --- импорт -----------------------------------------------------------

    private fun beginImport(uriString: String) {
        if (uriString.isEmpty()) return
        val uri = Uri.parse(uriString)
        val meeting = MeetingStore.create(this, imported = true)

        progress[meeting.id] = 0
        phase[meeting.id] = R.string.meeting_state_importing
        notifyChange()
        foregroundForWork()

        val fd = pendingFd
        pendingFd = null

        jobs[meeting.id] = scope.launch {
            val wav = WavWriter(MeetingStore.audioFile(this@MeetingService, meeting.id))
            val result = runCatching {
                AudioImport.run(
                    this@MeetingService, uri, wav,
                    onProgress = { pct ->
                        progress[meeting.id] = pct
                        notifyChange()
                    },
                    isCancelled = { jobs[meeting.id]?.isCancelled == true },
                    fd = fd,
                )
            }
            runCatching { fd?.close() }
            runCatching { wav.finish() }
            jobs.remove(meeting.id)
            progress.remove(meeting.id)
            phase.remove(meeting.id)

            result.fold(
                onSuccess = { seconds ->
                    MeetingStore.save(this@MeetingService, meeting.copy(seconds = seconds))
                    notifyChange()
                    beginTranscribe(meeting.id)
                },
                onFailure = { e ->
                    Log.e(TAG, "импорт не удался", e)
                    // Встреча без звука в списке бессмысленна — убираем след.
                    MeetingStore.delete(this@MeetingService, meeting.id)
                    notifyImportFailed()
                    notifyChange()
                },
            )
            stopIfIdle()
        }
    }

    // --- разделение говорящих ---------------------------------------------

    /**
     * Диаризация готовой встречи: если эмбеддинг-модель ещё не скачана,
     * сначала тянем её (второй раз уже не понадобится), потом гоним разбор.
     * Встреча всё это время остаётся «готовой» — таймлайн и экспорт живут,
     * прогресс рисуется поверх.
     */
    private fun beginDiarize(id: Long, numSpeakers: Int) {
        if (id == 0L || jobs.containsKey(id)) return
        val meeting = MeetingStore.load(this, id) ?: return

        progress[id] = 0
        phase[id] = if (Diarizer.modelsReady(this)) {
            R.string.meeting_state_diarizing
        } else {
            R.string.meeting_state_downloading
        }
        notifyChange()

        jobs[id] = scope.launch {
            val result = runCatching {
                if (!Diarizer.modelsReady(this@MeetingService)) {
                    Diarizer.download(
                        this@MeetingService,
                        onProgress = { pct ->
                            progress[id] = pct
                            notifyChange()
                        },
                        isCancelled = { jobs[id]?.isCancelled == true },
                    )
                    phase[id] = R.string.meeting_state_diarizing
                    progress[id] = 0
                    notifyChange()
                }
                Diarizer.run(
                    this@MeetingService, id, numSpeakers,
                    onProgress = { pct ->
                        if (progress[id] != pct) {
                            progress[id] = pct
                            notifyProgress(meeting, pct)
                            notifyChange()
                        }
                    },
                    isCancelled = { jobs[id]?.isCancelled == true },
                )
            }
            jobs.remove(id)
            progress.remove(id)
            phase.remove(id)
            manager().cancel(id.toInt())

            result.fold(
                onSuccess = { speakers ->
                    MeetingStore.load(this@MeetingService, id)?.let {
                        MeetingStore.save(this@MeetingService, it.copy(speakers = speakers))
                    }
                    notifyDiarized(meeting, speakers)
                },
                onFailure = { e ->
                    Log.e(TAG, "диаризация не удалась", e)
                    notifyDiarizeFailed(meeting)
                },
            )
            notifyChange()
            stopIfIdle()
        }
    }

    // --- обвязка ----------------------------------------------------------

    private fun stopIfIdle() {
        if (recordingId == null && jobs.isEmpty()) {
            releaseWakeLock()
            stopSelf()
        }
    }

    /**
     * Расшифровка и импорт — это dataSync; тип microphone оставляем только
     * пока идёт живая запись, иначе система сочтёт, что мы слушаем зря.
     */
    private fun foregroundForWork() {
        if (recordingId != null) return
        ServiceCompat.startForeground(
            this,
            NOTIF_WORK,
            workNotification(),
            ServiceInfo.FOREGROUND_SERVICE_TYPE_DATA_SYNC,
        )
        // Пока уведомление записи было лицом foreground-сервиса, cancel по
        // нему игнорировался — теперь лицо сменилось и его можно убрать.
        manager().cancel(NOTIF_RECORDING)
        holdWakeLock()
    }

    /** Счёт и запись не должны замирать с погасшим экраном. */
    private fun holdWakeLock() {
        if (wakeLock?.isHeld == true) return
        wakeLock = (getSystemService(Context.POWER_SERVICE) as PowerManager)
            .newWakeLock(PowerManager.PARTIAL_WAKE_LOCK, "solflow:meetings")
            .also { it.acquire(4 * 60 * 60 * 1000L) }
    }

    private fun releaseWakeLock() {
        wakeLock?.let { if (it.isHeld) it.release() }
        wakeLock = null
    }

    private fun manager() = getSystemService(NotificationManager::class.java)

    private fun openApp() = PendingIntent.getActivity(
        this, 0, Intent(this, MainActivity::class.java), PendingIntent.FLAG_IMMUTABLE,
    )

    private fun recordingNotification(startedAt: Long): Notification {
        val stop = PendingIntent.getService(
            this, 1,
            Intent(this, MeetingService::class.java).setAction(ACTION_RECORD_STOP),
            PendingIntent.FLAG_IMMUTABLE,
        )
        return Notification.Builder(this, CHANNEL)
            .setContentTitle(getString(R.string.meeting_recording_notif))
            .setSmallIcon(android.R.drawable.ic_btn_speak_now)
            .setWhen(startedAt)
            .setUsesChronometer(true)
            .setOngoing(true)
            .setContentIntent(openApp())
            .addAction(Notification.Action.Builder(null, getString(R.string.stop), stop).build())
            .build()
    }

    private fun workNotification(percent: Int = 0): Notification =
        Notification.Builder(this, CHANNEL)
            .setContentTitle(getString(R.string.meeting_transcribing_notif))
            .setSmallIcon(android.R.drawable.stat_sys_download)
            .setProgress(100, percent, percent == 0)
            .setOngoing(true)
            .setContentIntent(openApp())
            .build()

    private fun notifyProgress(meeting: Meeting, percent: Int) {
        if (recordingId == null) {
            manager().notify(NOTIF_WORK, workNotification(percent))
        }
    }

    private fun notifyDone(meeting: Meeting) {
        manager().notify(
            meeting.id.toInt(),
            Notification.Builder(this, CHANNEL)
                .setContentTitle(MeetingStore.displayTitle(this, meeting))
                .setContentText(getString(R.string.meeting_done_notif))
                .setSmallIcon(android.R.drawable.stat_sys_download_done)
                .setContentIntent(openApp())
                .setAutoCancel(true)
                .build(),
        )
    }

    private fun notifyDiarized(meeting: Meeting, speakers: Int) {
        manager().notify(
            meeting.id.toInt(),
            Notification.Builder(this, CHANNEL)
                .setContentTitle(MeetingStore.displayTitle(this, meeting))
                .setContentText(
                    resources.getQuantityString(R.plurals.diarize_done_notif, speakers, speakers)
                )
                .setSmallIcon(android.R.drawable.stat_sys_download_done)
                .setContentIntent(openApp())
                .setAutoCancel(true)
                .build(),
        )
    }

    private fun notifyDiarizeFailed(meeting: Meeting) {
        manager().notify(
            meeting.id.toInt(),
            Notification.Builder(this, CHANNEL)
                .setContentTitle(MeetingStore.displayTitle(this, meeting))
                .setContentText(getString(R.string.diarize_failed))
                .setSmallIcon(android.R.drawable.stat_notify_error)
                .setContentIntent(openApp())
                .setAutoCancel(true)
                .build(),
        )
    }

    private fun notifyImportFailed(reason: String? = null) {
        manager().notify(
            NOTIF_IMPORT_FAILED,
            Notification.Builder(this, CHANNEL)
                .setContentTitle(getString(R.string.meeting_import_failed))
                .apply { if (!reason.isNullOrBlank()) setContentText(reason) }
                .setSmallIcon(android.R.drawable.stat_notify_error)
                .setContentIntent(openApp())
                .setAutoCancel(true)
                .build(),
        )
    }

    private fun notifyChange() = onChange?.invoke()

    companion object {
        private const val TAG = "HandyVoice"
        private const val CHANNEL = "meetings"
        private const val NOTIF_RECORDING = 20
        private const val NOTIF_WORK = 21
        private const val NOTIF_IMPORT_FAILED = 22
        private const val ACTION_RECORD_START = "com.handy.voice.MEETING_RECORD_START"
        private const val ACTION_RECORD_STOP = "com.handy.voice.MEETING_RECORD_STOP"
        private const val ACTION_TRANSCRIBE = "com.handy.voice.MEETING_TRANSCRIBE"
        private const val ACTION_IMPORT = "com.handy.voice.MEETING_IMPORT"
        private const val ACTION_IMPORT_URL = "com.handy.voice.MEETING_IMPORT_URL"
        private const val ACTION_DIARIZE = "com.handy.voice.MEETING_DIARIZE"
        private const val EXTRA_ID = "id"
        private const val EXTRA_URI = "uri"
        private const val EXTRA_URL = "url"
        private const val EXTRA_SPEAKERS = "speakers"

        /** Id встречи, которая пишется прямо сейчас, или null. */
        @Volatile
        var recordingId: Long? = null
            private set

        /** Сколько секунд уже записано — для таймера на экране. */
        @Volatile
        var recordingSeconds: Int = 0
            private set

        /** Текущая громкость [0..1] — для волны на экране встреч. */
        @Volatile
        var recordingLevel: Float = 0f
            private set

        /** Проценты текущей работы по id встречи. */
        val progress = mutableMapOf<Long, Int>()

        /** Что за работа идёт: id строкового ресурса с местом под проценты. */
        val phase = mutableMapOf<Long, Int>()

        /** Экран подписывается сюда, чтобы перерисоваться по ходу работы. */
        @Volatile
        var onChange: (() -> Unit)? = null

        fun startRecording(context: Context) {
            context.startForegroundService(
                Intent(context, MeetingService::class.java).setAction(ACTION_RECORD_START)
            )
        }

        fun stopRecording(context: Context) {
            context.startService(
                Intent(context, MeetingService::class.java).setAction(ACTION_RECORD_STOP)
            )
        }

        fun transcribe(context: Context, id: Long) {
            context.startForegroundService(
                Intent(context, MeetingService::class.java)
                    .setAction(ACTION_TRANSCRIBE)
                    .putExtra(EXTRA_ID, id)
            )
        }

        /**
         * Файл, открытый на стороне вызывающего. Разрешение на чужой
         * `content://` (файл из «Поделиться») действует только там, поэтому
         * открываем сразу и передаём сервису уже готовый дескриптор.
         */
        @Volatile
        private var pendingFd: ParcelFileDescriptor? = null

        /** Расшифровка по ссылке: качаем сами, дальше — обычный импорт. */
        fun importUrl(context: Context, url: String) {
            context.startForegroundService(
                Intent(context, MeetingService::class.java)
                    .setAction(ACTION_IMPORT_URL)
                    .putExtra(EXTRA_URL, url)
            )
        }

        fun import(context: Context, uri: Uri) {
            runCatching { pendingFd?.close() }
            pendingFd = runCatching {
                context.contentResolver.openFileDescriptor(uri, "r")
            }.getOrNull()
            context.startForegroundService(
                Intent(context, MeetingService::class.java)
                    .setAction(ACTION_IMPORT)
                    .putExtra(EXTRA_URI, uri.toString())
            )
        }

        fun diarize(context: Context, id: Long, numSpeakers: Int) {
            context.startForegroundService(
                Intent(context, MeetingService::class.java)
                    .setAction(ACTION_DIARIZE)
                    .putExtra(EXTRA_ID, id)
                    .putExtra(EXTRA_SPEAKERS, numSpeakers)
            )
        }
    }
}
