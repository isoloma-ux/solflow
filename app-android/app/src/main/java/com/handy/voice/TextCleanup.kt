package com.handy.voice

/**
 * Чистка распознанного текста.
 *
 * Логика перенесена из десктопного Handy (`audio_toolkit/text.rs`): удаление
 * слов-паразитов, схлопывание заиканий и нормализация пробелов. Добавлена
 * обработка случаев, которые вылезли на русском.
 *
 * Принцип из оригинала сохранён: убираем только то, что не является словом ни
 * в одном языке. Убрать «ну» или «вот» заманчиво, но это настоящие слова, и
 * их удаление калечит осмысленный текст.
 */
object TextCleanup {

    /**
     * Междометия без лексического значения. Русская часть дополнена: в
     * оригинале был только минимум, потому что десктопный Handy опирался на
     * определение языка, а у нас модель заведомо русская.
     */
    private val FILLERS = setOf(
        // из оригинального Handy
        "uh", "uhm", "umm", "uhh", "uhhh", "ehh", "ehm", "ahm", "hmm", "hm", "mmm",
        // русские
        "хм", "хмм", "ммм", "мм", "эм", "эмм", "ээ", "эээ", "эх", "мхм",
    )

    /**
     * Дефис, прилипший к началу слова. Именно так выглядел артефакт в живой
     * диктовке: «работает достойно. -а моделька себя показывает».
     *
     * Суффиксы «-то», «-либо», «-нибудь» под правило не попадают: они пишутся
     * слитно с предыдущим словом, без пробела перед дефисом. Тире в прямой
     * речи тоже цело — после него стоит пробел.
     */
    private val DASHES = charArrayOf('-', '\u2013', '\u2014')

    /**
     * Слова-паразиты — не мычание, а настоящие слова. Убираются только по
     * просьбе из настроек: в чужой речи это уже редактура, и иногда меняет
     * смысл («это как бы работает» — сомнение, а не мусор).
     *
     * Список общий с десктопной версией (`cleanup.rs`).
     */
    private val PARASITES = listOf(
        "типа", "как бы", "короче", "ну вот", "вот это вот", "в общем-то",
        "то есть как бы", "собственно говоря", "так сказать", "в принципе",
        "you know", "i mean", "kind of", "sort of", "basically", "literally",
        "actually",
    ).sortedByDescending { it.length }

    private val MULTI_SPACE = Regex("\\s{2,}")
    private val SPACE_BEFORE_PUNCTUATION = Regex("\\s+([,.!?;:])")
    private val DOUBLED_PUNCTUATION = Regex("([,.!?;:])\\1+")

    /**
     * [dropParasites] включает вычистку слов-паразитов поверх обычной чистки.
     */
    fun clean(text: String, dropParasites: Boolean = false): String {
        if (text.isBlank()) return ""

        var result = dropGluedDashes(text)
        result = dropFillers(result)
        result = collapseStutters(result)
        if (dropParasites) result = dropParasiteWords(result)
        result = MULTI_SPACE.replace(result, " ")
        result = SPACE_BEFORE_PUNCTUATION.replace(result, "$1")
        result = DOUBLED_PUNCTUATION.replace(result, "$1")
        return result.trim()
    }

    /**
     * Снимает дефис, прилипший к началу слова. Разбором по словам, а не
     * регулярным выражением с просмотром назад: так поведение очевидно и не
     * зависит от тонкостей движка регулярок.
     */
    private fun dropGluedDashes(text: String): String =
        text.split(" ").joinToString(" ") { word ->
            val stripped = word.trimStart(*DASHES)
            // Снимаем, только если под дефисом оказалась буква: одиночное тире
            // между словами — законный знак препинания, его не трогаем.
            if (stripped.isNotEmpty() && stripped.first().isLetter()) stripped else word
        }

    /** Выкидывает междометия вместе с прилипшей к ним запятой. */
    private fun dropFillers(text: String): String =
        text.split(" ")
            .filter { word ->
                val bare = word.trim(',', '.', '!', '?', ';', ':').lowercase()
                bare.isEmpty() || bare !in FILLERS
            }
            .joinToString(" ")

    /**
     * Убирает слова-паразиты вместе с прилипшей к ним запятой. Ищем по
     * границам слов, а не подстрокой: иначе «типаж» превратился бы в «ж».
     * Длинные обороты идут первыми, чтобы «то есть как бы» не распадалось
     * на куски раньше времени.
     */
    private fun dropParasiteWords(text: String): String {
        var result = text
        for (phrase in PARASITES) {
            val pattern = Regex(
                "(?<![\\p{L}\\p{N}])" + Regex.escape(phrase) + "(?![\\p{L}\\p{N}])[,]?",
                RegexOption.IGNORE_CASE,
            )
            result = pattern.replace(result, " ")
        }
        return result
    }

    /**
     * Схлопывает три и более подряд идущих одинаковых слова в одно.
     *
     * Порог именно три, как в оригинале: два повтора часто осмысленны
     * («очень очень длинный»), а три подряд — почти всегда заикание.
     */
    private fun collapseStutters(text: String): String {
        val words = text.split(" ").filter { it.isNotEmpty() }
        if (words.isEmpty()) return text

        val out = mutableListOf<String>()
        var i = 0
        while (i < words.size) {
            val word = words[i]
            if (word.all { it.isLetter() }) {
                var count = 1
                while (i + count < words.size && words[i + count].equals(word, ignoreCase = true)) {
                    count++
                }
                out += word
                i += if (count >= 3) count else 1
            } else {
                out += word
                i++
            }
        }
        return out.joinToString(" ")
    }
}
