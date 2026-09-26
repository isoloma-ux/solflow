use serde_json::{json, Value};
use solflow_mcp_core::{collect, Grant, Store};
use std::{
    fs,
    io::{BufRead, BufReader, Write},
    path::PathBuf,
    process::{Command, Stdio},
};

struct Fixture {
    _tmp: tempfile::TempDir,
    source: PathBuf,
    export: PathBuf,
}
impl Fixture {
    fn new() -> Self {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path().canonicalize().unwrap();
        let source = root.join("source");
        let export = root.join("export");
        fs::create_dir_all(&source).unwrap();
        fs::write(
            source.join("projects.json"),
            json!([{"id":"10","name":"Клиент А"},{"id":"20","name":"Клиент А"}]).to_string(),
        )
        .unwrap();
        for (id, project, text, start) in [
            ("100", "10", "Согласовали бюджет Альфа на пятницу.", 12.5),
            ("101", "10", "Бюджет Альфа перенесли на понедельник.", 42.0),
            ("200", "20", "СЕКРЕТ Закрытая встреча.", 9.0),
        ] {
            let path = source.join("meetings").join(id);
            fs::create_dir_all(&path).unwrap();
            fs::write(path.join("meta.json"),json!({"title":format!("Встреча {id}"),"project":project,"state":"done","at":12345678,"updated":123,"summary":"Производное саммери","names":{"0":"Иван"}}).to_string()).unwrap();
            fs::write(
                path.join("transcript.json"),
                json!([{"s":start,"e":start+5.0,"text":text,"spk":0}]).to_string(),
            )
            .unwrap();
            fs::write(path.join("audio.wav"), b"ORIGINAL AUDIO").unwrap();
            fs::write(path.join("qa.json"), b"PRIVATE QA MUST NOT LEAK").unwrap();
        }
        fs::write(source.join("settings.json"), b"PRIVATE TOKENS").unwrap();
        Self {
            _tmp: tmp,
            source,
            export,
        }
    }
    fn allow(&self) -> Store {
        let mut s = Store::create(&self.export).unwrap();
        s.set_grant(
            "10",
            &Grant {
                transcript: true,
                ..Default::default()
            },
        )
        .unwrap();
        self.refresh(&mut s);
        s
    }
    fn refresh(&self, s: &mut Store) {
        s.invalidate().unwrap();
        let snapshot = collect(&self.source, &s.grants().unwrap()).unwrap();
        s.publish(&snapshot).unwrap();
    }
    fn meta(&self, id: &str, change: impl FnOnce(&mut Value)) {
        let p = self.source.join("meetings").join(id).join("meta.json");
        let mut m: Value = serde_json::from_slice(&fs::read(&p).unwrap()).unwrap();
        change(&mut m);
        fs::write(p, m.to_string()).unwrap();
    }
}
fn inventory(root: &std::path::Path) -> Vec<(PathBuf, Vec<u8>)> {
    let mut out = vec![];
    for e in fs::read_dir(root).unwrap() {
        let p = e.unwrap().path();
        if p.is_dir() {
            out.extend(inventory(&p));
        } else {
            out.push((p.clone(), fs::read(p).unwrap()));
        }
    }
    out.sort();
    out
}
#[test]
fn denied_by_default_and_source_bytes_unchanged() {
    let f = Fixture::new();
    let before = inventory(&f.source);
    let mut s = Store::create(&f.export).unwrap();
    f.refresh(&mut s);
    assert!(s.projects().unwrap().is_empty());
    assert!(s.fetch("100:transcript:0:0", None).is_err());
    s.set_grant(
        "10",
        &Grant {
            transcript: true,
            ..Default::default()
        },
    )
    .unwrap();
    f.refresh(&mut s);
    assert_eq!(s.projects().unwrap().len(), 1);
    let hits = s.search("бюджет Альф", None, 10).unwrap();
    assert_eq!(hits["results"].as_array().unwrap().len(), 2);
    assert_eq!(hits["results"][0]["speaker"], "Иван");
    assert_eq!(
        s.search("\"бюджет Альфа\"", None, 10).unwrap()["results"]
            .as_array()
            .unwrap()
            .len(),
        2
    );
    assert_eq!(
        s.search("\"на пятницу\"", None, 10).unwrap()["results"]
            .as_array()
            .unwrap()
            .len(),
        1
    );
    assert!(s.search("\"Альфа бюджет\"", None, 10).unwrap()["results"]
        .as_array()
        .unwrap()
        .is_empty());
    assert!(s.search("СЕКРЕТ", None, 10).unwrap()["results"]
        .as_array()
        .unwrap()
        .is_empty());
    assert!(s.fetch("200:transcript:0:0", None).is_err());
    assert!(s.fetch("../../settings.json", None).is_err());
    assert!(s.fetch("100:summary:0:0", None).is_err());
    assert_eq!(inventory(&f.source), before);
}
#[test]
fn revoke_survives_restart_and_removes_guessed_ids() {
    let f = Fixture::new();
    let mut s = f.allow();
    s.set_grant("10", &Grant::default()).unwrap();
    drop(s);
    let s = Store::reader(&f.export).unwrap();
    assert!(s.fetch("100:transcript:0:0", None).is_err());
    assert!(s.projects().unwrap().is_empty());
}
#[test]
fn edits_moves_deletions_and_partial_transcripts() {
    let f = Fixture::new();
    let mut s = f.allow();
    let old = s.fetch("100:transcript:0:0", None).unwrap()["document"]["revision"]
        .as_str()
        .unwrap()
        .to_owned();
    f.meta("100", |m| m["names"]["0"] = json!("Анна"));
    f.refresh(&mut s);
    let doc = s.fetch("100:transcript:0:0", Some(&old)).unwrap();
    assert_eq!(doc["revision_changed"], true);
    assert_eq!(doc["document"]["speaker"], "Анна");
    f.meta("100", |m| m["project"] = json!("20"));
    f.refresh(&mut s);
    assert!(s.fetch("100:transcript:0:0", None).is_err());
    f.meta("101", |m| m["state"] = json!("transcribing"));
    f.refresh(&mut s);
    assert!(s.fetch("101:transcript:0:0", None).is_err());
    f.meta("101", |m| m["state"] = json!("done"));
    fs::remove_dir_all(f.source.join("meetings/101")).unwrap();
    f.refresh(&mut s);
    assert!(s.fetch("101:transcript:0:0", None).is_err());
}
#[test]
fn corruption_fails_closed_and_keeps_user_files() {
    let f = Fixture::new();
    let mut s = f.allow();
    fs::write(f.export.join("my-notes.md"), b"KEEP").unwrap();
    let copy = s.text_copy().unwrap();
    assert!(copy.join("manifest.json").is_file());
    s.invalidate().unwrap();
    fs::write(f.source.join("meetings/100/transcript.json"), b"broken").unwrap();
    assert!(collect(&f.source, &s.grants().unwrap()).is_err());
    assert!(s.fetch("100:transcript:0:0", None).is_err());
    assert_eq!(fs::read(f.export.join("my-notes.md")).unwrap(), b"KEEP");
}
#[cfg(unix)]
#[test]
fn symlink_sources_and_exports_are_rejected() {
    use std::os::unix::fs::symlink;
    let f = Fixture::new();
    let mut s = f.allow();
    let p = f.source.join("meetings/100/transcript.json");
    fs::remove_file(&p).unwrap();
    symlink(f.source.join("settings.json"), p).unwrap();
    s.invalidate().unwrap();
    assert!(collect(&f.source, &s.grants().unwrap()).is_err());
    let alias = f.source.parent().unwrap().join("alias");
    symlink(&f.export, &alias).unwrap();
    assert!(Store::reader(&alias).is_err());
}
#[test]
fn permissions_are_rechecked_when_publishing_and_material_types_are_separate() {
    let f = Fixture::new();
    let mut s = f.allow();
    let old = collect(&f.source, &s.grants().unwrap()).unwrap();
    s.set_grant(
        "10",
        &Grant {
            summary: true,
            ..Default::default()
        },
    )
    .unwrap();
    s.publish(&old).unwrap();
    assert!(s.fetch("100:transcript:0:0", None).is_err());
    f.refresh(&mut s);
    let d = s.fetch("100:summary:0:0", None).unwrap();
    assert_eq!(d["document"]["generated"], true);
    assert!(s.search("бюджет", None, 10).unwrap()["results"]
        .as_array()
        .unwrap()
        .is_empty());
}
#[test]
#[ignore = "Run scripts/check-mcp.py to build and test the real server"]
fn stdio_sdk_roundtrip_and_live_revocation() {
    let binary =
        std::env::var("SOLFLOW_MCP_BIN").expect("SOLFLOW_MCP_BIN must point at the built server");
    let f = Fixture::new();
    let mut store = f.allow();
    let before = inventory(&f.source);
    let mut process = Command::new(binary)
        .args(["--export-dir", f.export.to_str().unwrap()])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .unwrap();
    let mut input = process.stdin.take().unwrap();
    let mut output = BufReader::new(process.stdout.take().unwrap());
    fn rpc(
        input: &mut impl Write,
        output: &mut impl BufRead,
        id: i32,
        method: &str,
        params: Value,
    ) -> Value {
        writeln!(
            input,
            "{}",
            json!({"jsonrpc":"2.0","id":id,"method":method,"params":params})
        )
        .unwrap();
        input.flush().unwrap();
        loop {
            let mut line = String::new();
            assert!(output.read_line(&mut line).unwrap() > 0);
            let v: Value = serde_json::from_str(&line).unwrap();
            if v["id"] == id {
                return v;
            }
        }
    }
    let init = rpc(
        &mut input,
        &mut output,
        1,
        "initialize",
        json!({"protocolVersion":"2025-11-25","capabilities":{},"clientInfo":{"name":"solflow-fixture","version":"1"}}),
    );
    assert!(init.get("error").is_none(), "{init}");
    writeln!(
        input,
        "{}",
        json!({"jsonrpc":"2.0","method":"notifications/initialized"})
    )
    .unwrap();
    input.flush().unwrap();
    let tools = rpc(&mut input, &mut output, 2, "tools/list", json!({}));
    assert_eq!(tools["result"]["tools"].as_array().unwrap().len(), 5);
    let found = rpc(
        &mut input,
        &mut output,
        3,
        "tools/call",
        json!({"name":"search","arguments":{"query":"бюджет"}}),
    );
    let body: Value =
        serde_json::from_str(found["result"]["content"][0]["text"].as_str().unwrap()).unwrap();
    assert_eq!(body["results"].as_array().unwrap().len(), 2);
    let denied = rpc(
        &mut input,
        &mut output,
        4,
        "tools/call",
        json!({"name":"fetch","arguments":{"id":"200:transcript:0:0"}}),
    );
    assert_eq!(denied["result"]["isError"], true);
    store.set_grant("10", &Grant::default()).unwrap();
    let revoked = rpc(
        &mut input,
        &mut output,
        5,
        "tools/call",
        json!({"name":"fetch","arguments":{"id":"100:transcript:0:0"}}),
    );
    assert_eq!(revoked["result"]["isError"], true);
    let _ = process.kill();
    process.wait().unwrap();
    assert_eq!(inventory(&f.source), before);
}
