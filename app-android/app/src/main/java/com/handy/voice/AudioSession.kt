package com.handy.voice

import android.content.Context
import android.media.AudioAttributes
import android.media.AudioDeviceInfo
import android.media.AudioFocusRequest
import android.media.AudioManager
import android.media.SoundPool
import android.os.Build

/**
 * Обвязка записи, не относящаяся к самому потоку с микрофона: короткий
 * сигнал в начале и приглушение чужого звука на время диктовки.
 *
 * Оба включаются в настройках и нужны обоим путям записи — экрану и
 * плавающей кнопке, — поэтому живут отдельным объектом, а не внутри
 * [AudioRecorder], который про настройки ничего не знает.
 */
object AudioSession {

    private var pool: SoundPool? = null
    private var startSound = 0
    private var loaded = false

    private var focusRequest: AudioFocusRequest? = null
    private var focusListener: AudioManager.OnAudioFocusChangeListener? = null

    /**
     * Сигнал начала записи — тот же pop, что в десктопной версии.
     *
     * SoundPool, а не MediaPlayer: файл крошечный, играть его надо мгновенно
     * и часто, а MediaPlayer каждый раз поднимает декодер.
     */
    fun playStart(context: Context) {
        if (!AppPrefs.startSound(context)) return
        val p = pool ?: SoundPool.Builder()
            .setMaxStreams(1)
            .setAudioAttributes(
                AudioAttributes.Builder()
                    .setUsage(AudioAttributes.USAGE_ASSISTANCE_SONIFICATION)
                    .setContentType(AudioAttributes.CONTENT_TYPE_SONIFICATION)
                    .build()
            )
            .build()
            .also { fresh ->
                pool = fresh
                fresh.setOnLoadCompleteListener { _, _, status -> loaded = status == 0 }
                startSound = fresh.load(context, R.raw.start, 1)
            }
        if (loaded) p.play(startSound, 1f, 1f, 1, 0, 1f)
    }

    /**
     * Просит систему приглушить чужой звук: музыка из динамика телефона
     * иначе попадает в микрофон. Фокус берём «эксклюзивный переходный» —
     * плеер ставится на паузу и сам возобновится, когда мы его отпустим.
     */
    fun mute(context: Context) {
        if (!AppPrefs.muteWhileRecording(context)) return
        val manager = context.getSystemService(AudioManager::class.java) ?: return
        // Слушатель обязателен, но реагировать нам не на что: запись идёт
        // своим потоком и чужой фокус ей не мешает.
        val listener = AudioManager.OnAudioFocusChangeListener { }
        focusListener = listener
        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.O) {
            val request = AudioFocusRequest
                .Builder(AudioManager.AUDIOFOCUS_GAIN_TRANSIENT_EXCLUSIVE)
                .setAudioAttributes(
                    AudioAttributes.Builder()
                        .setUsage(AudioAttributes.USAGE_ASSISTANCE_SONIFICATION)
                        .setContentType(AudioAttributes.CONTENT_TYPE_SPEECH)
                        .build()
                )
                .setOnAudioFocusChangeListener(listener)
                .build()
            focusRequest = request
            runCatching { manager.requestAudioFocus(request) }
        }
    }

    /** Отпускает фокус — чужой плеер продолжит с того же места. */
    fun unmute(context: Context) {
        val manager = context.getSystemService(AudioManager::class.java) ?: return
        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.O) {
            focusRequest?.let { runCatching { manager.abandonAudioFocusRequest(it) } }
        }
        focusRequest = null
        focusListener = null
    }
}

/**
 * Микрофоны, между которыми можно выбирать.
 *
 * Идентификатор устройства система выдаёт заново при каждом подключении,
 * поэтому запоминаем «тип:название» — оно переживает переподключение
 * наушников и перезагрузку.
 */
object MicDevices {

    data class Mic(val id: String, val title: String)

    fun list(context: Context): List<Mic> {
        val manager = context.getSystemService(AudioManager::class.java) ?: return emptyList()
        return manager.getDevices(AudioManager.GET_DEVICES_INPUTS)
            .filter { it.isSource && it.type != AudioDeviceInfo.TYPE_TELEPHONY }
            .map { Mic(idOf(it), titleOf(context, it)) }
            .distinctBy { it.id }
    }

    /** Устройство, закреплённое пользователем; null — как решит система. */
    fun preferred(context: Context): AudioDeviceInfo? {
        val wanted = AppPrefs.inputDevice(context) ?: return null
        val manager = context.getSystemService(AudioManager::class.java) ?: return null
        return manager.getDevices(AudioManager.GET_DEVICES_INPUTS)
            .firstOrNull { it.isSource && idOf(it) == wanted }
    }

    fun title(context: Context, id: String?): String =
        if (id == null) context.getString(R.string.mic_system)
        else list(context).firstOrNull { it.id == id }?.title
            ?: context.getString(R.string.mic_missing)

    private fun idOf(device: AudioDeviceInfo) =
        "${device.type}:${device.productName}"

    private fun titleOf(context: Context, device: AudioDeviceInfo): String {
        val kind = when (device.type) {
            AudioDeviceInfo.TYPE_BUILTIN_MIC -> R.string.mic_builtin
            AudioDeviceInfo.TYPE_BLUETOOTH_SCO, AudioDeviceInfo.TYPE_BLE_HEADSET ->
                R.string.mic_bluetooth
            AudioDeviceInfo.TYPE_WIRED_HEADSET -> R.string.mic_wired
            AudioDeviceInfo.TYPE_USB_DEVICE, AudioDeviceInfo.TYPE_USB_HEADSET -> R.string.mic_usb
            else -> R.string.mic_other
        }
        val name = device.productName?.toString()?.trim().orEmpty()
        val label = context.getString(kind)
        return if (name.isEmpty()) label else "$label · $name"
    }
}
