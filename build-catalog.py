#!/usr/bin/env python3
"""
Собирает каталог моделей для приложения из каталога Handy.

Оставляет только те архитектуры, которые умеет наш движок transcribe.cpp,
и переводит описания на русский по редполитике проекта: sentence case,
буква «е» вместо «ё», без восклицаний.

Запуск из корня проекта:  python3 build-catalog.py
"""

import json
import os

SRC = "upstream/src-tauri/src/catalog/catalog.json"
ARCH_DIR = "tcpp/src/arch"
DST = "app-android/app/src/main/assets/catalog.json"

# Ниже этой оценки скорости модель на телефоне работает медленнее живой речи —
# ждать дольше, чем говорил, бессмысленно, поэтому такие в приложение не
# попадают. Порог выбран по замерам на Xiaomi 15 (Snapdragon 8 Elite):
# 98 -> 11x, 79 -> 9x, 78 -> 2x прошли; 42 -> 0.77x, 35 -> 0.45x, 23 -> 0.37x нет.
MIN_SPEED_SCORE = 63

# Из шести вариантов квантования оставляем три осмысленных. Замеры GigaAM на
# Xiaomi 15: F16 (428 МБ) — 20x, F32 (843 МБ) — 14x, Q4_K_M (174 МБ) — 11x.
# Квантованная версия оказалась самой медленной: движок собран с поддержкой
# fp16-инструкций, поэтому F16 считается железом напрямую, а Q4 приходится
# распаковывать на каждом умножении.
#
# F32 выкидываем: вдвое тяжелее F16 и при этом медленнее, смысла нет.
# Q5_K_M и Q6_K тоже: они между Q4 и Q8 и ничего не решают, только загромождают
# список.
KEEP_QUANTS = ("Q4_K_M", "Q8_0", "F16", "BF16")

# Замеренные значения, одна реплика 8.4 с, Q4_K_M, три прогона на Xiaomi 15.
# Остальным моделям ставим качественную оценку по speed_score — выдумывать
# для них точные цифры нельзя.
MEASURED = {
    "Whisper Tiny": 18,
    "GigaAM v3 E2E-CTC": 11,
    "Whisper Base": 9,
    "Parakeet TDT 0.6B v3": 9,
    "Whisper Small": 2,
}


def speed_note(model: dict) -> str:
    """Человеческое объяснение: насколько быстро и с каким качеством."""
    name = model["name"]
    score = model.get("speed_score") or 0
    accuracy = model.get("accuracy_score") or 0
    single_language = model.get("language_count") == 1

    if name in MEASURED:
        speed = f"{MEASURED[name]}x быстрее речи"
    elif score >= 90:
        speed = "очень быстрая"
    elif score >= 75:
        speed = "быстрая"
    else:
        speed = "средней скорости"

    # Общий балл точности усреднён по всем языкам, поэтому для одноязычных
    # моделей он ничего не говорит: GigaAM с баллом 69 на русском точнее
    # многоязычных с баллом 88. Такие модели описываем через специализацию.
    if single_language:
        quality = "обучена под один язык"
    elif accuracy >= 85:
        quality = "высокая точность"
    elif accuracy >= 70:
        quality = "хорошая точность"
    else:
        quality = "качество слабее"

    return f"{speed}, {quality}"

