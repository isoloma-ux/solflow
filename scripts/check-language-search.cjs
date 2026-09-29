const fs = require('node:fs'), vm = require('node:vm'), assert = require('node:assert/strict');
const ctx = {}; vm.createContext(ctx);
vm.runInContext(fs.readFileSync(require('node:path').join(__dirname,'../desktop/ui/i18n.js'),'utf8'),ctx);
for (const [code,name] of [['zh','Chinese'],['ko','Korean']]) {
  for(let n=1;n<=name.length;n++) {
    assert(ctx.languageMatches(name.slice(0,n),code,name));
    assert(ctx.languageMatches(name.slice(0,n).toUpperCase(),code,name));
  }
}
for(const [q,code,name] of [[' KO ','ko','Korean'],['francais','fr','Français'],['中','zh','中文'],['한국','ko','한국어'],['','ja','日本語']]) assert(ctx.languageMatches(q,code,name));
assert(!ctx.languageMatches('xyz','ko','Korean'));
console.log('PASS: language search stays consistent for every prefix, case, accents, CJK and ISO codes');
