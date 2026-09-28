package com.handy.voice

import android.os.Bundle
import android.content.res.ColorStateList
import android.graphics.drawable.GradientDrawable
import android.view.Gravity
import android.view.View
import android.widget.*
import androidx.appcompat.app.AppCompatActivity
import androidx.lifecycle.lifecycleScope
import com.google.android.material.button.MaterialButton
import com.google.android.material.dialog.MaterialAlertDialogBuilder
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.launch
import kotlinx.coroutines.withContext
import java.text.DateFormat
import java.util.Date

/** Explicit restore actions on a full screen; permanent clear always confirms a fixed snapshot. */
class TrashActivity : AppCompatActivity() {
    private lateinit var body: LinearLayout
    private lateinit var clear: MaterialButton
    private lateinit var status: TextView
    private var working = false
    private val syncListener: () -> Unit = { runOnUiThread { if (!isDestroyed) render() } }
    private fun dp(n: Int) = (n * resources.displayMetrics.density).toInt()
    private fun text(value: String, size: Float, color: Int = R.color.ink) = TextView(this).apply {
        text = value; textSize = size; setTextColor(getColor(color)); typeface = resources.getFont(R.font.inter_regular)
    }
    private fun button(label: String, icon: Int, danger: Boolean = false) = MaterialButton(this, null, com.google.android.material.R.attr.materialButtonOutlinedStyle).apply {
        text = label; isAllCaps = false; minHeight = dp(48); cornerRadius = dp(24)
        typeface = resources.getFont(R.font.inter_medium)
        setTextColor(getColor(if(danger) R.color.danger else R.color.accent))
        iconTint = ColorStateList.valueOf(currentTextColor); setIconResource(icon); iconSize = dp(20)
        strokeColor = ColorStateList.valueOf(getColor(R.color.hairline)); strokeWidth = dp(1).coerceAtLeast(1)
        backgroundTintList = ColorStateList.valueOf(android.graphics.Color.TRANSPARENT)
    }
    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        val root = LinearLayout(this).apply { orientation = LinearLayout.VERTICAL; setBackgroundColor(getColor(R.color.canvas)) }
        setContentView(root); ScreenInsets.install(this, root) { safe -> ScreenInsets.pad(root, safe) }
        val top = LinearLayout(this).apply { gravity = Gravity.CENTER_VERTICAL }
        val back = button(getString(R.string.back), R.drawable.ic_sf_back).apply { setOnClickListener { finish() } }
        top.addView(back); top.addView(text(getString(R.string.shared_trash), 24f).apply { setPadding(dp(16),0,0,0) })
        root.addView(top)
        root.addView(text(getString(R.string.trash_description), 14f, R.color.fog).apply { setPadding(0,dp(16),0,dp(12)) })
        clear = button(getString(R.string.trash_clear),R.drawable.ic_sf_clear,true).apply { setOnClickListener { confirmClear(this@TrashActivity) { render() } } }
        root.addView(clear, LinearLayout.LayoutParams(-2,-2))
        status = text("",14f,R.color.fog); status.accessibilityLiveRegion = View.ACCESSIBILITY_LIVE_REGION_POLITE; root.addView(status)
        val scroll = ScrollView(this); body = LinearLayout(this).apply { orientation = LinearLayout.VERTICAL }; scroll.addView(body)
        root.addView(scroll,LinearLayout.LayoutParams(-1,0,1f)); render()
    }
    override fun onStart() { super.onStart(); SyncManager.addListener(syncListener); render() }
    override fun onStop() { SyncManager.removeListener(syncListener); super.onStop() }
    private fun render() {
        val rows = SharedTrash.rows(this)
        clear.isEnabled = !working && rows.any { !it.restoring && !it.clearing }
        status.text = SyncManager.message ?: if(SyncManager.running) getString(R.string.drawer_sync_running) else ""
        body.removeAllViews()
        if(rows.isEmpty()) {
            body.addView(text(getString(R.string.trash_empty),24f).apply { gravity=Gravity.CENTER; setPadding(0,dp(64),0,dp(16)) })
            body.addView(text(getString(R.string.trash_empty_hint),15f,R.color.fog).apply { gravity=Gravity.CENTER })
        }
        val projects = MeetingStore.projects(this).associateBy { it.id }
        for(row in rows) {
            val card = LinearLayout(this).apply {
                orientation = LinearLayout.VERTICAL; setPadding(dp(20),dp(20),dp(20),dp(16))
                background = GradientDrawable().apply { cornerRadius=dp(16).toFloat(); setColor(getColor(R.color.graphite)); setStroke(dp(1).coerceAtLeast(1),getColor(R.color.hairline)) }
            }
            card.addView(text(row.title,20f).apply {
                typeface=resources.getFont(R.font.inter_medium); setCompoundDrawablesRelativeWithIntrinsicBounds(R.drawable.ic_sf_file,0,0,0); compoundDrawablePadding=dp(12)
            })
            val project = projects[row.project]?.name ?: getString(R.string.project_none)
            card.addView(text(project+" · "+getString(if(row.audio) R.string.trash_audio_text else R.string.trash_text_only),13f,R.color.fog).apply { setPadding(0,dp(12),0,dp(4)) })
            card.addView(text(getString(R.string.trash_deleted_at,DateFormat.getDateTimeInstance(DateFormat.MEDIUM,DateFormat.SHORT).format(Date(row.deletedAt))),13f,R.color.fog))
            val restore = button(getString(if(row.clearing) R.string.trash_clearing else if(row.restoring) R.string.trash_pending else R.string.trash_restore),R.drawable.ic_sf_restore)
            restore.isEnabled = !working && !row.restoring && !row.clearing
            restore.setOnClickListener {
                working=true; render()
                lifecycleScope.launch {
                    val result=withContext(Dispatchers.IO) { runCatching { SharedTrash.restore(this@TrashActivity,row.id) } }
                    working=false; render()
                    result.onFailure { status.text=it.message ?: it.toString() }
                }
            }
            card.addView(restore,LinearLayout.LayoutParams(-1,-2).apply { topMargin=dp(16) })
            body.addView(card,LinearLayout.LayoutParams(-1,-2).apply { topMargin=dp(16) })
        }
    }
    companion object {
        fun confirmClear(activity: AppCompatActivity, changed: () -> Unit) {
            val snapshot=SharedTrash.rows(activity).filter { !it.restoring && !it.clearing }.map { it.id }
            if(snapshot.isEmpty()) return
            val scope=SharedTrash.root(activity).absolutePath
            val connected=SyncManager.connected(activity)
            val message=activity.getString(R.string.trash_clear_count,snapshot.size)+"\n\n"+activity.getString(if(connected) R.string.trash_clear_cloud else R.string.trash_clear_local)
            val dialog=MaterialAlertDialogBuilder(activity).setTitle(R.string.trash_clear_title).setMessage(message)
                .setNegativeButton(R.string.cancel,null).setPositiveButton(R.string.trash_clear_confirm) { _,_ ->
                    activity.lifecycleScope.launch {
                        val result=withContext(Dispatchers.IO) { runCatching { SharedTrash.clear(activity,snapshot,scope) } }
                        changed()
                        val text=result.fold({activity.getString(if(connected) R.string.trash_clear_queued else R.string.trash_cleared)},{it.message ?: it.toString()})
                        Toast.makeText(activity,text,Toast.LENGTH_LONG).show()
                    }
                }.create()
            dialog.setOnShowListener {
                dialog.getButton(android.content.DialogInterface.BUTTON_NEGATIVE).requestFocus()
                dialog.getButton(android.content.DialogInterface.BUTTON_POSITIVE).setTextColor(activity.getColor(R.color.danger))
            }
            dialog.show()
        }
    }
}
