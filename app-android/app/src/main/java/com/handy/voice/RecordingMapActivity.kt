package com.handy.voice

import android.content.Intent
import android.graphics.Color
import android.os.Bundle
import android.webkit.JavascriptInterface
import android.webkit.WebResourceRequest
import android.webkit.WebResourceResponse
import android.webkit.WebView
import android.webkit.WebViewClient
import android.widget.Toast
import android.widget.FrameLayout
import androidx.activity.result.contract.ActivityResultContracts
import androidx.appcompat.app.AppCompatActivity
import androidx.core.graphics.Insets
import androidx.core.view.ViewCompat
import androidx.core.view.WindowCompat
import androidx.core.view.WindowInsetsCompat
import org.json.JSONArray
import org.json.JSONObject
import java.nio.ByteBuffer
import java.security.MessageDigest

/** Local-only renderer shared with desktop. No model, network or arbitrary file access. */
class RecordingMapActivity : AppCompatActivity() {
    private lateinit var web: WebView
    private var id = 0L
    private var pending: ByteArray? = null
    private val exportFile = registerForActivityResult(ActivityResultContracts.StartActivityForResult()) { result ->
        val bytes = pending; pending = null
        if (result.resultCode == RESULT_OK && bytes != null) result.data?.data?.let { uri ->
            runCatching { contentResolver.openOutputStream(uri)?.use { it.write(bytes) } ?: error("Cannot open file") }
                .onSuccess { Toast.makeText(this, R.string.map_saved, Toast.LENGTH_LONG).show() }
                .onFailure { Toast.makeText(this, R.string.map_failed, Toast.LENGTH_LONG).show() }
        }
    }
    override fun onCreate(state: Bundle?) {
        super.onCreate(state)
        id = intent.getLongExtra("meeting", 0)
        val map = MeetingStore.load(this, id)?.mindmap
        if (map == null) { finish(); return }
        web = WebView(this)
        // Keep the entire WebView inside the safe area, including its toolbar.
        // Padding the WebView itself does not reliably move HTML controls.
        WindowCompat.setDecorFitsSystemWindows(window, false)
        val canvas = Color.rgb(244, 243, 241)
        val container = FrameLayout(this).apply {
            setBackgroundColor(canvas)
            addView(web, FrameLayout.LayoutParams(
                FrameLayout.LayoutParams.MATCH_PARENT, FrameLayout.LayoutParams.MATCH_PARENT,
            ))
        }
        web.setBackgroundColor(canvas)
        setContentView(container)
        WindowCompat.getInsetsController(window, container).apply {
            isAppearanceLightStatusBars = true
            isAppearanceLightNavigationBars = true
        }
        ScreenInsets.pad(container, Insets.NONE, edge = 16, column = false, top = 0)
        ViewCompat.setOnApplyWindowInsetsListener(container) { view, insets ->
            val handled = WindowInsetsCompat.Type.systemBars() or
                WindowInsetsCompat.Type.displayCutout() or WindowInsetsCompat.Type.ime()
            ScreenInsets.pad(view, ScreenInsets.safe(insets, keyboard = true), edge = 16, column = false, top = 0)
            // These areas are handled by the native container. Notify WebView
            // with zero insets so it neither pads twice nor retains keyboard space.
            WindowInsetsCompat.Builder(insets).setInsets(handled, Insets.NONE).build()
        }
        ViewCompat.requestApplyInsets(container)
        web.settings.javaScriptEnabled = true
        web.settings.allowFileAccess = false
        web.settings.allowContentAccess = false
        web.settings.blockNetworkLoads = true
        web.settings.domStorageEnabled = false
        web.webViewClient = object : WebViewClient() {
            override fun shouldOverrideUrlLoading(view: WebView, request: WebResourceRequest) = true
            override fun shouldInterceptRequest(view: WebView, request: WebResourceRequest) =
                WebResourceResponse("text/plain", "UTF-8", "".byteInputStream())
            override fun onPageFinished(view: WebView, url: String) {
                val lang = if (resources.configuration.locales[0].language == "ru") "ru" else "en"
                val data = runCatching { canonical(map) }.getOrElse {
                    Toast.makeText(this@RecordingMapActivity, R.string.map_changed, Toast.LENGTH_LONG).show()
                    finish(); return
                }
                val stale = data.optString("source") != source()
                web.evaluateJavascript("showMap(${JSONObject.quote(data.toString())},${JSONObject.quote(lang)},$stale)", null)
            }
        }
        web.addJavascriptInterface(Bridge(), "MapHost")
        val js = assets.open("map-fonts.js").bufferedReader().use { it.readText() } + "\n" + assets.open("mindmap.js").bufferedReader().use { it.readText() }
        val css = assets.open("mindmap.css").bufferedReader().use { it.readText() }
        // No transcript text is interpolated into HTML. The only scripts are bundled app code.
        val html = """<!doctype html><html><head><meta name="viewport" content="width=device-width,initial-scale=1"><meta http-equiv="Content-Security-Policy" content="default-src 'none'; script-src 'unsafe-inline'; style-src 'unsafe-inline'; img-src blob: data:; font-src data:;"><style>body{margin:0;background:#f4f3f1}$css</style></head><body><div id="map"></div><script>$js</script><script>
        function showMap(raw,lang,stale) {
          SolFlowMap.mount(document.getElementById('map'),JSON.parse(raw),{lang,stale,
            save:async(next,expected)=>{const r=JSON.parse(MapHost.save(JSON.stringify(next),JSON.stringify(expected)));if(r.error)throw Error(r.error);return r.map;},
            export:async(bytes,format,title)=>{let s='';for(let i=0;i<bytes.length;i+=8192)s+=String.fromCharCode(...bytes.subarray(i,i+8192));MapHost.exportImage(btoa(s),format,title);return lang==='ru'?'Выберите файл в системном окне сохранения.':'Choose a file in the system save dialog.';}
          });
        }</script></body></html>"""
        web.loadDataWithBaseURL("https://solflow.invalid/", html, "text/html", "UTF-8", null)
    }
    private fun source(): String {
        val digest = MessageDigest.getInstance("SHA-256")
        MeetingStore.loadTranscript(this, id).forEach {
            val bytes = it.text.toByteArray(Charsets.UTF_8)
            digest.update(ByteBuffer.allocate(8).putLong(bytes.size.toLong()).array()); digest.update(bytes)
        }
        return digest.digest().joinToString("") { "%02x".format(it) }
    }
    private fun canonical(raw: String): JSONObject {
        require(raw.length <= 250000)
        val data = JSONObject(raw)
        fun text(value: String, max: Int): String {
            require(value.isNotBlank() && value.codePointCount(0,value.length) <= max && value.none { it.isISOControl() && it != '\n' })
            return value
        }
        val branches = data.getJSONArray("branches"); require(branches.length() in 1..12)
        return JSONObject().apply {
            put("title", text(data.getString("title"),200))
            put("branches", JSONArray().apply {
                for (i in 0 until branches.length()) {
                    val b=branches.getJSONObject(i);val points=b.getJSONArray("points");require(points.length() in 1..8)
                    put(JSONObject().apply { put("title",text(b.getString("title"),160));put("points",JSONArray().apply {
                        for(j in 0 until points.length()) put(text(points.getString(j),600))
                    }) })
                }
            })
            put("source",data.optString("source"));put("revised",data.optLong("revised"))
        }
    }
    inner class Bridge {
        @JavascriptInterface fun save(raw: String, expected: String): String = try {
            check(!MeetingService.phase.containsKey(id))
            val next=canonical(raw);val old=canonical(expected)
            val current=MeetingStore.load(this@RecordingMapActivity,id) ?: error("Missing recording")
            check(current.mindmap != null && canonical(current.mindmap).toString()==old.toString())
            check(source()==old.getString("source"))
            TranscriptFiles.Draft(MeetingStore.dir(this@RecordingMapActivity,id))
            next.put("source",old.getString("source"));next.put("revised",maxOf(System.currentTimeMillis(),old.optLong("revised")+1))
            MeetingStore.save(this@RecordingMapActivity,current.copy(mindmap=next.toString()))
            JSONObject().put("map",next).toString()
        } catch (_: Exception) { JSONObject().put("error",getString(R.string.map_changed)).toString() }
        @JavascriptInterface fun exportImage(base64: String, format: String, title: String) {
            if (base64.length > 28_000_000 || format !in listOf("png","svg")) return
            val bytes=runCatching { android.util.Base64.decode(base64,android.util.Base64.DEFAULT) }.getOrNull() ?: return
            runOnUiThread {
                if (pending != null) return@runOnUiThread
                pending=bytes
                val safe=title.replace(Regex("[\\\\/:*?\"<>|\\p{Cntrl}]")," ").take(80)
                exportFile.launch(Intent(Intent.ACTION_CREATE_DOCUMENT).apply {
                    addCategory(Intent.CATEGORY_OPENABLE);type=if(format=="png") "image/png" else "image/svg+xml"
                    putExtra(Intent.EXTRA_TITLE,"$safe — Sol Flow.$format")
                })
            }
        }
    }
    override fun onDestroy() {
        if (::web.isInitialized) { web.removeJavascriptInterface("MapHost"); web.destroy() }
        pending=null;super.onDestroy()
    }
}
