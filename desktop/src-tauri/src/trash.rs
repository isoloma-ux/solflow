//! Shared reversible archive. Legacy clients ignore the nonnumeric trash-v1 prefix.
use crate::sync::provider::{Folder, Provider};
use anyhow::{bail, Result};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::{
    fs,
    path::{Path, PathBuf},
};
use tauri::AppHandle;

#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Archive {
    pub schema: u32,
    pub id: String,
    pub deleted_at: i64,
    pub meta: Value,
    pub transcript: Value,
    pub audio_md5: Option<String>,
    pub audio_size: Option<u64>,
}
#[derive(Serialize)]
pub struct Row {
    pub id: i64,
    pub title: String,
    pub deleted_at: i64,
    pub audio: bool,
    pub restoring: bool,
    pub clearing: bool,
    pub scope: String,
    pub project: String,
}
pub fn valid_id(id: &str) -> bool {
    !id.is_empty()
        && id.len() <= 19
        && id.bytes().all(|b| b.is_ascii_digit())
        && id
            .parse::<i64>()
            .is_ok_and(|n| n > 0 && n.to_string() == id)
}
pub fn restore_id(id: &str) -> i64 {
    let digest = Sha256::digest(format!("solflow-trash-restore-v1:{id}").as_bytes());
    4_000_000_000_000_000
        + digest[..6]
            .iter()
            .fold(0i64, |v, b| (v << 8) | i64::from(*b))
}
pub fn decode(bytes: &[u8]) -> Result<Archive> {
    if bytes.len() > 20_000_000 {
        bail!("Trash archive too large")
    }
    let a: Archive = serde_json::from_slice(bytes)?;
    if a.schema != 1
        || !valid_id(&a.id)
        || a.deleted_at <= 0
        || !a.meta.is_object()
        || !a.transcript.is_array()
        || a.audio_md5.is_some() != a.audio_size.is_some()
        || a.audio_md5
            .as_ref()
            .is_some_and(|s| s.len() != 32 || !s.bytes().all(|b| b.is_ascii_hexdigit()))
    {
        bail!("Invalid trash archive")
    }
    Ok(a)
}
pub fn root(app: &AppHandle) -> PathBuf {
    let status = crate::sync::status(app);
    let scope = if status.connected {
        format!("{}:{}", status.provider, status.login)
    } else {
        "local".into()
    };
    crate::sync::data_dir()
        .join("trash-v1")
        .join(format!("{:x}", Sha256::digest(scope.as_bytes())))
}
fn atomic(path: &Path, data: &[u8]) -> Result<()> {
    fs::create_dir_all(
        path.parent()
            .ok_or_else(|| anyhow::anyhow!("Invalid trash path"))?,
    )?;
    use std::io::Write;
    let temp = path.with_extension(format!("{}.tmp", std::process::id()));
    let mut file = fs::File::create(&temp)?;
    file.write_all(data)?;
    file.sync_all()?;
    drop(file);
    fs::rename(temp, path)?;
    Ok(())
}
fn archive_name(id: &str) -> String {
    format!("trash-v1-{id}.json")
}
fn restored_name(id: &str) -> String {
    format!("trash-v1-{id}.restored.json")
}
fn audio_name(id: &str) -> String {
    format!("trash-v1-{id}.wav")
}
// Permanent markers outlive payloads. They are scoped to the connected account.
fn purge_name(id: &str) -> String {
    format!("trash-v1-{id}.purged.json")
}
fn purge_bytes(id: &str) -> Vec<u8> {
    format!("{{\"id\":\"{id}\",\"schema\":1}}").into_bytes()
}
fn purged_at(base: &Path, id: &str) -> bool {
    base.join("purged").join(format!("{id}.json")).exists()
}
pub fn purged(app: &AppHandle, id: i64) -> bool {
    purged_at(&root(app), &id.to_string())
}

pub fn request_clear(app: &AppHandle, ids: Vec<i64>, scope: String) -> Result<usize> {
    let base = root(app);
    if base.to_string_lossy() != scope {
        bail!("Cloud account changed; reopen trash")
    }
    let mut count = 0;
    for id in ids {
        if id <= 0 {
            bail!("Invalid recording ID")
        }
        let dir = base.join(id.to_string());
        if !dir.join("archive.json").exists()
            || dir.join("restored.json").exists()
            || dir.join("restore-request").exists()
        {
            continue;
        }
        atomic(&dir.join("purge-request"), b"1")?;
        if !crate::sync::status(app).connected {
            atomic(
                &base.join("purged").join(format!("{id}.json")),
                &purge_bytes(&id.to_string()),
            )?;
            fs::remove_dir_all(dir)?;
        }
        count += 1;
    }
    crate::sync::touch(app);
    crate::meetings::notify(app);
    Ok(count)
}

