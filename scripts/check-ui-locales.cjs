const fs=require('node:fs'),vm=require('node:vm'),assert=require('node:assert/strict'),path=require('node:path');
const root=path.join(__dirname,'..');
const ctx={navigator:{language:'en-US'}};vm.createContext(ctx);
for(const f of ['desktop/ui/guide/locales.js','desktop/ui/i18n.js'])vm.runInContext(fs.readFileSync(path.join(root,f),'utf8'),ctx);
for(const tag of ['zh-Hans-CN','ko-KR','ja-JP','de-DE','fr-CA','es-MX','ru-RU','en_GB'])assert.equal(ctx.normalizeUiLanguage(tag),tag.slice(0,2));
assert.equal(ctx.normalizeUiLanguage('it-IT'),'en');
for(const locale of ['zh','ko','ja','de','fr','es']){
 for(const english of vm.runInContext('Object.values(EN)',ctx)) assert(Object.hasOwn(ctx.SOLFLOW_LOCALES[locale],english),`${locale}: missing desktop text: ${english}`);
 vm.runInContext(`UI_LANG=${JSON.stringify(locale)}`,ctx);
 assert.equal(ctx.t('Настройки'),ctx.SOLFLOW_LOCALES[locale]['Settings']);
 assert.notEqual(ctx.t('Настройки'),'Settings');
 assert.equal(ctx.t('Languages: {0}',12),ctx.SOLFLOW_LOCALES[locale]['Languages: {0}'].replace('{0}','12'));
 const privateText='User recording: keep EXACT text.';assert.equal(ctx.t(privateText),privateText);
 assert(ctx.languageMatches('Ko','ko',ctx.localizedLanguageName('ko','Korean'),'Korean'));
 assert(ctx.languageMatches('Korean','ko',ctx.localizedLanguageName('ko','Korean'),'Korean'));
}
vm.runInContext('UI_LANG="ru"',ctx);assert.equal(ctx.t('Настройки'),'Настройки');
vm.runInContext('UI_LANG="en"',ctx);assert.equal(ctx.t('Настройки'),'Settings');
console.log('PASS: eight UI languages, regional/system resolution, placeholders, source text preservation and English search aliases');
