//! Permission-filtered text export shared by the desktop app and its MCP reader.
//! The server receives only this export directory, never the application data root.
pub mod access;
use anyhow::{bail, Context, Result};
use rusqlite::{params, Connection, OpenFlags};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeMap,
    fs,
    io::Write,
    path::{Path, PathBuf},
    time::{Duration, SystemTime, UNIX_EPOCH},
};

const MAX_JSON: u64 = 64 * 1024 * 1024;
const SCHEMA: i64 = 1;

#[derive(Clone, Debug, Default, Deserialize, Serialize, PartialEq)]
#[serde(default, deny_unknown_fields)]
pub struct Grant {
    pub transcript: bool,
    pub summary: bool,
    pub map: bool,
    pub analyses: bool,
}
impl Grant {
    pub fn enabled(&self) -> bool {
        self.transcript || self.summary || self.map || self.analyses
    }
    fn allows(&self, kind: &str) -> bool {
        match kind {
            "transcript" => self.transcript,
            "summary" => self.summary,
            "map" => self.map,
            "analyses" => self.analyses,
            _ => false,
        }
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct Project {
    pub id: String,
    pub name: String,
}
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct Document {
    pub id: String,
    pub project_id: String,
    pub recording_id: String,
    pub title: String,
    pub at: i64,
    pub kind: String,
    pub revision: String,
    pub start: Option<f64>,
    pub end: Option<f64>,
    pub speaker: Option<String>,
    pub text: String,
    pub url: String,
    pub generated: bool,
}
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct Snapshot {
    pub schema: i64,
    pub updated_at: i64,
    pub projects: Vec<Project>,
    pub documents: Vec<Document>,
}

pub fn now_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as i64
}
fn hash(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}
fn valid_id(id: &str) -> bool {
    !id.is_empty() && id.len() <= 24 && id.bytes().all(|c| c.is_ascii_digit())
}

/// Reject existing symlink components. On Windows, reject reparse points too.
/// Never accept paths supplied by MCP callers.
pub fn checked_path(path: &Path) -> Result<PathBuf> {
    if !path.is_absolute() {
        bail!("absolute path required");
    }
    let mut current = PathBuf::new();
    for part in path.components() {
        match part {
            std::path::Component::ParentDir | std::path::Component::CurDir => bail!("unsafe path"),
            // A Windows prefix (for example \\?\C:) is not a filesystem
            // object until RootDir is appended. Querying it alone fails with
            // ERROR_INVALID_FUNCTION; inspect the rooted path next instead.
            std::path::Component::Prefix(_) => {
                current.push(part.as_os_str());
                continue;
            }
            _ => current.push(part.as_os_str()),
        }
        match fs::symlink_metadata(&current) {
            Ok(m) if m.file_type().is_symlink() => bail!("symlink is not allowed"),
            Ok(meta) => {
                #[cfg(windows)]
                {
                    use std::os::windows::fs::MetadataExt;
                    if meta.file_attributes() & 0x400 != 0 {
                        bail!("reparse point is not allowed");
                    }
                }
                #[cfg(not(windows))]
                let _ = meta;
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(e) => return Err(e.into()),
        }
    }
    Ok(path.to_owned())
}
fn read_json(path: &Path) -> Result<Vec<u8>> {
    checked_path(path)?;
    let m = fs::metadata(path)?;
    if !m.is_file() || m.len() > MAX_JSON {
        bail!("invalid or oversized document");
    }
    Ok(fs::read(path)?)
}
fn private_dir(path: &Path) -> Result<()> {
    checked_path(path)?;
    fs::create_dir_all(path)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(path, fs::Permissions::from_mode(0o700))?;
    }
    Ok(())
}
fn atomic_file(path: &Path, data: &[u8]) -> Result<()> {
    checked_path(path)?;
    let tmp = path.with_extension(format!("{}-{}.tmp", std::process::id(), now_ms()));
    let mut options = fs::OpenOptions::new();
    options.create_new(true).write(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let result = (|| -> Result<()> {
        let mut file = options.open(&tmp)?;
        file.write_all(data)?;
        file.sync_all()?;
        drop(file);
        fs::rename(&tmp, path)?;
        Ok(())
    })();
    if result.is_err() {
        let _ = fs::remove_file(tmp);
    }
    result
}

pub struct Store {
    conn: Connection,
    root: PathBuf,
}
impl Store {
    pub fn create(root: &Path) -> Result<Self> {
        private_dir(root)?;
        let db = checked_path(&root.join("export.sqlite3"))?;
        let conn = Connection::open(db)?;
        conn.busy_timeout(Duration::from_secs(10))?;
        conn.execute_batch("PRAGMA journal_mode=DELETE; PRAGMA synchronous=FULL;
            CREATE TABLE IF NOT EXISTS grants (id TEXT PRIMARY KEY, value TEXT NOT NULL);
            CREATE TABLE IF NOT EXISTS state (key TEXT PRIMARY KEY, value TEXT NOT NULL);
            CREATE TABLE IF NOT EXISTS projects (id TEXT PRIMARY KEY, value TEXT NOT NULL);
            CREATE TABLE IF NOT EXISTS documents (id TEXT PRIMARY KEY, project TEXT NOT NULL, recording TEXT NOT NULL, kind TEXT NOT NULL, value TEXT NOT NULL);
            CREATE VIRTUAL TABLE IF NOT EXISTS search_index USING fts5(id UNINDEXED, body, tokenize='unicode61');")?;
        let version: Option<String> = conn
            .query_row("SELECT value FROM state WHERE key='schema'", [], |r| {
                r.get(0)
            })
            .ok();
        if version.as_deref().is_some_and(|v| v != "1") {
            bail!("unsupported export schema");
        }
        conn.execute("INSERT OR IGNORE INTO state VALUES ('schema', '1')", [])?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(
                root.join("export.sqlite3"),
                fs::Permissions::from_mode(0o600),
            )?;
        }
        Ok(Self {
            conn,
            root: root.to_owned(),
        })
    }
    pub fn reader(root: &Path) -> Result<Self> {
        let db = checked_path(&root.join("export.sqlite3"))?;
        let conn = Connection::open_with_flags(
            db,
            OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX,
        )?;
        conn.busy_timeout(Duration::from_secs(10))?;
        let schema: String =
            conn.query_row("SELECT value FROM state WHERE key='schema'", [], |r| {
                r.get(0)
            })?;
        if schema != "1" {
            bail!("unsupported export schema");
        }
        // Pin one snapshot for each tool invocation. A fresh reader is opened
        // for every call; no cached permission or document survives revocation.
        conn.execute_batch("BEGIN DEFERRED")?;
        Ok(Self {
            conn,
            root: root.to_owned(),
        })
    }
    pub fn grants(&self) -> Result<BTreeMap<String, Grant>> {
        let mut stmt = self
            .conn
            .prepare("SELECT id,value FROM grants ORDER BY id")?;
        let rows = stmt.query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)))?;
        let mut out = BTreeMap::new();
        for row in rows {
            let (id, raw) = row?;
            out.insert(id, serde_json::from_str(&raw)?);
        }
        Ok(out)
    }
    pub fn set_grant(&mut self, id: &str, grant: &Grant) -> Result<()> {
        if !valid_id(id) {
            bail!("invalid project id");
        }
        let tx = self.conn.transaction()?;
        tx.execute(
            "DELETE FROM search_index WHERE id IN (SELECT id FROM documents WHERE project=?1)",
            [id],
        )?;
        tx.execute("DELETE FROM documents WHERE project=?1", [id])?;
        tx.execute("DELETE FROM projects WHERE id=?1", [id])?;
        tx.execute(
            "INSERT OR REPLACE INTO grants VALUES (?1,?2)",
            params![id, serde_json::to_string(grant)?],
        )?;
        tx.execute("INSERT OR REPLACE INTO state VALUES ('stale','true')", [])?;
        tx.commit()?;
        Ok(())
    }
    pub fn invalidate(&mut self) -> Result<()> {
        let tx = self.conn.transaction()?;
        tx.execute_batch(
            "DELETE FROM documents; DELETE FROM search_index; DELETE FROM projects;
            INSERT OR REPLACE INTO state VALUES ('stale','true');",
        )?;
        tx.commit()?;
        Ok(())
    }
    pub fn status(&self) -> Result<Value> {
        let updated: String = self
            .conn
            .query_row("SELECT value FROM state WHERE key='updated_at'", [], |r| {
                r.get(0)
            })
            .unwrap_or_else(|_| "0".into());
        let stale: String = self
            .conn
            .query_row("SELECT value FROM state WHERE key='stale'", [], |r| {
                r.get(0)
            })
            .unwrap_or_else(|_| "true".into());
        Ok(
            json!({"updated_at": updated.parse::<i64>().unwrap_or(0), "stale": stale != "false",
            "documents": self.conn.query_row("SELECT count(*) FROM documents", [], |r| r.get::<_,i64>(0))?,
            "grants": self.grants()?}),
        )
    }
    pub fn publish(&mut self, snapshot: &Snapshot) -> Result<()> {
        if snapshot.schema != SCHEMA {
            bail!("unsupported snapshot schema");
        }
        let grants = self.grants()?;
        // Only content allowed at commit time can enter the searchable export.
        let documents: Vec<_> = snapshot
            .documents
            .iter()
            .filter(|d| grants.get(&d.project_id).is_some_and(|g| g.allows(&d.kind)))
            .collect();
        let projects: Vec<_> = snapshot
            .projects
            .iter()
            .filter(|p| grants.get(&p.id).is_some_and(Grant::enabled))
            .collect();
        let tx = self.conn.transaction()?;
        tx.execute_batch("DELETE FROM documents; DELETE FROM search_index; DELETE FROM projects;")?;
        for p in &projects {
            tx.execute(
                "INSERT INTO projects VALUES (?1,?2)",
                params![p.id, serde_json::to_string(p)?],
            )?;
        }
        for d in &documents {
            if !projects.iter().any(|p| p.id == d.project_id) {
                bail!("orphan document");
            }
            tx.execute(
                "INSERT INTO documents VALUES (?1,?2,?3,?4,?5)",
                params![
                    d.id,
                    d.project_id,
                    d.recording_id,
                    d.kind,
                    serde_json::to_string(d)?
                ],
            )?;
            tx.execute(
                "INSERT INTO search_index VALUES (?1,?2)",
                params![
                    d.id,
                    format!(
                        "{} {} {} {}",
                        d.title,
                        d.at,
                        d.speaker.as_deref().unwrap_or(""),
                        d.text
                    )
                ],
            )?;
        }
        tx.execute(
            "INSERT OR REPLACE INTO state VALUES ('updated_at',?1)",
            [snapshot.updated_at.to_string()],
        )?;
        tx.execute("INSERT OR REPLACE INTO state VALUES ('stale','false')", [])?;
        tx.commit()?;
        Ok(())
    }
    /// Explicit text-copy export. Historical copies are not MCP permissions and
    /// cannot be remotely recalled; the UI explains this before creating one.
    pub fn text_copy(&self) -> Result<PathBuf> {
        let projects = self.projects()?;
        let documents = self.documents()?;
        let bytes = serde_json::to_vec(&documents)?;
        let generation = hash(&bytes);
        let folder = self.root.join("copies").join(&generation);
        private_dir(&folder)?;
        for d in &documents {
            let name = format!("{}.md", hash(d.id.as_bytes()));
            let text = format!("# {}\n\nProject: {} | Recording: {} | Revision: {}\n\nTime: {:?}–{:?} s | Speaker: {}\n\n{}\n", d.title,d.project_id,d.recording_id,d.revision,d.start,d.end,d.speaker.as_deref().unwrap_or("—"),d.text);
            atomic_file(&folder.join(name), text.as_bytes())?;
        }
        atomic_file(
            &folder.join("manifest.json"),
            &serde_json::to_vec_pretty(
                &json!({"schema":SCHEMA,"generation":generation,"projects":projects,"documents":documents.iter().map(|d| json!({"id":d.id,"file":format!("{}.md",hash(d.id.as_bytes()))})).collect::<Vec<_>>()}),
            )?,
        )?;
        Ok(folder)
    }
    pub fn projects(&self) -> Result<Vec<Project>> {
        let grants = self.grants()?;
        let mut stmt = self
            .conn
            .prepare("SELECT value FROM projects ORDER BY id")?;
        let rows = stmt.query_map([], |r| r.get::<_, String>(0))?;
        let mut out = vec![];
        for row in rows {
            let p: Project = serde_json::from_str(&row?)?;
            if grants.get(&p.id).is_some_and(Grant::enabled) {
                out.push(p);
            }
        }
        Ok(out)
    }
    fn documents(&self) -> Result<Vec<Document>> {
        let grants = self.grants()?;
        let mut stmt = self
            .conn
            .prepare("SELECT value FROM documents ORDER BY recording,id")?;
        let rows = stmt.query_map([], |r| r.get::<_, String>(0))?;
        let mut out = vec![];
        for row in rows {
            let d: Document = serde_json::from_str(&row?)?;
            if grants.get(&d.project_id).is_some_and(|g| g.allows(&d.kind)) {
                out.push(d);
            }
        }
        Ok(out)
    }
    pub fn fetch(&self, id: &str, revision: Option<&str>) -> Result<Value> {
        if id.len() > 180 {
            bail!("document unavailable");
        }
        let raw: String = self
            .conn
            .query_row("SELECT value FROM documents WHERE id=?1", [id], |r| {
                r.get(0)
            })
            .context("document unavailable")?;
        let d: Document = serde_json::from_str(&raw)?;
        if !self
            .grants()?
            .get(&d.project_id)
            .is_some_and(|g| g.allows(&d.kind))
        {
            bail!("document unavailable");
        }
        Ok(json!({"document":d,"revision_changed":revision.is_some_and(|r|r!=d.revision)}))
    }
    pub fn recordings(&self, project: &str, offset: usize, limit: usize) -> Result<Value> {
        if !self.projects()?.iter().any(|p| p.id == project) {
            bail!("project unavailable");
        }
        let mut recordings = BTreeMap::new();
        for d in self
            .documents()?
            .into_iter()
            .filter(|d| d.project_id == project)
        {
            recordings.entry(d.recording_id.clone()).or_insert(
                json!({"id":d.recording_id,"title":d.title,"at":d.at,"revision":d.revision}),
            );
        }
        let total = recordings.len();
        let count = limit.clamp(1, 100);
        Ok(
            json!({"recordings":recordings.values().skip(offset).take(count).collect::<Vec<_>>(),"total":total,
            "next_offset":if offset.saturating_add(count)<total {Some(offset+count)}else{None}}),
        )
    }
    pub fn overview(&self, project: &str) -> Result<Value> {
        let p = self
            .projects()?
            .into_iter()
            .find(|p| p.id == project)
            .context("project unavailable")?;
        Ok(
            json!({"project":p,"recordings":self.recordings(project,0,100)?,"status":self.public_status()?}),
        )
    }
    pub fn public_status(&self) -> Result<Value> {
        let s = self.status()?;
        Ok(json!({"updated_at":s["updated_at"],"stale":s["stale"]}))
    }
    pub fn search(&self, query: &str, project: Option<&str>, limit: usize) -> Result<Value> {
        if query.len() > 1024 {
            bail!("query too long");
        }
        let terms: Vec<_> = query
            .split(|c: char| !c.is_alphanumeric())
            .filter(|s| !s.is_empty())
            .take(24)
            .collect();
        if terms.is_empty() {
            bail!("enter words to search");
        }
        // Prefix matching is explicit; no claim of full Russian morphology.
        let trimmed = query.trim();
        let expr = if trimmed.starts_with('"') && trimmed.ends_with('"') {
            format!("\"{}\"", terms.join(" "))
        } else {
            terms
                .iter()
                .map(|s| format!("\"{}\"*", s))
                .collect::<Vec<_>>()
                .join(" AND ")
        };
        let grants = self.grants()?;
        let mut stmt = self.conn.prepare("SELECT d.value FROM search_index s JOIN documents d ON d.id=s.id WHERE search_index MATCH ?1 AND (?2 IS NULL OR d.project=?2) ORDER BY rank LIMIT ?3")?;
        let rows = stmt.query_map(params![expr, project, limit.clamp(1, 50) as i64], |r| {
            r.get::<_, String>(0)
        })?;
        let mut results = vec![];
        for row in rows {
            let d: Document = serde_json::from_str(&row?)?;
            if grants.get(&d.project_id).is_some_and(|g| g.allows(&d.kind)) {
                results.push(json!({"id":d.id,"title":d.title,"url":d.url,"project_id":d.project_id,"recording_id":d.recording_id,"revision":d.revision,"at":d.at,"start":d.start,"end":d.end,"speaker":d.speaker,"kind":d.kind,"generated":d.generated,"text":d.text}));
            }
        }
        Ok(json!({"results":results,"status":self.public_status()?}))
    }
}

