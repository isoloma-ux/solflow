//! Synthetic/offline model check. Does not initialize the app or sync.
fn main() -> anyhow::Result<()> {
    let args: Vec<_> = std::env::args().collect();
    anyhow::ensure!(args.len()==4,"check_map MODEL INPUT OUTPUT");
    let text=std::fs::read_to_string(&args[2])?;
    let start=std::time::Instant::now();
    let out=solflow_lib::summary::derive_with(std::path::Path::new(&args[1]),"mindmap",&text,
        |p| eprintln!("map_progress={p}"),std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false)))?;
    let map=solflow_lib::mindmap::parse(&out)?;
    std::fs::write(&args[3],serde_json::to_string_pretty(&map)?)?;
    eprintln!("map_ms={} branches={}",start.elapsed().as_millis(),map.branches.len());
    Ok(())
}