/// Repeated before and after normal sync. Verify the durable marker before removing any payload.
fn purge_at(base: &Path, cloud: &dyn Provider, token: &str) -> Result<()> {
    let listing = cloud.list(token, Folder::Meetings)?;
    let mut ids = std::collections::BTreeSet::new();
    for item in &listing {
        if let Some(id) = item
            .name
            .strip_prefix("trash-v1-")
            .and_then(|n| n.strip_suffix(".purged.json"))
        {
            if !valid_id(id) {
                continue;
            }
            let expected = purge_bytes(id);
            if item.md5 != format!("{:x}", md5::compute(&expected))
                || item.size != expected.len() as u64
            {
                bail!("Invalid permanent deletion marker")
            }
            atomic(
                &base.join("purged").join(format!("{id}.json")),
                &purge_bytes(id),
            )?;
            ids.insert(id.to_owned());
        }
    }
    for entry in fs::read_dir(base).into_iter().flatten().flatten() {
        let dir = entry.path();
        let id = entry.file_name().to_string_lossy().into_owned();
        if !valid_id(&id) || !dir.join("purge-request").exists() {
            continue;
        }
        // A restore completed on another device while confirmation was open: preserve it.
        if !purged_at(base, &id)
            && (listing.iter().any(|f| {
                f.name == restored_name(&id) || f.name == format!("{}.meta.json", restore_id(&id))
            }) || dir.join("restore-request").exists())
        {
            fs::remove_file(dir.join("purge-request"))?;
            continue;
        }
        put_checked(
            cloud,
            token,
            Folder::Meetings,
            &purge_name(&id),
            &purge_bytes(&id),
        )?;
        atomic(
            &base.join("purged").join(format!("{id}.json")),
            &purge_bytes(&id),
        )?;
        ids.insert(id);
    }
    // Remembered markers also repair a cloud where an old client reuploaded an archive.
    for entry in fs::read_dir(base.join("purged"))
        .into_iter()
        .flatten()
        .flatten()
    {
        if let Some(id) = entry.path().file_stem().and_then(|n| n.to_str()) {
            if valid_id(id) {
                ids.insert(id.to_owned());
            }
        }
    }
    let audio_listing = if ids.is_empty() {
        Vec::new()
    } else {
        cloud.list(token, Folder::Audio)?
    };
    for id in ids {
        if !listing.iter().any(|f| f.name == purge_name(&id)) {
            put_checked(
                cloud,
                token,
                Folder::Meetings,
                &purge_name(&id),
                &purge_bytes(&id),
            )?;
        }
        // Existing legacy tombstones may contain additional fields. Their presence is sufficient.
        if !listing.iter().any(|f| f.name == format!("{id}.deleted")) {
            put_checked(
                cloud,
                token,
                Folder::Meetings,
                &format!("{id}.deleted"),
                b"{}",
            )?;
        }
        for name in [
            archive_name(&id),
            format!("{id}.meta.json"),
            format!("{id}.transcript.json"),
        ] {
            if listing.iter().any(|f| f.name == name) {
                cloud.delete(token, Folder::Meetings, &name)?;
            }
        }
        for name in [audio_name(&id), format!("{id}.wav")] {
            if audio_listing.iter().any(|f| f.name == name) {
                cloud.delete(token, Folder::Audio, &name)?;
            }
        }
        let dir = base.join(&id);
        if dir.exists() {
            fs::remove_dir_all(dir)?;
        }
    }
    Ok(())
}
pub fn sync_purges(app: &AppHandle, cloud: &dyn Provider, token: &str) -> Result<()> {
    purge_at(&root(app), cloud, token)
}

