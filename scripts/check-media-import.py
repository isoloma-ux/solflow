#!/usr/bin/env python3
"""Compile actual fetch/tools/net modules with a minimal app-path adapter.
Default: isolated regression tests. --install DIR installs helpers only in DIR.
--fetch DIR URL downloads into DIR without browser cookies; --install must run first.
"""
from pathlib import Path
import os,subprocess,tempfile,sys
root=Path(__file__).resolve().parents[1]
deps=Path(os.environ.get('SOLFLOW_TEST_DEPS',root/'desktop/src-tauri/target/release/deps'))
with tempfile.TemporaryDirectory(prefix='solflow-media-test-') as out:
 p=Path(out)/'harness.rs'
 p.write_text('''extern crate self as tauri;
pub struct AppHandle;
pub trait Manager { fn path(&self)->Paths {Paths} }
impl Manager for AppHandle {}
pub struct Paths;
impl Paths { pub fn app_data_dir(&self)->std::io::Result<std::path::PathBuf> {
 Ok(std::path::PathBuf::from(std::env::var_os("SOLFLOW_MEDIA_TEST_DIR").expect("isolated test directory")))
} }
mod sys { pub fn command(p:impl AsRef<std::ffi::OsStr>)->std::process::Command {std::process::Command::new(p)} }
'''+''.join('#[path="'+(root/'desktop/src-tauri/src'/f'{n}.rs').as_posix()+'"] mod '+n+';\n' for n in ['net','tools','fetch'])+'''
fn main()->anyhow::Result<()> {
 let a:Vec<String>=std::env::args().collect();
 let dir=std::path::PathBuf::from(&a[2]); std::fs::create_dir_all(&dir)?;
 std::env::set_var("SOLFLOW_MEDIA_TEST_DIR",&dir);tools::init(&AppHandle);
 if a[1]=="--install" {tools::install(&|p|println!("helpers {p}%"))?;}
 else if a[1]=="--fetch" {
  let report=|d,t| {if d>0 {println!("bytes {d}/{t}");}};
  let result=fetch::fetch(&a[3],&dir,&fetch::Progress{report:&report,cancelled:&||false},None)?;
  println!("RESULT {} {}",result.0.display(),result.1);
 }
 Ok(())
}
''')
 test=len(sys.argv)==1
 cmd=['rustc','--edition=2021',str(p),'-L','dependency='+str(deps),'-o',str(Path(out)/('check.exe' if os.name=='nt' else 'check')),'-A','dead_code','-C','lto=thin']
 if test:cmd+=['--test']
 for name in ['serde_json','sha2','anyhow','ureq']:
  libs=sorted(deps.glob('lib'+name+'-*.rlib'),key=lambda p:p.stat().st_mtime,reverse=True)
  assert libs,name
  cmd+=['--extern',name+'='+str(libs[0])]
 subprocess.run(cmd,check=True)
 args=[str(Path(out)/('check.exe' if os.name=='nt' else 'check'))]+(['--test-threads=1'] if test else sys.argv[1:])
 subprocess.run(args,check=True,timeout=900)
