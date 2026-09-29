package com.handy.voice

import android.os.Bundle
import android.webkit.WebResourceRequest
import android.webkit.WebResourceResponse
import android.webkit.WebView
import android.webkit.WebViewClient
import android.widget.LinearLayout
import androidx.activity.addCallback
import androidx.appcompat.app.AppCompatActivity
import com.google.android.material.button.MaterialButton

/** Bundled guide shared with desktop. No bridge, network, or user-data access. */
class IntroActivity : AppCompatActivity() {
    private lateinit var web: WebView
    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        val root = LinearLayout(this).apply { orientation = LinearLayout.VERTICAL }
        web = WebView(this)
        root.addView(web, LinearLayout.LayoutParams(-1, 0, 1f))
        root.addView(MaterialButton(this).apply {
            setText(R.string.guide_close)
            setOnClickListener { done() }
        }, LinearLayout.LayoutParams(-1, -2))
        setContentView(root)
        ScreenInsets.install(this, root) { safe -> ScreenInsets.pad(root, safe, edge = 0, column = false, top = 0) }
        web.settings.apply {
            javaScriptEnabled = true
            allowFileAccess = false
            allowContentAccess = false
            blockNetworkLoads = true
            domStorageEnabled = false
            setSupportMultipleWindows(false)
        }
        web.webViewClient = object : WebViewClient() {
            override fun shouldOverrideUrlLoading(view: WebView, request: WebResourceRequest) = true
            override fun shouldInterceptRequest(view: WebView, request: WebResourceRequest): WebResourceResponse {
                val url = request.url
                val path = url.path.orEmpty().removePrefix("/")
                val allowed = path in setOf("guide.css", "guide.js", "guide-content.js", "locales.js") ||
                    path.matches(Regex("shots/[a-z-]+[.]png"))
                if (url.scheme != "https" || url.host != "guide.solflow.invalid" || !allowed) {
                    return WebResourceResponse("text/plain", "UTF-8", "".byteInputStream())
                }
                val mime = when {
                    path.endsWith(".css") -> "text/css"
                    path.endsWith(".js") -> "application/javascript"
                    else -> "image/png"
                }
                return runCatching { WebResourceResponse(mime, "UTF-8", assets.open(path)) }
                    .getOrElse { WebResourceResponse("text/plain", "UTF-8", "".byteInputStream()) }
            }
        }
        val lang = resources.configuration.locales[0].language.takeIf { it in setOf("ru","en","zh","ko","ja","de","fr","es") } ?: "en"
        web.loadDataWithBaseURL("https://guide.solflow.invalid/guide.html?platform=android&lang=$lang",
            assets.open("guide.html").bufferedReader().use { it.readText() }, "text/html", "UTF-8", null)
        onBackPressedDispatcher.addCallback(this) { done() }
    }
    private fun done() {
        AppPrefs.setIntroShown(this, true)
        finish()
    }
    override fun onDestroy() {
        web.stopLoading()
        web.destroy()
        super.onDestroy()
    }
}
