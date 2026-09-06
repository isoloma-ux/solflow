//! Local transcription attempts. Canonical transcript.json stays compatible
//! with existing apps and sync; drafts and previous versions never sync.
use std::fs::{self, File, OpenOptions};
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

static NEXT_FILE: AtomicU64 = AtomicU64::new(0);

fn unique_id() -> String {
    format!("{}-{}-{}", std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH).unwrap_or_default().as_nanos(),
        std::process::id(), NEXT_FILE.fetch_add(1, Ordering::Relaxed))
}

/// A complete, flushed sibling replaces the destination in one rename.
/// Never truncate/unlink the previous file. A failure leaves it intact.
pub fn atomic_write(path: &Path, bytes: &[u8]) -> io::Result<()> {
    let parent = path.parent().ok_or_else(|| io::Error::other("missing parent"))?;
    let tmp = parent.join(format!(".solflow-{}.tmp", unique_id()));
    let result = (|| {
        let mut file = OpenOptions::new().write(true).create_new(true).open(&tmp)?;
        file.write_all(bytes)?;
        file.sync_all()?;
        drop(file);
        fs::rename(&tmp, path)?;
        // Directory fsync is not supported on every OS/filesystem. The
        // replacement already succeeded; don't report a failed transaction.
        #[cfg(unix)]
        if let Ok(dir) = File::open(parent) { let _ = dir.sync_all(); }
        Ok(())
    })();
    if result.is_err() { let _ = fs::remove_file(&tmp); }
    result
}

pub struct TranscriptDraft {
    meeting: PathBuf,
    pub version: PathBuf,
}

impl TranscriptDraft {
    pub fn begin(meeting: &Path) -> io::Result<Self> {
        if !meeting.is_dir() {
            return Err(io::Error::new(io::ErrorKind::NotFound, "meeting disappeared"));
        }
        let versions = meeting.join("transcript-versions");
        fs::create_dir_all(&versions)?;
        let version = versions.join(unique_id());
        fs::create_dir(&version)?;
        let previous = version.join("previous");
        fs::create_dir(&previous)?;
        // Snapshot before marking metadata as transcribing. Include derived
        // text to allow a future restore UI to recover the complete old view.
        for entry in fs::read_dir(meeting)? {
            let entry = entry?;
            let name = entry.file_name();
            if entry.file_type()?.is_file()
                && name.to_string_lossy().ends_with(".json") {
                atomic_write(&previous.join(&name), &fs::read(entry.path())?)?;
            }
        }
        Ok(Self { meeting: meeting.to_owned(), version })
    }

    pub fn checkpoint(&self, clean: &[u8], raw: &[u8]) -> io::Result<()> {
        // Raw recognition survives cleanup and later editing.
        atomic_write(&self.version.join("raw-transcript.json"), raw)?;
        atomic_write(&self.version.join("transcript.json"), clean)?;
        atomic_write(&self.meeting.join("transcript.partial.json"), clean)
    }

    /// Caller has checked cancellation and nonempty output. Keep both the
    /// new attempt and the old snapshot even after a successful replacement.
    pub fn commit(&self) -> io::Result<()> {
        let bytes = fs::read(self.version.join("transcript.json"))?;
        atomic_write(&self.meeting.join("transcript.json"), &bytes)?;
        let _ = fs::remove_file(self.meeting.join("transcript.partial.json"));
        Ok(())
    }
}

/// An unfinished first attempt can still be read. A previous canonical
/// transcript always takes precedence; I/O/corruption isn't a missing file.
pub fn read_visible(meeting: &Path) -> io::Result<Vec<u8>> {
    match fs::read(meeting.join("transcript.json")) {
        Err(e) if e.kind() == io::ErrorKind::NotFound =>
            fs::read(meeting.join("transcript.partial.json")),
        other => other,
    }
}
