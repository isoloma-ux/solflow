// Real browser + real UI, mocked Tauri only. No user data or network access.
const { chromium } = require(process.env.PLAYWRIGHT_MODULE || 'playwright');
const { pathToFileURL } = require('node:url');
const path = require('node:path');
const assert = require('node:assert/strict');
const fs = require('node:fs');
(async () => {
  const output = process.argv[2];
  fs.mkdirSync(output, { recursive: true });
  const browser = await chromium.launch({ headless: true, executablePath: process.env.CHROME_BINARY });
  try {
    for (const language of ['ru', 'en']) {
      const context = await browser.newContext({ viewport: { width: 1000, height: 850 }, locale: language });
      const page = await context.newPage();
      const errors = [];
      page.on('pageerror', e => errors.push(e.message));
      await page.route(/^https?:/, route => route.abort());
      await page.addInitScript(({ language }) => {
        localStorage.setItem('introSeen', '1');
        localStorage.setItem('whatsnewSeen', '0.9.7-3');
        localStorage.setItem('solflow-lang', language);
        const listeners = {};
        window.testCalls = [];
        const settings = { language, theme: 'dark', coherent_dictation: false, history_limit: 50 };
        const source = 'Я хочу понять. Насколько это удобно.';
        let entries = [{ at: 1234567890000, text: source, audio: false }];
        Object.defineProperty(navigator, 'clipboard', { value: { writeText: async value => { window.copied = value; } } });
        const emit = (event, payload) => listeners[event]?.({ payload });
        window.__TAURI__ = {
          event: { listen: async (event, fn) => { listeners[event] = fn; return () => {}; } },
          core: { invoke: async (command, args) => {
            window.testCalls.push([command, args]);
            switch (command) {
              case 'get_settings': return settings;
              case 'set_option': settings[args.key] = args.value; return;
              case 'app_version': return '0.9.7';
              case 'check_update': return { current: '0.9.7', latest: '0.9.7' };
              case 'os_name': return 'macOS';
              case 'machine_chip': return 'Apple Silicon';
              case 'list_models': case 'list_languages': case 'meetings_list':
              case 'projects_list': case 'list_input_devices': return [];
              case 'history_list': return entries;
              case 'history_format':
                entries = [{ ...entries[0], text: 'Я хочу понять, насколько это удобно.', original_text: source, previous_text: source, punctuation_status: 'applied' }];
                setTimeout(() => emit('solflow-history'), 20); return;
              case 'sync_status': return { connected: false, running: false, configured_yandex: false, configured_google: false };
              case 'meetings_audio_usage': return 0;
              case 'ui_state':
                queueMicrotask(() => emit('solflow-state', { phase: 'ready', model: 'GigaAM', accessibility: true })); return;
              default: return false;
            }
          } },
        };
      }, { language });
      await page.goto(pathToFileURL(path.resolve(__dirname, '../desktop/ui/index.html')).href);
      await page.locator('[data-page="settings"]').click();
      await page.locator('#coherentDictation').scrollIntoViewIfNeeded();
      await page.locator('#coherentDictation').click();
      assert(await page.locator('#coherentDictation').evaluate(e => e.classList.contains('on')));
      assert(await page.evaluate(() => testCalls.some(([c, a]) => c === 'set_option' && a.key === 'coherent_dictation' && a.value === true)));
      await page.waitForTimeout(300); // let existing toggle animation finish
      await page.screenshot({ path: path.join(output, `settings-${language}.png`) });
      await page.locator('[data-page="history"]').click();
      await page.locator('.history-format').click();
      await page.locator('.history-source').first().waitFor();
      await page.locator('.history-source summary').first().click();
      await page.locator('.history-source .text-button').first().click();
      assert.equal(await page.evaluate(() => copied), 'Я хочу понять. Насколько это удобно.');
      assert.equal(await page.locator('.history-body > .history-text').textContent(), 'Я хочу понять, насколько это удобно.');
      await page.screenshot({ path: path.join(output, `history-${language}.png`) });
      assert.deepEqual(errors, [], JSON.stringify(errors));
      console.log(`${language}: toggle, stored-entry formatting, original/previous text, copy and rendering passed`);
      await context.close();
    }
  } finally { await browser.close(); }
})().catch(e => { console.error(e); process.exitCode = 1; });
