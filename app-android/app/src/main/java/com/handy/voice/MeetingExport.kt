package com.handy.voice

import android.content.ContentValues
import android.content.Context
import android.graphics.Paint
import android.graphics.Typeface
import android.graphics.pdf.PdfDocument
import android.os.Build
import android.os.Environment
import android.provider.MediaStore
import android.text.Layout
import android.text.StaticLayout
import android.text.TextPaint
import androidx.core.content.res.ResourcesCompat
import java.io.ByteArrayOutputStream
import java.io.File
import java.util.zip.ZipEntry
import java.util.zip.ZipOutputStream

enum class ExportFormat(val extension: String, val mime: String) {
    TXT("txt", "text/plain"),
    MD("md", "text/markdown"),
    PDF("pdf", "application/pdf"),
    DOCX("docx", "application/vnd.openxmlformats-officedocument.wordprocessingml.document"),
}

/** Итог экспорта: имя для подписи и адрес файла — для кнопки «Открыть». */
data class ExportResult(val name: String, val uri: android.net.Uri?)

/**
 * Экспорт расшифровки в файл в Загрузках — туда же, куда пользователь
 * привык класть APK, и откуда файлы видны любому другому приложению.
 *
 * txt и md собираются строками, PDF рисуется системным PdfDocument, а docx —
 * это zip с одним осмысленным XML внутри: полноценная библиотека ради
 * абзацев с меткой времени не нужна.
 */
object MeetingExport {

    /** Одна встреча внутри файла: экспорт нескольких — это те же куски. */
    private class Section(
        val title: String,
        val duration: String,
        val summary: String,
        val segments: List<MeetingSegment>,
        val speakerAt: (Int) -> String?,
    )

    /** Строки саммери для экспорта: (это заголовок?, текст) без markdown-меток. */
    private fun summaryLines(summary: String): List<Pair<Boolean, String>> =
        summary.lines().mapNotNull { raw ->
            val l = raw.trim()
            when {
                l.isEmpty() -> null
                l.startsWith("#") -> true to l.trimStart('#').trim()
                l.startsWith("- ") || l.startsWith("• ") -> false to "• " + l.substring(2).trim()
                else -> false to l
            }
        }

    private fun section(
        context: Context,
        meeting: Meeting,
        segments: List<MeetingSegment>,
    ): Section {
        // Подпись говорящего идёт заголовком на смене голоса, как в пьесе.
        // Если пользователь дал людям имена — в файл идут имена.
        val speakerAt: (Int) -> String? = { index ->
            val s = segments[index]
            if (s.speaker != null && s.speaker != segments.getOrNull(index - 1)?.speaker) {
                MeetingStore.speakerLabel(context, meeting, s.speaker)
            } else null
        }
        return Section(
            MeetingStore.displayTitle(context, meeting),
            MeetingStore.durationLabel(context, meeting.seconds),
            meeting.summary,
            segments,
            speakerAt,
        )
    }

    private fun render(
        context: Context,
        sections: List<Section>,
        format: ExportFormat,
    ): ByteArray = when (format) {
        ExportFormat.TXT -> text(sections).toByteArray()
        ExportFormat.MD -> markdown(sections).toByteArray()
        ExportFormat.PDF -> pdf(context, sections)
        ExportFormat.DOCX -> docx(sections)
    }

    fun save(
        context: Context,
        meeting: Meeting,
        segments: List<MeetingSegment>,
        format: ExportFormat,
    ): ExportResult {
        val s = section(context, meeting, segments)
        val bytes = render(context, listOf(s), format)
        val name = "${safeName(s.title)}.${format.extension}"
        return ExportResult(name, write(context, name, format.mime, bytes))
    }

    /** Только саммери в Markdown — в письмо или заметку, без всей расшифровки. */
    fun saveSummary(context: Context, meeting: Meeting): ExportResult {
        require(meeting.summary.isNotBlank()) { "саммери ещё нет" }
        val title = MeetingStore.displayTitle(context, meeting)
        val body = "# $title\n\n${meeting.summary.trim()}\n"
        val name = "${safeName(title)} — саммери.md"
        return ExportResult(name, write(context, name, "text/markdown", body.toByteArray()))
    }

    /** Несколько встреч одним файлом; каждая начинается со своего заголовка. */
    fun saveCombined(
        context: Context,
        meetings: List<Pair<Meeting, List<MeetingSegment>>>,
        format: ExportFormat,
    ): ExportResult {
        val sections = meetings.map { (m, segments) -> section(context, m, segments) }
        val bytes = render(context, sections, format)
        val date = java.text.SimpleDateFormat(
            "d MMMM yyyy, HH.mm", java.util.Locale.getDefault(),
        ).format(java.util.Date())
        val title = context.getString(R.string.export_combined_name, date)
        val name = "${safeName(title)}.${format.extension}"
        return ExportResult(name, write(context, name, format.mime, bytes))
    }

    private fun safeName(title: String) =
        title.replace(":", ".").replace(Regex("[\\\\/*?\"<>|,]"), "")

