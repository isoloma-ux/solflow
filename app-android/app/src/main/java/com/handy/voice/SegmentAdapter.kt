package com.handy.voice

import android.view.LayoutInflater
import android.view.View
import android.view.ViewGroup
import android.widget.TextView
import androidx.recyclerview.widget.RecyclerView

/**
 * Таймлайн расшифровки. Реплик у двухчасовой встречи сотни, поэтому это
 * RecyclerView, а не текст в ScrollView.
 *
 * После диаризации у реплик появляются заголовки говорящих — каждый в своём
 * цвете, тап по заголовку переименовывает человека.
 */
class SegmentAdapter(
    private val onSpeakerTap: (Int) -> Unit,
) : RecyclerView.Adapter<SegmentAdapter.Holder>() {

    private var items: List<MeetingSegment> = emptyList()
    private var labels: Map<Int, String> = emptyMap()

    /** Что ищут в открытой расшифровке; пусто — подсвечивать нечего. */
    private var query = ""

    /** Номер реплики, к которой сейчас перешли по поиску. */
    private var current: Int = -1

    /**
     * Поиск по открытой расшифровке: подсвечивает слово во всех репликах,
     * [current] выделяет ту, к которой человек перешёл последней.
     */
    fun setQuery(next: String, currentIndex: Int = -1) {
        if (query == next && current == currentIndex) return
        query = next
        current = currentIndex
        notifyDataSetChanged()
    }

    /** Подписи говорящих: номер — имя. Обновляется при переименовании. */
    fun setLabels(next: Map<Int, String>) {
        if (labels == next) return
        labels = next
        notifyDataSetChanged()
    }

    fun submit(list: List<MeetingSegment>) {
        // Во время расшифровки список только растёт — дорисовываем хвост,
        // чтобы экран не мигал и не сбрасывал позицию прокрутки.
        val old = items
        items = list
        if (list.size > old.size && list.take(old.size) == old) {
            notifyItemRangeInserted(old.size, list.size - old.size)
        } else if (list != old) {
            notifyDataSetChanged()
        }
    }

    override fun getItemCount() = items.size

    override fun onCreateViewHolder(parent: ViewGroup, viewType: Int) = Holder(
        LayoutInflater.from(parent.context).inflate(R.layout.item_segment, parent, false)
    )

    override fun onBindViewHolder(holder: Holder, position: Int) {
        val s = items[position]
        val context = holder.itemView.context
        holder.time.text = MeetingStore.clockLabel(s.start)
        holder.text.text = Highlight.of(context, s.text, query)
        // Текущее совпадение подсвечено плашкой: без неё после перехода
        // непонятно, на какой из подсвеченных реплик ты стоишь.
        val isCurrent = position == current && query.isNotBlank()
        holder.text.setBackgroundResource(if (isCurrent) R.drawable.bg_card_inner else 0)
        val pad = if (isCurrent) (8 * context.resources.displayMetrics.density).toInt() else 0
        holder.text.setPadding(pad, pad, pad, pad)

        // Заголовок говорящего — только на смене голоса, как в сценарии
        // пьесы: подписывать каждую реплику было бы шумно. Полоска слева
        // держит цвет говорящего на всех его репликах подряд.
        val speaker = s.speaker
        val previous = items.getOrNull(position - 1)?.speaker
        val shown = speaker != null && speaker != previous
        holder.speaker.visibility = if (shown) View.VISIBLE else View.GONE
        if (speaker != null) {
            val color = context.getColor(SPEAKER_COLORS[speaker % SPEAKER_COLORS.size])
            holder.stripe.visibility = View.VISIBLE
            holder.stripe.setBackgroundColor(color)
            if (shown) {
                holder.speaker.text = labels[speaker]
                    ?: context.getString(R.string.speaker_label, speaker + 1)
                holder.speaker.setTextColor(color)
                holder.speaker.setOnClickListener { onSpeakerTap(speaker) }
            }
        } else {
            holder.stripe.visibility = View.GONE
        }
    }

    class Holder(v: View) : RecyclerView.ViewHolder(v) {
        val speaker: TextView = v.findViewById(R.id.segmentSpeaker)
        val stripe: View = v.findViewById(R.id.segmentStripe)
        val time: TextView = v.findViewById(R.id.segmentTime)
        val text: TextView = v.findViewById(R.id.segmentText)
    }

    companion object {
        /** Дальше шестого человека цвета идут по кругу. */
        val SPEAKER_COLORS = intArrayOf(
            R.color.speaker_1, R.color.speaker_2, R.color.speaker_3,
            R.color.speaker_4, R.color.speaker_5, R.color.speaker_6,
        )
    }
}
