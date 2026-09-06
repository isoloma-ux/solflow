const { chromium }=require(process.env.PLAYWRIGHT_MODULE||'playwright');
const fs=require('node:fs'),path=require('node:path'),assert=require('node:assert/strict');
(async()=>{
 const [input,output]=process.argv.slice(2);fs.mkdirSync(output,{recursive:true});
 const model=JSON.parse(fs.readFileSync(input,'utf8'));
 const browser=await chromium.launch({headless:true,executablePath:process.env.CHROME_BINARY});
 try {
  const page=await browser.newPage({viewport:{width:1440,height:1000}});
  await page.route(/^https?:/,r=>r.abort());const errors=[];page.on('pageerror',e=>errors.push(e.message));
  await page.setContent('<html><body style="margin:0"><div id="map"></div></body></html>');
  await page.addStyleTag({path:path.resolve(__dirname,'../desktop/ui/map/mindmap.css')});
  await page.addScriptTag({path:path.resolve(__dirname,'../desktop/ui/map/map-fonts.js')});
  await page.addScriptTag({path:path.resolve(__dirname,'../desktop/ui/map/mindmap.js')});
  await page.evaluate(m=>SolFlowMap.mount(document.getElementById('map'),m,{lang:'ru',save:async m=>m,export:async()=>{}}),model);
  for(const format of ['svg','png']) {
   const bytes=await page.evaluate(async({m,f})=>Array.from(await SolFlowMap.exportData(m,f)),{m:model,f:format});
   fs.writeFileSync(path.join(output,'map.'+format),Buffer.from(bytes));
  }
  await page.screenshot({path:path.join(output,'desktop.png'),fullPage:true});
  await page.setViewportSize({width:390,height:844});
  assert(await page.evaluate(()=>document.documentElement.scrollWidth<=innerWidth));
  await page.screenshot({path:path.join(output,'phone-layout.png')});
  await page.getByRole('button',{name:'Масштаб +',exact:true}).click();
  const stress=await page.evaluate(async()=>{
   const m={title:'Очень длинная тема '.repeat(6),branches:Array.from({length:12},(_,i)=>({title:'Заголовок ветки '.repeat(6),points:Array.from({length:8},()=>('Много деталей и длинных слов '.repeat(22)).slice(0,600))}))};
   const rendered=SolFlowMap.svg(m),png=await SolFlowMap.exportData(m,'png');
   const blob=new Blob([png],{type:'image/png'}),bitmap=await createImageBitmap(blob);
   const result={pixels:bitmap.width*bitmap.height,watermark:rendered.text.includes('id="solflow-watermark"'),height:rendered.height};bitmap.close();return result;
  });
  assert(stress.pixels<=4005000);assert(stress.watermark);assert.deepEqual(errors,[]);
  fs.writeFileSync(path.join(output,'validation.json'),JSON.stringify({stress,errors},null,2));
  console.log('Desktop + phone layout, real map PNG/SVG, maximum-size PNG memory bound passed');
 }finally{await browser.close();}
})().catch(e=>{console.error(e);process.exit(1)});
