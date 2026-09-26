// Local permission controls; no client connection is made by opening this panel.
(() => {
  const get = id => document.getElementById(id);
  const call = window.__TAURI__.core.invoke;
  const fields = {transcript: 'mcpTranscript', summary: 'mcpSummary', map: 'mcpMap', analyses: 'mcpAnalyses'};
  let project = null, serial = 0, busy = false, loaded = false;
  let grants = {}, latestStatus = {};
  let clientConfig = null;
  const drafts = new Map();
  const enabled = grant => Object.keys(fields).some(key => !!grant?.[key]);
  const equal = (a, b) => Object.keys(fields).every(key => !!a?.[key] === !!b?.[key]);
  window.solflowMcpEnabled = id => enabled(grants[id]);
  window.solflowMcpDecorateProject = (item, id) => {
    const enabled = id != null && Object.keys(fields).some(key => grants[id]?.[key]);
    item.classList.toggle('mcp-enabled', enabled);
    item.querySelector('.nav-ai-badge')?.remove();
    if (id == null) return;
    const explanation = t('Проект доступен ИИ через MCP');
    item.title = (enabled ? explanation + '. ' : '') + t('Двойной клик — переименовать, правая кнопка — меню');
    if (!enabled) return;
    const badge = document.createElement('span');
    badge.className = 'nav-ai-badge';
    badge.setAttribute('role', 'img');
    badge.setAttribute('aria-label', explanation);
    badge.title = explanation;
    badge.innerHTML = '<svg viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.5" aria-hidden="true"><path d="M8 1.5 9.8 6.2 14.5 8l-4.7 1.8L8 14.5 6.2 9.8 1.5 8l4.7-1.8Z"/></svg>';
    const label = document.createElement('span');
    label.textContent = t('ИИ');
    badge.appendChild(label);
    item.insertBefore(badge, item.querySelector('.nav-count'));
  };
  function applyStatus(status) {
    if (!status?.grants) return;
    grants = status.grants;
    latestStatus = status;
    loaded = true;
    document.querySelectorAll('#navProjects .nav-project').forEach(item => {
      window.solflowMcpDecorateProject(item, item.dataset.project || null);
    });
    render();
  }
  function message(text = '', error = false) {
    get('mcpFeedback').textContent = text;
    get('mcpFeedback').hidden = !text;
    get('mcpFeedback').classList.toggle('error', error);
  }
  function render() {
    const saved = grants[project] || {};
    const active = enabled(saved);
    const draft = drafts.get(project) || (active ? saved : {transcript:true});
    const dirty = !equal(draft, saved);
    Object.entries(fields).forEach(([key,id]) => { get(id).checked = !!draft[key]; });
    get('mcpPanel').querySelectorAll('button,input').forEach(el => { el.disabled = busy || !loaded; });
    get('mcpState').textContent = t(!loaded ? 'Проверяем доступ…' : active ? 'Доступ включен' : 'Доступ выключен');
    get('mcpPanel').classList.toggle('access-on', active);
    get('mcpSave').textContent = t(active ? 'Сохранить изменения' : 'Подключить проект');
    get('mcpSave').disabled = busy || !loaded || !enabled(draft) || (active && !dirty);
    get('mcpRevoke').hidden = !active;
    get('mcpRefresh').disabled = busy || !loaded || !active;
    get('mcpTextCopy').disabled = busy || !loaded || !Object.values(grants).some(enabled);
    get('mcpStatus').textContent = t(!loaded ? 'Проверяем доступ…' : !enabled(draft) ? 'Выберите хотя бы один тип материалов.' :
      !active ? 'Доступ выключен. Выбранные материалы откроются после нажатия «Подключить проект».' :
      dirty ? 'Выбор изменен. Нажмите «Сохранить изменения», чтобы применить его.' :
      latestStatus.stale ? 'Материалы требуют обновления' : 'Разрешения сохранены. Готовые материалы обновляются автоматически.');
  }
  async function load() {
    const request = ++serial;
    try { const status = await call('mcp_status'); if(request === serial) applyStatus(status); }
    catch(error) { if(request === serial) message(t(String(error)), true); }
  }
  window.solflowMcpProject = current => {
    const next = current?.id || null;
    get('mcpPanel').hidden = !next;
    if (next === project) return;
    project = next; ++serial;
    get('mcpConfigBox').hidden = true;
    get('mcpConfig').setAttribute('aria-expanded', 'false');
    get('mcpConfigText').value = '';
    message(); render();
    if (project) load();
  };
  async function run(operation, success = '') {
    if (!project || busy || !loaded) return;
    const selected = project;
    busy = true; ++serial; render(); message(t('Сохраняю разрешения…'));
    try {
      const result = await operation(selected);
      if(result?.grants) { ++serial; applyStatus(result); }
      if(project === selected) message(success ? t(success) : '');
    } catch(error) { if(project === selected) message(t(String(error)), true); }
    finally { busy = false; render(); }
  }
  Object.values(fields).forEach(id => {
    get(id).onchange = () => {
      drafts.set(project, Object.fromEntries(Object.entries(fields).map(([key,id]) => [key,get(id).checked])));
      message(); render();
    };
  });
  get('mcpSave').onclick = () => {
    const grant = Object.fromEntries(Object.entries(fields).map(([key,id]) => [key,get(id).checked]));
    if(!enabled(grant)) return;
    const success = enabled(grants[project]) ? 'Изменения сохранены' : 'Доступ к проекту включен';
    run(async id => {
      const result = await call('mcp_set_access', {projectId:id, grant});
      drafts.delete(id);
      return result;
    }, success);
  };
  const revoke = () => run(async id => {
    const previous = grants[id];
    const result = await call('mcp_set_access', {projectId:id, grant:{}});
    if(enabled(previous)) drafts.set(id, {...previous});
    return result;
  }, 'Доступ к проекту отключен');
  get('mcpRevoke').onclick = revoke;
  window.solflowMcpOpen = async (id, disconnect = false) => {
    if(busy) return;
    projectFilter = id;
    closeMeeting(); showPage('meetings'); renderProjects(); renderMeetings();
    window.solflowMcpProject(meetProjects.find(p => p.id === id));
    get('mcpPanel').open = true;
    await load();
    if(project !== id) return;
    if(disconnect && enabled(grants[id])) await revoke();
    get('mcpPanel').scrollIntoView({block:'center'});
    if(!disconnect) get('mcpSave').focus();
  };
  get('mcpRefresh').onclick = () => run(() => call('mcp_refresh'), 'Материалы обновлены');
  function renderClientConfig() {
    if (!clientConfig) return;
    const server = clientConfig.mcpServers.solflow;
    const client = get('mcpClient').value;
    let config, help;
    if (client === 'claude') {
      config = JSON.stringify(clientConfig, null, 2);
      help = 'Добавьте запись solflow в mcpServers конфигурации Claude Desktop, сохранив остальные подключения, затем перезапустите Claude. Sol Flow не изменяет настройки Claude автоматически.';
    } else if (client === 'other') {
      config = JSON.stringify(server, null, 2);
      help = 'Создайте локальное подключение MCP (STDIO) в своем клиенте. Ниже указаны путь к серверу (command) и аргументы (args). Формат настроек зависит от клиента.';
    } else {
      config = '[mcp_servers.solflow]\ncommand = ' + JSON.stringify(server.command) + '\nargs = ' + JSON.stringify(server.args) + '\nenabled = true';
      help = 'В приложении ChatGPT или Codex откройте Settings → MCP servers. Добавьте локальный сервер STDIO с параметрами ниже или внесите этот блок в ~/.codex/config.toml, сохранив другие подключения. Для уже добавленного solflow нажмите Restart. Это инструкция для приложения на компьютере; ChatGPT в браузере требует отдельного подключения.';
    }
    get('mcpConfigText').value = config;
    get('mcpClientHelp').textContent = t(help);
  }
  get('mcpClient').onchange = renderClientConfig;
  get('mcpConfig').onclick = async () => {
    if(busy) return;
    if (!get('mcpConfigBox').hidden) {
      get('mcpConfigBox').hidden = true;
      get('mcpConfig').setAttribute('aria-expanded', 'false');
      return;
    }
    const selected = project;
    try {
      const config = JSON.parse(await call('mcp_claude_config'));
      const server = config?.mcpServers?.solflow;
      if (typeof server?.command !== 'string' || !Array.isArray(server.args) || !server.args.every(arg => typeof arg === 'string')) throw new Error(t('Не удалось получить настройки MCP'));
      if (selected !== project) return;
      clientConfig = config;
      renderClientConfig();
      get('mcpConfigBox').hidden = false;
      get('mcpConfig').setAttribute('aria-expanded', 'true');
    }
    catch(error) {message(t(String(error)), true);}
  };
  get('mcpCopyConfig').onclick = async () => {
    try {await navigator.clipboard.writeText(get('mcpConfigText').value);message(t('Конфигурация скопирована'));}
    catch(error){message(t(String(error)), true);}
  };
  get('mcpTextCopy').onclick = async () => {
    if(busy) return;
    busy = true; render();
    try { const path = await call('mcp_text_copy'); message(t('Текстовая копия сохранена: {0}',path)); }
    catch(error) {message(t(String(error)), true);}
    finally {busy = false; render();}
  };
  window.__TAURI__.event.listen('solflow-mcp', () => { if (!busy) load(); });
  window.solflowMcpProject(meetProjects.find(p => p.id === projectFilter));
  if (!project) load();

})();

