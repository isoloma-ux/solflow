package com.handy.voice

import android.view.LayoutInflater
import android.view.View
import android.view.ViewGroup
import android.widget.LinearLayout
import android.widget.TextView
import androidx.recyclerview.widget.RecyclerView

/** «174 МБ» или «1.2 ГБ» — байты в списке моделей читать невозможно. */
fun formatSize(bytes: Long): String {
    val mb = bytes / 1_048_576.0
    return if (mb >= 1024) "%.1f ГБ".format(mb / 1024) else "%.0f МБ".format(mb)
}

class ModelAdapter(
    private val onTap: (CatalogModel) -> Unit,
    private val onUse: (CatalogModel, ModelFile) -> Unit,
) : RecyclerView.Adapter<ModelAdapter.Holder>() {

    private var items: List<CatalogModel> = emptyList()
    private var downloaded: Set<String> = emptySet()
    private var active: String? = null

    fun submit(items: List<CatalogModel>, downloaded: Set<String>, active: String?) {
        this.items = items
        this.downloaded = downloaded
        this.active = active
        notifyDataSetChanged()
    }

    override fun getItemCount() = items.size

    override fun onCreateViewHolder(parent: ViewGroup, viewType: Int) =
        Holder(LayoutInflater.from(parent.context).inflate(R.layout.item_model, parent, false))

    override fun onBindViewHolder(holder: Holder, position: Int) {
        val m = items[position]
        val ctx = holder.itemView.context

        holder.name.text = m.name
        holder.description.text = m.description

        // Скорость важнее числа языков: языки уже названы в описании, а вот
        // насколько модель пригодна для диктовки, иначе не понять.
        holder.meta.text = ctx.getString(
            R.string.model_meta_one,
            m.speedNote,
            formatSize(m.defaultFile.sizeBytes),
        )

        val isActive = m.files.any { it.filename == active }
        val isDownloaded = m.files.any { it.filename in downloaded }

        // Зелёная пометка у всего скачанного, а не только у активной модели:
        // так сразу видно, что уже лежит на телефоне.
        holder.bullet.visibility = if (isDownloaded) View.VISIBLE else View.GONE
        when {
            isActive -> {
                holder.state.visibility = View.VISIBLE
                holder.state.setText(R.string.active)
            }
            isDownloaded -> {
                holder.state.visibility = View.VISIBLE
                holder.state.setText(R.string.downloaded)
            }
            else -> holder.state.visibility = View.GONE
        }

        // Скачанные версии показываем прямо в карточке и даём переключать
        // тапом: иначе не видно, что версий несколько, и приходится лезть в
        // окно загрузки, чтобы это узнать.
        val here = m.files.filter { it.filename in downloaded }
        holder.versions.removeAllViews()
        holder.versions.visibility = if (here.isEmpty()) View.GONE else View.VISIBLE
        holder.versionsLabel.visibility = holder.versions.visibility

        val inflater = LayoutInflater.from(ctx)
        for (file in here) {
            val chip = inflater.inflate(R.layout.item_version, holder.versions, false) as TextView
            chip.text = ctx.getString(
                R.string.version_chip, file.quant, formatSize(file.sizeBytes),
            )
            val on = file.filename == active
            chip.setTextColor(ctx.getColor(if (on) R.color.ink else R.color.fog))
            chip.setBackgroundResource(if (on) R.drawable.bg_tab_active else 0)
            chip.setOnClickListener { if (!on) onUse(m, file) }
            holder.versions.addView(chip)
        }

        holder.itemView.setOnClickListener { onTap(m) }
    }

    class Holder(v: View) : RecyclerView.ViewHolder(v) {
        val bullet: View = v.findViewById(R.id.bullet)
        val name: TextView = v.findViewById(R.id.name)
        val state: TextView = v.findViewById(R.id.state)
        val description: TextView = v.findViewById(R.id.description)
        val meta: TextView = v.findViewById(R.id.meta)
        val versions: LinearLayout = v.findViewById(R.id.versions)
        val versionsLabel: TextView = v.findViewById(R.id.versionsLabel)
    }
}