# Названия языков по-русски: в каталоге они кодами, а выбирать язык кодом
# невозможно. Покрывают все 103 языка, встречающиеся в моделях.
LANG_RU = {
    "af": "африкаанс", "am": "амхарский", "ar": "арабский", "as": "ассамский",
    "az": "азербайджанский", "ba": "башкирский", "be": "белорусский",
    "bg": "болгарский", "bn": "бенгальский", "bo": "тибетский", "br": "бретонский",
    "bs": "боснийский", "ca": "каталанский", "cs": "чешский", "cy": "валлийский",
    "da": "датский", "de": "немецкий", "el": "греческий", "en": "английский",
    "es": "испанский", "et": "эстонский", "eu": "баскский", "fa": "персидский",
    "fi": "финский", "fil": "филиппинский", "fo": "фарерский", "fr": "французский",
    "ga": "ирландский", "gl": "галисийский", "gu": "гуджарати", "ha": "хауса",
    "haw": "гавайский", "he": "иврит", "hi": "хинди", "hr": "хорватский",
    "ht": "гаитянский", "hu": "венгерский", "hy": "армянский",
    "id": "индонезийский", "is": "исландский", "it": "итальянский",
    "ja": "японский", "jw": "яванский", "ka": "грузинский", "kk": "казахский",
    "km": "кхмерский", "kn": "каннада", "ko": "корейский", "la": "латынь",
    "lb": "люксембургский", "ln": "лингала", "lo": "лаосский", "lt": "литовский",
    "lv": "латышский", "mg": "малагасийский", "mi": "маори", "mk": "македонский",
    "ml": "малаялам", "mn": "монгольский", "mr": "маратхи", "ms": "малайский",
    "mt": "мальтийский", "my": "бирманский", "nb": "норвежский букмол",
    "ne": "непальский", "nl": "нидерландский", "nn": "норвежский нюнорск",
    "no": "норвежский", "oc": "окситанский", "pa": "панджаби", "pl": "польский",
    "ps": "пушту", "pt": "португальский", "ro": "румынский", "ru": "русский",
    "sa": "санскрит", "sd": "синдхи", "si": "сингальский", "sk": "словацкий",
    "sl": "словенский", "sn": "шона", "so": "сомалийский", "sq": "албанский",
    "sr": "сербский", "su": "сунданский", "sv": "шведский", "sw": "суахили",
    "ta": "тамильский", "te": "телугу", "tg": "таджикский", "th": "тайский",
    "tk": "туркменский", "tl": "тагальский", "tr": "турецкий", "tt": "татарский",
    "uk": "украинский", "ur": "урду", "uz": "узбекский", "vi": "вьетнамский",
    "yi": "идиш", "yo": "йоруба", "yue": "кантонский", "zh": "китайский",
}

RU = {
    "100-language speech-to-text with auto language detection, segment-level timestamps.":
        "100 языков, автоопределение языка, тайминги по фразам",
    "100-language speech-to-text with translation, auto language detection, segment-level timestamps.":
        "100 языков, перевод, автоопределение языка, тайминги по фразам",
    "2-language speech-to-text with segment-level timestamps.":
        "2 языка, тайминги по фразам",
    "25-language speech-to-text with auto language detection, token-level timestamps.":
        "25 языков, автоопределение языка, тайминги по словам",
    "25-language speech-to-text with translation.":
        "25 языков, с переводом",
    "3-language speech-to-text.": "3 языка",
    "30-language speech-to-text with auto language detection.":
        "30 языков, автоопределение языка",
    "4-language speech-to-text with translation.": "4 языка, с переводом",
    "5-language speech-to-text with auto language detection.":
        "5 языков, автоопределение языка",
    "8-language speech-to-text with translation, auto language detection.":
        "8 языков, перевод, автоопределение языка",
    "99-language speech-to-text with translation, auto language detection, segment-level timestamps.":
        "99 языков, перевод, автоопределение языка, тайминги по фразам",
    "A tiny multilingual model": "Крошечная многоязычная модель",
    "Arabic speech-to-text.": "Арабский язык",
    "Broadest language, but may run a bit slow":
        "Самый широкий охват языков, но работает медленнее",
    "Chinese speech-to-text.": "Китайский язык",
    "English only. The best model for English speakers":
        "Только английский. Лучший выбор для английской речи",
    "English speech-to-text with segment-level timestamps.":
        "Английский язык, тайминги по фразам",
    "English speech-to-text with streaming, token-level timestamps.":
        "Английский язык, потоковый режим, тайминги по словам",
    "English speech-to-text with streaming.": "Английский язык, потоковый режим",
    "English speech-to-text with token-level timestamps.":
        "Английский язык, тайминги по словам",
    "English speech-to-text.": "Английский язык",
    "Excellent multilingual model": "Отличная многоязычная модель",
    "Fast and accurate. Supports 25 European languages":
        "Быстрая и точная. 25 европейских языков",
    "Fast, accurate live English transcription":
        "Быстрое и точное распознавание английского на лету",
    "Japanese speech-to-text.": "Японский язык",
    "Korean speech-to-text.": "Корейский язык",
    "Live multilingual transcription across 28 languages":
        "Многоязычное распознавание на лету, 28 языков",
    "Live multilingual, excellent on powerful machines":
        "Многоязычная, на лету, требует мощного устройства",
    "Optimized for Taiwanese Mandarin. Code-switching support.":
        "Тайваньский мандарин, поддержка переключения языков",
    "Russian speech-to-text with token-level timestamps.":
        "Русский язык, тайминги по словам",
    "Tiny and instant, runs well on any hardware":
        "Крошечная и мгновенная, пойдет на любом железе",
    "Ukrainian speech-to-text.": "Украинский язык",
    "Vietnamese speech-to-text.": "Вьетнамский язык",
}