fn file_md5(path: &Path) -> Result<String> {
    use std::io::Read;
    let mut file = fs::File::open(path)?;
    let mut hash = md5::Context::new();
    let mut block = [0u8; 65536];
    loop {
        let n = file.read(&mut block)?;
        if n == 0 {
            break;
        }
        hash.consume(&block[..n]);
    }
    Ok(format!("{:x}", hash.compute()))
}
/// Move before publishing deletion. No source directory is destroyed.
pub fn retain(app: &AppHandle, id: i64, outgoing: bool) -> Result<()> {
    if id <= 0 {
        bail!("Invalid recording ID")
    }
    let source = crate::meetings::dir(app, id);
    let target = root(app).join(id.to_string());
    if purged(app, id) {
        if source.exists() {
            fs::remove_dir_all(source)?;
        }
        return Ok(());
    }
    if source.exists() {
        let meta: Value = serde_json::from_slice(&fs::read(source.join("meta.json"))?)?;
        let transcript = if source.join("transcript.json").exists() {
            serde_json::from_slice(&fs::read(source.join("transcript.json"))?)?
        } else {
            serde_json::json!([])
        };
        let audio = source.join("audio.wav");
        let a = Archive {
            schema: 1,
            id: id.to_string(),
            deleted_at: crate::sync::provider::now_ms(),
            meta,
            transcript,
            audio_md5: if audio.exists() {
                Some(file_md5(&audio)?)
            } else {
                None
            },
            audio_size: fs::metadata(&audio).ok().map(|m| m.len()),
        };
        let bytes = serde_json::to_vec(&a)?;
        decode(&bytes)?;
        fs::create_dir_all(&target)?;
        if target.join("local-original").exists() {
            bail!("Trash contains another local copy; original retained")
        }
        if !target.join("archive.json").exists() {
            atomic(&target.join("archive.json"), &bytes)?;
        }
        fs::rename(&source, target.join("local-original"))?;
    }
    if outgoing {
        if !target.join("archive.json").exists() {
            bail!("No recording to archive")
        }
        atomic(&target.join("outgoing"), b"1")?;
    }
    Ok(())
}
fn local_audio(dir: &Path) -> PathBuf {
    let audio = dir.join("audio.wav");
    if audio.exists() {
        audio
    } else {
        dir.join("local-original/audio.wav")
    }
}

fn matches_audio(path: &Path, archive: &Archive) -> bool {
    match (&archive.audio_md5, archive.audio_size) {
        (Some(hash), Some(size)) => {
            fs::metadata(path).is_ok_and(|m| m.len() == size)
                && file_md5(path).is_ok_and(|actual| actual == *hash)
        }
        _ => false,
    }
}
fn ensure_archive_audio(
    dir: &Path,
    archive: &Archive,
    cloud: &dyn Provider,
    token: &str,
) -> Result<()> {
    if archive.audio_md5.is_none() || matches_audio(&local_audio(dir), archive) {
        return Ok(());
    }
    let temp = dir.join("audio.download");
    cloud.download_file(token, Folder::Audio, &audio_name(&archive.id), &temp)?;
    if !matches_audio(&temp, archive) {
        bail!("Trash audio checksum mismatch")
    }
    // The original remains in local-original. Only the verified cloud cache is replaced.
    fs::rename(temp, dir.join("audio.wav"))?;
    Ok(())
}

