// Real UI in isolated Chromium. Synthetic audio/text, mocked Tauri, no network.
const { chromium } = require(process.env.PLAYWRIGHT_MODULE || 'playwright');
const { pathToFileURL } = require('node:url');
const path = require('node:path');
const fs = require('node:fs');
const assert = require('node:assert/strict');
(async () => {
  const output = process.argv[2]; fs.mkdirSync(output, { recursive: true });
  const browser = await chromium.launch({ headless: true, executablePath: process.env.CHROME_BINARY });
  try {
    for (const [language, theme, width] of [['ru', 'dark', 1440], ['en', 'light', 920], ['ru', 'light', 700]]) {
      const context = await browser.newContext({ viewport: { width, height: 920 } });
      const page = await context.newPage(); const errors = [];
      page.on('pageerror', e => errors.push(e.message));
      await page.route(/^https?:/, route => route.abort());
      await page.addInitScript(({ language, theme }) => {
        localStorage.setItem('introSeen', '1');
        localStorage.setItem('whatsnewSeen', '0.9.8-3');
        localStorage.setItem('solflow-lang', language);
        const listeners = {}; window.testCalls = [];
        const settings = { language, theme, history_limit: 50 };
        let map = { title: 'Sol Flow: следующая версия', source:'synthetic', revised:1,
          branches:[
            {title:'Быстрая диктовка',points:['Вставлять текст после распознавания.','Глубокую правку запускать отдельно.']},
            {title:'Говорящие',points:['Проверять смешанные фрагменты.','Разрешить ручное назначение голоса.']},
            {title:'Карта записи',points:['Выделять главную тему и подтемы.','Редактировать карточки и сохранять PNG/SVG.']},
            {title:'Android',points:['Не загружать Qwen на телефон.','Показывать карту с компьютера.']},
            {title:'Проверка',points:['Протестировать на Mac, Windows и Android.','Сохранить прошлую версию перед обновлением.']},
            {title:'Публикация',points:['Дождаться подтверждения релиза.']}
          ]};
        const initialMap=structuredClone(map);let mapError=null;
        window.mapTest={
          clear:()=>{map=null;mapError=null;},
          fail:()=>{mapError='Synthetic generation failure';listeners['solflow-map-error']?.({payload:{id:123,error:mapError}});},
          complete:()=>{map=structuredClone(initialMap);mapError=null;listeners['solflow-map-ready']?.({payload:123});}
        };
        const meeting = { id: 123, title: 'Обсуждение новой версии', at: 1788367931560, seconds: 16,
          state: 'done', imported: true, speakers: 2, names: { '0': 'Иван', '1': 'Анна' }, audio: true,
          phase: null, summary: '## Договорились\n- Проверить диаризацию на двух устройствах.', project: null };
        let segments = [
          { s: 0, e: 4, text: 'Давайте проверим запись. Согласна, начинаем.', voices: [0, 1] },
          { s: 4, e: 8, text: 'Мне важно быстро находить спорные места и слушать их.', spk: 0 },
          { s: 8, e: 12, text: 'А затем можно выбрать, кто произнёс эту реплику.', spk: 1 },
          { s: 12, e: 16, text: 'После этого проверим результат на телефоне.' },
        ];
        // A valid silent 16-second WAV exercises seek/play/pause without user audio.
        const buffer = new ArrayBuffer(44 + 16000 * 2 * 16), v = new DataView(buffer);
        const str = (at, s) => [...s].forEach((c, i) => v.setUint8(at + i, c.charCodeAt(0)));
        str(0, 'RIFF'); v.setUint32(4, buffer.byteLength - 8, true); str(8, 'WAVEfmt ');
        v.setUint32(16, 16, true); v.setUint16(20, 1, true); v.setUint16(22, 1, true);
        v.setUint32(24, 16000, true); v.setUint32(28, 32000, true); v.setUint16(32, 2, true); v.setUint16(34, 16, true);
        str(36, 'data'); v.setUint32(40, buffer.byteLength - 44, true);
        const audioUrl = URL.createObjectURL(new Blob([buffer], { type: 'audio/wav' }));
        window.__TAURI__ = {
          event: { listen: async (event, fn) => { listeners[event] = fn; return () => {}; } },
          core: { convertFileSrc: () => audioUrl, invoke: async (command, args) => {
            window.testCalls.push([command, args]);
            switch (command) {
              case 'get_settings': return settings;
              case 'downloader_ready': return true;
              case 'meeting_import_url': return null;
              case 'app_version': return '0.9.8';
              case 'check_update': return { current: '0.9.8', latest: '0.9.8' };
              case 'os_name': return 'macOS'; case 'machine_chip': return 'Apple Silicon';
              case 'meetings_list': return [meeting];
              case 'meeting_segments': return structuredClone(segments);
              case 'meeting_audio_path': return '/synthetic/audio.wav';
              case 'meeting_assign_speaker': {
                if (JSON.stringify(segments[args.index]) !== JSON.stringify(args.expected)) throw new Error('stale edit');
                segments[args.index] = { ...segments[args.index], spk: args.speaker, voices: [] }; return;
              }
              case 'meeting_rename_speaker': meeting.names[args.speaker] = args.name; return;
              case 'meeting_map': return {map:structuredClone(map),stale:false,error:mapError};
              case 'meeting_map_save':
                if(JSON.stringify(args.expected)!==JSON.stringify(map)) throw Error('stale map');
                map={...args.map,revised:map.revised+1};return;
              case 'meeting_map_export': return '/synthetic/map.'+args.format;
              case 'meeting_extras': return { kind: 'meeting', items: {} };
              case 'summary_state': return [true, 2440]; case 'diarize_status': return [true, 27];
              case 'list_models': case 'list_languages': case 'projects_list': case 'history_list':
              case 'list_input_devices': case 'meeting_translations': case 'meeting_breakdowns': case 'meeting_qa': case 'translate_languages': return [];
              case 'sync_status': return { connected: false, running: false, configured_yandex: false, configured_google: false };
              case 'meetings_audio_usage': return 0;
              case 'ui_state': queueMicrotask(() => listeners['solflow-state']?.({ payload: { phase: 'ready', model: 'GigaAM', accessibility: true } })); return;
              default: return false;
            }
          } },
        };
      }, { language, theme });
      await page.goto(pathToFileURL(path.resolve(__dirname, '../desktop/ui/index.html')).href);
      await page.locator('[data-page="meetings"]').click();
      // The production entry point also exercises rendering of the library row.
      await page.evaluate(() => openMeeting(123));
      await page.locator('#meetSegments .segment').first().waitFor();
      assert.equal(await page.locator('#meetSegments .segment').count(), 4);
      assert.equal(await page.locator('.needs-review').count(), 1);
      await page.waitForFunction(() => document.getElementById('meetAudio').readyState >= 1);
      await page.locator('.segment-clock').nth(1).click();
      await page.waitForFunction(() => !document.getElementById('meetAudio').paused);
      assert((await page.locator('#meetAudio').evaluate(e => e.currentTime)) >= 4);
      await page.locator('#meetSpeed').selectOption('1.5');
      assert.equal(await page.locator('#meetAudio').evaluate(e => e.playbackRate), 1.5);
      await page.locator('#meetRewind').click();
      assert((await page.locator('#meetAudio').evaluate(e => e.currentTime)) < 1);
      await page.locator('#meetAudio').evaluate(e => e.pause());
      await page.locator('#meetSpeakerFilter').selectOption('review');
      await page.waitForFunction(() => document.querySelectorAll('#meetSegments .segment').length === 2);
      await page.locator('.segment-voice').first().selectOption('1');
      await page.waitForFunction(() => document.querySelectorAll('#meetSegments .segment').length === 1);
      const call = await page.evaluate(() => testCalls.find(([c]) => c === 'meeting_assign_speaker'));
      assert.equal(call[1].speaker, 1); assert.equal(call[1].index, 0);
      assert.equal(call[1].expected.text, 'Давайте проверим запись. Согласна, начинаем.');
      await page.locator('#meetSpeakerFilter').selectOption('');
      await page.waitForFunction(() => document.querySelectorAll('#meetSegments .segment').length === 4);
      if (width < 1320) {
        await page.locator('#meetAnalysisTab').click();
        assert(await page.locator('#meetSummaryText').isVisible());
        assert(!(await page.locator('#meetTranscript').isVisible()));
        await page.locator('#meetTextTab').click();
      } else assert(await page.locator('#meetSummaryText').isVisible());
      assert(await page.locator('#speakersPanel').isVisible());
      await page.locator('#speakersPanel summary').click();
      assert(await page.locator('#speakerName0').isVisible());
      await page.locator('#speakersPanel summary').click();
      assert(await page.evaluate(() => document.documentElement.scrollWidth <= innerWidth));
      await page.screenshot({ path: path.join(output, `workbench-${language}-${theme}-${width}.png`), fullPage: true });
      assert.equal(await page.locator('#meetMap svg').count(),1);
      assert(await page.locator('#meetMap').evaluate(e=>e.parentElement.contains(document.getElementById('meetSummary'))));
      await page.locator('#meetMap').click();
      await page.locator('#recordingMapDialog[open]').waitFor();
      assert.equal(await page.locator('#solflow-watermark').count(),1);
      await page.screenshot({path:path.join(output,`map-${language}-${width}.png`)});
      const toolbar=page.locator('.sf-map-toolbar');
      await toolbar.getByRole('button',{name:language==='ru'?'Изменить':'Edit',exact:true}).click();
      await page.locator('.sf-map-editor input').first().fill('План развития Sol Flow');
      await page.locator('.sf-map-editor button[type="submit"]').click();
      await page.waitForFunction(()=>document.querySelector('.sf-map-editor').hidden);
      for(const format of ['SVG','PNG']) {
        await toolbar.getByRole('button',{name:format,exact:true}).click();
        await page.waitForFunction(f=>testCalls.some(([c,a])=>c==='meeting_map_export'&&a.format===f),format.toLowerCase());
        const data=await page.evaluate(f=>testCalls.find(([c,a])=>c==='meeting_map_export'&&a.format===f)[1].bytes,format.toLowerCase());
        const bytes=Buffer.from(data);fs.writeFileSync(path.join(output,`export-${language}.${format.toLowerCase()}`),bytes);
        if(format==='SVG') {assert(bytes.toString().includes('solflow-watermark'));assert(bytes.toString().includes('План развития'));}
        else assert.deepEqual([...bytes.subarray(0,8)],[137,80,78,71,13,10,26,10]);
      }
      assert((await page.locator('.sf-map-note').textContent()).includes('/synthetic/map.png'));
      await page.locator('#recordingMapClose').click();
      await page.evaluate(async()=>{mapTest.clear();await renderRecordingMap(meetRows.find(r=>r.id===123));});
      await page.locator('#meetMap').click();
      await page.locator('#meetMapCard[data-state="working"]').waitFor();
      await page.evaluate(()=>mapTest.fail());
      await page.locator('#meetMapCard[data-state="error"]').waitFor();
      assert((await page.locator('#meetMapHint').textContent()).includes('Synthetic generation failure'));
      await page.evaluate(async()=>renderRecordingMap(meetRows.find(r=>r.id===123)));
      assert((await page.locator('#meetMapHint').textContent()).includes('Synthetic generation failure'));
      await page.locator('#meetMapCardAction').click();
      await page.evaluate(()=>mapTest.complete());
      await page.locator('#recordingMapDialog[open]').waitFor();
      await page.locator('#recordingMapClose').click();
      await page.locator('#meetMapCard[data-state="ready"]').waitFor();
      assert(await page.locator('#meetMapPreview').isVisible());
      const safety=await page.evaluate(()=>{
        const map={title:'<script>alert(1)</script>',branches:[{title:'A & B',points:['<image href="https://evil.invalid">']} ]};
        const result=SolFlowMap.svg(map);const dom=new DOMParser().parseFromString(result.text,'image/svg+xml');
        return {scripts:dom.querySelectorAll('script,image,parsererror').length,text:dom.querySelector('title').textContent};
      });
      assert.equal(safety.scripts,0);assert.equal(safety.text,'<script>alert(1)</script>');
      if(width<1320)await page.locator('#meetTextTab').click();
      await page.locator('.segment-clock').first().click();
      await page.locator('#meetBack').click();
      assert(await page.locator('#meetAudio').evaluate(e => e.paused && !e.getAttribute('src')));
      await page.locator('#meetUrl').fill('https://www.youtube.com/watch?v=demo');
      assert(await page.locator('#youtubeSession').isVisible());
      assert.equal(await page.locator('#youtubeBrowser').inputValue(), '');
      await page.locator('#meetUrlGo').click();
      await page.waitForFunction(()=>testCalls.some(([c])=>c==='meeting_import_url'));
      assert.equal(await page.evaluate(()=>testCalls.filter(([c])=>c==='meeting_import_url').at(-1)[1].browser),null);
      assert.equal(await page.locator('#meetUrl').inputValue(),'https://www.youtube.com/watch?v=demo');
      await page.locator('#meetUrl').fill('https://youtu.be/demo2');
      await page.locator('#youtubeBrowser').selectOption('firefox');
      await page.locator('#meetUrlGo').click();
      await page.waitForFunction(()=>testCalls.filter(([c])=>c==='meeting_import_url').length===2);
      assert.equal(await page.evaluate(()=>testCalls.filter(([c])=>c==='meeting_import_url').at(-1)[1].browser),'firefox');
      assert.equal(await page.locator('#youtubeBrowser').inputValue(),'');
      assert(await page.locator('#youtubeSession').isVisible());
      await page.locator('#meetUrl').fill('https://rutube.ru/video/demo');
      assert(!(await page.locator('#youtubeSession').isVisible()));
      await page.locator('#meetUrl').fill('https://youtube.com.evil.test/watch');
      assert(!(await page.locator('#youtubeSession').isVisible()));
      await page.evaluate(()=>showPage('settings'));
      await page.locator('#installDownloader').waitFor();
      assert.equal(await page.locator('#installDownloader').textContent(),language==='en'?'Update downloader':'Обновить загрузчик');
      assert.deepEqual(errors, []);
      console.log(`${language}/${theme}/${width}: review, correction, seek, speed, pause, panes and layout passed`);
      await context.close();
    }
  } finally { await browser.close(); }
})().catch(e => { console.error(e); process.exitCode = 1; });
