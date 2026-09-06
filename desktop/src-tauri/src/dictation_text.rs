//! Punctuation proposals are projected onto source words. Generated words
//! never enter the result. Unaligned words and their adjacent gaps stay intact.
use std::ops::Range;

pub const MAX_CHARS: usize = 6000;
pub const MAX_WORDS: usize = 800;
pub const PROMPT: &str = "Restore punctuation and capitalization in this Russian speech transcript.\n\
It has no reliable sentence boundaries. Use periods to separate independent sentences.\n\
Use commas to join subordinate clauses. Start each sentence with a capital letter.\n\
Return a readable paragraph. Preserve the original wording and word order.\n\
Return ONLY the punctuated paragraph, no introduction. /no_think";

fn editable(s: &str) -> bool {
    s.chars().all(|c| c.is_whitespace() || ".,!?;:".contains(c))
}

const SENTENCE_STARTS: &[&str] = &[
    "как", "каким", "какой", "какая", "какие", "что", "это", "он", "она", "они",
    "оно", "мы", "вы", "я", "именно", "хочу", "теперь", "сейчас", "потом", "дальше",
    "затем", "почему", "когда", "где", "если", "чтобы", "поэтому", "который", "которая",
];

/// Keep technical literals, domains, initials and numeric expressions intact.
pub fn protected(s: &str) -> bool {
    let core = s.trim_end_matches(['.', '!', '?', ',', ';', ':']);
    s.chars().any(|c| c.is_numeric() || "@/\\_:".contains(c))
        || (core.contains('.') && core.chars().any(|c| c.is_ascii_alphabetic()))
        // An unknown Cyrillic dot-token may also be a domain/file name.
        // Split only familiar sentence starts; do not guess arbitrary TLDs.
        || core.split('.').skip(1).any(|part| !SENTENCE_STARTS.contains(&part.to_lowercase().as_str()))
        || s.split('.').skip(1).any(|part| {
            let p = part.trim_matches(|c: char| !c.is_alphabetic()).to_lowercase();
            ["рф", "рус", "москва", "онлайн", "сайт", "дети", "орг", "ком"].contains(&p.as_str())
        })
        || (s.contains('.') && s.split('.').any(|part| part.chars().count() == 1 && part.chars().all(char::is_alphabetic)))
}

/// Conservative fix for glued Russian sentences. Latin domains and initials
/// are ambiguous, so leave them alone. This exact rule is shared with Android.
pub fn spacing(text: &str) -> String {
    text.split_whitespace().map(|chunk| {
        if protected(chunk) { return chunk.to_string(); }
        let cs: Vec<char> = chunk.chars().collect();
        let mut out = String::new();
        for (i, &c) in cs.iter().enumerate() {
            out.push(c);
            if ".!?".contains(c) && i > 0 && cs[i - 1].is_alphabetic()
                && cs.get(i + 1).map(|n| ('а'..='я').contains(n)
                    || ('А'..='Я').contains(n) || *n == 'ё' || *n == 'Ё').unwrap_or(false) {
                out.push(' ');
            }
        }
        out
    }).collect::<Vec<_>>().join(" ")
}

fn word_char(c: char) -> bool {
    c.is_alphanumeric() || ('\u{0300}'..='\u{036f}').contains(&c)
}

struct Word<'a> { text: &'a str, range: Range<usize>, locked: bool }

fn words(text: &str) -> Vec<Word<'_>> {
    let locks: Vec<Range<usize>> = text.match_indices(|c: char| !c.is_whitespace())
        .map(|(i, _)| i).scan(0, |end, start| {
            if start < *end { return Some(None); }
            *end = text[start..].find(char::is_whitespace).map(|n| start + n).unwrap_or(text.len());
            Some(protected(&text[start..*end]).then_some(start..*end))
        }).flatten().collect();
    let mut out = Vec::new();
    let mut start = None;
    for (i, c) in text.char_indices().chain(std::iter::once((text.len(), ' '))) {
        if word_char(c) { start.get_or_insert(i); }
        else if let Some(from) = start.take() {
            out.push(Word { text: &text[from..i], range: from..i,
                locked: locks.iter().any(|r| r.contains(&from)) });
        }
    }
    out
}

