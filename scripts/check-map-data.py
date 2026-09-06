#!/usr/bin/env python3
"""Test actual map + sync modules using already-built Rust dependencies, no app state."""
from pathlib import Path
import subprocess,tempfile,os
r=Path(__file__).resolve().parents[1]
deps=Path(os.environ.get('CARGO_TARGET_DIR',r/'.local-tools/mac-release'))/'release/deps'
meta=(r/'desktop/src-tauri/src/meetings.rs').read_text().split('#[derive(Serialize, Deserialize, Clone, Default)]',1)[1].split('/// Одна реплика',1)[0]
project='#[derive(Clone)] pub struct Project { pub id:String, pub name:String }'
with tempfile.TemporaryDirectory(prefix='solflow-map-tests-') as out:
 p=Path(out)/'check.rs'
 p.write_text('''extern crate serde; extern crate serde_json; extern crate sha2; extern crate anyhow;
#[path = "'''+str(r/'desktop/src-tauri/src/mindmap.rs')+'''"] mod mindmap;
mod meetings { use serde::{Serialize,Deserialize}; use std::collections::HashMap;
pub const STATE_DONE: &str = "done";
#[derive(Serialize, Deserialize, Clone, Default)]'''+meta+project+'''}
#[path = "'''+str(r/'desktop/src-tauri/src/sync/merge.rs')+'''"] mod merge;
#[test] fn old_meta_still_reads() {
 let m:meetings::Meta=serde_json::from_str(r#"{"title":"old","summary":"kept"}"#).unwrap();
 assert!(m.mindmap.is_none()); assert_eq!(m.summary,"kept");
}
#[test] fn map_survives_phone_rename_and_roundtrip() {
 let mut l=meetings::Meta::default();l.updated=100;
 let mut map=mindmap::parse(r#"{"title":"A","branches":[{"title":"B","points":["C"]}]}"#).unwrap();map.revised=20;
 l.mindmap=Some(map);let mut r=meetings::Meta::default();r.updated=200;r.title="Phone rename".into();
 let result=merge::merge_meta(&l,&r);assert_eq!(result.title,"Phone rename");assert_eq!(result.mindmap,l.mindmap);
 let encoded=serde_json::to_string(&result).unwrap();let decoded:meetings::Meta=serde_json::from_str(&encoded).unwrap();
 assert_eq!(decoded.mindmap,result.mindmap);
}
''')
 cmd=['rustc','--edition=2021','--test',str(p),'-L','dependency='+str(deps),'-o',str(Path(out)/'tests')]
 for name in ['serde','serde_json','sha2','anyhow']:
  libs=sorted(deps.glob('lib'+name+'-*.rlib'),key=lambda p:p.stat().st_mtime,reverse=True)
  assert libs,name
  cmd+=['--extern',name+'='+str(libs[0])]
 subprocess.run(cmd,check=True);subprocess.run([str(Path(out)/'tests')],check=True)
