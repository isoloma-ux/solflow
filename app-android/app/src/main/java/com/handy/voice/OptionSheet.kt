package com.handy.voice

import android.app.Activity
import android.view.LayoutInflater
import android.view.View
import android.widget.LinearLayout
import android.widget.TextView
import androidx.core.content.ContextCompat
import androidx.core.widget.TextViewCompat
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

/**
 * Пункт листа действий: иконка слева, опасное действие красным. Пустой
 * пункт — разделитель между смысловыми блоками: «скопировать, поделиться,
 * экспорт» отдельно от «переименовать» и «удалить».
 */
data class SheetOption(
    val value: String,
    val label: String,
    val icon: Int? = null,
    val danger: Boolean = false,
) {
    companion object {
        val DIVIDER = SheetOption("", "")
    }
}

/**
 * Лист действий — тот же нижний лист, что и выбор варианта, но пункты с
 * иконками и группами. Экран встречи и шторка показывают им одно и то же
 * меню, чтобы действия узнавались с первого взгляда.
 */
fun Activity.actionSheet(
    title: String,
    options: List<SheetOption>,
    onPick: (String) -> Unit,
) {
    val sheet = BottomSheetDialog(this)
    val view = LayoutInflater.from(this).inflate(R.layout.sheet_speakers, null)
    view.findViewById<TextView>(R.id.sheetTitle).text = title
    view.findViewById<TextView>(R.id.sheetHint).visibility = View.GONE

    val box = view.findViewById<LinearLayout>(R.id.sheetOptions)
    val density = resources.displayMetrics.density
    for (option in options) {
        if (option == SheetOption.DIVIDER) {
            val line = View(this)
            line.setBackgroundColor(ContextCompat.getColor(this, R.color.hairline))
            box.addView(
                line,
                LinearLayout.LayoutParams(
                    LinearLayout.LayoutParams.MATCH_PARENT, (1 * density).toInt().coerceAtLeast(1),
                ).apply {
                    topMargin = (6 * density).toInt()
                    bottomMargin = (6 * density).toInt()
                },
            )
            continue
        }
        val row = LayoutInflater.from(this)
            .inflate(R.layout.item_sheet_option, box, false) as TextView
        row.text = option.label
        val color = ContextCompat.getColor(this, if (option.danger) R.color.danger else R.color.fog)
        if (option.icon != null) {
            // Один размер для всех: у векторов разная собственная величина,
            // а ряд иконок должен читаться как ряд.
            val size = (22 * density).toInt()
            val icon = ContextCompat.getDrawable(this, option.icon)?.mutate()?.apply {
                setBounds(0, 0, size, size)
            }
            row.setCompoundDrawablesRelative(icon, null, null, null)
            row.compoundDrawablePadding = (16 * density).toInt()
            TextViewCompat.setCompoundDrawableTintList(row, android.content.res.ColorStateList.valueOf(color))
        }
        if (option.danger) row.setTextColor(color)
        row.setOnClickListener {
            sheet.dismiss()
            onPick(option.value)
        }
        box.addView(row)
    }

    sheet.setContentView(view)
    (view.parent as? View)?.setBackgroundColor(0)
    view.setBackgroundResource(R.drawable.bg_sheet)
    sheet.show()
}
