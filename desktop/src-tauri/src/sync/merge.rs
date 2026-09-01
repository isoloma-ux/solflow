//! Слияние при конфликте — когда одну и ту же встречу поправили на двух
//! устройствах, не успев синхронизироваться. Чистые функции без сети и
//! диска, чтобы их можно было гонять тестами; тот же алгоритм повторён в
//! Kotlin на Android.

use crate::meetings::{Meta, Project, STATE_DONE};

/// Мета встречи: побеждает та, что сохранена позже (`updated`), но пустое
/// поле победителя не затирает заполненное у проигравшего. Иначе саммери,
/// посчитанное на компьютере, пропадало бы от того, что телефон в ту же
/// минуту переименовал встречу.
///
/// Проект — исключение: «убрать из проекта» — осознанное действие, и
/// новое значение берётся как есть, даже пустое.
pub fn merge_meta(local: &Meta, remote: &Meta) -> Meta {
    let (newer, older) = if remote.updated >= local.updated {
        (remote, local)
    } else {
        (local, remote)
    };
    let mut merged = newer.clone();
    if merged.title.trim().is_empty() {
        merged.title = older.title.clone();
    }
    if merged.summary.is_empty() {
        merged.summary = older.summary.clone();
    }
    if merged.names.is_empty() {
        merged.names = older.names.clone();
    }
    if merged.speakers == 0 {
        merged.speakers = older.speakers;
    }
    // Готовая расшифровка важнее любого промежуточного состояния: если одно
    // устройство уже дошло до конца, встреча готова.
    if merged.state != STATE_DONE && older.state == STATE_DONE {
        merged.state = STATE_DONE.to_string();
        merged.error = None;
    }
    if merged.seconds < older.seconds {
        merged.seconds = older.seconds;
    }
    merged.updated = newer.updated.max(older.updated);
    merged
}

/// Проекты сливаются трёхсторонне — со снимком того, что было после
/// прошлой синхронизации. Так отличается «удалили там» от «добавили здесь»
/// без отдельных надгробий: чего нет на одной стороне, но было в снимке, —
/// удалено; чего не было в снимке — добавлено.
///
/// Порядок: сначала как на Диске, новые местные — в конец.
pub fn merge_projects(local: &[Project], remote: &[Project], snapshot: &[Project]) -> Vec<Project> {
    let find = |list: &[Project], id: &str| list.iter().find(|p| p.id == id).cloned();
    let mut out: Vec<Project> = Vec::new();

    for r in remote {
        match find(local, &r.id) {
            Some(l) => {
                // Есть на обеих сторонах: имя берём у того, кто его менял.
                // Меняли оба — местное, оно у человека перед глазами.
                let name = if l.name == r.name {
                    l.name.clone()
                } else if find(snapshot, &r.id).map(|s| s.name == l.name).unwrap_or(false) {
                    r.name.clone()
                } else {
                    l.name.clone()
                };
                out.push(Project { id: r.id.clone(), name });
            }
            None => {
                // Только на Диске: либо удалили здесь (был в снимке), либо
                // добавили там.
                if find(snapshot, &r.id).is_none() {
                    out.push(r.clone());
                }
            }
        }
    }
    for l in local {
        if find(remote, &l.id).is_none() && find(snapshot, &l.id).is_none() {
            out.push(l.clone());
        }
    }
    out
}

pub fn same_projects(a: &[Project], b: &[Project]) -> bool {
    a.len() == b.len() && a.iter().zip(b).all(|(x, y)| x.id == y.id && x.name == y.name)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    fn meta(updated: i64) -> Meta {
        Meta {
            title: String::new(),
            at: 1,
            seconds: 10.0,
            state: STATE_DONE.to_string(),
            imported: false,
            project: None,
            speakers: 0,
            names: HashMap::new(),
            summary: String::new(),
            error: None,
            updated,
        }
    }

    fn p(id: &str, name: &str) -> Project {
        Project { id: id.to_string(), name: name.to_string() }
    }

    #[test]
    fn newer_title_wins_but_summary_survives() {
        let mut local = meta(200);
        local.title = "Новое имя".into();
        let mut remote = meta(100);
        remote.summary = "Саммери".into();
        remote.title = "Старое имя".into();
        let m = merge_meta(&local, &remote);
        assert_eq!(m.title, "Новое имя");
        assert_eq!(m.summary, "Саммери");
        assert_eq!(m.updated, 200);
    }

    #[test]
    fn empty_newer_title_keeps_older() {
        let local = meta(300);
        let mut remote = meta(100);
        remote.title = "Имя".into();
        assert_eq!(merge_meta(&local, &remote).title, "Имя");
    }

    #[test]
    fn done_beats_recorded() {
        let mut local = meta(300);
        local.state = "recorded".into();
        let remote = meta(100);
        assert_eq!(merge_meta(&local, &remote).state, STATE_DONE);
    }

    #[test]
    fn project_removal_is_respected() {
        let local = meta(300);
        let mut remote = meta(100);
        remote.project = Some("p1".into());
        assert_eq!(merge_meta(&local, &remote).project, None);
    }

    #[test]
    fn projects_three_way() {
        let snapshot = vec![p("a", "А"), p("b", "Б"), p("c", "В")];
        // Локально: переименовали a, удалили b, добавили d.
        let local = vec![p("a", "А2"), p("c", "В"), p("d", "Г")];
        // На Диске: удалили c, добавили e, переименовали... ничего.
        let remote = vec![p("a", "А"), p("b", "Б"), p("e", "Д")];
        let merged = merge_projects(&local, &remote, &snapshot);
        let ids: Vec<&str> = merged.iter().map(|x| x.id.as_str()).collect();
        assert_eq!(ids, vec!["a", "e", "d"]);
        assert_eq!(merged[0].name, "А2");
    }

    #[test]
    fn projects_remote_rename_wins_when_local_untouched() {
        let snapshot = vec![p("a", "А")];
        let local = vec![p("a", "А")];
        let remote = vec![p("a", "Переименован")];
        assert_eq!(merge_projects(&local, &remote, &snapshot)[0].name, "Переименован");
    }

    #[test]
    fn projects_first_sync_is_union() {
        let local = vec![p("a", "А")];
        let remote = vec![p("b", "Б")];
        let merged = merge_projects(&local, &remote, &[]);
        assert_eq!(merged.len(), 2);
    }
}
