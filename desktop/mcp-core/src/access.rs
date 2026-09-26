//! Versioned project permissions shared through the user's existing cloud.
//! Revisions are logical counters: clock skew cannot resurrect an old grant.
use crate::{atomic_file, checked_path, Grant};
use anyhow::{bail, Result};
use serde::{Deserialize, Serialize};
use std::{collections::BTreeMap, path::Path};

pub const FILE: &str = "ai-access-v1.json";
const MAX_REV: u64 = 9_007_199_254_740_991;
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct Entry { pub revision: u64, pub grant: Grant }
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct Policy { pub schema: u32, pub projects: BTreeMap<String, Entry> }
impl Default for Policy { fn default() -> Self { Self { schema: 1, projects: BTreeMap::new() } } }
impl Policy {
    pub fn decode(bytes: &[u8]) -> Result<Self> {
        if bytes.len() > 4 * 1024 * 1024 { bail!("AI access file too large") }
        let value: Self = serde_json::from_slice(bytes)?;
        if value.schema != 1 || value.projects.len() > 10000 || value.projects.iter().any(|(id,e)| !crate::valid_id(id) || e.revision == 0 || e.revision > MAX_REV) { bail!("Invalid AI access policy") }
        Ok(value)
    }
    pub fn load(path: &Path) -> Result<Self> {
        checked_path(path)?;
        if !path.exists() { return Ok(Self::default()) }
        Self::decode(&std::fs::read(path)?)
    }
    pub fn save(&self, path: &Path) -> Result<()> { atomic_file(path, &serde_json::to_vec(self)?) }
    pub fn set(&mut self, id: &str, grant: Grant) -> Result<()> {
        if !crate::valid_id(id) { bail!("Invalid project") }
        let revision = self.projects.get(id).map_or(1, |e| e.revision + 1);
        if revision > MAX_REV { bail!("AI access revision limit") }
        self.projects.insert(id.into(), Entry { revision, grant }); Ok(())
    }
    pub fn merge(&self, remote: &Self) -> Self {
        let mut merged = self.clone();
        for (id,r) in &remote.projects {
            match merged.projects.get(id) {
                None => { merged.projects.insert(id.clone(), r.clone()); }
                Some(l) if r.revision > l.revision => { merged.projects.insert(id.clone(), r.clone()); }
                Some(l) if r.revision == l.revision => {
                    // Concurrent edits: deny wins for every material type.
                    let grant = Grant { transcript:l.grant.transcript && r.grant.transcript, summary:l.grant.summary && r.grant.summary, map:l.grant.map && r.grant.map, analyses:l.grant.analyses && r.grant.analyses };
                    merged.projects.insert(id.clone(), Entry { revision:r.revision, grant });
                }
                _ => {}
            }
        }
        merged
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn on() -> Grant { Grant { transcript:true, ..Grant::default() } }
    #[test] fn old_devices_cannot_restore_revoked_access() {
        let mut a=Policy::default();a.set("10",on()).unwrap();let stale=a.clone();
        a.set("10",Grant::default()).unwrap();assert_eq!(a.merge(&stale),a);assert_eq!(stale.merge(&a),a);
        a.set("10",on()).unwrap();assert!(a.projects["10"].grant.enabled());
    }
    #[test] fn concurrent_revoke_wins_and_merge_converges() {
        let mut a=Policy::default();a.set("10",on()).unwrap();let mut b=a.clone();
        a.set("10",Grant::default()).unwrap();b.set("10",Grant { summary:true,..on() }).unwrap();
        assert_eq!(a.merge(&b), b.merge(&a));assert!(!a.merge(&b).projects["10"].grant.enabled());
        assert_eq!(a.merge(&b).merge(&b),a.merge(&b));
    }
    #[test] fn shared_android_conflict_fixtures() {
        for line in include_str!("../../../tests/fixtures/ai-access.tsv").lines() {
            let c:Vec<_>=line.split('\t').collect();
            let entry=|i:usize| { let b=c[i+1].as_bytes(); Entry { revision:c[i].parse().unwrap(),grant:Grant{transcript:b[0]==b'1',summary:b[1]==b'1',map:b[2]==b'1',analyses:b[3]==b'1'} } };
            let policy=|i| Policy {schema:1,projects:BTreeMap::from([("10".to_string(),entry(i))])};
            assert_eq!(policy(0).merge(&policy(2)),policy(4));
            assert_eq!(policy(2).merge(&policy(0)),policy(4));
        }
    }
    #[test] fn malformed_or_future_schema_fails_closed() {
        for raw in [r#"{"schema":2,"projects":{}}"#,r#"{"schema":1,"projects":{"../x":{"revision":1,"grant":{}}}}"#,r#"{"schema":1,"projects":{"10":{"revision":0,"grant":{}}}}"#] {assert!(Policy::decode(raw.as_bytes()).is_err());}
    }
}