    /**
     * Сам звук встречи — WAV как есть, в Загрузки. Запись лежит во
     * внутреннем хранилище, куда с компьютера не добраться, а исходник
     * бывает нужен: переслать, разобрать в другом инструменте.
     */
    fun saveAudio(context: Context, meeting: Meeting): ExportResult {
        val source = MeetingStore.audioFile(context, meeting.id)
        require(source.exists()) { "звука нет" }
        val name = "${safeName(MeetingStore.displayTitle(context, meeting))}.wav"
        return if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.Q) {
            val values = ContentValues().apply {
                put(MediaStore.MediaColumns.DISPLAY_NAME, name)
                put(MediaStore.MediaColumns.MIME_TYPE, "audio/wav")
                put(MediaStore.MediaColumns.RELATIVE_PATH, Environment.DIRECTORY_DOWNLOADS)
            }
            val uri = context.contentResolver.insert(
                MediaStore.Downloads.EXTERNAL_CONTENT_URI, values
            ) ?: error("не удалось создать файл")
            context.contentResolver.openOutputStream(uri)!!.use { out ->
                source.inputStream().use { it.copyTo(out) }
            }
            ExportResult(name, uri)
        } else {
            val dir = context.getExternalFilesDir(Environment.DIRECTORY_DOWNLOADS)!!
            val target = File(dir, name)
            source.copyTo(target, overwrite = true)
            ExportResult(name, null)
        }
    }

    private fun write(
        context: Context,
        name: String,
        mime: String,
        bytes: ByteArray,
    ): android.net.Uri? {
        return if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.Q) {
            val values = ContentValues().apply {
                put(MediaStore.MediaColumns.DISPLAY_NAME, name)
                put(MediaStore.MediaColumns.MIME_TYPE, mime)
                put(MediaStore.MediaColumns.RELATIVE_PATH, Environment.DIRECTORY_DOWNLOADS)
            }
            val uri = context.contentResolver.insert(
                MediaStore.Downloads.EXTERNAL_CONTENT_URI, values
            ) ?: error("не удалось создать файл")
            context.contentResolver.openOutputStream(uri)!!.use { it.write(bytes) }
            uri
        } else {
            val dir = context.getExternalFilesDir(Environment.DIRECTORY_DOWNLOADS)!!
            File(dir, name).writeBytes(bytes)
            null
        }
    }

    // --- текстовые форматы ------------------------------------------------

    private fun text(sections: List<Section>) = buildString {
        for ((n, sec) in sections.withIndex()) {
            if (n > 0) {
                appendLine()
                appendLine("————————")
                appendLine()
            }
            appendLine(sec.title)
            appendLine(sec.duration)
            appendLine()
            for ((head, line) in summaryLines(sec.summary)) {
                if (head) appendLine()
                appendLine(line)
            }
            if (sec.summary.isNotEmpty()) appendLine()
            for ((i, s) in sec.segments.withIndex()) {
                sec.speakerAt(i)?.let {
                    if (i > 0) appendLine()
                    appendLine(it)
                }
                appendLine("${MeetingStore.clockLabel(s.start)}  ${s.text}")
            }
        }
    }

    private fun markdown(sections: List<Section>) = buildString {
        for ((n, sec) in sections.withIndex()) {
            if (n > 0) {
                appendLine("---")
                appendLine()
            }
            appendLine("# ${sec.title}")
            appendLine()
            appendLine("*${sec.duration}*")
            appendLine()
            if (sec.summary.isNotEmpty()) {
                appendLine(sec.summary.trim())
                appendLine()
                appendLine("---")
                appendLine()
            }
            for ((i, s) in sec.segments.withIndex()) {
                sec.speakerAt(i)?.let {
                    appendLine("## $it")
                    appendLine()
                }
                appendLine("**${MeetingStore.clockLabel(s.start)}** ${s.text}")
                appendLine()
            }
        }
    }

    // --- PDF --------------------------------------------------------------

    private const val PAGE_W = 595 // A4 в пунктах
    private const val PAGE_H = 842
    private const val MARGIN = 56f

    private fun pdf(context: Context, sections: List<Section>): ByteArray {
        val regular = ResourcesCompat.getFont(context, R.font.inter_regular)
            ?: Typeface.DEFAULT
        val medium = ResourcesCompat.getFont(context, R.font.inter_medium)
            ?: Typeface.DEFAULT_BOLD

        val ink = 0xFF111111.toInt()
        val fog = 0xFF8A8A8A.toInt()

        val titlePaint = TextPaint(Paint.ANTI_ALIAS_FLAG).apply {
            typeface = medium; textSize = 18f; color = ink
        }
        val mutedPaint = TextPaint(Paint.ANTI_ALIAS_FLAG).apply {
            typeface = regular; textSize = 10f; color = fog
        }
        val bodyPaint = TextPaint(Paint.ANTI_ALIAS_FLAG).apply {
            typeface = regular; textSize = 11.5f; color = ink
        }
        val speakerPaint = TextPaint(Paint.ANTI_ALIAS_FLAG).apply {
            typeface = medium; textSize = 12f; color = ink
        }

        val doc = PdfDocument()
        val width = (PAGE_W - 2 * MARGIN).toInt()
        var pageNo = 0
        var page: PdfDocument.Page? = null
        var y = 0f

        fun newPage() {
            page?.let { doc.finishPage(it) }
            pageNo++
            page = doc.startPage(
                PdfDocument.PageInfo.Builder(PAGE_W, PAGE_H, pageNo).create()
            )
            y = MARGIN
        }

        fun layout(text: String, paint: TextPaint): StaticLayout =
            StaticLayout.Builder.obtain(text, 0, text.length, paint, width)
                .setAlignment(Layout.Alignment.ALIGN_NORMAL)
                .setLineSpacing(0f, 1.3f)
                .build()

        fun draw(l: StaticLayout, gapAfter: Float) {
            if (y + l.height > PAGE_H - MARGIN) newPage()
            val canvas = page!!.canvas
            canvas.save()
            canvas.translate(MARGIN, y)
            l.draw(canvas)
            canvas.restore()
            y += l.height + gapAfter
        }

        // Каждая встреча начинается со своей страницы: склееный экспорт
        // читается как подшивка, а не как один бесконечный поток.
        for (sec in sections) {
            newPage()
            draw(layout(sec.title, titlePaint), 4f)
            draw(layout(sec.duration, mutedPaint), 18f)
            for ((head, line) in summaryLines(sec.summary)) {
                if (head) draw(layout(line, speakerPaint), 6f)
                else draw(layout(line, bodyPaint), 4f)
            }
            for ((i, s) in sec.segments.withIndex()) {
                sec.speakerAt(i)?.let { draw(layout(it, speakerPaint), 6f) }
                draw(layout(MeetingStore.clockLabel(s.start), mutedPaint), 2f)
                draw(layout(s.text, bodyPaint), 12f)
            }
        }
        page?.let { doc.finishPage(it) }

        val out = ByteArrayOutputStream()
        doc.writeTo(out)
        doc.close()
        return out.toByteArray()
    }

    // --- DOCX -------------------------------------------------------------

    private fun docx(sections: List<Section>): ByteArray {
        val body = buildString {
            for ((n, sec) in sections.withIndex()) {
                // Разрыв страницы между встречами — как в PDF.
                if (n > 0) append("<w:p><w:r><w:br w:type=\"page\"/></w:r></w:p>")
                append(paragraph(esc(sec.title), size = 32, medium = true))
                append(paragraph(esc(sec.duration), color = "8A8A8A"))
                for ((head, line) in summaryLines(sec.summary)) {
                    if (head) append(paragraph(esc(line), size = 24, medium = true))
                    else append(paragraph(esc(line)))
                }
                for ((i, s) in sec.segments.withIndex()) {
                    sec.speakerAt(i)?.let { append(paragraph(esc(it), size = 24, medium = true)) }
                    append(
                        "<w:p><w:r><w:rPr><w:color w:val=\"8A8A8A\"/><w:sz w:val=\"18\"/></w:rPr>" +
                            "<w:t xml:space=\"preserve\">${esc(MeetingStore.clockLabel(s.start))}  </w:t></w:r>" +
                            "<w:r><w:t xml:space=\"preserve\">${esc(s.text)}</w:t></w:r></w:p>"
                    )
                }
            }
        }

        val document =
            """<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main">
<w:body>$body</w:body></w:document>"""

        val contentTypes =
            """<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types">
<Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/>
<Default Extension="xml" ContentType="application/xml"/>
<Override PartName="/word/document.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.document.main+xml"/>
</Types>"""

        val rels =
            """<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships">
<Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument" Target="word/document.xml"/>
</Relationships>"""

        val out = ByteArrayOutputStream()
        ZipOutputStream(out).use { zip ->
            for ((path, content) in listOf(
                "[Content_Types].xml" to contentTypes,
                "_rels/.rels" to rels,
                "word/document.xml" to document,
            )) {
                zip.putNextEntry(ZipEntry(path))
                zip.write(content.toByteArray())
                zip.closeEntry()
            }
        }
        return out.toByteArray()
    }

    /** Размер в docx задаётся полукеглями: sz 32 — это 16pt. */
    private fun paragraph(
        text: String,
        size: Int = 22,
        medium: Boolean = false,
        color: String? = null,
    ): String {
        val props = buildString {
            if (medium) append("<w:b/>")
            color?.let { append("<w:color w:val=\"$it\"/>") }
            append("<w:sz w:val=\"$size\"/>")
        }
        return "<w:p><w:r><w:rPr>$props</w:rPr>" +
            "<w:t xml:space=\"preserve\">$text</w:t></w:r></w:p>"
    }

    private fun esc(s: String) = s
        .replace("&", "&amp;")
        .replace("<", "&lt;")
        .replace(">", "&gt;")
}