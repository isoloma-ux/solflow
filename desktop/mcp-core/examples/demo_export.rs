//! Isolated fictional pilot data. Never accepts an existing directory at creation.
use anyhow::{bail, Context, Result};
use serde_json::json;
use solflow_mcp_core::{collect, Grant, Store};
use std::{fs, path::PathBuf};

fn main() -> Result<()> {
    let args: Vec<_> = std::env::args_os().skip(1).collect();
    if args.len() != 2 {
        bail!("Usage: demo_export <create|revoke|restore> <absolute demo directory>");
    }
    let action = args[0].to_str().context("invalid action")?;
    let root = solflow_mcp_core::checked_path(&PathBuf::from(&args[1]))?;
    let marker = root.join("solflow-fictional-demo.txt");
    let source = root.join("source");
    let export = root.join("export");
    if action == "create" {
        fs::create_dir(&root).context("demo directory must not exist")?;
        fs::write(&marker, "Sol Flow fictional MCP demo v1\n")?;
        fs::create_dir(&source)?;
        fs::write(
            source.join("projects.json"),
            json!([
                {"id":"10","name":"MCP demo — вымышленный проект"},
                {"id":"20","name":"Закрытый тестовый проект"}
            ])
            .to_string(),
        )?;
        for (id, project, title, at, start, text) in [
            ("100", "10", "План — вымышленная встреча", 1790326800000i64, 12.5,
             "Для вымышленного запуска Альфа согласовали бюджет 100 тысяч рублей и срок до пятницы."),
            ("101", "10", "Уточнение — вымышленная встреча", 1790413200000i64, 42.0,
             "Для вымышленного запуска Альфа бюджет увеличили до 120 тысяч рублей, а срок перенесли на понедельник."),
            ("200", "20", "Закрытая вымышленная встреча", 1790413200000i64, 9.0,
             "Секретный тестовый маркер: ФИОЛЕТОВЫЙ БАРСУК. Этот текст не должен появиться через MCP."),
        ] {
            let folder = source.join("meetings").join(id);
            fs::create_dir_all(&folder)?;
            fs::write(folder.join("meta.json"), json!({"title":title,"project":project,
                "state":"done","at":at,"updated":at,"names":{"0":"Тестовый участник"}}).to_string())?;
            fs::write(folder.join("transcript.json"), json!([
                {"s":start,"e":start+8.0,"text":text,"spk":0}
            ]).to_string())?;
        }
    } else if !matches!(action, "revoke" | "restore") {
        bail!("unknown action");
    }
    if fs::read_to_string(&marker)? != "Sol Flow fictional MCP demo v1\n" {
        bail!("not a fictional demo directory");
    }
    let mut store = Store::create(&export)?;
    store.set_grant(
        "10",
        &Grant {
            transcript: action != "revoke",
            ..Default::default()
        },
    )?;
    store.invalidate()?;
    let snapshot = collect(&source, &store.grants()?)?;
    store.publish(&snapshot)?;
    println!(
        "{}",
        serde_json::to_string_pretty(&json!({"fictional":true,
        "export_dir":export,"status":store.status()?}))?
    );
    Ok(())
}
