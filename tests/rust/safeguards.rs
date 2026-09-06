#[path = "../../desktop/src-tauri/src/cleanup.rs"]
mod cleanup;
#[path = "../../desktop/src-tauri/src/dictation_text.rs"]
mod dictation_text;
#[path = "../../desktop/src-tauri/src/transcript_store.rs"]
mod transcript_store;
#[path = "../../desktop/src-tauri/src/speaker_attribution.rs"]
mod speaker_attribution;
use std::fs;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use transcript_store::{atomic_write, read_visible, TranscriptDraft};

#[test]
fn shared_speaker_fixtures() {
    for line in include_str!("../fixtures/speakers.tsv").lines() {
        let c: Vec<_> = line.split('\t').collect();
        let bounds: Vec<_> = c[1].split(';').map(|v| {
            let n: Vec<f32> = v.split(',').map(|x| x.parse().unwrap()).collect(); (n[0], n[1])
        }).collect();
        let turns: Vec<_> = if c[2] == "-" { vec![] } else { c[2].split(';').map(|v| {
            let n: Vec<_> = v.split(',').collect(); (n[0].parse().unwrap(), n[1].parse().unwrap(), n[2].parse().unwrap())
        }).collect() };
        let (labels, count) = speaker_attribution::assign(&bounds, &turns);
        let encoded = labels.iter().map(|label| {
            if label.voices.len() > 1 { format!("mix:{}", label.voices.iter().map(|n| n.to_string()).collect::<Vec<_>>().join(",")) }
            else { label.speaker.map(|n| n.to_string()).unwrap_or_else(|| "?".into()) }
        }).collect::<Vec<_>>().join(";");
        assert_eq!(encoded, c[3], "{}", c[0]);
        assert_eq!(count, c[4].parse::<usize>().unwrap(), "{}", c[0]);
    }
}

static NEXT: AtomicU64 = AtomicU64::new(0);
struct Temp(PathBuf);
impl Temp {
    fn new() -> Self {
        let p = std::env::temp_dir().join(format!("solflow-safeguards-{}-{}", std::process::id(), NEXT.fetch_add(1, Ordering::Relaxed)));
        fs::create_dir(&p).unwrap();
        Self(p)
    }
}
impl Drop for Temp { fn drop(&mut self) { let _ = fs::remove_dir_all(&self.0); } }

#[test]
fn shared_cleanup_fixtures() {
    for (i, line) in include_str!("../fixtures/cleanup.tsv").lines().enumerate() {
        let cols: Vec<_> = line.split('\t').collect();
        assert_eq!(cols.len(), 3, "line {}", i + 1);
        let expected = if cols[2] == "<EMPTY>" { "" } else { cols[2] };
        assert_eq!(cleanup::clean_with(cols[1], cols[0] == "1"), expected, "fixture {}", i + 1);
    }
}

#[test]
fn cancelling_or_dropping_attempt_preserves_complete_transcript_and_raw() {
    let tmp = Temp::new();
    fs::write(tmp.0.join("transcript.json"), b"old complete text").unwrap();
    fs::write(tmp.0.join("meta.json"), b"old metadata").unwrap();
    let version;
    {
        let draft = TranscriptDraft::begin(&tmp.0).unwrap();
        version = draft.version.clone();
        draft.checkpoint(b"partial", "raw including ООО".as_bytes()).unwrap();
        assert_eq!(read_visible(&tmp.0).unwrap(), b"old complete text");
    }
    assert_eq!(read_visible(&tmp.0).unwrap(), b"old complete text");
    assert_eq!(fs::read(version.join("raw-transcript.json")).unwrap(), "raw including ООО".as_bytes());
    assert_eq!(fs::read(version.join("previous/meta.json")).unwrap(), b"old metadata");
}

#[test]
fn first_attempt_partial_remains_readable_after_interruption() {
    let tmp = Temp::new();
    let draft = TranscriptDraft::begin(&tmp.0).unwrap();
    draft.checkpoint(b"partial", b"raw").unwrap();
    drop(draft);
    assert!(!tmp.0.join("transcript.json").exists());
    assert_eq!(read_visible(&tmp.0).unwrap(), b"partial");
}

#[test]
fn successful_replacement_keeps_previous_version_and_source() {
    let tmp = Temp::new();
    fs::write(tmp.0.join("transcript.json"), b"old").unwrap();
    let draft = TranscriptDraft::begin(&tmp.0).unwrap();
    draft.checkpoint(b"new complete", b"raw complete").unwrap();
    draft.commit().unwrap();
    assert_eq!(read_visible(&tmp.0).unwrap(), b"new complete");
    assert_eq!(fs::read(draft.version.join("previous/transcript.json")).unwrap(), b"old");
    assert_eq!(fs::read(draft.version.join("raw-transcript.json")).unwrap(), b"raw complete");
    assert!(!tmp.0.join("transcript.partial.json").exists());
}

#[test]
fn failed_checkpoint_does_not_replace_complete_text() {
    let tmp = Temp::new();
    fs::write(tmp.0.join("transcript.json"), b"old").unwrap();
    let draft = TranscriptDraft::begin(&tmp.0).unwrap();
    fs::create_dir(draft.version.join("transcript.json")).unwrap();
    assert!(draft.checkpoint(b"new", b"raw").is_err());
    assert_eq!(read_visible(&tmp.0).unwrap(), b"old");
    assert!(draft.commit().is_err());
    assert_eq!(read_visible(&tmp.0).unwrap(), b"old");
}

#[test]
fn failed_atomic_replacement_leaves_existing_destination() {
    let tmp = Temp::new();
    let target = tmp.0.join("transcript.json");
    fs::create_dir(&target).unwrap();
    fs::write(target.join("keep"), b"safe").unwrap();
    assert!(atomic_write(&target, b"new").is_err());
    assert_eq!(fs::read(target.join("keep")).unwrap(), b"safe");
    assert_eq!(fs::read_dir(&tmp.0).unwrap().count(), 1);
}

#[test]
fn absent_meeting_is_not_recreated_by_draft_or_atomic_write() {
    let tmp = Temp::new();
    let missing = tmp.0.join("deleted");
    assert!(TranscriptDraft::begin(&missing).is_err());
    assert!(atomic_write(&missing.join("meta.json"), b"{}").is_err());
    assert!(!missing.exists());
}