// Open a recording locally, never fetch a URL supplied by a document.
async function openMcpSources(urls) {
  for (const raw of urls || []) {
    let url;
    try {url = new URL(raw);} catch {continue;}
    if (url.protocol !== 'solflow:' || url.hostname !== 'recording' ||
        url.username || url.password || url.port || !/^\/[0-9]{1,16}$/.test(url.pathname)) continue;
    const id = Number(url.pathname.slice(1));
    const seconds = Number(url.searchParams.get('t') || 0);
    if (!Number.isSafeInteger(id) || id <= 0 || !Number.isFinite(seconds) || seconds < 0) continue;
    await refreshMeetings();
    const meeting = meetRows.find(row => row.id === id);
    if (!meeting) continue;
    showPage('meetings');
    openMeeting(id);
    await loadMcpAudio(meeting, seconds);
  }
}
async function loadMcpAudio(meeting, seconds) {
  // Seeking never starts playback or microphone capture automatically.
  if (!meeting.audio) return;
  await setupMeetingAudio(meeting);
  if (detailId !== meeting.id) return;
  const audio = document.getElementById('meetAudio');
  const seek = () => {if(detailId === meeting.id) audio.currentTime = Math.min(seconds, Number.isFinite(audio.duration) ? audio.duration : seconds);};
  if (audio.readyState >= 1) seek();
  else audio.addEventListener('loadedmetadata', seek, {once:true});
}
window.__TAURI__.event.listen('deep-link://new-url', e => openMcpSources(e.payload).catch(console.error));
window.__TAURI__.core.invoke('mcp_start_links').then(openMcpSources).catch(console.error);
