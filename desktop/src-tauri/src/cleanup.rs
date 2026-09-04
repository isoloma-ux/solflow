//! Чистка распознанного текста — порт TextCleanup из Android-версии
//! (та, в свою очередь, выросла из audio_toolkit/text.rs десктопного Handy).
//! Принцип: убираем только то, что не является словом ни в одном языке.

const FILLERS: &[&str] = &[
    "uh", "uhm", "umm", "uhh", "uhhh", "ehh", "ehm", "ahm", "hmm", "hm", "mmm",
    "хм", "хмм", "ммм", "мм", "эм", "эмм", "ээ", "эээ", "эх", "мхм",
];

const PUNCT: &[char] = &[',', '.', '!', '?', ';', ':'];

/// Слова-паразиты — не мычание, а настоящие слова: «типа», «как бы».
/// Они убираются только по просьбе: в чужой речи это уже редактура, и
/// иногда меняет смысл («это как бы работает» — сомнение, а не мусор).
const PARASITES: &[&str] = &[
    "типа", "как бы", "короче", "ну вот", "вот это вот", "в общем-то",
    "то есть как бы", "собственно говоря", "так сказать", "в принципе",
    "you know", "i mean", "kind of", "sort of", "basically", "literally",
    "actually",
];

pub fn clean(text: &str) -> String {
    clean_with(text, false)
}

/// [drop_parasites] включает вычистку слов-паразитов поверх обычной чистки.
pub fn clean_with(text: &str, drop_parasites: bool) -> String {
    if text.trim().is_empty() {
        return String::new();
    }
    let result = drop_glued_dashes(text);
    let result = drop_fillers(&result);
    let result = collapse_stutters(&result);
    let result = if drop_parasites {
        drop_parasite_words(&result)
    } else {
        result
    };
    let result = normalize_spaces_and_punct(&result);
    result.trim().to_string()
}

/// Убирает слова-паразиты вместе с прилипшей к ним запятой. Ищем по
/// словам, а не подстрокой: иначе «типаж» превратится в «ж».
fn drop_parasite_words(text: &str) -> String {
    let mut result = text.to_string();
    for phrase in PARASITES {
        let mut out = String::with_capacity(result.len());
        let lower = result.to_lowercase();
        let mut from = 0usize;
        while let Some(found) = lower[from..].find(phrase) {
            let at = from + found;
            let end = at + phrase.len();
            let before_ok = at == 0
                || !lower[..at]
                    .chars()
                    .next_back()
                    .map(|c| c.is_alphanumeric())
                    .unwrap_or(false);
            let after = lower[end..].chars().next();
            let after_ok = after
                .map(|c| !c.is_alphanumeric())
                .unwrap_or(true);
            if before_ok && after_ok {
                out.push_str(&result[from..at]);
                // Съедаем запятую и пробел, оставшиеся от вырезанного слова.
                let tail: String = result[end..].chars().take_while(|c| *c == ',').collect();
                from = end + tail.len();
            } else {
                out.push_str(&result[from..end]);
                from = end;
            }
        }
        out.push_str(&result[from..]);
        result = out;
    }
    result
}

