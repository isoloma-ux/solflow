const fs = require('node:fs'), vm = require('node:vm'), assert = require('node:assert/strict');
const path = require('node:path');
const ctx = {window:{}}; vm.createContext(ctx);
vm.runInContext(fs.readFileSync(path.join(__dirname, '../desktop/ui/mcp-setup.js'),'utf8'),ctx);
vm.runInContext(fs.readFileSync(path.join(__dirname, '../desktop/ui/i18n.js'),'utf8'),ctx);
const servers = [
 {command:'/Applications/Sol Flow.app/Contents/MacOS/solflow-mcp',args:['--export-dir','/Users/Test User/Library/Application Support/Sol Flow/mcp-export']},
 {command:String.raw`C:\Users\Иван Тест\AppData\Local\Sol Flow\solflow-mcp.exe`,args:['--export-dir',String.raw`C:\Users\Иван Тест\AppData\Roaming\Sol Flow\mcp-export`]},
 {command:'/tmp/quotes " and $() ` /solflow-mcp',args:['--export-dir','/tmp/line\nnew/export']}
];
for (const language of ['ru','en']) {
 vm.runInContext('UI_LANG = '+JSON.stringify(language),ctx);
 for (const client of ['openai','claude','other']) for (const server of servers) {
  const prompt = ctx.window.solflowMcpSetupPrompt(server,client,ctx.t);
  const json = JSON.parse(prompt.slice(prompt.lastIndexOf('\n{')+1));
  assert.deepEqual(json,{name:'solflow',transport:'stdio',...server});
  const prose=prompt.slice(0,prompt.lastIndexOf('\n{'));
  if(language==='en') assert.doesNotMatch(prose,/[А-Яа-яЁё]/);
  assert.match(prose,/list_projects/);
  if(client==='claude') assert.match(prose,/claude_desktop_config.json/);
  if(client==='openai') assert.match(prose,/CODEX_HOME/);
 }
}
console.log('PASS: 18 prompt variants; exact Mac/Windows/special paths; RU/EN; client-specific target and metadata-only check.');
