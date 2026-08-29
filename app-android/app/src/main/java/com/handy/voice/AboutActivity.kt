package com.handy.voice

import android.content.ActivityNotFoundException
import android.content.ClipData
import android.content.ClipboardManager
import android.content.Context
import android.content.Intent
import android.net.Uri
import android.os.Build
import android.os.Bundle
import android.os.StatFs
import android.widget.Toast
import androidx.appcompat.app.AppCompatActivity
import androidx.core.view.ViewCompat
import androidx.core.view.WindowInsetsCompat
import androidx.lifecycle.lifecycleScope
import com.handy.voice.databinding.ActivityAboutBinding
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.launch
import kotlinx.coroutines.withContext
import org.json.JSONObject
import java.net.HttpURLConnection
import java.net.URL

/**
 * О проекте: кто это делает, из чего собрано, где искать новую версию и
 * как пожаловаться, если сломалось.
 *
 * Отчёт о проблеме никуда сам не уходит: письмо открывается в почтовой
 * программе, отправлять или нет — решает человек.
 */
class AboutActivity : AppCompatActivity() {

    private lateinit var ui: ActivityAboutBinding

    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        ui = ActivityAboutBinding.inflate(layoutInflater)
        setContentView(ui.root)

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
        ui.version.text = getString(R.string.about_version, BuildConfig.VERSION_NAME)

        ui.linkSite.setOnClickListener { open("https://ivansolomin.ru") }
        ui.linkTelegram.setOnClickListener { open("https://t.me/russian_cmo") }
        ui.linkYoutube.setOnClickListener { open("https://www.youtube.com/@IvanSPro") }
        ui.linkMail.setOnClickListener { open("mailto:$MAIL") }
        ui.linkEngine.setOnClickListener {
            open("https://github.com/handy-computer/transcribe.cpp")
        }

        ui.checkUpdate.setOnClickListener { checkUpdate() }
        ui.bugSend.setOnClickListener { sendReport() }
        ui.bugCopy.setOnClickListener {
            copy(report(ui.bugText.text?.toString().orEmpty()))
            toast(getString(R.string.report_copied))
        }
        ui.showIntro.setOnClickListener {
            startActivity(Intent(this, IntroActivity::class.java))
        }
    }

    // --- обновления -------------------------------------------------------

    /**
     * Спрашивает GitHub о последнем выпуске. Репозиторий пока не заведён —
     * тогда проверка честно говорит, что не дозвонилась, и на этом всё.
     */
    private fun checkUpdate() {
        ui.checkUpdate.isEnabled = false
        ui.updateStatus.setText(R.string.update_checking)
        lifecycleScope.launch {
            val latest = withContext(Dispatchers.IO) { latestTag() }
            ui.checkUpdate.isEnabled = true
            when {
                latest == null -> ui.updateStatus.setText(R.string.update_failed)
                newer(latest, BuildConfig.VERSION_NAME) -> {
                    ui.updateStatus.text = getString(R.string.update_available, latest)
                    open(RELEASES_PAGE)
                }
                else -> ui.updateStatus.setText(R.string.update_current)
            }
        }
    }

    private fun latestTag(): String? = runCatching {
        val conn = (URL(RELEASES_API).openConnection() as HttpURLConnection).apply {
            instanceFollowRedirects = true
            connectTimeout = 8_000
            readTimeout = 10_000
            setRequestProperty("User-Agent", "SolFlow")
        }
        try {
            if (conn.responseCode !in 200..299) return null
            JSONObject(conn.inputStream.bufferedReader().readText()).optString("tag_name")
                .takeIf { it.isNotBlank() }
        } finally {
            conn.disconnect()
        }
    }.getOrNull()

    /** Сравнение версий вида «2.2.1» по числам, а не по строкам. */
    private fun newer(latest: String, current: String): Boolean {
        fun parts(v: String) = v.trimStart('v', 'V')
            .split('.')
            .map { it.takeWhile(Char::isDigit).toIntOrNull() ?: 0 }
        val a = parts(latest)
        val b = parts(current)
        for (i in 0 until maxOf(a.size, b.size)) {
            val l = a.getOrElse(i) { 0 }
            val c = b.getOrElse(i) { 0 }
            if (l != c) return l > c
        }
        return false
    }

    // --- отчёт о проблеме -------------------------------------------------

    /**
     * Что происходит с телефоном: без версии, модели и свободного места
     * чинить чужую беду вслепую невозможно.
     */
    private fun report(description: String): String {
        val free = runCatching {
            val stat = StatFs(filesDir.absolutePath)
            stat.availableBytes / (1024 * 1024 * 1024.0)
        }.getOrDefault(0.0)
        val model = ModelStore.activeFile(this)?.name ?: getString(R.string.no_model)
        return buildString {
            appendLine(getString(R.string.report_header))
            appendLine()
            appendLine(description.ifBlank { getString(R.string.report_no_description) })
            appendLine()
            appendLine("— Sol Flow ${BuildConfig.VERSION_NAME} (${BuildConfig.VERSION_CODE})")
            appendLine("— ${Build.MANUFACTURER} ${Build.MODEL}, Android ${Build.VERSION.RELEASE}")
            appendLine("— модель: $model")
            appendLine("— свободно: ${"%.1f".format(free)} ГБ")
            appendLine("— спецвозможности: ${yesNo(HandyAccessibilityService.isGranted(this@AboutActivity))}")
            appendLine("— плавающая кнопка: ${yesNo(AppPrefs.bubbleEnabled(this@AboutActivity))}")
        }
    }

    private fun yesNo(value: Boolean) = getString(if (value) R.string.yes else R.string.no)

    private fun sendReport() {
        val body = report(ui.bugText.text?.toString().orEmpty())
        val intent = Intent(Intent.ACTION_SENDTO).apply {
            data = Uri.parse("mailto:$MAIL")
            putExtra(Intent.EXTRA_SUBJECT, getString(R.string.report_subject))
            putExtra(Intent.EXTRA_TEXT, body)
        }
        try {
            startActivity(intent)
        } catch (_: ActivityNotFoundException) {
            // Почтовой программы может не быть вовсе — тогда отчёт хотя бы
            // окажется в буфере, и его можно отправить чем угодно.
            copy(body)
            toast(getString(R.string.report_copied))
        }
    }

    // --- служебное --------------------------------------------------------

    private fun open(url: String) {
        runCatching {
            startActivity(Intent(Intent.ACTION_VIEW, Uri.parse(url)))
        }.onFailure { toast(getString(R.string.link_failed)) }
    }

    private fun copy(text: String) {
        val cm = getSystemService(Context.CLIPBOARD_SERVICE) as ClipboardManager
        cm.setPrimaryClip(ClipData.newPlainText("report", text))
    }

    private fun toast(text: String) = Toast.makeText(this, text, Toast.LENGTH_SHORT).show()

    private companion object {
        const val MAIL = "me@isoloma.ru"

        /** Где искать новые версии — тот же релиз, что у Mac и Windows. */
        const val RELEASES_API = "https://api.github.com/repos/isoloma-ux/solflow/releases/latest"
        const val RELEASES_PAGE = "https://github.com/isoloma-ux/solflow/releases/latest"
    }
}
