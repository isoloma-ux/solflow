package com.handy.voice

/** Same logical revision protocol as desktop/mcp-core/src/access.rs. */
data class AiGrant(val transcript: Boolean = false, val summary: Boolean = false, val map: Boolean = false, val analyses: Boolean = false) {
    val enabled get() = transcript || summary || map || analyses
    fun intersect(other: AiGrant) = AiGrant(transcript && other.transcript, summary && other.summary, map && other.map, analyses && other.analyses)
}
data class AiAccessEntry(val revision: Long, val grant: AiGrant)
object AiAccessRules {
    const val MAX_REV = 9007199254740991L
    fun merge(local: Map<String, AiAccessEntry>, remote: Map<String, AiAccessEntry>): Map<String, AiAccessEntry> {
        val result = local.toMutableMap()
        for ((id, r) in remote) {
            val l = result[id]
            result[id] = when {
                l == null || r.revision > l.revision -> r
                r.revision == l.revision -> AiAccessEntry(l.revision, l.grant.intersect(r.grant))
                else -> l
            }
        }
        return result.toSortedMap()
    }
}