/// Дефис, прилипший к началу слова («-а моделька»), — артефакт распознавания.
/// Одиночное тире между словами — законный знак, его не трогаем.
fn drop_glued_dashes(text: &str) -> String {
    text.split(' ')
        .map(|word| {
            let stripped = word.trim_start_matches(['-', '\u{2013}', '\u{2014}']);
            match stripped.chars().next() {
                Some(c) if c.is_alphabetic() => stripped,
                _ => word,
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

/// Мычание, которое распознавание пишет как есть: «э-э», «а-аа», «э-ээ»,
/// «м-м», «эээ». Одна повторяющаяся гласная (или «м») с дефисами или без —
/// не слово. Одиночные «а», «о», «у» — настоящие слова, их не трогаем;
/// одиночное «э» — только мычание.
fn is_hesitation(bare: &str) -> bool {
    let letters: Vec<char> = bare.chars().filter(|c| *c != '-').collect();
    let Some(&first) = letters.first() else { return false };
    if !"эаоум".contains(first) || letters.iter().any(|c| *c != first) {
        return false;
    }
    bare.contains('-') || first == 'э' || letters.len() >= 2
}

/// Междометия и мычание выкидываются вместе с прилипшей к ним запятой.
/// Если выкинутое стояло с большой буквы — начинало фразу, — заглавная
/// переходит к следующему слову: «А-а, послушать» → «Послушать».
fn drop_fillers(text: &str) -> String {
    let mut out: Vec<String> = Vec::new();
    let mut capitalize_next = false;
    for word in text.split(' ') {
        let bare = word.trim_matches(PUNCT).to_lowercase();
        let filler = !bare.is_empty() && (FILLERS.contains(&bare.as_str()) || is_hesitation(&bare));
        if filler {
            if word.chars().next().map(|c| c.is_uppercase()).unwrap_or(false) {
                capitalize_next = true;
            }
            continue;
        }
        if capitalize_next && !word.is_empty() {
            capitalize_next = false;
            let mut chars = word.chars();
            let first = chars.next().unwrap();
            out.push(first.to_uppercase().collect::<String>() + chars.as_str());
        } else {
            out.push(word.to_string());
        }
    }
    out.join(" ")
}

/// Три и более одинаковых слова подряд — почти всегда заикание; два — часто
/// осмысленны («очень очень длинный»), поэтому порог именно три.
fn collapse_stutters(text: &str) -> String {
    let words: Vec<&str> = text.split(' ').filter(|w| !w.is_empty()).collect();
    let mut out: Vec<&str> = Vec::with_capacity(words.len());
    let mut i = 0;
    while i < words.len() {
        let word = words[i];
        if word.chars().all(|c| c.is_alphabetic()) {
            let mut count = 1;
            while i + count < words.len()
                && words[i + count].eq_ignore_ascii_case(word)
            {
                count += 1;
            }
            // eq_ignore_ascii_case не берёт кириллицу — сравним и в нижнем
            // регистре целиком.
            if count == 1 {
                let lower = word.to_lowercase();
                while i + count < words.len() && words[i + count].to_lowercase() == lower {
                    count += 1;
                }
            }
            out.push(word);
            i += if count >= 3 { count } else { 1 };
        } else {
            out.push(word);
            i += 1;
        }
    }
    out.join(" ")
}

/// Лишние пробелы, пробел перед знаком, задвоенные знаки — одним проходом.
fn normalize_spaces_and_punct(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut prev_space = false;
    let mut prev_punct: Option<char> = None;
    for c in text.chars() {
        if c.is_whitespace() {
            prev_space = true;
            continue;
        }
        if PUNCT.contains(&c) {
            // пробел перед знаком не пишем; задвоенный знак пропускаем
            if prev_punct == Some(c) {
                prev_space = false;
                continue;
            }
            out.push(c);
            prev_punct = Some(c);
            prev_space = false;
            continue;
        }
        if prev_space && !out.is_empty() {
            out.push(' ');
        }
        out.push(c);
        prev_space = false;
        prev_punct = None;
    }
    out
}

#[cfg(test)]
mod tests {
    use super::{clean, is_hesitation};

    #[test]
    fn hesitations_are_recognised() {
        for w in ["э", "ээ", "э-э", "э-ээ", "а-а", "а-аа", "м-м", "ммм", "у-у"] {
            assert!(is_hesitation(w), "{w}");
        }
        for w in ["а", "о", "у", "ага", "угу", "эх", "мама", "а-то"] {
            assert!(!is_hesitation(w), "{w}");
        }
    }

    #[test]
    fn hesitations_drop_and_capital_moves_on() {
        assert_eq!(
            clean("А-а, послушать нас с Иваном, а-а, на такую тему. Э-ээ, и всё."),
            "Послушать нас с Иваном, на такую тему. И всё."
        );
        assert_eq!(clean("Ну и всё. Тогда давайте, ага. А рынок сжимается."), "Ну и всё. Тогда давайте, ага. А рынок сжимается.");
    }
}
