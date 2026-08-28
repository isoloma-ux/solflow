package com.handy.voice

import android.app.Activity
import android.view.LayoutInflater
import android.view.View
import android.widget.LinearLayout
import android.widget.TextView
import androidx.core.content.ContextCompat
import com.google.android.material.bottomsheet.BottomSheetDialog

/**
 * Список вариантов нижним листом — тот же жест, что выбор числа говорящих
 * во встречах. Выбранный отмечен акцентным цветом: галочки в этой палитре
 * выглядели бы чужеродно, а цвет уже значит «текущее» по всему приложению.
 */
fun Activity.optionSheet(
    title: String,
    options: List<Pair<String, String>>,
    selected: String?,
    hint: String? = null,
    onPick: (String) -> Unit,
) {
    val sheet = BottomSheetDialog(this)
    val view = LayoutInflater.from(this).inflate(R.layout.sheet_speakers, null)
    view.findViewById<TextView>(R.id.sheetTitle).text = title
    view.findViewById<TextView>(R.id.sheetHint).apply {
        text = hint.orEmpty()
        visibility = if (hint.isNullOrBlank()) View.GONE else View.VISIBLE
    }

    val box = view.findViewById<LinearLayout>(R.id.sheetOptions)
    for ((value, label) in options) {
        val row = LayoutInflater.from(this)
            .inflate(R.layout.item_sheet_option, box, false) as TextView
        row.text = label
        if (value == selected) {
            row.setTextColor(ContextCompat.getColor(this, R.color.accent))
            row.typeface = resources.getFont(R.font.inter_medium)
        }
        row.setOnClickListener {
            sheet.dismiss()
            onPick(value)
        }
        box.addView(row)
    }

    sheet.setContentView(view)
    // Фон рисует наша разметка со скруглённым верхом, контейнер — прозрачный.
    (view.parent as? View)?.setBackgroundColor(0)
    view.setBackgroundResource(R.drawable.bg_sheet)
    sheet.show()
}