def main() -> None:
    src = json.load(open(SRC, encoding="utf-8"))
    supported = set(os.listdir(ARCH_DIR))

    out = {"mirrors": src["mirrors"], "languages": {}, "models": []}
    missing = set()

    skipped_slow = 0
    for m in src["models"]:
        if m.get("architecture") not in supported:
            continue
        if (m.get("speed_score") or 0) < MIN_SPEED_SCORE:
            skipped_slow += 1
            continue
        files = [
            {
                "filename": f["filename"],
                "quant": f["quant"],
                "size_bytes": f["size_bytes"],
                "sha256": f["sha256"],
            }
            for f in m.get("files", [])
            # Основную версию оставляем всегда, даже если она не в списке:
            # только она лежит на запасном зеркале, и без неё пользователи с
            # заблокированным Hugging Face остались бы вообще без загрузки.
            if f.get("sha256")
            and (f["quant"] in KEEP_QUANTS or f["quant"] == m.get("default_quant"))
        ]
        if not files:
            continue

        english = m.get("description", "")
        if english and english not in RU:
            missing.add(english)

        out["models"].append({
            "id": m["id"],
            "revision": m["revision"],
            "name": m["name"],
            "architecture": m["architecture"],
            "description": RU.get(english, english),
            "license": m.get("license", ""),
            "languages": m.get("languages", []),
            "language_count": m.get("language_count", 0),
            "speed_score": m.get("speed_score"),
            "accuracy_score": m.get("accuracy_score"),
            "speed_note": speed_note(m),
            # Запасное зеркало Handy хранит только основную версию: проверено,
            # Q4_K_M и F16 отдают 404, Q8_0 отдаётся. Приложению это нужно,
            # чтобы подсказать выход, когда Hugging Face заблокирован.
            "default_quant": m.get("default_quant", ""),
            # Умения модели: потоковый режим (текст появляется по ходу речи)
            # и перевод на английский. По ним в приложении фильтры.
            "streaming": bool(m.get("capabilities", {}).get("streaming")),
            "translate": bool(m.get("capabilities", {}).get("translate")),
            "files": files,
        })

    # В приложение кладём только те языки, что реально встречаются, вместе с
    # числом моделей — чтобы список выбора не был свалкой из ста строк.
    counts: dict[str, int] = {}
    for m in out["models"]:
        for code in m["languages"]:
            counts[code] = counts.get(code, 0) + 1
    out["languages"] = {
        code: {"name": LANG_RU.get(code, code), "models": n}
        for code, n in sorted(counts.items(), key=lambda kv: (-kv[1], LANG_RU.get(kv[0], kv[0])))
    }

    os.makedirs(os.path.dirname(DST), exist_ok=True)
    json.dump(out, open(DST, "w", encoding="utf-8"),
              ensure_ascii=False, separators=(",", ":"))

    print(f"моделей: {len(out['models'])} (отсеяно как слишком медленные: {skipped_slow})")
    print(f"языков: {len(out['languages'])}")
    print(f"размер: {os.path.getsize(DST) / 1024:.1f} КБ")
    if missing:
        print("\nБез перевода — допишите их в RU:")
        for s in sorted(missing):
            print(" •", s)


if __name__ == "__main__":
    main()
