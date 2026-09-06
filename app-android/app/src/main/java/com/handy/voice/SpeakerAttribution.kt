package com.handy.voice

/** Same rules as desktop speaker_attribution.rs. No inferred word boundaries. */
object SpeakerAttribution {
    data class Turn(val start: Float, val end: Float, val speaker: Int)
    data class Label(val speaker: Int?, val voices: List<Int>)

    fun assign(bounds: List<Pair<Float, Float>>, source: List<Turn>): Pair<List<Label>, Int> {
        val turns = source.filter { it.start.isFinite() && it.end.isFinite() && it.start >= 0 && it.end > it.start }
            .sortedWith(compareBy<Turn> { it.start }.thenBy { it.speaker })
        val order = turns.map { it.speaker }.distinct()
        val labels = bounds.map { (start, end) ->
            val spans = FloatArray(order.size)
            if (start.isFinite() && end.isFinite() && end > start) {
                for (turn in turns) {
                    val overlap = minOf(end, turn.end) - maxOf(start, turn.start)
                    if (overlap > 0) spans[order.indexOf(turn.speaker)] += overlap
                }
            }
            val total = spans.sum()
            val ranked = spans.indices.filter { spans[it] > 0 }
                .sortedWith(compareByDescending<Int> { spans[it] }.thenBy { it })
            val mixed = ranked.drop(1).any { spans[it] >= 0.3f && spans[it] >= total * 0.1f }
            Label(if (mixed) null else ranked.firstOrNull(), if (mixed) ranked else emptyList())
        }
        return labels to order.size
    }

    fun bigSpeakerThreshold(totalSpeech: Float): Float = minOf(30f, totalSpeech * 0.05f)
}
