(() => {
 const get=id=>document.getElementById(id),params=new URLSearchParams(location.search);
 const supported=['ru','en','zh','ko','ja','de','fr','es'];
 let lang=supported.includes(params.get('lang'))?params.get('lang'):'en', platform=['mac','windows','android'].includes(params.get('platform'))?params.get('platform'):'mac',current=0;
 const translated=(text,locale)=>globalThis.SOLFLOW_LOCALES?.[locale]?.[text]||text;
 const tr=(ru,en)=>lang==='ru'?ru:translated(en,lang);
 function localize(value,locale){if(typeof value==='string')return translated(value,locale);if(Array.isArray(value))return value.map(v=>localize(v,locale));if(value&&typeof value==='object')return Object.fromEntries(Object.entries(value).map(([k,v])=>[k,localize(v,locale)]));return value;}
 for(const locale of supported.filter(code=>!['ru','en'].includes(code)))GUIDE[locale]=localize(GUIDE.en,locale);
 for(const [code,name] of [['zh','简体中文'],['ko','한국어'],['ja','日本語'],['de','Deutsch'],['fr','Français'],['es','Español']]){const option=document.createElement('option');option.value=code;option.textContent=name;get('language').append(option);}
 get('language').value=lang;get('platform').value=platform;
 function render(){
  document.documentElement.lang=lang;const rows=GUIDE[lang],base=rows[current],s=platform==='android'?{...base,...base.phone}:base;
  document.title=tr('Sol Flow — руководство','Sol Flow — guide');get('guideName').textContent=tr('Руководство','Guide');get('platformLabel').textContent=tr('Устройство','Device');get('languageLabel').textContent=tr('Язык','Language');get('contents').setAttribute('aria-label',tr('Разделы руководства','Guide sections'));
  get('contents').replaceChildren(...rows.map((row,i)=>{const b=document.createElement('button');b.textContent=row.nav;b.setAttribute('aria-current',i===current?'page':'false');b.onclick=()=>{current=i;render();get('main').focus({preventScroll:true});window.scrollTo(0,0);};return b;}));
  get('progress').textContent=tr('Раздел {0} из {1}','Section {0} of {1}').replace('{0}',current+1).replace('{1}',rows.length);get('title').textContent=s.title;get('lead').textContent=s.lead;
  get('platformNote').textContent=platform==='android'?'':(s[platform]||s.desktop||'');
  get('steps').replaceChildren(...s.steps.map(text=>{const li=document.createElement('li');li.textContent=text;return li;}));
  get('details').replaceChildren(...s.details.map(([title,text])=>{const section=document.createElement('section');section.className='detail';const h=document.createElement('h2'),p=document.createElement('p');h.textContent=title;p.textContent=text;section.append(h,p);return section;}));
  if(s.source){const p=document.createElement('p'),a=document.createElement('a');a.textContent=tr('Официальная инструкция','Official instructions');a.href=s.source;a.target='_blank';a.rel='noopener noreferrer';p.append(a);get('details').append(p);}
  const image=s.shot;
  get('shot').classList.toggle('phone-shot',platform==='android');
  get('shot').hidden=!image;
  if(image){get('screenshot').src='shots/'+image;get('screenshot').alt=s.title;get('caption').textContent=platform==='android'?tr('Экран Sol Flow для Android с учебными данными.','Sol Flow on Android with sample data.')+' '+(s.caption||''):tr('Интерфейс настольного приложения с учебными данными. На телефоне расположение элементов отличается.','Desktop interface with sample data. Mobile controls use a different layout.');}
  get('screenshot').onerror=()=>{get('shot').hidden=true;};
  get('tip').textContent=s.tip;get('previous').textContent=tr('Назад','Back');get('next').textContent=current===rows.length-1?tr('К началу','Back to start'):tr('Следующий раздел','Next section');get('previous').disabled=current===0;
 }
 get('language').onchange=()=>{lang=get('language').value;render();};get('platform').onchange=()=>{platform=get('platform').value;render();};
 function move(delta){current=(current+delta+GUIDE[lang].length)%GUIDE[lang].length;render();get('main').focus({preventScroll:true});window.scrollTo(0,0);}
 get('previous').onclick=()=>move(-1);get('next').onclick=()=>move(1);render();
})();
