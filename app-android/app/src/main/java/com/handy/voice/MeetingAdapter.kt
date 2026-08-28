package com.handy.voice

import android.view.LayoutInflater
import android.view.View
import android.view.ViewGroup
import android.widget.ImageView
import android.widget.LinearLayout
import android.widget.ProgressBar
import android.widget.TextView
import androidx.recyclerview.widget.RecyclerView

/**
 * Список встреч: название, длительность и состояние расшифровки.
 *
 * В результатах поиска под строкой состояния появляется найденная реплика,
 * а долгое нажатие включает выбор нескольких встреч — для расшифровки,
 * переноса в проект или удаления пачкой.
 */
class MeetingAdapter(
    private val onTap: (Meeting) -> Unit,
    private val onLongTap: (Meeting) -> Unit = {},
    /** Тап по найденному месту: открыть встречу на этой реплике. */
    private val onQuoteTap: (Meeting, MeetingStore.Quote) -> Unit = { _, _ -> },
) : RecyclerView.Adapter<MeetingAdapter.Holder>() {

    private var items: List<Meeting> = emptyList()
    private var hits: Map<Long, MeetingStore.Hit> = emptyMap()
    private var query = ""

    /** Что отмечено в режиме выбора; пустое множество — режим выключен. */
    var selected: Set<Long> = emptySet()
        set(value) {
            field = value
            notifyDataSetChanged()
        }

    fun submit(
        list: List<Meeting>,
        hits: Map<Long, MeetingStore.Hit> = emptyMap(),
        query: String = "",
    ) {
        items = list
        this.hits = hits
        this.query = query
        notifyDataSetChanged()
    }

    override fun getItemCount() = items.size

    override fun onCreateViewHolder(parent: ViewGroup, viewType: Int) = Holder(
        LayoutInflater.from(parent.context).inflate(R.layout.item_meeting, parent, false)
    )

    override fun onBindViewHolder(holder: Holder, position: Int) {
        val meeting = items[position]
        val context = holder.itemView.context

        holder.glyph.setImageResource(
            if (meeting.imported) R.drawable.ic_file else R.drawable.ic_mic_small
        )
        holder.title.text = MeetingStore.displayTitle(context, meeting)

        val percent = MeetingService.progress[meeting.id]
        val phase = MeetingService.phase[meeting.id]
        val state = when {
            meeting.id == MeetingService.recordingId ->
                context.getString(R.string.meeting_state_recording)
            percent != null && phase != null -> context.getString(phase, percent)
            meeting.state == Meeting.STATE_DONE && meeting.speakers > 0 ->
                context.getString(R.string.meeting_state_done_speakers, meeting.speakers)
            meeting.state == Meeting.STATE_DONE ->
                context.getString(R.string.meeting_state_done)
            meeting.state == Meeting.STATE_FAILED ->
                context.getString(R.string.meeting_state_failed)
            else -> context.getString(R.string.meeting_state_recorded)
        }
        // Длительность живёт в подписи, а не в строке названия: с глифом
        // слева заголовку иначе не хватает ширины и он переносится.
        holder.state.text = context.getString(
            R.string.meeting_meta, MeetingStore.durationLabel(context, meeting.seconds), state,
        )

        holder.bar.visibility =
            if (percent != null && phase != null) View.VISIBLE else View.GONE
        // Полоса ползёт к новому значению, а не прыгает.
        holder.bar.setProgress(percent ?: 0, percent != null)

        // Найденные места: до трёх строк со временем, каждая ведёт к своей
        // реплике. Если совпадений больше, последняя строка говорит сколько.
        val hit = hits[meeting.id]
        holder.quotes.removeAllViews()
        holder.quotes.visibility =
            if (hit == null || hit.quotes.isEmpty()) View.GONE else View.VISIBLE
        if (hit != null) {
            val inflater = LayoutInflater.from(context)
            for (quote in hit.quotes) {
                val row = inflater.inflate(R.layout.item_meeting_quote, holder.quotes, false)
                row.findViewById<TextView>(R.id.quoteTime).text =
                    MeetingStore.clockLabel(quote.start)
                row.findViewById<TextView>(R.id.quoteText).text =
                    Highlight.of(context, quote.text, query)
                row.setOnClickListener { onQuoteTap(meeting, quote) }
                holder.quotes.addView(row)
            }
            if (hit.count > hit.quotes.size) {
                val more = TextView(context).apply {
                    text = context.getString(R.string.search_more, hit.count - hit.quotes.size)
                    setTextColor(context.getColor(R.color.fog))
                    textSize = 13f
                    setPadding(0, (6 * context.resources.displayMetrics.density).toInt(), 0, 0)
                }
                holder.quotes.addView(more)
            }
        }

        holder.card.setBackgroundResource(
            if (meeting.id in selected) R.drawable.bg_card_selected else R.drawable.bg_card
        )
        holder.itemView.setOnClickListener { onTap(meeting) }
        holder.itemView.setOnLongClickListener {
            onLongTap(meeting)
            true
        }
    }

    class Holder(v: View) : RecyclerView.ViewHolder(v) {
        val glyph: ImageView = v.findViewById(R.id.meetingItemGlyph)
        val title: TextView = v.findViewById(R.id.meetingItemTitle)
        val state: TextView = v.findViewById(R.id.meetingItemState)
        val bar: ProgressBar = v.findViewById(R.id.meetingItemProgress)
        val quotes: LinearLayout = v.findViewById(R.id.meetingItemQuotes)
        val card: View = v.findViewById(R.id.meetingItemCard)
    }
}