/// Strict allowlisted reads, finished transcripts only. A changing source aborts
/// the refresh and leaves the export invalidated rather than publishing a mix.
pub fn collect(source: &Path, grants: &BTreeMap<String, Grant>) -> Result<Snapshot> {
    checked_path(source)?;
    let mut snapshot = Snapshot {
        schema: SCHEMA,
        updated_at: now_ms(),
        ..Default::default()
    };
    if !grants.values().any(Grant::enabled) {
        return Ok(snapshot);
    }
    let project_path = source.join("projects.json");
    let project_bytes = read_json(&project_path)?;
    let projects: Vec<Project> = serde_json::from_slice(&project_bytes)?;
    snapshot.projects = projects
        .into_iter()
        .filter(|p| valid_id(&p.id) && grants.get(&p.id).is_some_and(Grant::enabled))
        .collect();
    let root = source.join("meetings");
    checked_path(&root)?;
    if !root.exists() {
        return Ok(snapshot);
    }
    for entry in fs::read_dir(root)? {
        let entry = entry?;
        let id = entry.file_name().to_string_lossy().to_string();
        if !valid_id(&id) {
            continue;
        }
        if !entry.file_type()?.is_dir() {
            continue;
        }
        let meta_path = entry.path().join("meta.json");
        let meta_bytes = read_json(&meta_path)?;
        let meta: Value = serde_json::from_slice(&meta_bytes)?;
        let project = meta["project"].as_str().unwrap_or("");
        let Some(grant) = grants.get(project).filter(|g| g.enabled()) else {
            continue;
        };
        if !snapshot.projects.iter().any(|p| p.id == project) || meta["state"] != "done" {
            continue;
        }
        let transcript_path = entry.path().join("transcript.json");
        let transcript_bytes = read_json(&transcript_path)?;
        let segments: Vec<Value> = serde_json::from_slice(&transcript_bytes)?;
        if segments.is_empty() {
            continue;
        }
        let mut revision_bytes = meta_bytes.clone();
        revision_bytes.extend_from_slice(&transcript_bytes);
        let extras_path = entry.path().join("extras.json");
        let extras_bytes = if grant.analyses && extras_path.exists() {
            Some(read_json(&extras_path)?)
        } else {
            None
        };
        if let Some(bytes) = &extras_bytes {
            revision_bytes.extend_from_slice(bytes);
        }
        let revision = hash(&revision_bytes);
        let base = Document {
            id: String::new(),
            project_id: project.into(),
            recording_id: id.clone(),
            title: meta["title"].as_str().unwrap_or("Recording").into(),
            at: meta["at"].as_i64().unwrap_or(0),
            kind: String::new(),
            revision,
            start: None,
            end: None,
            speaker: None,
            text: String::new(),
            url: format!("solflow://recording/{id}"),
            generated: false,
        };
        if grant.transcript {
            for (i, s) in segments.iter().enumerate() {
                let text = s["text"].as_str().unwrap_or("");
                if text.trim().is_empty() {
                    continue;
                }
                let start = s["s"].as_f64().context("missing timestamp")?;
                let end = s["e"].as_f64().context("missing timestamp")?;
                if !start.is_finite() || !end.is_finite() || start < 0.0 || end < start {
                    bail!("invalid timestamp");
                }
                let speaker = s["spk"].as_u64().map(|n| {
                    meta["names"][n.to_string()]
                        .as_str()
                        .map(str::to_owned)
                        .unwrap_or_else(|| format!("Speaker {n}"))
                });
                for (part, chunk) in chunks(text).into_iter().enumerate() {
                    snapshot.documents.push(Document {
                        id: format!("{id}:transcript:{i}:{part}"),
                        kind: "transcript".into(),
                        start: Some(start),
                        end: Some(end),
                        speaker: speaker.clone(),
                        text: chunk,
                        url: format!("solflow://recording/{id}?t={start}"),
                        ..base.clone()
                    });
                }
            }
        }
        let mut derived = Vec::new();
        if grant.summary {
            if let Some(text) = meta["summary"].as_str().filter(|s| !s.trim().is_empty()) {
                derived.push(("summary", text.to_owned()));
            }
        }
        if grant.map && meta.get("mindmap").is_some_and(|v| !v.is_null()) {
            derived.push(("map", serde_json::to_string_pretty(&meta["mindmap"])?));
        }
        if let Some(bytes) = &extras_bytes {
            let extras: Value = serde_json::from_slice(bytes)?;
            if let Some(items) = extras["items"].as_object() {
                for (key, value) in items {
                    if let Some(text) = value.as_str() {
                        derived.push(("analyses", format!("{key}\n{text}")));
                    }
                }
            }
        }
        for (index, (kind, text)) in derived.into_iter().enumerate() {
            for (part, chunk) in chunks(&text).into_iter().enumerate() {
                snapshot.documents.push(Document {
                    id: format!("{id}:{kind}:{index}:{part}"),
                    kind: kind.into(),
                    text: chunk,
                    generated: true,
                    ..base.clone()
                });
            }
        }
        if read_json(&meta_path)? != meta_bytes || read_json(&transcript_path)? != transcript_bytes
        {
            bail!("source changed; retry refresh");
        }
        if let Some(bytes) = extras_bytes {
            if read_json(&extras_path)? != bytes {
                bail!("source changed; retry refresh");
            }
        }
    }
    if read_json(&project_path)? != project_bytes {
        bail!("projects changed; retry refresh");
    }
    Ok(snapshot)
}
fn chunks(text: &str) -> Vec<String> {
    let chars: Vec<char> = text.chars().collect();
    chars.chunks(2400).map(|c| c.iter().collect()).collect()
}
