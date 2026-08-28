package com.handy.voice

import android.content.Context
import android.view.LayoutInflater
import android.view.View
import android.view.ViewGroup
import android.widget.ImageView
import android.widget.TextView
import androidx.recyclerview.widget.RecyclerView

/**
 * История расшифровок, сгруппированная по дням: заголовок дня — обычная
 * строка списка, а не липкая шапка, чтобы не усложнять разметку.
 *
 * У записи со звуком появляется подвал с тремя действиями: переслушать,
 * расшифровать заново другой моделью и удалить. Без звука остаётся только
 * удаление — переслушивать нечего.
 */
class HistoryAdapter(
    private val onTap: (Transcript) -> Unit,
    private val onPlay: (Transcript) -> Unit,
    private val onRetry: (Transcript) -> Unit,
    private val onDelete: (Transcript) -> Unit,
) : RecyclerView.Adapter<RecyclerView.ViewHolder>() {

    private sealed interface Row {
        data class Day(val label: String) : Row
        data class Item(val transcript: Transcript) : Row
    }

    private var rows: List<Row> = emptyList()

    /** Момент диктовки, которая сейчас играет; null — тишина. */
    var playingAt: Long? = null
        set(value) {
            field = value
            notifyDataSetChanged()
        }

    fun submit(context: Context, items: List<Transcript>) {
        val out = mutableListOf<Row>()
        var lastDay: String? = null
        for (t in items) {
            val day = TranscriptStore.dayLabel(context, t.at)
            if (day != lastDay) {
                out += Row.Day(day)
                lastDay = day
            }
            out += Row.Item(t)
        }
        rows = out
        notifyDataSetChanged()
    }

    override fun getItemCount() = rows.size

    override fun getItemViewType(position: Int) =
        if (rows[position] is Row.Day) TYPE_DAY else TYPE_ITEM

    override fun onCreateViewHolder(parent: ViewGroup, viewType: Int): RecyclerView.ViewHolder {
        val inflater = LayoutInflater.from(parent.context)
        return if (viewType == TYPE_DAY) {
            DayHolder(inflater.inflate(R.layout.item_history_day, parent, false))
        } else {
            ItemHolder(inflater.inflate(R.layout.item_history, parent, false))
        }
    }

    override fun onBindViewHolder(holder: RecyclerView.ViewHolder, position: Int) {
        when (val row = rows[position]) {
            is Row.Day -> (holder as DayHolder).label.text = row.label
            is Row.Item -> with(holder as ItemHolder) {
                val item = row.transcript
                text.text = item.text
                time.text = TranscriptStore.timeLabel(item.at)
                itemView.setOnClickListener { onTap(item) }

                val playing = playingAt == item.at
                play.visibility = if (item.audio) View.VISIBLE else View.GONE
                retry.visibility = if (item.audio) View.VISIBLE else View.GONE
                play.setImageResource(if (playing) R.drawable.ic_pause else R.drawable.ic_play)
                duration.text =
                    if (item.audio) itemView.context.getString(R.string.history_seconds, item.seconds)
                    else ""

                play.setOnClickListener { onPlay(item) }
                retry.setOnClickListener { onRetry(item) }
                remove.setOnClickListener { onDelete(item) }
            }
        }
    }

    class DayHolder(v: View) : RecyclerView.ViewHolder(v) {
        val label: TextView = v.findViewById(R.id.day)
    }

    class ItemHolder(v: View) : RecyclerView.ViewHolder(v) {
        val text: TextView = v.findViewById(R.id.text)
        val time: TextView = v.findViewById(R.id.time)
        val duration: TextView = v.findViewById(R.id.duration)
        val play: ImageView = v.findViewById(R.id.play)
        val retry: ImageView = v.findViewById(R.id.retry)
        val remove: ImageView = v.findViewById(R.id.remove)
    }

    private companion object {
        const val TYPE_DAY = 0
        const val TYPE_ITEM = 1
    }
}
