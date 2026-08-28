package com.handy.voice

import android.content.Context
import org.json.JSONObject

data class ModelFile(
    val filename: String,
    val quant: String,
    val sizeBytes: Long,
    val sha256: String,
)

data class CatalogModel(
    val id: String,
    val revision: String,
    val name: String,
    val architecture: String,
    val description: String,
    val license: String,
    val languages: List<String>,
    val languageCount: Int,
    val speedScore: Int,
    val accuracyScore: Int,
    /** Человеческое объяснение скорости и качества, готовится генератором каталога. */
    val speedNote: String,
    /** Версия, которая лежит на запасном зеркале Handy. Остальные — только на Hugging Face. */
    val defaultQuant: String,
    val files: List<ModelFile>,
) {
    val supportsRussian: Boolean get() = languages.contains("ru")

    /** Q4_K_M — разумный размер при малой потере качества; иначе самый лёгкий. */
    val defaultFile: ModelFile
        get() = files.firstOrNull { it.quant == "Q4_K_M" } ?: files.minByOrNull { it.sizeBytes }!!
}

/**
 * Каталог моделей Handy, отфильтрованный до архитектур, которые умеет
 * transcribe.cpp. Лежит в ассетах, чтобы список открывался без сети.
 */
/** Язык из каталога: код, русское название и сколько моделей его знают. */
data class CatalogLanguage(val code: String, val name: String, val models: Int)

object Catalog {

    private var cached: List<CatalogModel>? = null
    private var mirrors: List<String> = emptyList()
    private var languages: List<CatalogLanguage> = emptyList()

    fun models(context: Context): List<CatalogModel> {
        cached?.let { return it }

        val raw = context.assets.open("catalog.json").bufferedReader().use { it.readText() }
        val root = JSONObject(raw)

        mirrors = root.optJSONArray("mirrors")?.let { arr ->
            (0 until arr.length()).map { arr.getString(it) }
        }.orEmpty()

        val langObj = root.optJSONObject("languages")
        languages = langObj?.keys()?.asSequence()?.map { code ->
            val item = langObj.getJSONObject(code)
            CatalogLanguage(code, item.getString("name"), item.getInt("models"))
        }?.sortedByDescending { it.models }?.toList().orEmpty()

        val arr = root.getJSONArray("models")
        val list = (0 until arr.length()).map { i ->
            val m = arr.getJSONObject(i)
            val langsArr = m.optJSONArray("languages")
            val filesArr = m.getJSONArray("files")
            CatalogModel(
                id = m.getString("id"),
                revision = m.getString("revision"),
                name = m.getString("name"),
                architecture = m.getString("architecture"),
                description = m.optString("description"),
                license = m.optString("license"),
                languages = langsArr?.let { a -> (0 until a.length()).map { a.getString(it) } }.orEmpty(),
                languageCount = m.optInt("language_count"),
                speedScore = m.optInt("speed_score"),
                accuracyScore = m.optInt("accuracy_score"),
                speedNote = m.optString("speed_note"),
                defaultQuant = m.optString("default_quant"),
                files = (0 until filesArr.length()).map { j ->
                    val f = filesArr.getJSONObject(j)
                    ModelFile(
                        filename = f.getString("filename"),
                        quant = f.getString("quant"),
                        sizeBytes = f.getLong("size_bytes"),
                        sha256 = f.getString("sha256"),
                    )
                },
            )
        }

        cached = list
        return list
    }

    /**
     * Источники для файла: сначала Hugging Face, затем зеркало Handy.
     * Обе ссылки закреплены на revision, поэтому байты неизменны и их можно
     * проверить по sha256 из каталога.
     */
    fun urlsFor(context: Context, model: CatalogModel, file: ModelFile): List<String> {
        models(context)
        val hf = "https://huggingface.co/${model.id}/resolve/${model.revision}/${file.filename}"
        return listOf(hf) + mirrors.map { "$it/${model.id}/${model.revision}/${file.filename}" }
    }

    fun languages(context: Context): List<CatalogLanguage> {
        models(context)
        return languages
    }

    /** Модель, которой принадлежит скачанный файл. */
    fun findByFilename(context: Context, filename: String): Pair<CatalogModel, ModelFile>? =
        models(context).firstNotNullOfOrNull { m ->
            m.files.firstOrNull { it.filename == filename }?.let { m to it }
        }
}