pub fn eligible(text: &str) -> bool {
    text.chars().count() <= MAX_CHARS && (1..=MAX_WORDS).contains(&words(text).len())
}

/// Only the model input loses unreliable sentence boundaries. Source is kept.
pub fn prepare(text: &str) -> String {
    let ws = words(text);
    let mut out = String::new();
    let mut end = 0;
    for (i, w) in ws.iter().enumerate() {
        let gap = &text[end..w.range.start];
        if i > 0 && !w.locked && !ws[i - 1].locked && editable(gap) {
            out.extend(gap.chars().map(|c| if ".!?;".contains(c) { ' ' } else { c }));
        } else { out.push_str(gap); }
        let acronym = w.text.chars().count() >= 2 && w.text.chars().filter(|c| c.is_alphabetic()).all(char::is_uppercase);
        if !w.locked && !acronym {
            out.extend(w.text.chars().flat_map(|c| {
                if ('А'..='Я').contains(&c) || c == 'Ё' { c.to_lowercase().collect::<Vec<_>>() }
                else { vec![c] }
            }));
        } else { out.push_str(w.text); }
        end = w.range.end;
    }
    out.push_str(&text[end..]);
    out
}

/// Monotonic word alignment, then copy ONLY punctuation/case at exact matches.
/// More than 10% edits (none permitted for short phrases) rejects the proposal.
pub fn project(source: &str, proposed: &str) -> Result<String, &'static str> {
    if !eligible(source) || proposed.chars().count() > MAX_CHARS * 2 {
        return Err("too_long");
    }
    let a = words(source);
    let b = words(proposed);
    let (n, m) = (a.len(), b.len());
    if m == 0 || m > MAX_WORDS * 2 || !editable(&proposed[..b[0].range.start]) {
        return Err("invalid");
    }
    let al: Vec<_> = a.iter().map(|w| w.text.to_lowercase()).collect();
    let bl: Vec<_> = b.iter().map(|w| w.text.to_lowercase()).collect();
    let mut d = vec![0u16; (n + 1) * (m + 1)];
    let at = |i: usize, j: usize| i * (m + 1) + j;
    for i in 0..=n { d[at(i, 0)] = i as u16; }
    for j in 0..=m { d[at(0, j)] = j as u16; }
    for i in 1..=n { for j in 1..=m {
        d[at(i, j)] = (d[at(i - 1, j - 1)] + u16::from(al[i - 1] != bl[j - 1]))
            .min(d[at(i - 1, j)] + 1).min(d[at(i, j - 1)] + 1);
    }}
    let allowed = if n < 20 { 0 } else { n / 10 };
    if d[at(n, m)] as usize > allowed { return Err("changed_words"); }
    let mut mapping = vec![None; n];
    let (mut i, mut j) = (n, m);
    while i > 0 || j > 0 {
        if i > 0 && j > 0 && d[at(i, j)] == d[at(i - 1, j - 1)] + u16::from(al[i - 1] != bl[j - 1]) {
            if al[i - 1] == bl[j - 1] { mapping[i - 1] = Some(j - 1); }
            i -= 1; j -= 1;
        } else if i > 0 && d[at(i, j)] == d[at(i - 1, j)] + 1 { i -= 1; }
        else { j -= 1; }
    }
    let mut out = source[..a[0].range.start].to_string();
    for (i, w) in a.iter().enumerate() {
        if i > 0 {
            let original_gap = &source[a[i - 1].range.end..w.range.start];
            let gap = match (mapping[i - 1], mapping[i]) {
                (Some(p), Some(q)) if q == p + 1 && !w.locked && !a[i - 1].locked => {
                    let candidate = &proposed[b[p].range.end..b[q].range.start];
                    // This mode repairs false sentence boundaries. Do not
                    // introduce fresh stops/commas inside an intact phrase.
                    if original_gap.contains(['.', '!', '?'])
                        && editable(original_gap) && editable(candidate) && candidate.len() <= 16
                        && !candidate.is_empty() { candidate } else { original_gap }
                }
                _ => original_gap,
            };
            out.push_str(gap);
        }
        let preserved_stop = out.trim_end().ends_with(['.', '!', '?'])
            && w.text.chars().next().map(char::is_uppercase).unwrap_or(false);
        let original_lower = w.text.chars().next().map(char::is_lowercase).unwrap_or(false);
        let keep_case = original_lower || i == 0 || preserved_stop || w.locked || w.text.chars().any(|c| c.is_ascii_uppercase())
            || (w.text.chars().count() >= 2 && w.text.chars().filter(|c| c.is_alphabetic()).all(char::is_uppercase));
        let rendered = if keep_case { w.text } else {
            mapping[i].map(|j| b[j].text).unwrap_or(w.text)
        };
        let first = rendered.chars().next().unwrap();
        if !w.locked && (i == 0 || out.trim_end().ends_with(['.', '!', '?']))
            && (('а'..='я').contains(&first) || first == 'ё') {
            out.extend(first.to_uppercase());
            out.push_str(&rendered[first.len_utf8()..]);
        } else { out.push_str(rendered); }
    }
    let last = a.last().unwrap();
    let original_tail = &source[last.range.end..];
    let tail = if mapping[n - 1] == Some(m - 1) && !last.locked {
        let candidate = &proposed[b[m - 1].range.end..];
        if editable(original_tail) && editable(candidate) && candidate.len() <= 16 { candidate }
        else { original_tail }
    } else { original_tail };
    out.push_str(tail);
    // Independent final invariant: same words in the same order, including numbers.
    if words(&out).iter().map(|w| w.text.to_lowercase()).collect::<Vec<_>>() != al {
        return Err("invalid");
    }
    Ok(out.trim().to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn joins_and_keeps_lexical_content() {
        assert_eq!(project("Я хочу понять. Насколько это удобно.", "Я хочу понять, насколько это удобно.").unwrap(),
            "Я хочу понять, насколько это удобно.");
        assert!(project("Не отправляй письмо Ивану.", "Отправляй письмо Ивану.").is_err());
        assert!(project("Первый второй третий.", "Третий второй первый.").is_err());
        assert!(project("Полный текст без сокращений.", "Полный текст.").is_err());
        assert_eq!(project("Хочу понять это сейчас.", "Хочу. Понять это сейчас.").unwrap(),
            "Хочу понять это сейчас.");
        assert_eq!(project("Текст.как это работает.", "Текст. Как это работает.").unwrap(),
            "Текст. Как это работает.");
    }
    #[test]
    fn technical_literals_and_noneditable_symbols_are_preserved() {
        let a = "Цена -1,5. Срок 05.09.2026 в 12:30. Сайт example.ru и https://пример.рф/a?b=1. ООО и Chat GPT.";
        let b = "Цена -1.5. Срок 05,09,2026 в 12,30. Сайт example,ru и https://пример,рф/a?b=1. ооо и chat gpt.";
        assert_eq!(project(a, b).unwrap(), a);
        assert_eq!(project("Да — нет.", "Да, нет.").unwrap(), "Да — нет.");
        assert!(project("Сохрани весь исходный текст.", "Вот исправленный текст: Сохрани весь исходный текст.").is_err());
    }
    #[test]
    fn preparation_does_not_damage_literals() {
        assert_eq!(prepare("Хочу понять. Насколько это удобно. ООО Иван Chat GPT 1.5 example.ru"),
            "хочу понять  насколько это удобно  ООО иван Chat GPT 1.5 example.ru");
    }
    #[test]
    fn real_fixture_projection_preserves_every_source_word() {
        let source = include_str!("../../../tests/fixtures/dictation/recognizer-raw.txt").trim();
        let proposal = include_str!("../../../tests/fixtures/dictation/model-proposal.txt").trim();
        let result = project(source, proposal).unwrap();
        assert!(result.contains("может быть, больше"), "{result}");
        assert!(result.contains("Chat GPT Astra теперь"), "{result}");
        assert!(result.contains("Ес ли"), "recognizer spelling must survive: {result}");
        assert!(!result.contains("Тот,"));
        assert!(!result.contains("Более"));
        assert!(result.starts_with("Проверка работы приложения того, как"));
        assert!(!result.contains("Больше точно,"));
    }
}
