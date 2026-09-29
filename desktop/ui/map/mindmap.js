/* Shared desktop/Android map renderer. Model text is always inert SVG text. */
(() => {
  const esc = s => String(s).replace(/[&<>"']/g, c => ({'&':'&amp;','<':'&lt;','>':'&gt;','"':'&quot;',"'":'&apos;'}[c]));
  const colors = { canvas:'#f4f3f1', ink:'#16171a', fog:'#6b6e70', card:'#ffffff', header:'#eceae6', line:'#ccceca', accent:'#437414', solid:'#7bc72e', center:'#1c1d20' };
  const font = 'Inter, Arial, sans-serif';
  const fonts=window.SolFlowMapFonts||{};
  const fontCss=Object.entries(fonts).map(([key,url])=>`@font-face{font-family:Inter;src:url(${url}) format('truetype');font-weight:${key==='medium'?500:400};}`).join('');
  const fontsReady=Promise.all(Object.entries(fonts).map(async([key,url])=>{
    const face=new FontFace('Inter',`url(${url})`,{weight:key==='medium'?'500':'400'});
    document.fonts.add(await face.load());
  })).catch(()=>{});
  function validate(m) {
    const valid = (s,n) => typeof s === 'string' && s.trim() && [...s].length <= n && !/[\x00-\x09\x0b-\x1f\x7f]/.test(s);
    if (!m || !valid(m.title,200) || !Array.isArray(m.branches) || m.branches.length < 1 || m.branches.length > 12 ||
      m.branches.some(b => !valid(b.title,160) || !Array.isArray(b.points) || b.points.length < 1 || b.points.length > 8 || b.points.some(p => !valid(p,600)))) throw Error('Invalid map');
    return m;
  }
  function svg(m) {
    validate(m);
    const ctx = document.createElement('canvas').getContext('2d');
    function wrap(text, width, size, bold=false) {
      ctx.font = `${bold ? '500 ' : ''}${size}px ${font}`;
      const lines=[]; let line='';
      for (const word of text.replace(/\s+/g,' ').trim().split(' ')) {
        if (line && ctx.measureText(line+' '+word).width <= width) { line+=' '+word; continue; }
        if (line) lines.push(line); line='';
        for (const char of word) {
          if (line && ctx.measureText(line+char).width > width) { lines.push(line); line=''; }
          line+=char;
        }
      }
      if (line) lines.push(line);
      return lines;
    }
    const W=1560, cardW=470, colX=[40,1050], gap=24;
    const cards=m.branches.map((b,i) => {
      const title=wrap(b.title,cardW-48,22,true), points=b.points.map(p=>wrap(p,cardW-72,19));
      return {b,i,title,points,head:26+title.length*28,h:26+title.length*28+22+points.reduce((n,p)=>n+p.length*26+12,0)};
    });
    const heights=[0,0]; cards.forEach(c=>{const side=c.i%2;c.x=colX[side];c.y=100+heights[side];heights[side]+=c.h+gap;});
    const H=Math.max(700,Math.max(...heights)+170), cy=(H-70)/2;
    const title=wrap(m.title,420,30,true), centerH=Math.max(170, title.length*39+76), centerY=cy-centerH/2;
    let out=`<svg xmlns="http://www.w3.org/2000/svg" width="${W}" height="${H}" viewBox="0 0 ${W} ${H}" role="img" aria-label="${esc(m.title)}"><title>${esc(m.title)}</title><style>${fontCss}</style><rect width="${W}" height="${H}" fill="${colors.canvas}"/><g font-family="${font}">`;
    out+=`<text x="40" y="48" font-size="15" letter-spacing="1" fill="${colors.fog}">Sol Flow</text>`;
    cards.forEach(c=>{const left=c.i%2===0, x1=left?550:1010,x2=left?510:1050,y=c.y+c.h/2;
      out+=`<path d="M${x1} ${cy} C${(x1+x2)/2} ${cy},${(x1+x2)/2} ${y},${x2} ${y}" fill="none" stroke="${colors.line}" stroke-width="2" opacity=".5"/>`;});
    out+=`<rect x="550" y="${centerY}" width="460" height="${centerH}" rx="10" fill="${colors.center}"/>`;
    title.forEach((line,i)=>{out+=`<text x="780" y="${cy-(title.length-1)*39/2+11+i*39}" text-anchor="middle" font-size="30" font-weight="500" fill="#ffffff">${esc(line)}</text>`;});
    cards.forEach(c=>{
      const color=colors.line;out+=`<rect x="${c.x}" y="${c.y}" width="${cardW}" height="${c.h}" rx="10" fill="${colors.card}" stroke="${color}" stroke-width="2"/><path d="M${c.x+10} ${c.y}H${c.x+cardW-10}Q${c.x+cardW} ${c.y} ${c.x+cardW} ${c.y+10}V${c.y+c.head}H${c.x}V${c.y+10}Q${c.x} ${c.y} ${c.x+10} ${c.y}" fill="${colors.header}"/>`;
      c.title.forEach((line,i)=>out+=`<text x="${c.x+24}" y="${c.y+30+i*28}" font-size="22" font-weight="500" fill="${colors.ink}">${esc(line)}</text>`);
      let y=c.y+c.head+32;
      c.points.forEach(lines=>{out+=`<circle cx="${c.x+25}" cy="${y-6}" r="4" fill="${colors.accent}"/>`;
        lines.forEach(line=>{out+=`<text x="${c.x+43}" y="${y}" font-size="19" fill="${colors.ink}">${esc(line)}</text>`;y+=26;});y+=12;});
    });
    // Reserved footer, always part of the SVG and every PNG derived from it.
    out+=`<g id="solflow-watermark" opacity=".46" transform="translate(${W-221},${H-64})"><rect width="44" height="44" rx="11" fill="#1c1d20"/>`;
    [[9,20,24],[14,16,28],[19,12,32],[24,16,28],[29,20,24]].forEach(([x,a,b])=>out+=`<rect x="${x}" y="${a}" width="4" height="${b-a}" rx="2" fill="#7bc72e"/>`);
    out+=`<text x="57" y="29" font-size="24" font-weight="500" fill="${colors.ink}">Sol Flow</text></g></g></svg>`;
    return {text:out,width:W,height:H};
  }
  async function exportData(m,format) {
    await fontsReady;
    const rendered=svg(m);
    if(format==='svg') return new TextEncoder().encode(rendered.text);
    if(format!=='png') throw Error('Invalid format');
    // Bound SVG image decode too, before allocating the PNG canvas (important on phones).
    const scale=Math.min(1,Math.sqrt(4000000/(rendered.width*rendered.height)),8192/rendered.width,8192/rendered.height);
    const width=Math.round(rendered.width*scale),height=Math.round(rendered.height*scale);
    const rasterSvg=rendered.text.replace(`width="${rendered.width}" height="${rendered.height}"`,`width="${width}" height="${height}"`);
    const blob=new Blob([rasterSvg],{type:'image/svg+xml'}),url=URL.createObjectURL(blob);
    try {
      const img=new Image(); await new Promise((resolve,reject)=>{img.onload=resolve;img.onerror=()=>reject(Error('Image export failed'));img.src=url;});
      const canvas=document.createElement('canvas');
      canvas.width=width;canvas.height=height;
      canvas.getContext('2d').drawImage(img,0,0,canvas.width,canvas.height);
      const png=await new Promise(resolve=>canvas.toBlob(resolve,'image/png'));
      if(!png) throw Error('Image export failed');return new Uint8Array(await png.arrayBuffer());
    } finally { URL.revokeObjectURL(url); }
  }
  const words={en:{'Карта сохранена в записи. PNG/SVG — отдельный экспорт в файл.':'The map is saved in this recording. PNG/SVG exports a separate file.','Экспорт отменен. Карта остается в записи.':'Export cancelled. The map remains in the recording.','Карта записи':'Recording map','Изменить':'Edit','Сохранить':'Save','Отмена':'Cancel','По ширине':'Fit width','Сверьте выводы с расшифровкой.':'Check the map against the transcript.','Расшифровка изменилась. Создайте карту заново.':'The transcript has changed. Generate a new map.','Заголовок':'Title','Подтема':'Topic','Пункты — по одному на строку':'One point per line','Сохранено':'Saved','Закрыть':'Close','Масштаб':'Zoom'}};
  function mount(root, initial, options={}) {
    const t=s=>options.lang==='ru'?s:(globalThis.SOLFLOW_LOCALES?.[options.lang]?.[words.en[s]]||words.en[s]||s);
    let model=JSON.parse(JSON.stringify(validate(initial))), zoom=1,editing=false;
    root.textContent='';root.classList.add('sf-map');
    const toolbar=document.createElement('div');toolbar.className='sf-map-toolbar';root.append(toolbar);
    const status=document.createElement('p');status.className='sf-map-note';status.setAttribute('role','status');
    status.textContent=t(options.stale?'Расшифровка изменилась. Создайте карту заново.':'Карта сохранена в записи. PNG/SVG — отдельный экспорт в файл.');root.append(status);
    const viewport=document.createElement('div');viewport.className='sf-map-viewport';viewport.tabIndex=0;root.append(viewport);
    const preview=document.createElement('div');preview.className='sf-map-image';viewport.append(preview);
    const editor=document.createElement('form');editor.className='sf-map-editor';editor.hidden=true;root.append(editor);
    function button(label,fn){const b=document.createElement('button');b.type='button';b.textContent=t(label);b.addEventListener('click',async()=>{b.disabled=true;try{await fn();}catch(e){status.textContent=String(e.message||e);}finally{b.disabled=false;}});toolbar.append(b);return b;}
    function draw(){preview.innerHTML=svg(model).text;preview.style.width=`${zoom*100}%`;preview.style.minWidth=`${zoom*360}px`;}
    const edit=button('Изменить',()=>{editing=true;editor.hidden=false;viewport.hidden=true;edit.hidden=true;editor.textContent='';
      function field(label,value,max,multiline=false){const wrap=document.createElement('label');wrap.textContent=t(label);const input=document.createElement(multiline?'textarea':'input');input.value=value;input.maxLength=max;if(multiline)input.rows=4;wrap.append(input);editor.append(wrap);return input;}
      const title=field('Заголовок',model.title,200);
      const fields=model.branches.map(b=>({title:field('Подтема',b.title,160),points:field('Пункты — по одному на строку',b.points.join('\n'),4807,true)}));
      const save=document.createElement('button');save.type='submit';save.textContent=t('Сохранить');editor.append(save);
      const cancel=document.createElement('button');cancel.type='button';cancel.textContent=t('Отмена');cancel.onclick=()=>{editing=false;editor.hidden=true;viewport.hidden=false;edit.hidden=false;};editor.append(cancel);
      editor.onsubmit=async e=>{e.preventDefault();save.disabled=true;try{
        const next=validate({...model,title:title.value.trim(),branches:fields.map(f=>({title:f.title.value.trim(),points:f.points.value.split('\n').map(s=>s.trim()).filter(Boolean)}))});
        model=await options.save(next,model)||next;editing=false;editor.hidden=true;viewport.hidden=false;edit.hidden=false;draw();status.textContent=t('Сохранено');
      }catch(e){status.textContent=String(e.message||e);}finally{save.disabled=false;}};
      title.focus();
    });
    edit.disabled=!options.save||options.stale;
    button('−',()=>{zoom=Math.max(.5,zoom-.25);draw();}).setAttribute('aria-label',t('Масштаб')+' −');
    button('+',()=>{zoom=Math.min(3,zoom+.25);draw();}).setAttribute('aria-label',t('Масштаб')+' +');
    button('По ширине',()=>{zoom=1;draw();});
    ['PNG','SVG'].forEach(format=>button(format,async()=>{
      if(editing) {status.textContent=t('Сохранить')+' / '+t('Отмена');return;}
      const message=await options.export(await exportData(model,format.toLowerCase()),format.toLowerCase(),model.title);
      status.textContent=message||t('Экспорт отменен. Карта остается в записи.');
    }));
    draw();fontsReady.then(()=>{if(root.isConnected&&!editing)draw();});return {dirty:()=>editing};
  }
  window.SolFlowMap={svg,validate,exportData,mount};
})();