pub fn rows(app: &AppHandle) -> Vec<Row> {
    let mut rows = Vec::new();
    for e in fs::read_dir(root(app)).into_iter().flatten().flatten() {
        let p = e.path();
        if p.join("restored.json").exists()
            || p.file_name()
                .and_then(|n| n.to_str())
                .is_some_and(|id| purged_at(&root(app), id))
        {
            continue;
        }
        if let Ok(a) = fs::read(p.join("archive.json"))
            .map_err(anyhow::Error::from)
            .and_then(|b| decode(&b))
        {
            rows.push(Row {
                id: a.id.parse().unwrap(),
                title: a.meta["title"].as_str().unwrap_or("Запись").to_owned(),
                deleted_at: a.deleted_at,
                audio: a.audio_md5.is_some(),
                restoring: p.join("restore-request").exists(),
                clearing: p.join("purge-request").exists(),
                scope: root(app).to_string_lossy().into_owned(),
                project: a.meta["project"].as_str().unwrap_or("").to_owned(),
            });
        }
    }
    rows.sort_by_key(|r| std::cmp::Reverse(r.deleted_at));
    rows
}
pub fn pending(app: &AppHandle) -> Vec<i64> {
    fs::read_dir(root(app))
        .into_iter()
        .flatten()
        .flatten()
        .filter_map(|e| {
            let p = e.path();
            if p.join("outgoing").exists() && !p.join("deleted-sent").exists() {
                p.file_name()?.to_str()?.parse().ok()
            } else {
                None
            }
        })
        .collect()
}
pub fn deleted_sent(app: &AppHandle, id: i64) -> Result<()> {
    let path = root(app).join(id.to_string());
    if path.exists() {
        atomic(&path.join("deleted-sent"), b"1")?;
    }
    Ok(())
}
pub fn request_restore(app: &AppHandle, id: i64) -> Result<i64> {
    if crate::meetings::busy_ids(app).contains(&id) {
        bail!("Recording is busy")
    }
    let path = root(app).join(id.to_string());
    if purged(app, id) || path.join("purge-request").exists() {
        bail!("Recording is being permanently deleted")
    }
    let manifest = if path.join("cloud-archive.json").exists() {
        path.join("cloud-archive.json")
    } else {
        path.join("archive.json")
    };
    let a = decode(&fs::read(manifest)?)?;
    let new_id = restore_id(&a.id);
    let target = crate::meetings::dir(app, new_id);
    if !target.exists() {
        let staging = root(app).join(format!("restore-{new_id}"));
        fs::create_dir_all(&staging)?;
        let mut meta = a.meta.clone();
        meta["updated"] = serde_json::json!(a.deleted_at);
        meta["restored_from"] = serde_json::json!(a.id);
        atomic(&staging.join("meta.json"), &serde_json::to_vec(&meta)?)?;
        atomic(
            &staging.join("transcript.json"),
            &serde_json::to_vec(&a.transcript)?,
        )?;
        let audio = local_audio(&path);
        if audio.exists() && (a.audio_md5.is_none() || matches_audio(&audio, &a)) {
            fs::copy(audio, staging.join("audio.wav"))?;
        } else if staging.join("audio.wav").exists() {
            fs::remove_file(staging.join("audio.wav"))?;
        }
        atomic(&path.join("restore-request"), new_id.to_string().as_bytes())?;
        fs::create_dir_all(target.parent().unwrap())?;
        fs::rename(staging, target)?;
    } else {
        if !path.join("restore-request").exists() && !path.join("restored.json").exists() {
            bail!("Restore identity collision; existing recording preserved")
        }
    }
    atomic(&path.join("restore-request"), new_id.to_string().as_bytes())?;
    if !crate::sync::status(app).connected {
        atomic(&path.join("restored.json"), b"{}")?;
    }
    crate::sync::touch(app);
    crate::meetings::notify(app);
    Ok(new_id)
}
fn put_checked(
    cloud: &dyn Provider,
    token: &str,
    folder: Folder,
    name: &str,
    bytes: &[u8],
) -> Result<()> {
    let digest = format!("{:x}", md5::compute(bytes));
    if let Some(existing) = cloud
        .list(token, folder)?
        .into_iter()
        .find(|f| f.name == name)
    {
        if existing.md5 != digest {
            bail!("Concurrent trash archive differs; both local copies retained")
        }
    } else {
        cloud.upload(token, folder, name, bytes)?;
    }
    let result = cloud
        .list(token, folder)?
        .into_iter()
        .find(|f| f.name == name)
        .ok_or_else(|| anyhow::anyhow!("Archive upload incomplete"))?;
    if result.md5 != digest || result.size != bytes.len() as u64 {
        bail!("Archive checksum mismatch")
    };
    Ok(())
}
pub fn publish(
    app: &AppHandle,
    id: i64,
    cloud: &dyn Provider,
    token: &str,
    audio_enabled: bool,
) -> Result<()> {
    if purged(app, id) {
        return Ok(());
    }
    publish_at(
        &root(app).join(id.to_string()),
        id,
        cloud,
        token,
        audio_enabled,
    )
}
fn publish_at(
    dir: &Path,
    id: i64,
    cloud: &dyn Provider,
    token: &str,
    audio_enabled: bool,
) -> Result<()> {
    if cloud
        .list(token, Folder::Meetings)?
        .iter()
        .any(|f| f.name == purge_name(&id.to_string()))
    {
        return Ok(());
    }
    if !dir.join("archive.json").exists() {
        bail!("No recovery archive: deletion paused")
    }
    let existing = cloud
        .list(token, Folder::Meetings)?
        .into_iter()
        .find(|f| f.name == archive_name(&id.to_string()));
    if let Some(item) = existing {
        let bytes = cloud.download(token, Folder::Meetings, &item.name)?;
        if format!("{:x}", md5::compute(&bytes)) != item.md5 {
            bail!("Archive changed")
        }
        let a = decode(&bytes)?;
        if a.id != id.to_string() {
            bail!("Wrong archive")
        }
        if let Some(hash) = a.audio_md5 {
            let audio = cloud
                .list(token, Folder::Audio)?
                .into_iter()
                .find(|f| f.name == audio_name(&a.id))
                .ok_or_else(|| anyhow::anyhow!("Missing archived audio"))?;
            if audio.md5 != hash || Some(audio.size) != a.audio_size {
                bail!("Archived audio is incomplete")
            }
        }
        atomic(&dir.join("cloud-archive.json"), &bytes)?;
        return Ok(());
    }
    let mut a = decode(&fs::read(dir.join("archive.json"))?)?;
    let original_audio = cloud
        .list(token, Folder::Audio)?
        .into_iter()
        .find(|f| f.name == format!("{id}.wav"));
    // Preserve audio already in the cloud even if new audio uploads are disabled.
    if let Some(remote) = &original_audio {
        let path = local_audio(&dir);
        if !path.exists() || file_md5(&path)? != remote.md5 {
            let temp = dir.join("audio.download");
            cloud.download_file(token, Folder::Audio, &remote.name, &temp)?;
            if file_md5(&temp)? != remote.md5 || fs::metadata(&temp)?.len() != remote.size {
                bail!("Original audio changed")
            }
            fs::rename(temp, dir.join("audio.wav"))?;
        }
        a.audio_md5 = Some(remote.md5.clone());
        a.audio_size = Some(remote.size);
    }
    if audio_enabled || original_audio.is_some() {
        if let Some(hash) = &a.audio_md5 {
            let name = audio_name(&a.id);
            let path = local_audio(&dir);
            if file_md5(&path)? != *hash {
                bail!("Local archive audio changed")
            }
            if let Some(existing) = cloud
                .list(token, Folder::Audio)?
                .into_iter()
                .find(|f| f.name == name)
            {
                if existing.md5 != *hash {
                    bail!("Archive audio conflict")
                }
            } else {
                cloud.upload_file(token, Folder::Audio, &name, &path)?;
            }
            let check = cloud
                .list(token, Folder::Audio)?
                .into_iter()
                .find(|f| f.name == name)
                .ok_or_else(|| anyhow::anyhow!("Missing archive audio"))?;
            if check.md5 != *hash || Some(check.size) != a.audio_size {
                bail!("Archive audio incomplete")
            }
        }
    } else {
        a.audio_md5 = None;
        a.audio_size = None;
    }
    let bytes = serde_json::to_vec(&a)?;
    put_checked(cloud, token, Folder::Meetings, &archive_name(&a.id), &bytes)?;
    atomic(&dir.join("cloud-archive.json"), &bytes)
}
pub fn sync(app: &AppHandle, cloud: &dyn Provider, token: &str, audio_enabled: bool) -> Result<()> {
    sync_purges(app, cloud, token)?;
    let listing = cloud.list(token, Folder::Meetings)?;
    for item in &listing {
        let Some(id) = item
            .name
            .strip_prefix("trash-v1-")
            .and_then(|n| n.strip_suffix(".json"))
        else {
            continue;
        };
        if !valid_id(id) || purged_at(&root(app), id) {
            continue;
        }
        let dir = root(app).join(id);
        fs::create_dir_all(&dir)?;
        let bytes = cloud.download(token, Folder::Meetings, &item.name)?;
        if format!("{:x}", md5::compute(&bytes)) != item.md5 {
            bail!("Trash changed during download")
        }
        let a = decode(&bytes)?;
        if a.id != id {
            bail!("Wrong trash identity")
        }
        if !dir.join("archive.json").exists() {
            atomic(&dir.join("archive.json"), &bytes)?;
        }
        atomic(&dir.join("cloud-archive.json"), &bytes)?;
        if audio_enabled || dir.join("restore-request").exists() {
            ensure_archive_audio(&dir, &a, cloud, token)?;
        }
        if let Some(marker) = listing.iter().find(|f| f.name == restored_name(id)) {
            let data = cloud.download(token, Folder::Meetings, &marker.name)?;
            if format!("{:x}", md5::compute(&data)) != marker.md5 {
                bail!("Restore marker changed during download")
            }
            let v: Value = serde_json::from_slice(&data)?;
            if v["schema"] != 1 || v["id"] != id || v["restored_id"] != restore_id(id).to_string() {
                bail!("Invalid restore marker")
            }
            atomic(&dir.join("restored.json"), &data)?;
        }
    }
    for row in rows(app).into_iter().filter(|r| r.restoring) {
        let id = row.id.to_string();
        let new_id = restore_id(&id);
        let path = root(app).join(&id);
        let manifest = path.join("cloud-archive.json");
        let mut archive = decode(&fs::read(if manifest.exists() {
            manifest
        } else {
            path.join("archive.json")
        })?)?;
        if !path.join("cloud-archive.json").exists() {
            archive.audio_md5 = None;
            archive.audio_size = None;
        }
        if let Some(hash) = &archive.audio_md5 {
            let source = local_audio(&path);
            let restored_audio = crate::meetings::dir(app, new_id).join("audio.wav");
            if !source.exists() {
                bail!("Archived audio is not downloaded yet")
            }
            if file_md5(&source)? != *hash {
                bail!("Archived audio differs")
            }
            if !restored_audio.exists() {
                fs::copy(&source, &restored_audio)?;
            }
            let name = format!("{new_id}.wav");
            if !cloud
                .list(token, Folder::Audio)?
                .iter()
                .any(|f| f.name == name)
            {
                cloud.upload_file(token, Folder::Audio, &name, &source)?;
            }
        }
        let audio_ready = archive.audio_md5.is_none()
            || cloud.list(token, Folder::Audio)?.iter().any(|f| {
                f.name == format!("{new_id}.wav") && Some(&f.md5) == archive.audio_md5.as_ref()
            });
        if audio_ready
            && listing
                .iter()
                .any(|f| f.name == format!("{new_id}.meta.json"))
            && listing
                .iter()
                .any(|f| f.name == format!("{new_id}.transcript.json"))
        {
            let marker = serde_json::to_vec(
                &serde_json::json!({"schema":1,"id":id,"restored_id":new_id.to_string()}),
            )?;
            put_checked(cloud, token, Folder::Meetings, &restored_name(&id), &marker)?;
            atomic(&root(app).join(&id).join("restored.json"), &marker)?;
        }
    }
    crate::meetings::notify(app);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sync::provider::{DeviceCode, Poll, RemoteFile, Tokens};
    use std::sync::Mutex;
    #[derive(Default)]
    struct Memory {
        files: Mutex<std::collections::HashMap<(Folder, String), Vec<u8>>>,
        fail_delete: std::sync::atomic::AtomicBool,
        list_calls: std::sync::atomic::AtomicUsize,
    }
    impl Provider for Memory {
        fn id(&self) -> &'static str {
            "synthetic"
        }
        fn title(&self) -> &'static str {
            "Synthetic"
        }
        fn configured(&self) -> bool {
            true
        }
        fn device_code(&self, _: &str) -> Result<DeviceCode> {
            unreachable!()
        }
        fn poll_token(&self, _: &DeviceCode) -> Result<Poll> {
            unreachable!()
        }
        fn refresh(&self, _: &str) -> Result<Tokens> {
            unreachable!()
        }
        fn revoke(&self, _: &str) {}
        fn account(&self, _: &str) -> Result<String> {
            Ok("test".into())
        }
        fn prepare(&self, _: &str) -> Result<()> {
            Ok(())
        }
        fn list(&self, _: &str, folder: Folder) -> Result<Vec<RemoteFile>> {
            self.list_calls
                .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            Ok(self
                .files
                .lock()
                .unwrap()
                .iter()
                .filter(|((f, _), _)| *f == folder)
                .map(|((_, name), b)| RemoteFile {
                    name: name.clone(),
                    md5: format!("{:x}", md5::compute(b)),
                    modified: 1,
                    size: b.len() as u64,
                })
                .collect())
        }
        fn upload(&self, _: &str, f: Folder, n: &str, b: &[u8]) -> Result<()> {
            self.files.lock().unwrap().insert((f, n.into()), b.to_vec());
            Ok(())
        }
        fn upload_file(&self, t: &str, f: Folder, n: &str, p: &Path) -> Result<()> {
            self.upload(t, f, n, &fs::read(p)?)
        }
        fn download(&self, _: &str, f: Folder, n: &str) -> Result<Vec<u8>> {
            self.files
                .lock()
                .unwrap()
                .get(&(f, n.into()))
                .cloned()
                .ok_or_else(|| anyhow::anyhow!("missing"))
        }
        fn download_file(&self, t: &str, f: Folder, n: &str, p: &Path) -> Result<()> {
            fs::write(p, self.download(t, f, n)?)?;
            Ok(())
        }
        fn delete(&self, _: &str, folder: Folder, name: &str) -> Result<()> {
            if self.fail_delete.load(std::sync::atomic::Ordering::Relaxed) {
                bail!("synthetic interruption")
            }
            let mut files = self.files.lock().unwrap();
            assert!(
                files.contains_key(&(Folder::Meetings, purge_name("100"))),
                "marker must be durable before payload deletion"
            );
            files.remove(&(folder, name.into()));
            Ok(())
        }
    }
    fn fixture() -> (PathBuf, Memory) {
        static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
        let path = std::env::temp_dir().join(format!(
            "solflow-trash-test-{}-{}-{}",
            std::process::id(),
            crate::sync::provider::now_ms(),
            NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
        ));
        fs::create_dir_all(&path).unwrap();
        let a = Archive {
            schema: 1,
            id: "100".into(),
            deleted_at: 1000,
            meta: serde_json::json!({"title":"Keep","future":{"keep":true}}),
            transcript: serde_json::json!([]),
            audio_md5: None,
            audio_size: None,
        };
        fs::write(path.join("archive.json"), serde_json::to_vec(&a).unwrap()).unwrap();
        (path, Memory::default())
    }
    #[test]
    fn identity_and_path_boundaries() {
        for id in ["", "0", "01", "../1", "-1", "9223372036854775808"] {
            assert!(!valid_id(id));
        }
        assert!(valid_id("100"));
        assert_ne!(restore_id("100"), restore_id("101"));
        assert!(restore_id("100") < 9_007_199_254_740_992);
    }
    #[test]
    fn archive_preserves_remote_audio_when_downloads_disabled() {
        let (path, cloud) = fixture();
        cloud
            .upload("", Folder::Audio, "100.wav", b"synthetic audio")
            .unwrap();
        publish_at(&path, 100, &cloud, "", false).unwrap();
        publish_at(&path, 100, &cloud, "", false).unwrap();
        let a = decode(
            &cloud
                .download("", Folder::Meetings, "trash-v1-100.json")
                .unwrap(),
        )
        .unwrap();
        assert_eq!(
            a.audio_md5,
            Some(format!("{:x}", md5::compute(b"synthetic audio")))
        );
        assert!(a.meta["future"]["keep"].as_bool().unwrap());
        assert_eq!(
            cloud.download("", Folder::Audio, "100.wav").unwrap(),
            b"synthetic audio"
        );
        fs::remove_dir_all(path).unwrap();
    }
    #[test]
    fn damaged_existing_archive_audio_blocks_deletion() {
        let (path, cloud) = fixture();
        cloud
            .upload("", Folder::Audio, "100.wav", b"sound")
            .unwrap();
        publish_at(&path, 100, &cloud, "", true).unwrap();
        cloud
            .upload("", Folder::Audio, "trash-v1-100.wav", b"corrupted")
            .unwrap();
        assert!(publish_at(&path, 100, &cloud, "", true).is_err());
        assert!(path.join("archive.json").exists());
        fs::remove_dir_all(path).unwrap();
    }
    #[test]
    fn stale_cached_audio_is_replaced_but_original_is_retained() {
        let (path, cloud) = fixture();
        fs::create_dir_all(path.join("local-original")).unwrap();
        fs::write(path.join("local-original/audio.wav"), b"old local sound").unwrap();
        fs::write(path.join("audio.wav"), b"stale cloud cache").unwrap();
        let canonical = b"canonical cloud sound";
        cloud
            .upload("", Folder::Audio, "trash-v1-100.wav", canonical)
            .unwrap();
        let mut archive = decode(&fs::read(path.join("archive.json")).unwrap()).unwrap();
        archive.audio_md5 = Some(format!("{:x}", md5::compute(canonical)));
        archive.audio_size = Some(canonical.len() as u64);
        ensure_archive_audio(&path, &archive, &cloud, "").unwrap();
        assert_eq!(fs::read(path.join("audio.wav")).unwrap(), canonical);
        assert_eq!(
            fs::read(path.join("local-original/audio.wav")).unwrap(),
            b"old local sound"
        );
        fs::write(path.join("audio.wav"), b"old cache again").unwrap();
        cloud
            .upload("", Folder::Audio, "trash-v1-100.wav", b"broken")
            .unwrap();
        assert!(ensure_archive_audio(&path, &archive, &cloud, "").is_err());
        assert_eq!(
            fs::read(path.join("audio.wav")).unwrap(),
            b"old cache again"
        );
        fs::remove_dir_all(path).unwrap();
    }
    fn purge_fixture() -> (PathBuf, Memory) {
        let (base, cloud) = fixture();
        fs::create_dir_all(base.join("100")).unwrap();
        fs::rename(base.join("archive.json"), base.join("100/archive.json")).unwrap();
        fs::write(base.join("100/purge-request"), b"1").unwrap();
        publish_at(&base.join("100"), 100, &cloud, "", false).unwrap();
        (base, cloud)
    }
    #[test]
    fn purge_is_durable_idempotent_and_preserves_unselected_items() {
        let (base, cloud) = purge_fixture();
        cloud
            .upload("", Folder::Meetings, "trash-v1-101.json", b"unselected")
            .unwrap();
        cloud
            .upload("", Folder::Audio, "100.wav", b"deleted original audio")
            .unwrap();
        purge_at(&base, &cloud, "").unwrap();
        assert!(!base.join("100").exists());
        assert!(purged_at(&base, "100"));
        assert_eq!(
            cloud
                .download("", Folder::Meetings, &purge_name("100"))
                .unwrap(),
            b"{\"id\":\"100\",\"schema\":1}"
        );
        assert!(cloud.download("", Folder::Audio, "100.wav").is_err());
        // A stale legacy client reintroduces its cache; the next pass removes it again.
        cloud
            .upload("", Folder::Meetings, &archive_name("100"), b"stale")
            .unwrap();
        purge_at(&base, &cloud, "").unwrap();
        assert!(cloud
            .download("", Folder::Meetings, &archive_name("100"))
            .is_err());
        assert_eq!(
            cloud
                .download("", Folder::Meetings, "trash-v1-101.json")
                .unwrap(),
            b"unselected"
        );
        fs::remove_dir_all(base).unwrap();
    }
    #[test]
    fn interrupted_purge_retains_local_payload_and_retries() {
        let (base, cloud) = purge_fixture();
        cloud
            .fail_delete
            .store(true, std::sync::atomic::Ordering::Relaxed);
        assert!(purge_at(&base, &cloud, "").is_err());
        assert!(base.join("100/archive.json").exists());
        assert!(cloud
            .download("", Folder::Meetings, &purge_name("100"))
            .is_ok());
        cloud
            .fail_delete
            .store(false, std::sync::atomic::Ordering::Relaxed);
        purge_at(&base, &cloud, "").unwrap();
        assert!(!base.join("100").exists());
        fs::remove_dir_all(base).unwrap();
    }
    #[test]
    fn invalid_purge_marker_cannot_remove_payload() {
        let (base, cloud) = purge_fixture();
        cloud
            .upload(
                "",
                Folder::Meetings,
                &purge_name("100"),
                b"{\"id\":\"101\",\"schema\":1}",
            )
            .unwrap();
        assert!(purge_at(&base, &cloud, "").is_err());
        assert!(base.join("100/archive.json").exists());
        assert!(cloud
            .download("", Folder::Meetings, &archive_name("100"))
            .is_ok());
        fs::remove_dir_all(base).unwrap();
    }
    #[test]
    fn restore_racing_with_confirmation_is_preserved() {
        let (base, cloud) = purge_fixture();
        let name = format!("{}.meta.json", restore_id("100"));
        cloud
            .upload("", Folder::Meetings, &name, b"restored recording")
            .unwrap();
        purge_at(&base, &cloud, "").unwrap();
        assert!(!purged_at(&base, "100"));
        assert!(!base.join("100/purge-request").exists());
        assert_eq!(
            cloud.download("", Folder::Meetings, &name).unwrap(),
            b"restored recording"
        );
        fs::remove_dir_all(base).unwrap();
    }
    #[test]
    fn receiving_purge_without_local_request_removes_only_its_archive() {
        let (base, cloud) = purge_fixture();
        fs::remove_file(base.join("100/purge-request")).unwrap();
        cloud
            .upload(
                "",
                Folder::Meetings,
                &purge_name("100"),
                &purge_bytes("100"),
            )
            .unwrap();
        purge_at(&base, &cloud, "").unwrap();
        assert!(!base.join("100").exists());
        fs::remove_dir_all(base).unwrap();
    }
    #[test]
    fn old_purge_markers_require_only_two_listings_per_pass() {
        let (base, cloud) = fixture();
        for n in 100..200 {
            let id = n.to_string();
            cloud
                .upload("", Folder::Meetings, &purge_name(&id), &purge_bytes(&id))
                .unwrap();
            cloud
                .upload(
                    "",
                    Folder::Meetings,
                    &format!("{id}.deleted"),
                    b"{\"legacy\":true}",
                )
                .unwrap();
        }
        purge_at(&base, &cloud, "").unwrap();
        assert_eq!(
            cloud.list_calls.load(std::sync::atomic::Ordering::Relaxed),
            2
        );
        fs::remove_dir_all(base).unwrap();
    }
}
