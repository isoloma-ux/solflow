// Окно Sol Flow: слушает состояние из Rust и рисует волну. Вся логика
// записи и распознавания живёт на нативной стороне — окно можно закрыть,
// хоткей продолжит работать.

const { invoke } = window.__TAURI__.core;
const { listen } = window.__TAURI__.event;

const el = (id) => document.getElementById(id);

// Язык применяем до первой отрисовки: настройки приедут из Rust чуть позже,
// а показывать полсекунды русский текст английскому человеку не хочется.
// Поэтому выбор запоминается ещё и здесь, рядом с окном.
UI_LANG = localStorage.getItem("solflow-lang") || systemLanguage();
translateDocument();

// Какая система под окном: значки клавиш, «Универсальный доступ» и часть
// подписей на Windows выглядят иначе. Берём из user-agent — он приходит
// вместе с окном, ещё до первого ответа из Rust.
const IS_MAC = navigator.userAgent.includes("Mac");

const statusText = {
  no_model: t("Модель не найдена — положите .gguf в папку моделей"),
  loading: t("Загружаю модель в память"),
  ready: t("Готово. Нажмите кнопку или сочетание"),
  recording: t("Идет запись"),
  transcribing: t("Распознаю"),
};

// --- волна ---------------------------------------------------------------

const BARS = 36;
const levels = new Array(BARS).fill(0);
const wave = el("wave");
const ctx = wave.getContext("2d");

function drawWave() {
  const w = wave.width;
  const h = wave.height;
  ctx.clearRect(0, 0, w, h);
  ctx.fillStyle = getComputedStyle(document.documentElement)
    .getPropertyValue("--accent-solid").trim();
  const bar = w / (BARS * 2 - 1);
  const minH = h * 0.08;
  for (let i = 0; i < BARS; i++) {
    const value = levels[i];
    const bh = Math.max(minH, minH + (h - minH) * value);
    const x = i * bar * 2;
    const y = (h - bh) / 2;
    const r = bar / 2;
    ctx.beginPath();
    ctx.roundRect(x, y, bar, bh, r);
    ctx.fill();
  }
}

// --- состояние -----------------------------------------------------------

let phase = "loading";

function render(state) {
  phase = state.phase;
  el("modelName").textContent = state.model || "Sol Flow";
  const text = state.detail || statusText[state.phase] || "";
  el("status").textContent = text;
  el("sidebarStatus").textContent = text;
  if (state.hotkey_label && !capturing) {
    el("hotkeyLabel").textContent = state.hotkey_label;
    el("hotkeyLabel2").textContent = state.hotkey_label;
  }

  const recording = state.phase === "recording";
  el("record").disabled = state.phase !== "ready" && !recording;
  el("iconMic").hidden = recording;
  el("iconStop").hidden = !recording;
  el("pulse").classList.toggle("on", recording);
  el("wave").hidden = !recording;
  if (!recording) levels.fill(0);

  setPerm("permAccessibility", state.accessibility);

  // Чем считается модель — видно в подсказке переключателя: обещать
  // видеокарту и молча считать процессором нельзя.
  if (state.device) {
    lastDevice = state.device;
    const hint = el("gpuHint");
    if (hint) hint.textContent = t("Сейчас считает {0}", state.device);
  }
}

let lastDevice = null;

function setPerm(id, granted) {
  // Строки может не быть: «Универсальный доступ» на Windows убран совсем.
  const perm = el(id);
  if (!perm) return;
  perm.querySelector(".perm-done").hidden = !granted;
  perm.querySelector("button").hidden = granted;
}

listen("solflow-state", (e) => render(e.payload));
listen("solflow-level", (e) => {
  levels.shift();
  levels.push(e.payload);
  drawWave();
});
listen("solflow-result", (e) => {
  const text = e.payload;
  el("result").textContent = text || t("Ничего не распознано");
  el("result").hidden = false;
  el("copy").hidden = !text;
});
listen("solflow-history", () => {
  if (page === "history") refreshHistory();
});
listen("solflow-history-failed", (e) => {
  el("historyHint").textContent = t("Не вышло: {0}", e.payload);
});

el("record").addEventListener("click", () => invoke("ui_toggle"));
el("copy").addEventListener("click", () => {
  navigator.clipboard.writeText(el("result").textContent);
});
el("grantAccessibility")?.addEventListener("click", () =>
  invoke("open_accessibility")
);

// --- назначение сочетания -------------------------------------------------
// «Изменить» переводит поле в режим захвата: следующее нажатие с
// модификатором становится новым сочетанием.

let capturing = false;

el("changeHotkey").addEventListener("click", () => {
  capturing = !capturing;
  el("changeHotkey").textContent = capturing ? t("Отмена") : t("Изменить");
  el("hotkeyHint").textContent = capturing
    ? t("Нажмите новое сочетание")
    : t("Можно назначить своё");
  if (capturing) el("hotkeyLabel2").textContent = "…";
  else invoke("ui_state");
});

window.addEventListener("keydown", async (e) => {
  if (!capturing) return;
  e.preventDefault();
  // Одни модификаторы — ещё не сочетание.
  if (["Shift", "Control", "Alt", "Meta"].includes(e.key)) return;

  const parts = [];
  if (e.metaKey) parts.push("cmd");
  if (e.ctrlKey) parts.push("ctrl");
  if (e.altKey) parts.push("alt");
  if (e.shiftKey) parts.push("shift");
  if (parts.length === 0) {
    el("hotkeyHint").textContent = IS_MAC
      ? t("Нужен модификатор: ⌘, ⌥, ⌃ или ⇧")
      : t("Нужен модификатор: Ctrl, Alt или Shift");
    return;
  }

  const code = e.code
    .replace(/^Key/, "")
    .replace(/^Digit/, "")
    .toLowerCase();
  parts.push(code);

  try {
    const label = await invoke("set_hotkey", { combo: parts.join("+") });
    el("hotkeyLabel").textContent = label;
    el("hotkeyLabel2").textContent = label;
    el("hotkeyHint").textContent = t("Сохранено");
  } catch (err) {
    el("hotkeyHint").textContent = String(err);
  }
  capturing = false;
  el("changeHotkey").textContent = t("Изменить");
});
// --- каталог моделей ------------------------------------------------------

function sizeLabel(bytes) {
  const mb = bytes / 1e6;
  return mb >= 1000 ? t("{0} ГБ", (mb / 1000).toFixed(1)) : t("{0} МБ", Math.round(mb));
}

let modelRows = [];
let languageRows = [];
let allModelsShown = false;
let languageFilter = null; // код языка или null — «любой»
let onlyDownloaded = false;
let onlyStreaming = false;
let onlyTranslate = false;

/**
 * Пятёрка, с которой стоит начать: у каждой строки ярлык, чем она хороша.
 * Названия моделей ни о чём не говорят, а список из полусотни читать никто
 * не станет.
 */
/** Название языка с большой буквы: «Русский», «Английский». */
function languageName(code) {
  const row = languageRows.find((l) => l.code === code);
  if (!row) return code;
  return row.name.charAt(0).toUpperCase() + row.name.slice(1);
}

/** Лучшая одноязычная модель для языка — по точности. */
function bestFor(rows, code) {
  return rows
    .filter((m) => m.language_count === 1 && m.language_codes.includes(code))
    .sort((a, b) => b.accuracy - a.accuracy)[0];
}

/** Лучшая многоязычная — тоже по точности. */
function bestMulti(rows, code) {
  return rows
    .filter((m) => m.language_count > 1 && (!code || m.language_codes.includes(code)))
    .sort((a, b) => b.accuracy - a.accuracy)[0];
}

/**
 * Совет обычными словами. Главное, что нужно объяснить: баллы точности
 * сравнивают модели на общих многоязычных тестах, где русского почти нет.
 * Из-за этого GigaAM с её «69» разбирает русскую речь лучше, чем модель с
 * «90», обученная на английском, — без этой оговорки список вводит в
 * заблуждение.
 */
function modelAdvice(rows) {
  if (languageFilter) {
    const own = bestFor(rows, languageFilter);
    const multi = bestMulti(rows, languageFilter);
    const name = languageName(languageFilter);
    if (own) {
      let text =
        t("{0} язык: берите {1} — она обучена только ему и потому ", name, own.name) +
        t("разбирает его точнее многоязычных, даже если общий балл у них выше ") +
        t("(баллы считаются на многоязычных тестах).");
      if (multi) text += t(" Если нужны и другие языки — {0}.", multi.name);
      return text;
    }
    if (multi) {
      return t("{0} язык: отдельной модели под него нет, берите многоязычную — {1}.", name, multi.name);
    }
    return "";
  }

  const ru = bestFor(rows, "ru");
  const en = bestFor(rows, "en");
  const multi = bestMulti(rows, null);
  const parts = [];
  if (ru) parts.push(t("для русского — {0}", ru.name));
  if (en) parts.push(t("для английского — {0}", en.name));
  if (multi) parts.push(t("для смеси языков — {0}", multi.name));
  if (!parts.length) return "";
  return (
    t("Под один язык модели работают точнее: {0}. ", parts.join(", ")) +
    t("Баллы точности считаются на общих многоязычных тестах, поэтому у ") +
    t("одноязычной модели балл бывает ниже, а на своём языке она лучше.")
  );
}

function pickTop(shown) {
  const top = shown.slice(0, 5);
  const labels = new Map();
  if (!top.length) return { top, labels };

  const best = (key) =>
    top.reduce((a, b) => (b[key] > a[key] ? b : a));
  const lightest = top.reduce((a, b) => (b.size_bytes < a.size_bytes ? b : a));

  labels.set(best("accuracy").id, t("Точнее всех"));
  const fastest = best("speed");
  if (!labels.has(fastest.id)) labels.set(fastest.id, t("Быстрее всех"));
  if (!labels.has(lightest.id)) labels.set(lightest.id, t("Легче всех"));
  for (const m of top) {
    if (!labels.has(m.id)) {
      labels.set(
        m.id,
        languageFilter && m.language_count === 1
          ? `Идеально для этого языка`
          : t("Хороший баланс")
      );
    }
  }
  return { top, labels };
}

function renderModels() {
  const needle = el("modelSearch").value.trim().toLowerCase();
  const list = el("modelList");
  list.textContent = "";

  let shown = modelRows.filter((m) => {
    if (
      needle &&
      !m.name.toLowerCase().includes(needle) &&
      !m.description.toLowerCase().includes(needle) &&
      !m.languages.toLowerCase().includes(needle)
    ) return false;
    if (languageFilter && !m.language_codes.includes(languageFilter)) return false;
    if (onlyDownloaded && !m.downloaded) return false;
    if (onlyStreaming && !m.streaming) return false;
    if (onlyTranslate && !m.translate) return false;
    return true;
  });

  // Когда выбран конкретный язык, одноязычные модели идут первыми:
  // обученные на одном языке точнее многоязычных на нём, хотя их общий
  // балл ниже. Активная и скачанные — всегда сверху.
  const rank = (m) =>
    (m.active ? 8 : 0) +
    (m.downloaded ? 4 : 0) +
    (languageFilter && m.language_count === 1 ? 2 : 0);
  shown = shown.slice().sort((a, b) => rank(b) - rank(a) || b.accuracy - a.accuracy);

  const { top, labels } = pickTop(shown);
  const topBox = el("modelTop");
  topBox.textContent = "";
  for (const m of top) topBox.appendChild(modelRow(m, labels.get(m.id)));

  el("topHead").textContent = languageFilter
    ? t(
        "Что взять: {0} язык",
        UI_LANG === "en" ? languageName(languageFilter) : languageName(languageFilter).toLowerCase()
      )
    : t("Что взять");
  el("modelAdvice").textContent = modelAdvice(modelRows);

  // Остальные — под кнопкой: полсотни строк сразу читать невозможно.
  const rest = shown.slice(top.length);
  for (const m of rest) list.appendChild(modelRow(m));
  el("showAll").hidden = rest.length === 0;
  el("showAll").textContent = allModelsShown
    ? t("Свернуть список")
    : t("Показать остальные ({0})", rest.length);
  list.hidden = !allModelsShown;
}

/** Одна строка каталога; [label] — ярлык для рекомендованной пятёрки. */
function modelRow(m, label) {
  const row = document.createElement("div");
  row.className = "model";

  const text = document.createElement("div");
  text.className = "model-text";
  const name = document.createElement("p");
  name.className = "model-name";
  if (m.active) {
    const dot = document.createElement("span");
    dot.className = "bullet";
    name.appendChild(dot);
  }
  name.appendChild(document.createTextNode(m.name));
  if (label) {
    const tag = document.createElement("span");
    tag.className = "model-tag";
    tag.textContent = label;
    name.appendChild(tag);
  }

  // Умения показываем словами: «потоковая» и «с переводом» — то, по чему
  // модели и выбирают, а из названия этого не видно.
  const skills = [];
  if (m.streaming) skills.push(t("потоковая"));
  if (m.translate) skills.push(t("с переводом"));

  const meta = document.createElement("p");
  meta.className = "model-meta";
  const note = m.note ? ` — ${m.note}` : ` — ${m.description}`;
  const extra = skills.length ? ` · ${skills.join(", ")}` : "";
  meta.textContent = `${m.languages} · ${sizeLabel(m.size_bytes)}${extra}${note}`;
  text.append(name, meta);

  if (m.progress !== null && m.progress !== undefined) {
    const barBox = document.createElement("div");
    barBox.className = "model-progress";
    const bar = document.createElement("div");
    bar.style.width = `${m.progress}%`;
    barBox.appendChild(bar);
    text.appendChild(barBox);
  }
  row.appendChild(text);

  const button = document.createElement("button");
  button.className = "pill-inset compact";
  if (m.progress !== null && m.progress !== undefined) {
    button.textContent = t("Отмена");
    button.onclick = () => invoke("cancel_model", { filename: m.filename });
  } else if (m.active) {
    button.textContent = t("Активна");
    button.classList.add("active-mark");
    button.disabled = true;
  } else if (m.downloaded) {
    button.textContent = t("Выбрать");
    button.onclick = () => invoke("set_active_model", { filename: m.filename });
  } else {
    button.textContent = t("Скачать");
    button.onclick = () => invoke("download_model", { id: m.id });
  }
  row.appendChild(button);

  if (m.downloaded && !m.active) {
    const remove = document.createElement("button");
    remove.className = "model-remove";
    remove.textContent = t("Удалить");
    remove.onclick = () => invoke("delete_model", { filename: m.filename });
    row.appendChild(remove);
  }
  return row;
}

async function refreshModels() {
  modelRows = await invoke("list_models");
  renderModels();
  renderModelPick();
}

el("showAll").addEventListener("click", () => {
  allModelsShown = !allModelsShown;
  renderModels();
});

/** Список скачанных моделей внизу сайдбара: сменить одним нажатием. */
function renderModelPick() {
  const downloaded = modelRows.filter((m) => m.downloaded);
  const active = modelRows.find((m) => m.active);
  el("modelPickName").textContent = active ? active.name : t("Модель не выбрана");
  el("modelPick").disabled = downloaded.length === 0;

  const menu = el("modelPickMenu");
  menu.textContent = "";
  if (!downloaded.length) {
    const empty = document.createElement("p");
    empty.className = "lang-empty";
    empty.textContent = t("Ни одна модель не скачана");
    menu.appendChild(empty);
    return;
  }
  for (const m of downloaded) {
    const row = document.createElement("button");
    row.className = "lang-row" + (m.active ? " chosen" : "");
    const dot = document.createElement("span");
    dot.className = "bullet";
    const name = document.createElement("span");
    name.className = "lang-name";
    name.textContent = m.name;
    const size = document.createElement("span");
    size.className = "lang-count";
    size.textContent = sizeLabel(m.size_bytes);
    row.append(dot, name, size);
    row.onclick = () => {
      menu.hidden = true;
      if (!m.active) invoke("set_active_model", { filename: m.filename });
    };
    menu.appendChild(row);
  }
}

el("modelPick").addEventListener("click", (e) => {
  e.stopPropagation();
  const menu = el("modelPickMenu");
  menu.hidden = !menu.hidden;
});
document.addEventListener("click", () => {
  el("modelPickMenu").hidden = true;
});

// --- фильтры: язык и «Скачанные» ------------------------------------------
// Языков больше сотни, поэтому выбор — выпадающий список с поиском,
// как диалог на Android. Подчеркивание — только у включенного фильтра.

function renderFilters() {
  const lang = languageRows.find((l) => l.code === languageFilter);
  const title = lang
    ? lang.name.charAt(0).toUpperCase() + lang.name.slice(1)
    : t("Все языки");
  el("filterLanguage").textContent = `${title} ▾`;
  el("filterLanguage").classList.toggle("on", languageFilter !== null);
  el("filterDownloaded").classList.toggle("on", onlyDownloaded);
  el("filterStreaming").classList.toggle("on", onlyStreaming);
  el("filterTranslate").classList.toggle("on", onlyTranslate);
  renderModels();
}

function renderLanguages() {
  const needle = el("languageSearch").value.trim().toLowerCase();
  const list = el("languageList");
  list.textContent = "";

  const addRow = (title, count, code) => {
    const row = document.createElement("button");
    row.className = "lang-row" + (code === languageFilter ? " chosen" : "");
    const dot = document.createElement("span");
    dot.className = "bullet";
    const name = document.createElement("span");
    name.className = "lang-name";
    name.textContent = title.charAt(0).toUpperCase() + title.slice(1);
    row.append(dot, name);
    if (count !== null) {
      const models = document.createElement("span");
      models.className = "lang-count";
      models.textContent = count;
      row.appendChild(models);
    }
    row.onclick = () => {
      languageFilter = code;
      closeLanguagePanel();
      renderFilters();
    };
    list.appendChild(row);
  };

  if (!needle) addRow(t("Любой язык"), null, null);
  const matches = languageRows.filter(
    (l) => !needle || l.name.startsWith(needle) || l.code === needle
  );
  for (const lang of matches) addRow(lang.name, lang.models, lang.code);
  if (!matches.length && needle) {
    const empty = document.createElement("p");
    empty.className = "lang-empty";
    empty.textContent = t("Такого языка в каталоге нет");
    list.appendChild(empty);
  }
}

function closeLanguagePanel() {
  el("languagePanel").hidden = true;
}

el("filterLanguage").addEventListener("click", (e) => {
  e.stopPropagation();
  const panel = el("languagePanel");
  panel.hidden = !panel.hidden;
  if (!panel.hidden) {
    el("languageSearch").value = "";
    el("languageClear").hidden = true;
    renderLanguages();
    el("languageSearch").focus();
  }
});
el("languagePanel").addEventListener("click", (e) => e.stopPropagation());
document.addEventListener("click", closeLanguagePanel);
window.addEventListener("keydown", (e) => {
  if (e.key === "Escape") closeLanguagePanel();
});

el("filterDownloaded").addEventListener("click", () => {
  onlyDownloaded = !onlyDownloaded;
  renderFilters();
});
el("filterStreaming").addEventListener("click", () => {
  onlyStreaming = !onlyStreaming;
  renderFilters();
});
el("filterTranslate").addEventListener("click", () => {
  onlyTranslate = !onlyTranslate;
  renderFilters();
});

// Каталог живёт в приложении, но модели на диске могли поменяться руками —
// пересканируем и заодно проверяем, не появилось ли обновлений каталога.
el("rescanModels").addEventListener("click", async () => {
  el("rescanModels").textContent = t("Проверяю");
  await refreshModels();
  const news = await invoke("catalog_news").catch(() => null);
  el("rescanModels").textContent = t("Обновить список");
  el("chipHint").textContent = news
    ? news
    : t("Список обновлен, новых моделей нет");
});

// Поле с крестиком очистки — как clearableSearch на Android.
function clearableSearch(inputId, clearId, onChange) {
  const input = el(inputId);
  const clear = el(clearId);
  input.addEventListener("input", () => {
    clear.hidden = !input.value;
    onChange();
  });
  clear.addEventListener("click", () => {
    input.value = "";
    clear.hidden = true;
    input.focus();
    onChange();
  });
}

clearableSearch("modelSearch", "modelSearchClear", renderModels);
clearableSearch("languageSearch", "languageClear", renderLanguages);

listen("solflow-models", refreshModels);

// --- навигация ------------------------------------------------------------
// Разделы живут в сайдбаре, он всегда на месте. История переходов своя:
// по ней работает возврат двухпальцевым свайпом вправо на трекпаде.

const PAGES = ["dictation", "history", "meetings", "models", "settings", "about"];
let page = "dictation";
const backStack = [];

function showPage(name, fromHistory = false) {
  if (!PAGES.includes(name)) return;
  if (!fromHistory && name !== page) backStack.push({ page, meeting: detailId });

  page = name;
  for (const p of PAGES) {
    const box = el("page" + p.charAt(0).toUpperCase() + p.slice(1));
    if (box) box.hidden = p !== name;
  }
  document.querySelectorAll(".nav-item[data-page]").forEach((b) => {
    b.classList.toggle("on", b.dataset.page === name);
  });
  el("content").scrollTop = 0;
  if (name === "history") refreshHistory();
  if (name === "settings") refreshSettings(true);
  if (name === "about") {
    document.querySelector('.nav-item[data-page="about"]')?.classList.remove("has-news");
    if (pendingUpdate) markUpdate(pendingUpdate);
    else checkUpdate(false);
  }
}

/** Шаг назад: из карточки встречи — в список, иначе в прошлый раздел. */
function goBack() {
  if (page === "meetings" && detailId !== null) {
    closeMeeting();
    return;
  }
  const previous = backStack.pop();
  if (!previous) return;
  showPage(previous.page, true);
  if (previous.page === "meetings" && previous.meeting !== null) {
    openMeeting(previous.meeting);
  }
}

document.querySelectorAll(".nav-item[data-page]").forEach((button) => {
  button.addEventListener("click", () => {
    if (button.dataset.page === "meetings") {
      projectFilter = null;
      closeMeeting();
      renderProjects();
    }
    showPage(button.dataset.page);
  });
});

// Свайп двумя пальцами по трекпаду: вправо — назад, влево — вперед по
// карточке встречи. Порог накапливается, чтобы обычная горизонтальная
// прокрутка внутри таблиц не считалась жестом.
let swipeX = 0;
let swipeLocked = false;
el("content").addEventListener(
  "wheel",
  (e) => {
    if (Math.abs(e.deltaX) < Math.abs(e.deltaY)) return;
    swipeX += e.deltaX;
    if (swipeLocked) return;
    if (swipeX < -120) {
      swipeLocked = true;
      goBack();
    } else if (swipeX > 120 && page === "meetings" && detailId === null) {
      // Вперед — в первую встречу списка: обратный ход того же жеста.
      swipeLocked = true;
      const first = visibleMeetings()[0];
      if (first) openMeeting(first.id);
    }
  },
  { passive: true }
);
// Жест закончился — пауза без событий колеса сбрасывает накопленное.
setInterval(() => {
  swipeX = 0;
  swipeLocked = false;
}, 400);

// ⌘[ на Mac и Alt+← на Windows — тот же возврат, что и свайп: если жест
// почему-то не доходит до окна, привычное системе сочетание остаётся.
window.addEventListener("keydown", (e) => {
  if ((e.metaKey && e.key === "[") || (e.altKey && e.key === "ArrowLeft")) {
    e.preventDefault();
    goBack();
  }
});

// Кнопка «наверх» — длинные расшифровки иначе не отлистать.
el("content").addEventListener("scroll", () => {
  el("toTop").hidden = el("content").scrollTop < 400;
});
el("toTop").addEventListener("click", () => {
  el("content").scrollTo({ top: 0, behavior: "smooth" });
});

// --- встречи: подписи ------------------------------------------------------

/** Русское склонение по числу: 1 файл, 2 файла, 5 файлов. */
function plural(n, one, few, many) {
  // В английском форм две, и правило простое; русские три формы приходят
  // сюда уже переведёнными, поэтому выбираем из них по-английски.
  if (UI_LANG === "en") return `${n} ${n === 1 ? one : many}`;

  const mod100 = n % 100;
  const mod10 = n % 10;
  let word = many;
  if (mod100 < 11 || mod100 > 14) {
    if (mod10 === 1) word = one;
    else if (mod10 >= 2 && mod10 <= 4) word = few;
  }
  return `${n} ${word}`;
}

function fmtClock(seconds) {
  const total = Math.floor(seconds);
  const h = Math.floor(total / 3600);
  const m = Math.floor((total % 3600) / 60);
  const s = total % 60;
  const mm = String(m).padStart(2, "0");
  const ss = String(s).padStart(2, "0");
  return h > 0 ? `${h}:${mm}:${ss}` : `${mm}:${ss}`;
}

function fmtDur(seconds) {
  // Переменная нарочно не «t»: так зовётся функция перевода, и локальная
  // переменная её затирала — любая запись с длительностью роняла отрисовку.
  const total = Math.floor(seconds);
  const h = Math.floor(total / 3600);
  const m = Math.floor((total % 3600) / 60);
  if (h > 0) return t("{0} ч {1} мин", h, m);
  if (m > 0) return t("{0} мин", m);
  return t("{0} с", total);
}

function fmtDate(at) {
  const d = new Date(at);
  const date = d.toLocaleDateString("ru", { day: "numeric", month: "long" });
  const time = d.toLocaleTimeString("ru", { hour: "2-digit", minute: "2-digit" });
  return `${date}, ${time}`;
}

function meetingTitle(m) {
  if (m.title) return m.title;
  return (m.imported ? t("Импорт ") : t("Встреча ")) + fmtDate(m.at);
}

/** Скорость считаем сами: Rust шлёт только «скачано из всего». */
const fetchSeen = new Map();

function fetchLabel(m) {
  if (!m.fetched) return t("Качаю по ссылке");
  const [done, total] = m.fetched;
  const now = Date.now();
  const previous = fetchSeen.get(m.id);
  fetchSeen.set(m.id, { done, at: now });

  let speed = "";
  if (previous && now > previous.at && done > previous.done) {
    const perSecond = ((done - previous.done) * 1000) / (now - previous.at);
    speed = t(" · {0} МБ/с", (perSecond / 1e6).toFixed(1));
  }
  const mb = (bytes) => (bytes / 1e6).toFixed(1);
  if (total > 0) {
    const pct = Math.min(99, Math.floor((done * 100) / total));
    return t("Качаю {0}% · {1} из {2} МБ{3}", pct, mb(done), mb(total), speed);
  }
  return t("Качаю {0} МБ{1}", mb(done), speed);
}

function stateLabel(m) {
  const pct = m.progress != null ? ` ${m.progress}%` : "";
  if (m.phase === "fetching") return fetchLabel(m);
  if (m.phase === "importing") return t("Импортирую");
  if (m.phase === "helper") return t("Ставлю ffmpeg{0}", pct);
  if (m.phase === "downloading") return t("Качаю модель голосов{0}", pct);
  if (m.phase === "diarizing") return t("Разделяю говорящих{0}", pct);
  if (m.phase === "queued") return t("В очереди на расшифровку");
  if (m.phase === "llm_downloading") return t("Качаю модель саммери{0}", pct);
  if (m.phase === "summarizing") return t("Делаю саммери{0}", pct);
  if (m.phase === "asking") return t("Отвечаю на вопрос{0}", pct);
  if (m.phase === "classifying") return t("Определяю тип записи{0}", pct);
  if (m.phase && DERIVE_PHASES[m.phase]) return t(DERIVE_PHASES[m.phase], pct);
  if (m.phase === "titling") return t("Придумываю название");
  if (m.phase === "transcribing") return t("Расшифровываю{0}", pct);
  // Причину показываем прямо в строке: раньше она уходила в подпись над
  // списком, и неудавшийся импорт выглядел так, будто ничего не случилось.
  if (m.state === "failed") return m.error ? t("Не вышло: {0}", m.error) : t("Не удалось расшифровать");
  if (m.state === "transcribing") return t("Расшифровка прервана");
  if (m.state === "recorded") return t("Ожидает расшифровки");
  return "";
}

// --- встречи: список и проекты --------------------------------------------

const MEET_HINT =
  t("Запись уходит в файл на диске, расшифровка — на этом компьютере. ") +
  t("Файл можно перетащить в окно");

let meetRows = [];
let meetProjects = [];
/** Отмеченные галочками записи — для групповых действий. */
const selected = new Set();
/** Что нашёл поиск: где именно и сколько раз. */
let meetHits = null;
let projectFilter = null; // id проекта или null — «Все»
let meetMatches = null; // id встреч, подошедших под поиск; null — поиска нет
let detailId = null;
let detailSegments = [];

function projectName(id) {
  const p = meetProjects.find((p) => p.id === id);
  return p ? p.name : null;
}

/**
 * Проекты живут подпунктами «Встреч» в сайдбаре — как папки: их можно
 * развернуть и увидеть записи внутри, переименовать двойным кликом или
 * правой кнопкой, а записи — перетащить мышью прямо на проект.
 */
const openProjects = new Set();

function renderProjects() {
  const box = el("navProjects");
  box.textContent = "";

  const addProject = (label, id) => {
    const item = document.createElement("div");
    item.className =
      "nav-item nav-project" +
      (page === "meetings" && projectFilter === id ? " on" : "");
    item.dataset.project = id === null ? "" : id;

    // Стрелка разворота — только у настоящих проектов и «Всех записей».
    const twisty = document.createElement("span");
    twisty.className = "twisty" + (openProjects.has(String(id)) ? " open" : "");
    twisty.innerHTML =
      '<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" ' +
      'stroke-linecap="round" stroke-linejoin="round" aria-hidden="true">' +
      '<path d="M9 5l7 7-7 7"/></svg>';
    twisty.onclick = (e) => {
      e.stopPropagation();
      const key = String(id);
      if (openProjects.has(key)) openProjects.delete(key);
      else openProjects.add(key);
      renderProjects();
    };

    const name = document.createElement("span");
    name.className = "nav-label";
    name.textContent = label;

    const count = document.createElement("span");
    count.className = "nav-count";
    const inside = meetRows.filter((m) => id === null || m.project === id);
    count.textContent = inside.length ? String(inside.length) : "";

    item.append(twisty, name, count);
    item.onclick = () => {
      projectFilter = id;
      closeMeeting();
      showPage("meetings");
      renderProjects();
      renderMeetings();
    };
    if (id !== null) {
      item.title = t("Двойной клик — переименовать, правая кнопка — меню");
      item.ondblclick = () => startInlineRename(item, id);
      item.oncontextmenu = (e) => {
        e.preventDefault();
        openProjectMenu(item, id, label);
      };
    }
    box.appendChild(item);

    // Развёрнутый проект показывает свои записи списком.
    if (openProjects.has(String(id))) {
      const list = document.createElement("div");
      list.className = "nav-meetings";
      if (!inside.length) {
        const empty = document.createElement("p");
        empty.className = "nav-empty";
        empty.textContent = t("Пусто");
        list.appendChild(empty);
      }
      for (const m of inside.slice(0, 30)) {
        const link = document.createElement("div");
        link.className = "nav-meeting" + (detailId === m.id ? " on" : "");
        link.textContent = meetingTitle(m);
        link.title =
          t("Двойной клик — переименовать, правая кнопка — меню. ") +
          t("Можно перетащить в другой проект");
        link.onclick = () => {
          showPage("meetings");
          openMeeting(m.id);
        };
        // Отсюда запись тоже перетаскивается — в том числе между проектами.
        link.onmousedown = (e) => startMeetingDrag(e, m);
        link.ondblclick = (e) => {
          e.stopPropagation();
          startMeetingRename(link, m);
        };
        link.oncontextmenu = (e) => {
          e.preventDefault();
          e.stopPropagation();
          openSidebarMeetingMenu(link, m, e.clientX, e.clientY);
        };
        list.appendChild(link);
      }
      box.appendChild(list);
    }
  };

  addProject(t("Все записи"), null);
  for (const p of meetProjects) addProject(p.name, p.id);

  const plus = document.createElement("button");
  plus.className = "nav-item nav-add";
  plus.textContent = t("+ Проект");
  plus.onclick = () => startInlineCreate(plus);
  box.appendChild(plus);

  const current = meetProjects.find((p) => p.id === projectFilter);
  el("meetingsTitle").textContent = current ? current.name : t("Записи и расшифровки");
  el("deleteProject").hidden = !current;
}

/** Поле вместо строки: Enter сохраняет, Escape и потеря фокуса отменяют. */
function inlineField(anchor, value, placeholder, commit) {
  const input = document.createElement("input");
  input.className = "nav-input";
  input.value = value;
  input.placeholder = placeholder;
  anchor.replaceWith(input);
  input.focus();
  input.select();

  let done = false;
  const finish = (save) => {
    if (done) return;
    done = true;
    if (save) commit(input.value.trim());
    renderProjects();
  };
  input.addEventListener("keydown", (e) => {
    if (e.key === "Enter") finish(true);
    if (e.key === "Escape") finish(false);
  });
  input.addEventListener("blur", () => finish(true));
}

function startInlineCreate(anchor) {
  inlineField(anchor, "", t("Название проекта"), (name) => {
    if (name) invoke("project_create", { name }).then(refreshMeetings);
  });
}

function startInlineRename(anchor, id) {
  const project = meetProjects.find((p) => p.id === id);
  inlineField(anchor, project ? project.name : "", t("Название"), (name) => {
    if (name) invoke("project_rename", { id, name }).then(refreshMeetings);
  });
}

/** Переименование записи прямо в сайдбаре — поле вместо строки. */
function startMeetingRename(anchor, meeting) {
  inlineField(anchor, meeting.title, meetingTitle(meeting), (title) => {
    invoke("meeting_rename", { id: meeting.id, title }).then(refreshMeetings);
  });
}

/** Иконки пунктов меню записи — та же одинарная обводка, что и в окне. */
const MENU_ICONS = {
  open: '<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.7" stroke-linecap="round" stroke-linejoin="round"><path d="M14 4h6v6M20 4l-9 9M18 13v5a2 2 0 0 1-2 2H6a2 2 0 0 1-2-2V8a2 2 0 0 1 2-2h5"/></svg>',
  rename: '<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.7" stroke-linecap="round" stroke-linejoin="round"><path d="M4 20h4l10.5-10.5a2.1 2.1 0 0 0-3-3L5 17z M13 7l3 3"/></svg>',
  export: '<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.7" stroke-linecap="round" stroke-linejoin="round"><path d="M12 3.5v11M8 10.5l4 4 4-4M4.5 19.5h15"/></svg>',
  transcript: '<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.7" stroke-linecap="round" stroke-linejoin="round"><path d="M5 6.5h14M5 11h9M5 15.5h11M5 20h7"/></svg>',
  move: '<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.7" stroke-linecap="round" stroke-linejoin="round"><path d="M3.5 7.5A2 2 0 0 1 5.5 5.5h3.6l1.7 2.2h7.7a2 2 0 0 1 2 2v7.8a2 2 0 0 1-2 2h-13a2 2 0 0 1-2-2z"/><path d="M10 13.5h5M13 11l2.5 2.5L13 16"/></svg>',
  trash: '<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.7" stroke-linecap="round"><path d="M4 7h16M9.5 7V4.5h5V7M6.5 7l1 13h9l1-13M10.5 11v6M13.5 11v6"/></svg>',
  chevron: '<svg class="chev" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M9 6l6 6-6 6"/></svg>',
};

/**
 * Меню записи в сайдбаре — по правой кнопке, у курсора и правее него, чтобы
 * соседние записи оставались на виду. Длинные наборы (экспорт, расшифровка,
 * перенос) спрятаны в подменю, которые выезжают вправо при наведении.
 */
function openSidebarMeetingMenu(anchor, meeting, x, y) {
  closeProjectMenu();
  const menu = document.createElement("div");
  menu.className = "lang-panel menu project-menu floating";
  menu.onclick = (e) => e.stopPropagation();

  const label = (icon, text) => {
    const wrap = document.createDocumentFragment();
    const i = document.createElement("span");
    i.className = "menu-icon";
    i.innerHTML = MENU_ICONS[icon];
    const t = document.createElement("span");
    t.className = "menu-text";
    t.textContent = text;
    wrap.append(i, t);
    return wrap;
  };

  const row = (icon, text, action, into = menu) => {
    const button = document.createElement("button");
    button.className = "lang-row menu-item";
    button.appendChild(label(icon, text));
    button.onclick = () => {
      closeProjectMenu();
      action();
    };
    into.appendChild(button);
    return button;
  };

  /** Пункт с подменю: наведение раскрывает список справа. */
  const sub = (icon, text, children) => {
    const item = document.createElement("div");
    item.className = "lang-row menu-item has-sub";
    item.appendChild(label(icon, text));
    const chev = document.createElement("span");
    chev.className = "menu-chevron";
    chev.innerHTML = MENU_ICONS.chevron;
    item.appendChild(chev);
    const panel = document.createElement("div");
    panel.className = "lang-panel submenu";
    for (const [name, hint, action] of children) {
      const button = document.createElement("button");
      button.className = "lang-row";
      const n = document.createElement("span");
      n.className = "lang-name";
      n.textContent = name;
      button.appendChild(n);
      if (hint) {
        const h = document.createElement("span");
        h.className = "lang-count";
        h.textContent = hint;
        button.appendChild(h);
      }
      button.onclick = () => {
        closeProjectMenu();
        action();
      };
      panel.appendChild(button);
    }
    item.appendChild(panel);
    menu.appendChild(item);
  };

  row("open", t("Открыть"), () => {
    showPage("meetings");
    openMeeting(meeting.id);
  });
  row("rename", t("Переименовать"), () => startMeetingRename(anchor, meeting));

  const done = meeting.state === "done";
  if (done || meeting.audio) {
    const exports = [];
    if (done) {
      exports.push(
        [t("Текст"), ".txt", () => exportFromList(meeting, "txt")],
        ["Markdown", ".md", () => exportFromList(meeting, "md")],
        ["Word", ".docx", () => exportFromList(meeting, "docx")],
        ["PDF", ".pdf", () => exportFromList(meeting, "pdf")]
      );
      if (meeting.summary) {
        exports.push([t("Только саммери"), ".md", () => exportFromList(meeting, "summary-md")]);
      }
    }
    if (meeting.audio) exports.push([t("Звук"), ".wav", () => exportFromList(meeting, "wav")]);
    sub("export", t("Экспорт"), exports);

    const work = [];
    if (meeting.audio) {
      work.push([t("Расшифровать заново"), null, () => invoke("meeting_transcribe", { id: meeting.id })]);
    }
    if (done) {
      work.push(
        [meeting.summary ? t("Обновить саммери") : t("Саммери"), null, async () => {
          if (await summaryReady()) invoke("meeting_summarize", { id: meeting.id });
        }],
        [t("Придумать название"), null, async () => {
          if (await summaryReady()) invoke("meeting_autotitle", { id: meeting.id });
        }]
      );
    }
    sub("transcript", t("Расшифровка"), work);
  }

  // Перенос без перетаскивания — на случай, когда мышью неудобно.
  const targets = [{ id: null, name: t("Без проекта") }, ...meetProjects]
    .filter((target) => target.id !== meeting.project)
    .map((target) => [target.name, null, () =>
      invoke("meeting_set_project", { id: meeting.id, project: target.id }),
    ]);
  if (targets.length) sub("move", t("Переместить"), targets);

  const remove = row("trash", t("Удалить запись"), () => {}, menu);
  remove.classList.add("danger");
  let armed = false;
  remove.onclick = () => {
    if (!armed) {
      armed = true;
      remove.querySelector(".menu-text").textContent = t("Точно удалить?");
      return;
    }
    closeProjectMenu();
    if (detailId === meeting.id) closeMeeting();
    invoke("meeting_delete", { id: meeting.id });
  };

  // У курсора и правее него: соседние записи остаются видны, и по промаху
  // можно сразу нажать правой кнопкой на другую. Не влезает — сдвигаем.
  const rect = anchor.getBoundingClientRect();
  const px = x ?? rect.right;
  const py = y ?? rect.top;
  menu.style.left = `${Math.round(px + 6)}px`;
  menu.style.top = `${Math.round(py - 6)}px`;
  document.body.appendChild(menu);
  const box = menu.getBoundingClientRect();
  if (box.bottom > window.innerHeight - 8) {
    menu.style.top = `${Math.max(8, window.innerHeight - 8 - box.height)}px`;
  }
  // Подменю выезжают вправо; у правого края окна — влево.
  if (box.right + 200 > window.innerWidth) menu.classList.add("sub-left");
  projectMenuEl = menu;
}

function deleteProject(id) {
  invoke("project_delete", { id }).then(() => {
    if (projectFilter === id) projectFilter = null;
    refreshMeetings();
  });
}

/** Меню проекта по правой кнопке: переименовать, свернуть, удалить. */
let projectMenuEl = null;

function closeProjectMenu() {
  projectMenuEl?.remove();
  projectMenuEl = null;
}

function openProjectMenu(anchor, id, label) {
  closeProjectMenu();
  const menu = document.createElement("div");
  menu.className = "lang-panel menu project-menu";
  menu.onclick = (e) => e.stopPropagation();

  const item = (text, action, danger) => {
    const row = document.createElement("button");
    row.className = "lang-row" + (danger ? " danger" : "");
    row.textContent = text;
    row.onclick = () => {
      closeProjectMenu();
      action();
    };
    menu.appendChild(row);
  };

  item(t("Открыть"), () => {
    projectFilter = id;
    closeMeeting();
    showPage("meetings");
    renderProjects();
    renderMeetings();
  });
  item(t("Переименовать"), () => startInlineRename(anchor, id));
  item(
    openProjects.has(String(id)) ? t("Свернуть") : t("Развернуть"),
    () => {
      const key = String(id);
      if (openProjects.has(key)) openProjects.delete(key);
      else openProjects.add(key);
      renderProjects();
    }
  );

  const remove = document.createElement("button");
  remove.className = "lang-row danger";
  remove.textContent = t("Удалить «{0}»", label);
  let armed = false;
  remove.onclick = () => {
    if (!armed) {
      armed = true;
      remove.textContent = t("Точно удалить?");
      return;
    }
    closeProjectMenu();
    deleteProject(id);
  };
  menu.appendChild(remove);

  anchor.appendChild(menu);
  projectMenuEl = menu;
}

document.addEventListener("click", closeProjectMenu);

// --- перетаскивание записей в проекты --------------------------------------
// Своя механика на мыши, а не HTML5 drag: системное перетаскивание файлов
// в окно перехватывает Tauri, и обычный dragstart внутри окна не доходит.

let dragging = null;

function startMeetingDrag(event, meeting) {
  if (event.button !== 0) return;
  if (event.target.closest("input, button")) return;
  const startX = event.clientX;
  const startY = event.clientY;
  let ghost = null;
  let target = null;

  const move = (e) => {
    const far =
      Math.abs(e.clientX - startX) > 6 || Math.abs(e.clientY - startY) > 6;
    if (!ghost && far) {
      ghost = document.createElement("div");
      ghost.className = "drag-ghost";
      const ids = selected.has(meeting.id) ? selected.size : 1;
      ghost.textContent =
        ids > 1 ? t("{0} записи", ids) : meetingTitle(meeting);
      document.body.appendChild(ghost);
      document.body.classList.add("dragging-meeting");
    }
    if (!ghost) return;

    ghost.style.left = `${e.clientX + 12}px`;
    ghost.style.top = `${e.clientY + 12}px`;

    // Цель ищем под курсором: так работает и для развёрнутых проектов.
    const under = document.elementFromPoint(e.clientX, e.clientY);
    // «+ Проект» тоже принимает записи: новый проект создаётся на месте.
    const project = under?.closest(".nav-project, .nav-add");
    if (target !== project) {
      target?.classList.remove("drop-here");
      target = project;
      target?.classList.add("drop-here");
    }
  };

  const up = () => {
    document.removeEventListener("mousemove", move);
    document.removeEventListener("mouseup", up);
    document.body.classList.remove("dragging-meeting");
    ghost?.remove();
    target?.classList.remove("drop-here");
    if (!ghost || !target) return;

    const ids = selected.has(meeting.id) ? [...selected] : [meeting.id];
    if (target.classList.contains("nav-add")) {
      // Бросили на «+ Проект»: спрашиваем имя прямо в строке, создаём и
      // сразу кладём туда записи.
      inlineField(target, "", t("Название проекта"), async (name) => {
        if (!name) return;
        const created = await invoke("project_create", { name });
        if (!created) return;
        for (const id of ids) await invoke("meeting_set_project", { id, project: created.id });
        clearSelection();
        refreshMeetings();
        el("meetStatus").textContent = t("Перенес {0} в «{1}»",
          plural(ids.length, t("запись"), t("записи"), t("записей")), name);
      });
      return;
    }

    const raw = target.dataset.project;
    const project = raw === "" ? null : raw;
    for (const id of ids) invoke("meeting_set_project", { id, project });
    clearSelection();

    const where = project
      ? `«${meetProjects.find((p) => p.id === project)?.name || t("проект")}»`
      : t("«Все записи»");
    el("meetStatus").textContent =
      ids.length > 1
        ? t("Перенес {0} в {1}", plural(ids.length, t("запись"), t("записи"), t("записей")), where)
        : t("Перенес запись в {0}", where);
  };

  document.addEventListener("mousemove", move);
  document.addEventListener("mouseup", up);
}

/**
 * Что показываем сейчас: фильтр по проекту плюс поиск. Rust ищет по
 * сохраненному названию и тексту; собранные из даты заголовки вроде
 * «Встреча 27 августа» он не знает — их добираем здесь.
 */
function visibleMeetings() {
  const needle = el("meetSearch").value.trim().toLowerCase();
  return meetRows.filter(
    (m) =>
      (projectFilter === null || m.project === projectFilter) &&
      (!needle ||
        meetingTitle(m).toLowerCase().includes(needle) ||
        (meetMatches !== null && meetMatches.includes(m.id)))
  );
}

function renderMeetings() {
  // Пока открыто меню строки, список не трогаем: перерисовка уносит меню
  // из-под курсора, а прогресс расшифровки шлёт обновления каждые полсекунды.
  if (rowMenuEl) return;

  const list = el("meetingList");
  list.textContent = "";
  const shown = visibleMeetings();
  el("meetEmpty").hidden = meetRows.length > 0;

  for (const m of shown) {
    const row = document.createElement("div");
    row.className = "meeting";
    // Тащить можно за всю строку — так запись кладут в проект сайдбара.
    row.onmousedown = (e) => {
      if (e.target.closest("button")) return;
      startMeetingDrag(e, m);
    };

    // Галочка выбора: по наведению видна у всех, у выбранных — всегда.
    const check = document.createElement("button");
    check.className = "meet-check" + (selected.has(m.id) ? " on" : "");
    check.title = t("Выбрать");
    check.onclick = (e) => {
      e.stopPropagation();
      toggleSelected(m.id);
    };
    row.appendChild(check);

    const text = document.createElement("div");
    text.className = "model-text";
    const name = document.createElement("p");
    name.className = "model-name";
    name.textContent = meetingTitle(m);
    const meta = document.createElement("p");
    meta.className = "model-meta";
    const parts = [fmtDate(m.at)];
    if (m.seconds > 0) parts.push(fmtDur(m.seconds));
    const project = projectName(m.project);
    if (project && projectFilter === null) parts.push(project);
    const state = stateLabel(m);
    if (state) parts.push(state);
    meta.textContent = parts.join(" · ");
    text.append(name, meta);

    let cancelButton = null;
    if (m.phase) {
      const stop = document.createElement("button");
      stop.className = "row-cancel";
      stop.textContent = t("Отмена");
      stop.title = t("Прервать работу");
      stop.onclick = (e) => {
        e.stopPropagation();
        invoke("meeting_cancel", { id: m.id });
      };
      cancelButton = stop;

      const barBox = document.createElement("div");
      barBox.className = "model-progress" + (m.progress == null ? " busy" : "");
      const bar = document.createElement("div");
      if (m.progress != null) bar.style.width = `${m.progress}%`;
      barBox.appendChild(bar);
      text.appendChild(barBox);
    }

    // Поиск показывает не только какие записи подошли, но и что в них
    // нашлось: по одному названию не понять, та ли это встреча.
    const hit = meetHits?.find((h) => h.id === m.id);
    if (hit?.quotes?.length) {
      const found = document.createElement("div");
      found.className = "hits";
      for (const [at, quote] of hit.quotes) {
        const line = document.createElement("p");
        line.className = "hit";
        const clock = document.createElement("span");
        clock.className = "hit-clock";
        clock.textContent = fmtClock(at);
        line.appendChild(clock);
        line.appendChild(highlighted(quote, el("meetSearch").value.trim()));
        found.appendChild(line);
      }
      if (hit.count > hit.quotes.length) {
        const more = document.createElement("p");
        more.className = "hit-more";
        more.textContent = t("и еще {0}", hit.count - hit.quotes.length);
        found.appendChild(more);
      }
      text.appendChild(found);
    }

    row.appendChild(text);
    if (cancelButton) row.appendChild(cancelButton);

    const menu = document.createElement("button");
    menu.className = "row-menu";
    menu.innerHTML =
      '<svg viewBox="0 0 24 24" fill="currentColor" aria-hidden="true">' +
      '<circle cx="5" cy="12" r="1.7"/><circle cx="12" cy="12" r="1.7"/>' +
      '<circle cx="19" cy="12" r="1.7"/></svg>';
    menu.title = t("Действия");
    menu.onclick = (e) => {
      e.stopPropagation();
      openRowMenu(m, menu);
    };
    row.appendChild(menu);

    // Пока идёт выбор, клик по строке добавляет её к выделению, а не
    // проваливается внутрь: иначе выбирать несколько мучительно.
    row.onclick = () => (selected.size ? toggleSelected(m.id) : openMeeting(m.id));
    list.appendChild(row);
  }
}

/** Кусок текста с выделенным словом. Собираем узлами, а не разметкой,
 * чтобы чужой текст не толковался как HTML. */
function highlighted(text, needle) {
  const box = document.createElement("span");
  const lower = text.toLowerCase();
  const target = needle.toLowerCase();
  if (!target) {
    box.textContent = text;
    return box;
  }
  let from = 0;
  let at = lower.indexOf(target, from);
  while (at !== -1) {
    box.appendChild(document.createTextNode(text.slice(from, at)));
    const mark = document.createElement("mark");
    mark.textContent = text.slice(at, at + target.length);
    box.appendChild(mark);
    from = at + target.length;
    at = lower.indexOf(target, from);
  }
  box.appendChild(document.createTextNode(text.slice(from)));
  return box;
}

// --- выбор нескольких записей ---------------------------------------------

function toggleSelected(id) {
  if (selected.has(id)) selected.delete(id);
  else selected.add(id);
  renderSelection();
  renderMeetings();
}

function clearSelection() {
  selected.clear();
  renderSelection();
  renderMeetings();
}

function renderSelection() {
  const count = selected.size;
  document.body.classList.toggle("selecting", count > 0);
  el("bulkBar").hidden = count === 0;
  el("bulkCount").textContent = t("Выбрано {0}", count);
  if (!count) {
    el("bulkMenu").hidden = true;
    el("bulkHowMenu").hidden = true;
  }
}

function selectedTitles() {
  return [...selected].map((id) => {
    const m = meetRows.find((r) => r.id === id);
    return m ? meetingTitle(m) : t("Встреча");
  });
}

el("bulkCancel").addEventListener("click", clearSelection);

el("bulkExport").addEventListener("click", (e) => {
  e.stopPropagation();
  const menu = el("bulkMenu");
  menu.hidden = !menu.hidden;
});
// Формат выбран. Одна встреча уходит сразу; для нескольких — второй
// вопрос: по файлам или одним, как на Android.
let bulkFormat = null;

async function runBulkExport(format, combined) {
  const ids = [...selected];
  el("meetStatus").textContent = t("Готовлю {0}", plural(ids.length, t("файл"), t("файла"), t("файлов")));
  try {
    if (combined) {
      const date = new Intl.DateTimeFormat(UI_LANG === "ru" ? "ru" : "en", {
        day: "numeric",
        month: "long",
        hour: "2-digit",
        minute: "2-digit",
      }).format(new Date());
      const path = await invoke("meetings_export_combined", {
        ids,
        format,
        titles: selectedTitles(),
        title: t("Встречи {0}", date),
      });
      el("meetStatus").textContent = path
        ? t("Сохранено одним файлом: {0}", ids.length)
        : "";
    } else {
      const done = await invoke("meetings_export", {
        ids,
        format,
        titles: selectedTitles(),
      });
      el("meetStatus").textContent =
        done === ids.length
          ? t("Сохранено в Загрузки: {0}", done)
          : t("Сохранено {0} из {1} — у остальных нет расшифровки", done, ids.length);
    }
  } catch (err) {
    el("meetStatus").textContent = String(err);
  }
  clearSelection();
}

el("bulkMenu").addEventListener("click", async (e) => {
  e.stopPropagation();
  const format = e.target.closest("[data-bulk-format]")?.dataset.bulkFormat;
  if (!format) return;
  el("bulkMenu").hidden = true;
  // Звук склеивать не во что — всегда отдельными файлами, без вопроса.
  if (selected.size > 1 && format !== "wav") {
    bulkFormat = format;
    el("bulkHowMenu").hidden = false;
    return;
  }
  await runBulkExport(format, false);
});

el("bulkHowMenu").addEventListener("click", async (e) => {
  e.stopPropagation();
  const how = e.target.closest("[data-bulk-how]")?.dataset.bulkHow;
  if (!how) return;
  el("bulkHowMenu").hidden = true;
  const format = bulkFormat;
  bulkFormat = null;
  if (format) await runBulkExport(format, how === "single");
});

// «В проект» для отмеченных: то же, что перетащить их в сайдбар.
el("bulkProject").addEventListener("click", (e) => {
  e.stopPropagation();
  const menu = el("bulkProjectMenu");
  menu.hidden = !menu.hidden;
  if (menu.hidden) return;

  menu.textContent = "";
  const addRow = (label, id) => {
    const row = document.createElement("button");
    row.className = "lang-row";
    row.textContent = label;
    row.onclick = () => {
      menu.hidden = true;
      const ids = [...selected];
      for (const meeting of ids) {
        invoke("meeting_set_project", { id: meeting, project: id });
      }
      clearSelection();
      el("meetStatus").textContent =
        t("Перенес {0} в «{1}»", plural(ids.length, t("запись"), t("записи"), t("записей")), label);
    };
    menu.appendChild(row);
  };
  addRow(t("Без проекта"), null);
  for (const p of meetProjects) addRow(p.name, p.id);
});

document.addEventListener("click", () => {
  const menu = el("bulkProjectMenu");
  if (menu) menu.hidden = true;
  const how = el("bulkHowMenu");
  if (how) how.hidden = true;
});

el("bulkAgain").addEventListener("click", () => {
  invoke("meetings_transcribe", { ids: [...selected] });
  clearSelection();
});
// Саммери и названия для пачки — по очереди, как расшифровки. Без модели
// не запускаем: скачивать гигабайты от группового нажатия было бы сюрпризом.
el("bulkSummary").addEventListener("click", async () => {
  if (!(await summaryReady())) return;
  invoke("meetings_summarize", { ids: [...selected] });
  clearSelection();
});
el("bulkTitle").addEventListener("click", async () => {
  if (!(await summaryReady())) return;
  invoke("meetings_autotitle", { ids: [...selected] });
  clearSelection();
});

// Удаление группы — в два нажатия, как и одиночное.
let bulkDeleteArmed = null;
el("bulkDelete").addEventListener("click", () => {
  if (!bulkDeleteArmed) {
    el("bulkDeleteLabel").textContent = t("Удалить {0}?", selected.size);
    bulkDeleteArmed = setTimeout(() => {
      bulkDeleteArmed = null;
      el("bulkDeleteLabel").textContent = t("Удалить");
    }, 3000);
    return;
  }
  clearTimeout(bulkDeleteArmed);
  bulkDeleteArmed = null;
  el("bulkDeleteLabel").textContent = t("Удалить");
  invoke("meetings_delete", { ids: [...selected] });
  clearSelection();
});

// --- меню строки -----------------------------------------------------------

let rowMenuEl = null;

function closeRowMenu() {
  rowMenuEl?.remove();
  rowMenuEl = null;
  document.querySelectorAll(".row-menu.open").forEach((b) => b.classList.remove("open"));
}

function openRowMenu(meeting, button) {
  const wasOpen = rowMenuEl?.dataset.id === String(meeting.id);
  closeRowMenu();
  if (wasOpen) return;
  button.classList.add("open");

  const menu = document.createElement("div");
  menu.className = "lang-panel menu row-menu-panel";
  menu.dataset.id = String(meeting.id);
  menu.onclick = (e) => e.stopPropagation();

  const item = (label, hint, action) => {
    const row = document.createElement("button");
    row.className = "lang-row";
    const name = document.createElement("span");
    name.className = "lang-name";
    name.textContent = label;
    row.appendChild(name);
    if (hint) {
      const tail = document.createElement("span");
      tail.className = "lang-count";
      tail.textContent = hint;
      row.appendChild(tail);
    }
    row.onclick = () => {
      closeRowMenu();
      action();
    };
    menu.appendChild(row);
  };

  const title = meetingTitle(meeting);
  const exportAs = async (format) => {
    el("meetStatus").textContent = t("Готовлю файл");
    try {
      await invoke("meeting_export", { id: meeting.id, format, title });
      el("meetStatus").textContent = t("Файл .{0} сохранен в Загрузки", format);
    } catch (err) {
      el("meetStatus").textContent = String(err);
    }
  };

  item(t("Открыть"), null, () => openMeeting(meeting.id));
  item(t("Экспорт"), ".txt", () => exportAs("txt"));
  item(t("Экспорт"), ".md", () => exportAs("md"));
  item(t("Экспорт"), ".docx", () => exportAs("docx"));
  item(t("Экспорт"), ".pdf", () => exportAs("pdf"));
  if (meeting.summary) item(t("Только саммери"), ".md", () => exportAs("summary-md"));
  if (meeting.audio) {
    item(t("Экспорт"), ".wav", () => exportAs("wav"));
    item(t("Расшифровать заново"), null, () =>
      invoke("meeting_transcribe", { id: meeting.id })
    );
  }
  if (meeting.state === "done") {
    item(meeting.summary ? t("Обновить саммери") : t("Саммери"), null, async () => {
      if (await summaryReady()) invoke("meeting_summarize", { id: meeting.id });
    });
    item(t("Придумать название"), null, async () => {
      if (await summaryReady()) invoke("meeting_autotitle", { id: meeting.id });
    });
  }
  item(t("Выбрать"), null, () => toggleSelected(meeting.id));

  // Удаление — сразу из меню, но с подтверждением на том же месте.
  const remove = document.createElement("button");
  remove.className = "lang-row danger";
  remove.textContent = t("Удалить");
  let armed = false;
  remove.onclick = () => {
    if (!armed) {
      armed = true;
      remove.textContent = t("Точно удалить?");
      return;
    }
    closeRowMenu();
    invoke("meeting_delete", { id: meeting.id });
  };
  menu.appendChild(remove);

  button.parentElement.appendChild(menu);
  // У нижних записей меню открывается вверх, иначе оно уходит за край окна.
  if (menu.getBoundingClientRect().bottom > window.innerHeight - 8) {
    menu.classList.add("above");
  }
  rowMenuEl = menu;
}

document.addEventListener("click", closeRowMenu);

async function refreshMeetings() {
  [meetRows, meetProjects] = await Promise.all([
    invoke("meetings_list"),
    invoke("projects_list"),
  ]);
  const query = el("meetSearch").value.trim();
  meetHits = query ? await invoke("meeting_search", { query }) : null;
  meetMatches = meetHits ? meetHits.map((h) => h.id) : null;

  // Подпись под заголовком показывает, чем приложение занято сейчас.
  const working = meetRows.filter((m) => m.phase);
  el("meetStatus").textContent = working.length
    ? working.some((m) => m.phase === "importing")
      ? t("Вытаскиваю звук из файла")
      : working.some((m) => m.phase === "diarizing" || m.phase === "downloading")
        ? t("Разделяю говорящих")
        : t("Расшифровываю запись")
    : MEET_HINT;

  renderProjects();
  renderMeetings();
  renderDetail();
}

// Удалить проект можно, стоя в нём: кнопка появляется в шапке раздела.
el("deleteProject").addEventListener("click", () => {
  if (!projectFilter) return;
  if (!deleteProjectArmed) {
    el("deleteProjectLabel").textContent = t("Точно удалить проект?");
    deleteProjectArmed = setTimeout(() => {
      deleteProjectArmed = null;
      el("deleteProjectLabel").textContent = t("Удалить проект");
    }, 3000);
    return;
  }
  clearTimeout(deleteProjectArmed);
  deleteProjectArmed = null;
  el("deleteProjectLabel").textContent = t("Удалить проект");
  deleteProject(projectFilter);
});
let deleteProjectArmed = null;

clearableSearch("meetSearch", "meetSearchClear", refreshMeetings);
listen("solflow-meetings", refreshMeetings);
listen("solflow-import-failed", (e) => {
  el("meetStatus").textContent = t("Импорт не удался: {0}", e.payload);
});

// --- встречи: запись -------------------------------------------------------

let meetRecActive = false;
const MEET_BARS = 24;
const meetLevels = new Array(MEET_BARS).fill(0);

function drawMeetWave() {
  const canvas = el("meetWave");
  const ctx2 = canvas.getContext("2d");
  const w = canvas.width;
  const h = canvas.height;
  ctx2.clearRect(0, 0, w, h);
  ctx2.fillStyle = getComputedStyle(document.documentElement)
    .getPropertyValue("--accent-solid").trim();
  const bar = w / (MEET_BARS * 2 - 1);
  const minH = h * 0.12;
  for (let i = 0; i < MEET_BARS; i++) {
    const bh = Math.max(minH, minH + (h - minH) * meetLevels[i]);
    ctx2.beginPath();
    ctx2.roundRect(i * bar * 2, (h - bh) / 2, bar, bh, bar / 2);
    ctx2.fill();
  }
}

function renderRec(p) {
  meetRecActive = p.active;
  el("meetIconMic").hidden = p.active;
  el("meetIconStop").hidden = !p.active;
  el("meetRecLive").hidden = !p.active;
  el("meetRecHint").hidden = p.active;
  el("meetPause").hidden = !p.active;
  // Подпись меняется только по делу: событие записи приходит пять раз в
  // секунду, и постоянная замена текстового узла роняла клики в WebKit —
  // нажатие, попавшее между заменами, не превращалось в click.
  const pauseLabel = p.paused ? t("Продолжить") : t("Пауза");
  if (el("meetPause").textContent !== pauseLabel) {
    el("meetPause").textContent = pauseLabel;
  }
  if (p.error) el("meetStatus").textContent = t("Запись: {0}", p.error);
  if (p.active) {
    el("meetTimer").textContent = fmtClock(p.seconds);
    meetLevels.shift();
    meetLevels.push(p.level);
    drawMeetWave();
  } else {
    meetLevels.fill(0);
  }
}

el("meetRecord").addEventListener("click", () =>
  invoke(meetRecActive ? "meeting_record_stop" : "meeting_record_start")
);
el("meetPause").addEventListener("click", () => invoke("meeting_record_pause"));
el("meetImport").addEventListener("click", () => invoke("meeting_import"));

// --- расшифровка по ссылке -------------------------------------------------

clearableSearch("meetUrl", "meetUrlClear", () => {});

async function importUrl() {
  const url = el("meetUrl").value.trim();
  if (!url) return;
  if (!/^https?:\/\//i.test(url)) {
    el("urlHint").textContent = t("Ссылка должна начинаться с http");
    el("urlHint").hidden = false;
    return;
  }

  // Для страниц видеосервисов нужен загрузчик — предупреждаем заранее,
  // а не после долгой попытки.
  const host = url.split("/")[2] || "";
  const direct =
    /\.(mp3|m4a|wav|aac|aiff|caf|mp4|mov|m4v|mkv|webm|ogg|opus|flac)($|\?)/i.test(url) ||
    /yandex|yadi\.sk/i.test(host);
  if (!direct && !(await invoke("downloader_ready"))) {
    el("urlHint").textContent =
      t("Для этой ссылки нужен загрузчик — включите его в настройках");
    el("urlHint").hidden = false;
    return;
  }

  el("urlHint").textContent = t("Качаю по ссылке");
  el("urlHint").hidden = false;
  el("meetUrl").value = "";
  el("meetUrlClear").hidden = true;
  invoke("meeting_import_url", { url });
}

el("meetUrlGo").addEventListener("click", importUrl);
el("meetUrl").addEventListener("keydown", (e) => {
  if (e.key === "Enter") importUrl();
});
listen("solflow-meetrec", (e) => renderRec(e.payload));

// --- перетаскивание файлов -------------------------------------------------
// Файл можно бросить в любое место окна: кнопка импорта на это время
// становится полем с пунктиром, чтобы было видно, куда целиться.

function setDropping(on, count) {
  document.body.classList.toggle("dropping", on);
  el("meetImportLabel").textContent = on
    ? count > 1
      ? t("Отпустите — {0}", plural(count, t("файл"), t("файла"), t("файлов")))
      : t("Отпустите файл")
    : t("Импортировать файл");
}

listen("tauri://drag-enter", (e) => {
  showPage("meetings");
  setDropping(true, e.payload?.paths?.length || 1);
});
listen("tauri://drag-leave", () => setDropping(false));
listen("tauri://drag-drop", (e) => {
  setDropping(false);
  const paths = e.payload?.paths || [];
  if (!paths.length) return;
  // Деталь закрываем только теперь: до броска пользователь мог передумать.
  showPage("meetings");
  closeMeeting();
  invoke("meeting_import_paths", { paths });
});

// --- встречи: деталь -------------------------------------------------------

function openMeeting(id) {
  detailId = id;
  el("meetFind").value = "";
  el("meetFindClear").hidden = true;
  el("meetFindCount").hidden = true;
  el("meetDetailStatus").hidden = true;
  renderDetail();
}

function closeMeeting() {
  detailId = null;
  el("meetDetail").hidden = true;
  el("meetHome").hidden = false;
}

// --- саммери ---------------------------------------------------------------

/** Текст саммери — простая разметка: «## Заголовок» и «- пункт». */
function renderSummary(m) {
  const busy = !!m.phase;
  const done = m.state === "done";
  const button = el("meetSummary");
  button.hidden = !done && !m.summary;
  button.disabled = busy;
  el("meetAutoTitle").hidden = !done;
  el("meetAutoTitle").disabled = busy;
  if (!summaryArmed) {
    el("meetSummaryLabel").textContent = m.summary ? t("Обновить саммери") : t("Саммери");
  }
  el("exportSummaryMd").hidden = !m.summary;
  el("exportSummaryTxt").hidden = !m.summary;

  const box = el("meetSummaryBox");
  box.hidden = !m.summary;
  if (!m.summary) return;
  const target = el("meetSummaryText");
  target.textContent = "";
  for (const raw of m.summary.split("\n")) {
    const line = raw.trim();
    if (!line) continue;
    const p = document.createElement("p");
    if (line.startsWith("##")) {
      p.className = "summary-head";
      p.textContent = line.replace(/^#+\s*/, "");
    } else if (/^[-•]/.test(line)) {
      p.className = "summary-item";
      p.textContent = line.replace(/^[-•]\s*/, "");
    } else {
      p.textContent = line;
    }
    target.appendChild(p);
  }
}

el("meetSummaryCopy").addEventListener("click", () => {
  const m = meetRows.find((r) => r.id === detailId);
  if (!m || !m.summary) return;
  navigator.clipboard.writeText(m.summary);
  showDetailStatus(t("Саммери скопировано"));
});

/** Саммери без модели не посчитать — говорим, где её взять, вместо тихой закачки. */
async function summaryReady() {
  const [ready] = await invoke("summary_state");
  if (!ready) {
    el("meetStatus").textContent =
      t("Модель саммери не скачана — откройте встречу и нажмите «Саммери»");
  }
  return ready;
}

/** Экспорт из списка или сайдбара: без открытой встречи, статус — в шапке. */
async function exportFromList(meeting, format) {
  el("meetStatus").textContent = t("Готовлю файл");
  try {
    await invoke("meeting_export", { id: meeting.id, format, title: meetingTitle(meeting) });
    el("meetStatus").textContent = t("Файл .{0} сохранен в Загрузки", format.replace("summary-", ""));
  } catch (err) {
    el("meetStatus").textContent = String(err);
  }
}

// --- разборы записи: решения и задачи, письмо, оглавление -------------------

// Заголовки карточек и подписи фаз — по id разбора из summary::BREAKDOWNS.
const EXTRA_TITLES = {
  tasks: "Решения и задачи",
  letter: "Письмо по итогам",
  outline: "Оглавление",
  theses: "Тезисы",
  advice: "Советы и рекомендации",
  cases: "Кейсы и цифры",
  qa_session: "Вопросы и ответы",
  quotes: "Цитаты",
  guest: "О собеседнике",
  glossary: "Глоссарий",
  post: "Пересказ для поста",
};
const DERIVE_PHASES = {
  tasks: "Ищу решения и задачи{0}",
  letter: "Пишу письмо по итогам{0}",
  outline: "Составляю оглавление{0}",
  theses: "Выписываю тезисы{0}",
  advice: "Собираю советы{0}",
  cases: "Ищу кейсы и цифры{0}",
  qa_session: "Собираю вопросы и ответы{0}",
  quotes: "Выбираю цитаты{0}",
  guest: "Собираю факты о собеседнике{0}",
  glossary: "Составляю глоссарий{0}",
  post: "Пишу пересказ{0}",
};
// Какие разборы предлагать какому типу записи.
const KIND_MENUS = {
  meeting: ["tasks", "letter", "outline", "glossary"],
  talk: ["theses", "advice", "cases", "qa_session", "outline", "post", "glossary"],
  interview: ["quotes", "qa_session", "guest", "theses", "outline", "post", "glossary"],
  other: ["theses", "quotes", "outline", "post", "glossary"],
};

/** Карточка разбора: заголовок, копирование, удаление, текст с метками. */
function extraCard(kind, title, text) {
  const card = document.createElement("section");
  card.className = "meet-summary meet-extra";
  const head = document.createElement("div");
  head.className = "summary-head-row";
  const kicker = document.createElement("p");
  kicker.className = "kicker";
  kicker.textContent = t(title);
  const tools = document.createElement("span");
  tools.className = "extra-tools";
  const copy = document.createElement("button");
  copy.className = "icon-only";
  copy.title = t("Скопировать");
  copy.innerHTML =
    '<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.7" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><rect x="9" y="9" width="11" height="11" rx="2"/><path d="M5 15V6a2 2 0 0 1 2-2h9"/></svg>';
  copy.onclick = () => {
    navigator.clipboard.writeText(text);
    showDetailStatus(t("Скопировано"));
  };
  const remove = document.createElement("button");
  remove.className = "icon-only";
  remove.title = t("Убрать");
  remove.innerHTML =
    '<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.7" stroke-linecap="round" aria-hidden="true"><path d="M4 7h16M9.5 7V4.5h5V7M6.5 7l1 13h9l1-13M10.5 11v6M13.5 11v6"/></svg>';
  remove.onclick = async () => {
    await invoke("meeting_extras_clear", { id: detailId, kind });
    const m = meetRows.find((r) => r.id === detailId);
    if (m) renderExtras(m);
  };
  tools.append(copy, remove);
  head.append(kicker, tools);
  card.appendChild(head);

  const body = document.createElement("div");
  body.className = "qa-a";
  for (const raw of text.split("\n")) {
    const line = raw.trim();
    if (!line) continue;
    const p = document.createElement("p");
    if (line.startsWith("##")) {
      p.className = "summary-head";
      p.textContent = line.replace(/^#+\s*/, "");
    } else if (/^[-•]/.test(line)) {
      p.className = "summary-item";
      p.appendChild(answerLine(line.replace(/^[-•]\s*/, "")));
    } else {
      p.appendChild(answerLine(line));
    }
    body.appendChild(p);
  }
  card.appendChild(body);
  return card;
}

// Тип записи спрашиваем у модели один раз на встречу — и только если
// модель уже скачана: ради типа гигабайты не качаются.
const kindAsked = new Set();

async function renderExtras(m) {
  const done = m.state === "done";
  el("meetDerive").hidden = !done;
  el("meetDerive").disabled = !!m.phase;
  el("meetKindSelect").hidden = !done;
  const box = el("meetExtras");
  box.textContent = "";
  if (!done) return;
  const extras = await invoke("meeting_extras", { id: m.id });
  const kind = extras.kind || "meeting";
  el("meetKindSelect").value = kind;
  if (!extras.kind && !m.phase && !kindAsked.has(m.id)) {
    kindAsked.add(m.id);
    invoke("meeting_kind_detect", { id: m.id });
  }

  const items = el("deriveItems");
  items.textContent = "";
  for (const id of KIND_MENUS[kind] || KIND_MENUS.other) {
    const row = document.createElement("button");
    row.className = "lang-row";
    row.dataset.derive = id;
    const name = document.createElement("span");
    name.className = "lang-name";
    name.textContent = t(EXTRA_TITLES[id]);
    row.appendChild(name);
    items.appendChild(row);
  }

  const order = [...(KIND_MENUS[kind] || []), ...Object.keys(EXTRA_TITLES)];
  const shown = new Set();
  for (const id of order) {
    if (shown.has(id) || !extras.items?.[id]) continue;
    shown.add(id);
    box.appendChild(extraCard(id, EXTRA_TITLES[id], extras.items[id]));
  }
}

el("meetKindSelect").addEventListener("change", async () => {
  if (detailId === null) return;
  await invoke("meeting_set_kind", { id: detailId, kind: el("meetKindSelect").value });
  const m = meetRows.find((r) => r.id === detailId);
  if (m) renderExtras(m);
});

el("meetDerive").addEventListener("click", async (e) => {
  e.stopPropagation();
  const menu = el("deriveMenu");
  menu.hidden = !menu.hidden;
  if (menu.hidden) return;
  const [ready, mb] = await invoke("summary_state");
  el("deriveHint").textContent = ready
    ? t("Считает локальная модель, ответ — под саммери")
    : t("Первый раз скачает модель ~{0} ГБ", (mb / 1024).toFixed(1));
});
el("deriveMenu").addEventListener("click", (e) => {
  e.stopPropagation();
  const kind = e.target.closest("[data-derive]")?.dataset.derive;
  if (!kind || detailId === null) return;
  el("deriveMenu").hidden = true;
  invoke("meeting_derive", { id: detailId, kind });
});
document.addEventListener("click", () => {
  el("deriveMenu").hidden = true;
});

listen("solflow-extras", (e) => {
  if (e.payload !== detailId) return;
  const m = meetRows.find((r) => r.id === detailId);
  if (m) renderExtras(m);
});

listen("solflow-extras-error", (e) => {
  el("meetDetailStatus").textContent = t("Разбор: {0}", e.payload);
  el("meetDetailStatus").hidden = false;
});

// --- вопрос к записи ------------------------------------------------------

/** Время «[мм:сс]» или «[ч:мм:сс]» из ответа модели → секунды. */
function clockToSeconds(text) {
  const parts = text.split(":").map(Number);
  if (parts.some(Number.isNaN)) return null;
  return parts.reduce((acc, v) => acc * 60 + v, 0);
}

/** Прокрутить к реплике, в которую попадает момент записи. */
function jumpToClock(seconds) {
  const rows = [...document.querySelectorAll("#meetSegments .segment")];
  let target = null;
  for (const row of rows) {
    if (Number(row.dataset.s) <= seconds + 0.01) target = row;
    else break;
  }
  if (!target) target = rows[0];
  if (!target) return;
  rows.forEach((r) => r.classList.remove("jumped"));
  target.classList.add("jumped");
  target.scrollIntoView({ block: "center", behavior: "smooth" });
  setTimeout(() => target.classList.remove("jumped"), 2500);
}

/** Строка ответа: текст с кликабельными метками времени. */
function answerLine(text) {
  const frag = document.createDocumentFragment();
  const re = /\[(\d{1,2}(?::\d{2}){1,2})\]/g;
  let last = 0;
  for (const match of text.matchAll(re)) {
    if (match.index > last) frag.appendChild(document.createTextNode(text.slice(last, match.index)));
    const seconds = clockToSeconds(match[1]);
    if (seconds === null) {
      frag.appendChild(document.createTextNode(match[0]));
    } else {
      const button = document.createElement("button");
      button.className = "qa-time";
      button.textContent = match[1];
      button.title = t("К этому месту записи");
      button.onclick = () => jumpToClock(seconds);
      frag.appendChild(button);
    }
    last = match.index + match[0].length;
  }
  if (last < text.length) frag.appendChild(document.createTextNode(text.slice(last)));
  return frag;
}

async function renderQa(m) {
  const box = el("meetAskBox");
  const done = m.state === "done";
  box.hidden = !done;
  if (!done) return;
  const busy = !!m.phase;
  el("meetAsk").disabled = busy;
  el("meetAskSend").disabled = busy;

  const items = await invoke("meeting_qa", { id: m.id });
  el("meetQaClear").hidden = !items.length;
  const list = el("meetQaList");
  list.textContent = "";
  for (const item of items) {
    const q = document.createElement("p");
    q.className = "qa-q";
    q.textContent = item.q;
    const a = document.createElement("div");
    a.className = "qa-a";
    for (const raw of item.a.split("\n")) {
      const line = raw.trim();
      if (!line) continue;
      const p = document.createElement("p");
      if (/^[-•]/.test(line)) {
        p.className = "summary-item";
        p.appendChild(answerLine(line.replace(/^[-•]\s*/, "")));
      } else {
        p.appendChild(answerLine(line.replace(/^#+\s*/, "")));
      }
      a.appendChild(p);
    }
    list.append(q, a);
  }
}

let askArmed = null;
async function askMeeting() {
  if (detailId === null) return;
  const question = el("meetAsk").value.trim();
  if (!question) return;
  const [ready, mb] = await invoke("summary_state");
  // Модель тяжёлая: без неё первое нажатие только предупреждает о размере.
  if (!ready && !askArmed) {
    el("meetAskSend").textContent = t("Скачать модель ~{0} ГБ?", (mb / 1024).toFixed(1));
    askArmed = setTimeout(() => {
      askArmed = null;
      el("meetAskSend").textContent = t("Спросить");
    }, 5000);
    return;
  }
  if (askArmed) {
    clearTimeout(askArmed);
    askArmed = null;
    el("meetAskSend").textContent = t("Спросить");
  }
  invoke("meeting_ask", { id: detailId, question });
  el("meetAsk").value = "";
}

el("meetAskSend").addEventListener("click", askMeeting);
el("meetAsk").addEventListener("keydown", (e) => {
  if (e.key === "Enter") askMeeting();
});

el("meetQaClear").addEventListener("click", async () => {
  if (detailId === null) return;
  await invoke("meeting_qa_clear", { id: detailId });
  const m = meetRows.find((r) => r.id === detailId);
  if (m) renderQa(m);
});

listen("solflow-qa", (e) => {
  if (e.payload !== detailId) return;
  const m = meetRows.find((r) => r.id === detailId);
  if (m) renderQa(m);
});

listen("solflow-qa-error", (e) => {
  el("meetDetailStatus").textContent = t("Вопрос: {0}", e.payload);
  el("meetDetailStatus").hidden = false;
});

// Модель тяжёлая, поэтому без неё кнопка сначала честно спрашивает про
// скачивание, и только второе нажатие запускает работу.
let summaryArmed = null;
el("meetSummary").addEventListener("click", async () => {
  if (detailId === null) return;
  const [ready, mb] = await invoke("summary_state");
  if (!ready && !summaryArmed) {
    el("meetSummaryLabel").textContent = t("Скачать модель ~{0} ГБ?", (mb / 1024).toFixed(1));
    summaryArmed = setTimeout(() => {
      summaryArmed = null;
      const m = meetRows.find((r) => r.id === detailId);
      if (m) renderSummary(m);
    }, 5000);
    return;
  }
  if (summaryArmed) {
    clearTimeout(summaryArmed);
    summaryArmed = null;
  }
  invoke("meeting_summarize", { id: detailId });
});

// Ради названия гигабайты не качаются: без модели кнопка отправляет к
// «Саммери» — там и предупреждение о размере, и загрузка.
el("meetAutoTitle").addEventListener("click", async () => {
  if (detailId === null) return;
  const [ready] = await invoke("summary_state");
  if (!ready) {
    el("meetDetailStatus").textContent =
      t("Сначала скачайте модель кнопкой «Саммери»");
    el("meetDetailStatus").hidden = false;
    return;
  }
  invoke("meeting_autotitle", { id: detailId });
});

el("meetWorkCancel").addEventListener("click", () => {
  if (detailId !== null) invoke("meeting_cancel", { id: detailId });
});

listen("solflow-summary-device", (e) => {
  el("meetDetailStatus").textContent = t("Саммери считает: {0}", e.payload);
  el("meetDetailStatus").hidden = false;
});

listen("solflow-summary-error", (e) => {
  el("meetDetailStatus").textContent = t("Саммери: {0}", e.payload);
  el("meetDetailStatus").hidden = false;
});

async function renderDetail() {
  if (detailId === null) return;
  const m = meetRows.find((r) => r.id === detailId);
  if (!m) {
    closeMeeting();
    return;
  }
  el("meetHome").hidden = true;
  el("meetDetail").hidden = false;
  if (el("meetTitleInput").hidden) {
    el("meetTitle").textContent = meetingTitle(m);
  }

  const parts = [fmtDate(m.at)];
  if (m.seconds > 0) parts.push(fmtDur(m.seconds));
  const state = stateLabel(m);
  if (state) parts.push(state);
  el("meetInfo").textContent = parts.join(" · ");

  const select = el("meetProjectSelect");
  select.textContent = "";
  const none = document.createElement("option");
  none.value = "";
  none.textContent = t("Без проекта");
  select.appendChild(none);
  for (const p of meetProjects) {
    const option = document.createElement("option");
    option.value = p.id;
    option.textContent = p.name;
    select.appendChild(option);
  }
  select.value = m.project || "";

  // Встреча приехала по синхронизации без звука: расшифровать заново,
  // разделить голоса и выгрузить .wav нечем — кнопок не показываем.
  el("meetAgain").hidden = !m.audio;
  el("meetSpeakers").hidden = !m.audio;
  el("exportMenu").querySelector('[data-format="wav"]').hidden = !m.audio;

  // Идущая работа: полоса с процентами и отменой, не только текст в шапке.
  const working = !!m.phase;
  el("meetWorkRow").hidden = !working;
  if (working) {
    el("meetWorkBar").className =
      "model-progress" + (m.progress == null ? " busy" : "");
    el("meetWorkFill").style.width = `${m.progress ?? 100}%`;
    el("meetWorkLabel").textContent = stateLabel(m);
  }

  renderSummary(m);
  renderExtras(m);
  renderQa(m);

  detailSegments = await invoke("meeting_segments", { id: detailId });
  renderSpeakersPanel(m, detailSegments);

  const box = el("meetSegments");
  box.textContent = "";

  let lastSpeaker = null;
  for (const s of detailSegments) {
    // Подпись говорящего — на смене голоса, как в пьесе. Клик по ней даёт
    // человеку имя; имя уходит и в экспорт.
    if (s.spk !== null && s.spk !== undefined && s.spk !== lastSpeaker) {
      lastSpeaker = s.spk;
      const head = document.createElement("button");
      head.className = `speaker speaker-${s.spk % 6}`;
      head.textContent = speakerName(m, s.spk);
      head.title = t("Нажмите, чтобы дать имя");
      head.onclick = () => focusSpeakerField(s.spk);
      box.appendChild(head);
    }

    const row = document.createElement("div");
    row.className = "segment";
    row.dataset.s = String(s.s);
    const clock = document.createElement("span");
    clock.className = "segment-clock";
    clock.textContent = fmtClock(s.s);
    const body = document.createElement("p");
    body.className = "segment-text";
    const needle = el("meetFind").value.trim();
    if (needle && s.text.toLowerCase().includes(needle.toLowerCase())) {
      body.appendChild(highlighted(s.text, needle));
      row.classList.add("found");
    } else {
      body.textContent = s.text;
    }
    row.append(clock, body);
    box.appendChild(row);
  }
  const needle = el("meetFind").value.trim();
  const found = box.querySelectorAll(".segment.found").length;
  el("meetFindCount").hidden = !needle;
  el("meetFindCount").textContent = needle
    ? found
      ? plural(found, t("совпадение"), t("совпадения"), t("совпадений"))
      : t("ничего")
    : "";
  findIndex = -1;

  if (!detailSegments.length && m.phase) {
    const hint = document.createElement("p");
    hint.className = "muted small";
    hint.textContent = t("Реплики появляются по мере расшифровки");
    box.appendChild(hint);
  }
}

// --- говорящие -------------------------------------------------------------

function speakerName(meeting, index) {
  const own = meeting.names?.[String(index)];
  return own && own.trim() ? own : t("Говорящий {0}", index + 1);
}

/**
 * Панель со всеми найденными голосами: имена правятся в полях и тут же
 * подставляются в текст. Клик по подписи в тексте наводит сюда фокус —
 * так понятно, где именно править.
 */
function renderSpeakersPanel(meeting, segments) {
  const panel = el("speakersPanel");
  const box = el("speakersList");
  box.textContent = "";

  // Сколько речи у каждого — по нему понятно, кто здесь главный.
  const seconds = new Map();
  for (const s of segments) {
    if (s.spk === null || s.spk === undefined) continue;
    seconds.set(s.spk, (seconds.get(s.spk) || 0) + (s.e - s.s));
  }
  const found = [...seconds.keys()].sort((a, b) => a - b);
  panel.hidden = found.length === 0;
  if (!found.length) return;

  for (const index of found) {
    const row = document.createElement("div");
    row.className = "speaker-row";

    const dot = document.createElement("span");
    dot.className = `speaker-dot speaker-bg-${index % 6}`;

    const input = document.createElement("input");
    input.className = "search";
    input.id = `speakerName${index}`;
    input.type = "text";
    input.placeholder = t("Говорящий {0}", index + 1);
    input.value = meeting.names?.[String(index)] || "";
    const commit = () => {
      invoke("meeting_rename_speaker", {
        id: meeting.id,
        speaker: index,
        name: input.value,
      });
    };
    input.addEventListener("change", commit);
    input.addEventListener("keydown", (e) => {
      if (e.key === "Enter") input.blur();
    });

    const share = document.createElement("span");
    share.className = "speaker-share";
    share.textContent = fmtDur(seconds.get(index));

    row.append(dot, input, share);
    box.appendChild(row);
  }
}

/** Клик по подписи в тексте ведёт к её полю в панели. */
function focusSpeakerField(index) {
  const input = el(`speakerName${index}`);
  if (!input) return;
  input.focus();
  input.select();
  input.scrollIntoView({ block: "center", behavior: "smooth" });
}

el("meetSpeakers").addEventListener("click", async (e) => {
  e.stopPropagation();
  const menu = el("speakersMenu");
  menu.hidden = !menu.hidden;
  if (menu.hidden) return;
  // Первая диаризация тянет модель голосов — предупреждаем о докачке.
  const [ready, mb] = await invoke("diarize_status");
  el("speakersHint").textContent = ready
    ? t("Разбор идет на этом компьютере")
    : t("Первый раз докачает модель голосов, {0} МБ", mb);
});

el("speakersMenu").addEventListener("click", (e) => {
  e.stopPropagation();
  const value = e.target.closest("[data-speakers]")?.dataset.speakers;
  if (value === undefined) return;
  el("speakersMenu").hidden = true;
  invoke("meeting_diarize", { id: detailId, speakers: Number(value) });
  showDetailStatus(t("Разделяю говорящих"));
});

document.addEventListener("click", () => {
  el("speakersMenu").hidden = true;
});

listen("solflow-diarize-failed", (e) =>
  showDetailStatus(t("Не удалось разделить говорящих: {0}", e.payload))
);

// Поиск по открытой расшифровке: подсвечивает слова и ведёт по ним.
clearableSearch("meetFind", "meetFindClear", () => renderDetail());

el("meetFind").addEventListener("keydown", (e) => {
  if (e.key !== "Enter") return;
  const found = [...document.querySelectorAll("#meetSegments .segment.found")];
  if (!found.length) return;
  findIndex = (findIndex + 1) % found.length;
  found.forEach((row, i) => row.classList.toggle("current", i === findIndex));
  found[findIndex].scrollIntoView({ block: "center", behavior: "smooth" });
  el("meetFindCount").textContent = t("{0} из {1}", findIndex + 1, found.length);
});

let findIndex = -1;

el("meetBack").addEventListener("click", closeMeeting);

el("meetProjectSelect").addEventListener("change", () => {
  const value = el("meetProjectSelect").value;
  invoke("meeting_set_project", {
    id: detailId,
    project: value || null,
  });
});

el("meetCopy").addEventListener("click", () => {
  const text = detailSegments.map((s) => s.text).join("\n");
  navigator.clipboard.writeText(text);
  showDetailStatus(t("Скопировано"));
});

function showDetailStatus(text) {
  el("meetDetailStatus").textContent = text;
  el("meetDetailStatus").hidden = false;
}

async function exportMeeting(format) {
  const m = meetRows.find((r) => r.id === detailId);
  if (!m) return;
  showDetailStatus(t("Готовлю файл"));
  try {
    await invoke("meeting_export", {
      id: detailId,
      format,
      title: meetingTitle(m),
    });
    showDetailStatus(t("Файл .{0} сохранен в Загрузки", format));
  } catch (err) {
    showDetailStatus(String(err));
  }
}

el("meetExport").addEventListener("click", (e) => {
  e.stopPropagation();
  const menu = el("exportMenu");
  menu.hidden = !menu.hidden;
});
el("exportMenu").addEventListener("click", (e) => {
  e.stopPropagation();
  const format = e.target.closest("[data-format]")?.dataset.format;
  if (!format) return;
  el("exportMenu").hidden = true;
  exportMeeting(format);
});
document.addEventListener("click", () => {
  el("exportMenu").hidden = true;
});

el("meetAgain").addEventListener("click", () => {
  invoke("meeting_transcribe", { id: detailId });
  showDetailStatus(t("Расшифровка пошла заново"));
});

// Удаление в два нажатия — окно подтверждения тут ни к чему.
let deleteArmed = null;
el("meetDelete").addEventListener("click", () => {
  if (!deleteArmed) {
    el("meetDeleteLabel").textContent = t("Точно удалить?");
    deleteArmed = setTimeout(() => {
      deleteArmed = null;
      el("meetDeleteLabel").textContent = t("Удалить");
    }, 3000);
    return;
  }
  clearTimeout(deleteArmed);
  deleteArmed = null;
  el("meetDeleteLabel").textContent = t("Удалить");
  invoke("meeting_delete", { id: detailId });
  closeMeeting();
});

// Переименование — по клику на заголовок, как на Android.
el("meetTitle").addEventListener("click", () => {
  const m = meetRows.find((r) => r.id === detailId);
  if (!m) return;
  el("meetTitle").hidden = true;
  const input = el("meetTitleInput");
  input.hidden = false;
  input.value = m.title || meetingTitle(m);
  input.focus();
  input.select();
});

function commitTitle(save) {
  const input = el("meetTitleInput");
  if (input.hidden) return;
  input.hidden = true;
  el("meetTitle").hidden = false;
  if (save && detailId !== null) {
    invoke("meeting_rename", { id: detailId, title: input.value });
  }
}
el("meetTitleInput").addEventListener("keydown", (e) => {
  if (e.key === "Enter") commitTitle(true);
  if (e.key === "Escape") commitTitle(false);
});
el("meetTitleInput").addEventListener("blur", () => commitTitle(true));


// --- история диктовки ------------------------------------------------------

let historyRows = [];

function fmtWhen(at) {
  const d = new Date(at);
  const today = new Date();
  const sameDay = d.toDateString() === today.toDateString();
  const time = d.toLocaleTimeString("ru", { hour: "2-digit", minute: "2-digit" });
  if (sameDay) return time;
  return `${d.toLocaleDateString("ru", { day: "numeric", month: "short" })}, ${time}`;
}

async function refreshHistory() {
  historyRows = await invoke("history_list");
  renderHistory();
}

function renderHistory() {
  const needle = el("historySearch").value.trim().toLowerCase();
  const list = el("historyList");
  list.textContent = "";

  const shown = historyRows.filter(
    (h) => !needle || h.text.toLowerCase().includes(needle)
  );
  el("historyEmpty").hidden = historyRows.length > 0;

  for (const entry of shown) {
    const row = document.createElement("div");
    row.className = "history-item";

    const when = document.createElement("span");
    when.className = "history-when";
    when.textContent = fmtWhen(entry.at);

    const body = document.createElement("div");
    body.className = "history-body";

    const text = document.createElement("p");
    text.className = "history-text";
    text.textContent = entry.text;
    body.appendChild(text);

    // Плеер появляется только у записей со звуком: он и так лежит рядом,
    // а переслушать проще, чем гадать, что там было сказано.
    if (entry.audio) {
      const player = document.createElement("div");
      player.className = "player";

      const play = document.createElement("button");
      play.className = "player-play";
      play.title = t("Прослушать");
      play.innerHTML = ICON_PLAY;

      const bar = document.createElement("div");
      bar.className = "player-bar";
      const fill = document.createElement("div");
      bar.appendChild(fill);

      const time = document.createElement("span");
      time.className = "player-time";
      time.textContent = fmtClock(entry.seconds || 0);

      let audio = null;
      play.onclick = async () => {
        if (audio && !audio.paused) {
          audio.pause();
          play.innerHTML = ICON_PLAY;
          return;
        }
        if (!audio) {
          const bytes = await invoke("history_audio", { at: entry.at });
          const blob = new Blob([new Uint8Array(bytes)], { type: "audio/wav" });
          audio = new Audio(URL.createObjectURL(blob));
          audio.ontimeupdate = () => {
            const done = audio.duration ? audio.currentTime / audio.duration : 0;
            fill.style.width = `${done * 100}%`;
            time.textContent = fmtClock(audio.currentTime);
          };
          audio.onended = () => {
            play.innerHTML = ICON_PLAY;
            fill.style.width = "0%";
            time.textContent = fmtClock(entry.seconds || 0);
          };
        }
        audio.play();
        play.innerHTML = ICON_PAUSE;
      };

      // Перемотка щелчком по полосе — иначе длинную диктовку не отлистать.
      bar.onclick = (e) => {
        if (!audio || !audio.duration) return;
        const box = bar.getBoundingClientRect();
        audio.currentTime = ((e.clientX - box.left) / box.width) * audio.duration;
      };

      player.append(play, bar, time);
      body.appendChild(player);
    }

    const actions = document.createElement("div");
    actions.className = "history-actions";

    const iconButton = (icon, title, action, danger) => {
      const button = document.createElement("button");
      button.className = "icon-only" + (danger ? " danger" : "");
      button.title = title;
      button.innerHTML = icon;
      button.onclick = action;
      actions.appendChild(button);
      return button;
    };

    iconButton(ICON_COPY, t("Скопировать текст"), () => {
      navigator.clipboard.writeText(entry.text);
      el("historyHint").textContent = t("Скопировано");
      setTimeout(() => (el("historyHint").textContent = HISTORY_HINT), 1500);
    });

    if (entry.audio) {
      iconButton(ICON_REDO, t("Расшифровать заново"), () => {
        el("historyHint").textContent = t("Расшифровываю заново");
        invoke("history_retranscribe", { at: entry.at });
      });
    }

    let armed = false;
    const remove = iconButton(
      ICON_TRASH,
      t("Удалить запись"),
      () => {
        if (!armed) {
          armed = true;
          remove.classList.add("armed");
          setTimeout(() => {
            armed = false;
            remove.classList.remove("armed");
          }, 3000);
          return;
        }
        invoke("history_delete", { at: entry.at });
      },
      true
    );

    row.append(when, body, actions);
    list.appendChild(row);
  }
}

const ICON_PLAY =
  '<svg viewBox="0 0 24 24" fill="currentColor" aria-hidden="true">' +
  '<path d="M8 5.5v13l11-6.5z"/></svg>';
const ICON_PAUSE =
  '<svg viewBox="0 0 24 24" fill="currentColor" aria-hidden="true">' +
  '<rect x="7" y="5.5" width="3.5" height="13" rx="1"/>' +
  '<rect x="13.5" y="5.5" width="3.5" height="13" rx="1"/></svg>';
const ICON_COPY =
  '<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.7" ' +
  'stroke-linecap="round" stroke-linejoin="round" aria-hidden="true">' +
  '<rect x="9" y="9" width="11" height="11" rx="2"/>' +
  '<path d="M15 6.5V6a2 2 0 0 0-2-2H6a2 2 0 0 0-2 2v7a2 2 0 0 0 2 2h.5"/></svg>';
const ICON_REDO =
  '<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.7" ' +
  'stroke-linecap="round" stroke-linejoin="round" aria-hidden="true">' +
  '<path d="M20 11a8 8 0 1 0-2.3 5.7"/><path d="M20 5v6h-6"/></svg>';
const ICON_TRASH =
  '<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.7" ' +
  'stroke-linecap="round" aria-hidden="true">' +
  '<path d="M4 7h16M9.5 7V4.5h5V7M6.5 7l1 13h9l1-13"/></svg>';

const HISTORY_HINT =
  t("Хранятся последние триста расшифровок. Нажмите на текст, чтобы скопировать");

clearableSearch("historySearch", "historySearchClear", renderHistory);

// Очистка всей истории — тоже в два нажатия, как удаление встреч.
let historyClearArmed = null;
el("historyClear").addEventListener("click", () => {
  if (!historyClearArmed) {
    el("historyClearLabel").textContent = t("Точно очистить?");
    historyClearArmed = setTimeout(() => {
      historyClearArmed = null;
      el("historyClearLabel").textContent = t("Очистить историю");
    }, 3000);
    return;
  }
  clearTimeout(historyClearArmed);
  historyClearArmed = null;
  el("historyClearLabel").textContent = t("Очистить историю");
  invoke("history_clear");
});

// --- настройки -------------------------------------------------------------

// Перечисление микрофонов на Windows идёт через WASAPI и занимает заметное
// время, а настройки перечитываются после каждого переключателя — поэтому
// список берётся один раз и обновляется только при открытии экрана.
let knownDevices = null;

async function refreshSettings(reloadDevices = false) {
  if (reloadDevices || !knownDevices) {
    knownDevices = await invoke("list_input_devices");
  }
  const [settings, devices] = [await invoke("get_settings"), knownDevices];

  const select = el("inputDevice");
  select.textContent = "";
  const auto = document.createElement("option");
  auto.value = "";
  auto.textContent = t("Системный по умолчанию");
  select.appendChild(auto);
  for (const name of devices) {
    const option = document.createElement("option");
    option.value = name;
    option.textContent = name;
    select.appendChild(option);
  }
  // Записанный микрофон мог отключиться — показываем его отдельной строкой,
  // чтобы выбор не сбрасывался молча.
  if (settings.input_device && !devices.includes(settings.input_device)) {
    const missing = document.createElement("option");
    missing.value = settings.input_device;
    missing.textContent = t("{0} — сейчас не подключен", settings.input_device);
    select.appendChild(missing);
  }
  select.value = settings.input_device || "";

  // Настройки — источник правды: если в них другой язык, окно
  // перезагружается один раз и дальше держит его.
  const wanted = settings.language === "auto" || !settings.language
    ? systemLanguage()
    : settings.language;
  if (wanted !== UI_LANG) {
    localStorage.setItem("solflow-lang", wanted);
    location.reload();
    return;
  }

  applyTheme(settings.theme);
  markSegments("languageSegments", "language", settings.language || "auto");
  markSegments("startHiddenSegments", "start", settings.start_hidden ? "hide" : "show");
  markSegments("overlayStyleSegments", "overlay", settings.overlay_style);
  markSegments("overlayPositionSegments", "position", settings.overlay_position);
  markSegments("clipboardSegments", "clipboard", settings.clipboard_handling);
  markSegments(
    "submitSegments",
    "submit",
    settings.auto_submit ? settings.auto_submit_key : "off"
  );
  el("overlayPositionRow").hidden = settings.overlay_style === "none";

  markToggle("useGpu", "useGpuLabel", [t("Включено"), t("Выключено")], settings.use_gpu);
  el("gpuHint").textContent = lastDevice
    ? t("Сейчас считает {0}", lastDevice)
    : settings.use_gpu
      ? t("Расшифровка идет быстрее, если видеокарта подходит")
      : t("Считает процессор");

  markToggle("trayIcon", "trayIconLabel", [t("Показана"), t("Скрыта")], settings.show_tray_icon);
  el("trayHint").textContent = settings.show_tray_icon
    ? t("Через нее открывается окно и выход")
    : t("Окно вернется, если запустить приложение снова");
  markToggle(
    "muteRecording",
    "muteRecordingLabel",
    [t("Включено"), t("Выключено")],
    settings.mute_while_recording
  );
  markToggle(
    "removeFillers",
    "removeFillersLabel",
    [t("Включено"), t("Выключено")],
    settings.remove_fillers
  );
  markToggle("keepAudio", "keepAudioLabel", [t("Включено"), t("Выключено")], settings.keep_audio);
  el("modelUnload").value = settings.model_unload;
  el("historyLimit").value = String(settings.history_limit);
  el("historyRetention").value = settings.history_retention;

  const autostart = await invoke("autostart_enabled");
  el("autostart").classList.toggle("on", autostart);
  el("autostartLabel").textContent = autostart ? t("Включен") : t("Выключен");
  el("startSound").classList.toggle("on", settings.start_sound);
  el("startSoundLabel").textContent = settings.start_sound ? t("Включен") : t("Выключен");
  markToggle(
    "keepAwake",
    "keepAwakeLabel",
    [t("Включено"), t("Выключено")],
    settings.keep_audio_awake
  );
  el("keepAwakeRow").hidden = (await invoke("os_name")) !== "Windows";

  const keep = settings.downloads_dir;
  el("downloadsHint").textContent = keep
    ? t("Сохраняю в {0}", keep)
    : t("Файл удаляется после расшифровки — приложению нужен только звук");
  el("clearDownloadsDir").hidden = !keep;
  el("pickDownloadsDir").textContent = keep ? t("Другая папка") : t("Выбрать папку");

  const exportDir = settings.export_dir;
  const exportMode = settings.export_ask ? "ask" : exportDir ? "folder" : "downloads";
  markSegments("exportSegments", "export", exportMode);
  el("pickExportDir").hidden = exportMode !== "folder";
  el("exportHint").textContent =
    exportMode === "ask"
      ? t("Спрошу папку и имя при каждом экспорте")
      : exportMode === "folder"
        ? t("Сохраняю в {0} — папка открывается после сохранения", exportDir)
        : t("Сейчас в «Загрузки» — после сохранения папка открывается сама");

  const hasDownloader = await invoke("downloader_ready");
  el("downloaderDone").hidden = !hasDownloader;
  el("installDownloader").hidden = hasDownloader;
  if (hasDownloader) {
    el("downloaderHint").textContent =
      t("Ссылки на YouTube и VK скачиваются и расшифровываются");
  }

  renderSync(await invoke("sync_status"));
}

// --- синхронизация ---------------------------------------------------------

/** Ссылка со страницей ввода кода — пока код живёт. */
let syncVerificationUrl = "https://oauth.yandex.ru/device";

/** «сегодня в 14:30», «вчера в 09:12» или дата — для последней синхронизации. */
function fmtSyncTime(ms) {
  const d = new Date(ms);
  const time = d.toLocaleTimeString("ru", { hour: "2-digit", minute: "2-digit" });
  const today = new Date();
  const sameDay = (a, b) =>
    a.getFullYear() === b.getFullYear() &&
    a.getMonth() === b.getMonth() &&
    a.getDate() === b.getDate();
  const yesterday = new Date(today);
  yesterday.setDate(today.getDate() - 1);
  if (sameDay(d, today)) return t("сегодня в {0}", time);
  if (sameDay(d, yesterday)) return t("вчера в {0}", time);
  return `${d.toLocaleDateString("ru", { day: "numeric", month: "long" })}, ${time}`;
}

/** Нажали «Синхронизировать» руками — по окончании кнопка ответит цветом. */
let syncManual = false;
let syncWasRunning = false;
let syncFlashUntil = 0;

function flashSync(ok) {
  syncFlashUntil = Date.now() + 2500;
  for (const id of ["meetSyncNow", "syncNow"]) {
    const button = el(id);
    button.classList.remove("ok", "fail");
    button.classList.add(ok ? "ok" : "fail");
    button.textContent = ok ? t("Синхронизировано") : t("Не вышло");
  }
  setTimeout(() => {
    for (const id of ["meetSyncNow", "syncNow"]) {
      el(id).classList.remove("ok", "fail");
      el(id).textContent = t("Синхронизировать");
    }
  }, 2500);
}

function renderSync(s) {
  const hint = el("syncHint");
  hint.classList.remove("sync-error");

  // Ручная синхронизация закончилась: зелёным — сошлось, красным — нет.
  if (syncWasRunning && !s.running && syncManual) {
    syncManual = false;
    flashSync(!s.message);
  }
  syncWasRunning = s.running;
  const flashing = !s.running && Date.now() < syncFlashUntil;

  el("syncConnect").hidden = s.connected || !!s.code || !s.configured;
  // Код принят или протух — окно входа больше не нужно.
  if (s.connected || !s.code) el("syncDialog").hidden = true;
  el("syncDisconnect").hidden = !s.connected;
  el("syncNow").hidden = !s.connected;
  el("syncCodeRow").hidden = !s.code;
  el("syncOptions").hidden = !s.connected;

  if (!s.configured) {
    hint.textContent = t("В этой сборке нет ключей Яндекс OAuth — синхронизация недоступна");
  } else if (s.code) {
    el("syncCode").textContent = s.code.user_code;
    syncVerificationUrl = s.code.verification_url;
    hint.textContent = t("Откройте страницу Яндекса, введите код и разрешите доступ к папке приложения. Жду подтверждения");
  } else if (s.connected) {
    const who = s.login ? t("Аккаунт {0}. ", s.login) : "";
    if (s.progress) {
      hint.textContent = s.progress;
    } else if (s.running) {
      hint.textContent = who + t("Синхронизирую");
    } else if (s.message) {
      hint.textContent = who + t("Не вышло: {0}", s.message);
      hint.classList.add("sync-error");
    } else if (s.last_sync) {
      hint.textContent = who + t("Синхронизировано {0}", fmtSyncTime(s.last_sync));
    } else {
      hint.textContent = who + t("Еще не синхронизировалось");
    }
  } else if (s.message) {
    hint.textContent = t("Не вышло: {0}", s.message);
    hint.classList.add("sync-error");
  } else {
    hint.textContent = t(
      "Встречи и проекты синхронизируются между устройствами через папку приложения на вашем Диске"
    );
  }

  el("syncNow").disabled = s.running;
  if (!flashing) {
    el("syncNow").textContent = s.running ? t("Синхронизирую…") : t("Синхронизировать");
  }
  el("syncInterval").value = s.sync_interval || "min2";

  // Кнопка на странице встреч: живёт, пока подключено, и показывает время
  // последней синхронизации подсказкой.
  const quick = el("meetSyncNow");
  quick.hidden = !s.connected;
  quick.disabled = s.running;
  if (!flashing) {
    quick.textContent = s.running ? t("Синхронизирую…") : t("Синхронизировать");
  }
  quick.title = s.last_sync
    ? t("Синхронизировано {0}", fmtSyncTime(s.last_sync))
    : t("Еще не синхронизировалось");
  markToggle("syncAudio", "syncAudioLabel", [t("Включено"), t("Выключено")], s.sync_audio);
  markToggle(
    "syncAutoSummary",
    "syncAutoSummaryLabel",
    [t("Включено"), t("Выключено")],
    s.sync_auto_summary
  );
}

listen("solflow-sync", (e) => renderSync(e.payload));

/** Код в буфер: человеку остаётся только вставить его на странице. */
function copySyncCode() {
  const code = el("syncDialogCode").textContent || el("syncCode").textContent;
  if (code) navigator.clipboard.writeText(code);
}

el("syncConnect").addEventListener("click", async () => {
  el("syncConnect").disabled = true;
  try {
    const code = await invoke("sync_connect");
    syncVerificationUrl = code.verification_url;
    // Код сразу в буфере, а страница не открывается сама: сначала окно с
    // тремя шагами, чтобы было ясно, что делать дальше.
    el("syncDialogCode").textContent = code.user_code;
    copySyncCode();
    el("syncDialog").hidden = false;
    renderSync(await invoke("sync_status"));
  } catch (err) {
    el("syncHint").textContent = t("Не вышло: {0}", String(err));
    el("syncHint").classList.add("sync-error");
  } finally {
    el("syncConnect").disabled = false;
  }
});

el("syncOpenPage").addEventListener("click", () =>
  invoke("open_link", { url: syncVerificationUrl })
);
el("syncDialogOpen").addEventListener("click", () => {
  copySyncCode();
  invoke("open_link", { url: syncVerificationUrl });
});
el("syncDialogCopy").addEventListener("click", () => {
  copySyncCode();
  el("syncDialogCopy").textContent = t("Скопировано");
  setTimeout(() => (el("syncDialogCopy").textContent = t("Скопировать код")), 1500);
});
el("syncDialogCancel").addEventListener("click", () => {
  el("syncDialog").hidden = true;
  invoke("sync_connect_cancel");
});
el("syncCopyCode").addEventListener("click", () => {
  navigator.clipboard.writeText(el("syncCode").textContent);
  el("syncCopyCode").textContent = t("Скопировано");
  setTimeout(() => (el("syncCopyCode").textContent = t("Скопировать код")), 1500);
});
el("syncCancelCode").addEventListener("click", () => invoke("sync_connect_cancel"));
const syncByHand = () => {
  syncManual = true;
  invoke("sync_now");
};
el("syncNow").addEventListener("click", syncByHand);
el("meetSyncNow").addEventListener("click", syncByHand);
el("syncInterval").addEventListener("change", () =>
  option("sync_interval", el("syncInterval").value)
);
// Состояние кнопки на странице встреч нужно и до первого захода в настройки.
invoke("sync_status").then(renderSync);

// Отключение в два нажатия: встречи остаются и здесь, и на Диске, но
// вход придётся повторять.
let syncDisconnectArmed = false;
el("syncDisconnect").addEventListener("click", async () => {
  if (!syncDisconnectArmed) {
    syncDisconnectArmed = true;
    el("syncDisconnect").textContent = t("Точно отключить?");
    setTimeout(() => {
      syncDisconnectArmed = false;
      el("syncDisconnect").textContent = t("Отключить");
    }, 3000);
    return;
  }
  syncDisconnectArmed = false;
  el("syncDisconnect").textContent = t("Отключить");
  await invoke("sync_disconnect");
  renderSync(await invoke("sync_status"));
});

bindToggle("syncAudio", "syncAudioLabel", [t("Включено"), t("Выключено")], (on) =>
  option("sync_audio", on)
);
bindToggle(
  "syncAutoSummary",
  "syncAutoSummaryLabel",
  [t("Включено"), t("Выключено")],
  (on) => option("sync_auto_summary", on)
);

/** Сегменты: один выбранный из нескольких. */
function bindSegments(boxId, attr, onPick) {
  document.querySelectorAll(`#${boxId} .segment`).forEach((button) => {
    button.addEventListener("click", () => {
      document.querySelectorAll(`#${boxId} .segment`).forEach((b) => {
        b.classList.toggle("on", b === button);
      });
      onPick(button.dataset[attr]);
    });
  });
}

function markSegments(boxId, attr, value) {
  document.querySelectorAll(`#${boxId} .segment`).forEach((b) => {
    b.classList.toggle("on", b.dataset[attr] === value);
  });
}

/** Тумблер: сам переключается и сообщает новое состояние. */
function bindToggle(id, labelId, onOff, onChange) {
  el(id).addEventListener("click", () => {
    const on = !el(id).classList.contains("on");
    el(id).classList.toggle("on", on);
    el(labelId).textContent = on ? onOff[0] : onOff[1];
    onChange(on);
  });
}

function markToggle(id, labelId, onOff, on) {
  el(id).classList.toggle("on", on);
  el(labelId).textContent = on ? onOff[0] : onOff[1];
}

const option = (key, value) => invoke("set_option", { key, value });

bindSegments("startHiddenSegments", "start", (v) =>
  option("start_hidden", v === "hide")
);
bindSegments("overlayStyleSegments", "overlay", (v) => {
  option("overlay_style", v);
  el("overlayPositionRow").hidden = v === "none";
});
bindSegments("overlayPositionSegments", "position", (v) =>
  option("overlay_position", v)
);
bindSegments("clipboardSegments", "clipboard", (v) =>
  option("clipboard_handling", v)
);
bindSegments("submitSegments", "submit", (v) => {
  option("auto_submit", v !== "off");
  if (v !== "off") option("auto_submit_key", v);
});

bindToggle("trayIcon", "trayIconLabel", [t("Показана"), t("Скрыта")], (on) => {
  option("show_tray_icon", on);
  el("trayHint").textContent = on
    ? t("Через нее открывается окно и выход")
    : t("Окно вернется, если запустить приложение снова");
});
bindToggle("muteRecording", "muteRecordingLabel", [t("Включено"), t("Выключено")], (on) =>
  option("mute_while_recording", on)
);
bindToggle("keepAwake", "keepAwakeLabel", [t("Включено"), t("Выключено")], (on) =>
  option("keep_audio_awake", on)
);
bindToggle("removeFillers", "removeFillersLabel", [t("Включено"), t("Выключено")], (on) =>
  option("remove_fillers", on)
);
bindToggle("keepAudio", "keepAudioLabel", [t("Включено"), t("Выключено")], (on) =>
  option("keep_audio", on)
);

el("modelUnload").addEventListener("change", () =>
  option("model_unload", el("modelUnload").value)
);
el("historyLimit").addEventListener("change", async () => {
  await option("history_limit", Number(el("historyLimit").value));
  refreshHistory();
});
el("historyRetention").addEventListener("change", async () => {
  await option("history_retention", el("historyRetention").value);
  refreshHistory();
});

/** Тема применяется сразу и запоминается в настройках. */
function applyTheme(theme) {
  const root = document.documentElement;
  if (theme === "light" || theme === "dark") {
    root.setAttribute("data-theme", theme);
  } else {
    root.removeAttribute("data-theme");
  }
  document.querySelectorAll("#themeSegments .segment").forEach((b) => {
    b.classList.toggle("on", b.dataset.theme === (theme || "system"));
  });
}

el("autostart").addEventListener("click", async () => {
  const enabled = !el("autostart").classList.contains("on");
  // Переключатель встаёт сразу, запись в систему идёт следом: она занимает
  // доли секунды, но ждать ответа, глядя на неподвижный переключатель,
  // неприятно. Если система откажет — вернём как было и скажем почему.
  el("autostart").classList.toggle("on", enabled);
  el("autostartLabel").textContent = enabled ? t("Включен") : t("Выключен");
  el("autostartHint").textContent = enabled
    ? t("Приложение появится в трее после входа")
    : t("Запускать придется вручную");
  try {
    await invoke("set_autostart", { enabled });
  } catch (err) {
    el("autostart").classList.toggle("on", !enabled);
    el("autostartLabel").textContent = !enabled ? t("Включен") : t("Выключен");
    el("autostartHint").textContent = String(err);
  }
});

document.querySelectorAll("#themeSegments .segment").forEach((button) => {
  button.addEventListener("click", () => {
    const theme = button.dataset.theme;
    applyTheme(theme);
    invoke("set_theme", { theme });
  });
});

el("pickDownloadsDir").addEventListener("click", async () => {
  const dir = await invoke("pick_downloads_dir");
  if (!dir) return;
  await invoke("set_downloads_dir", { dir });
  refreshSettings();
});

el("useGpu").addEventListener("click", async () => {
  const enabled = !el("useGpu").classList.contains("on");
  markToggle("useGpu", "useGpuLabel", [t("Включено"), t("Выключено")], enabled);
  // Устройство выбирается при загрузке модели, поэтому она поднимается
  // заново — пара секунд, и в подсказке появится, чем считает.
  el("gpuHint").textContent = t("Перезагружаю модель");
  lastDevice = null;
  await invoke("set_use_gpu", { enabled });
});

bindSegments("languageSegments", "language", async (choice) => {
  await invoke("set_language", { language: choice });
  const wanted = choice === "auto" ? systemLanguage() : choice;
  localStorage.setItem("solflow-lang", wanted);
  // Перерисовать уже показанное окно на ходу дороже и грязнее, чем открыть
  // его заново: перезагрузка отрабатывает мгновенно и ничего не забывает.
  if (wanted !== UI_LANG) location.reload();
});

bindSegments("exportSegments", "export", async (mode) => {
  await invoke("set_export_mode", { mode });
  // «Папка» без выбранной папки — сразу диалог: иначе непонятно, куда
  // приложение собралось сохранять.
  if (mode === "folder") {
    const dir = await invoke("pick_export_dir");
    if (dir) await invoke("set_export_dir", { dir });
  }
  refreshSettings();
});

el("pickExportDir").addEventListener("click", async () => {
  const dir = await invoke("pick_export_dir");
  if (!dir) return;
  await invoke("set_export_dir", { dir });
  refreshSettings();
});

el("clearDownloadsDir").addEventListener("click", async () => {
  await invoke("set_downloads_dir", { dir: null });
  refreshSettings();
});

// Проценты приходят из Rust: на Windows качается не только yt-dlp, но и
// ffmpeg — это десятки мегабайт, и молчащая кнопка выглядит как зависшая.
listen("solflow-downloader-progress", (e) => {
  const pct = e.payload;
  if (pct > 0 && pct < 100) {
    el("downloaderHint").textContent = t("Ставлю загрузчик, {0}%", pct);
  }
});

el("installDownloader").addEventListener("click", async () => {
  el("installDownloader").disabled = true;
  el("downloaderHint").textContent = IS_MAC
    ? t("Ставлю загрузчик, это займет минуту")
    : t("Ставлю загрузчик и ffmpeg, это займет несколько минут");
  try {
    await invoke("install_downloader");
    el("downloaderHint").textContent = t("Готово, ссылки на видео теперь работают");
  } catch (err) {
    el("downloaderHint").textContent = String(err);
  }
  el("installDownloader").disabled = false;
  refreshSettings();
});

el("inputDevice").addEventListener("change", () => {
  invoke("set_input_device", { device: el("inputDevice").value || null });
});

el("startSound").addEventListener("click", async () => {
  const enabled = !el("startSound").classList.contains("on");
  await invoke("set_start_sound", { enabled });
  refreshSettings();
});


// --- о проекте и вводный экран ---------------------------------------------

// Ссылки открываются в браузере, а не внутри окна.
document.querySelectorAll(".link[data-url]").forEach((button) => {
  button.addEventListener("click", () => invoke("open_link", { url: button.dataset.url }));
});

/**
 * Вводный экран при первом запуске. Вместо картинок — мини-макеты из тех же
 * токенов, что и интерфейс: они не разъезжаются при правках и живут в обеих
 * темах.
 */
const INTRO = [
  {
    title: t("Голос становится текстом на вашем {0}", IS_MAC ? "Mac" : t("компьютере")),
    text:
      t("Ничего не уходит в интернет: ни диктовки, ни записи встреч. Модель ") +
      t("распознавания живет на диске и работает без сети."),
    shot: `<div class="shot">
        <div class="shot-side">
          <div class="shot-line on"></div><div class="shot-line short"></div>
          <div class="shot-line short"></div><div class="shot-line short"></div>
        </div>
        <div class="shot-main">
          <div class="shot-title"></div>
          <div class="shot-dot"></div>
          <div class="shot-wave">
            <i style="height:8px"></i><i style="height:16px"></i><i style="height:24px"></i>
            <i style="height:14px"></i><i style="height:20px"></i><i style="height:10px"></i>
            <i style="height:18px"></i><i style="height:12px"></i>
          </div>
        </div>
      </div>`,
  },
  {
    title: t("Диктуйте в любое приложение"),
    text:
      t("Нажмите {0} где угодно: быстрое нажатие — ", IS_MAC ? t("⌥Пробел") : t("Ctrl+Пробел")) +
      t("запись пошла, второе — ") +
      t("текст вставился в активное поле. Или зажмите, скажите и отпустите. ") +
      t("Сочетание и микрофон меняются в настройках."),
    shot: `<div class="shot">
        <div class="shot-side">
          <div class="shot-line on"></div><div class="shot-line short"></div>
          <div class="shot-line short"></div>
        </div>
        <div class="shot-main">
          <div class="shot-row wide"></div>
          <div class="shot-row mid"></div>
          <div class="shot-tags"><span class="shot-pill filled"></span></div>
          <div class="shot-row wide"></div>
        </div>
      </div>`,
  },
  {
    title: t("Встречи: запись, расшифровка, говорящие"),
    text:
      t("Пишите встречу часами или бросьте в окно файл — подойдет аудио и ") +
      t("видео, можно дать ссылку на YouTube или Яндекс.Диск. Приложение ") +
      t("разложит речь по времени и разделит голоса, а имена подставит в текст."),
    shot: `<div class="shot">
        <div class="shot-side">
          <div class="shot-line"></div><div class="shot-line on short"></div>
          <div class="shot-line short"></div><div class="shot-line short"></div>
        </div>
        <div class="shot-main">
          <div class="shot-title"></div>
          <div class="shot-tags">
            <span class="shot-pill"></span><span class="shot-pill"></span>
          </div>
          <div class="shot-row wide"></div>
          <div class="shot-row mid"></div>
          <div class="shot-row wide"></div>
        </div>
      </div>`,
  },
  {
    title: t("Проекты, поиск и экспорт"),
    text:
      t("Записи раскладываются по проектам — перетащите их мышью в папку ") +
      t("слева. Готовое отдается в txt, Markdown, Word и PDF: заголовок, ") +
      t("метки времени, имена говорящих."),
    shot: `<div class="shot">
        <div class="shot-side">
          <div class="shot-line"></div><div class="shot-line on short"></div>
          <div class="shot-line short"></div><div class="shot-line short"></div>
        </div>
        <div class="shot-main">
          <div class="shot-title"></div>
          <div class="shot-tags">
            <span class="shot-pill"></span><span class="shot-pill"></span>
            <span class="shot-pill filled"></span>
          </div>
          <div class="shot-row wide"></div>
          <div class="shot-row mid"></div>
        </div>
      </div>`,
  },
  {
    title: t("Встречи на всех устройствах"),
    text:
      t("Подключите Яндекс.Диск в настройках — вход по короткому коду. ") +
      t("Записи, проекты и саммери станут одинаковыми на телефоне и ") +
      t("компьютере; данные лежат в папке приложения на вашем Диске."),
    shot: `<div class="shot">
        <div class="shot-side">
          <div class="shot-line"></div><div class="shot-line short"></div>
          <div class="shot-line short"></div><div class="shot-line on short"></div>
        </div>
        <div class="shot-main">
          <div class="shot-title"></div>
          <div class="shot-row wide"></div>
          <div class="shot-tags"><span class="shot-pill filled"></span><span class="shot-pill"></span></div>
          <div class="shot-row mid"></div>
        </div>
      </div>`,
  },
];

let introStep = 0;

function renderIntro() {
  const step = INTRO[introStep];
  el("introBody").innerHTML =
    `${step.shot}<p class="intro-title">${step.title}</p>` +
    `<p class="intro-text">${step.text}</p>`;
  el("introDots").innerHTML = INTRO.map(
    (_, i) => `<span class="${i === introStep ? "on" : ""}"></span>`
  ).join("");
  el("introNext").textContent =
    introStep === INTRO.length - 1 ? t("Начать") : t("Дальше");
  el("introSkip").hidden = introStep === INTRO.length - 1;
}

function showIntro() {
  introStep = 0;
  renderIntro();
  el("intro").hidden = false;
}

function closeIntro() {
  el("intro").hidden = true;
  try {
    localStorage.setItem("introSeen", "1");
  } catch (e) {
    // Приватный режим или запрет на хранение — просто покажем ещё раз.
  }
}

el("introNext").addEventListener("click", () => {
  if (introStep === INTRO.length - 1) {
    closeIntro();
    return;
  }
  introStep += 1;
  renderIntro();
});
el("introSkip").addEventListener("click", closeIntro);
el("showIntro").addEventListener("click", showIntro);

// --- что нового ------------------------------------------------------------

// Показывается один раз после смены версии. На первом запуске хватает
// вводного экрана, поэтому окно молча помечает версию как увиденную.
// Суффикс поднимают, когда текст обновился внутри той же версии.
const WHATSNEW_REV = "-3";

/** «1.2.3» новее «1.2.2»? Пустая или кривая строка считается древней. */
function versionNewer(a, b) {
  const pa = String(a).split(".").map((n) => parseInt(n, 10) || 0);
  const pb = String(b).split(".").map((n) => parseInt(n, 10) || 0);
  for (let i = 0; i < 3; i++) {
    if ((pa[i] || 0) !== (pb[i] || 0)) return (pa[i] || 0) > (pb[i] || 0);
  }
  return false;
}

function maybeShowWhatsNew(version) {
  const seen = version + WHATSNEW_REV;
  let previous = "";
  try {
    const stored = localStorage.getItem("whatsnewSeen") || "";
    if (stored === seen) return;
    // Что человек видел в прошлый раз — чтобы показать и пропущенные
    // версии, если он перепрыгнул через релиз.
    previous = stored.split("-")[0];
    localStorage.setItem("whatsnewSeen", seen);
    if (!localStorage.getItem("introSeen")) return;
  } catch (e) {
    return; // Хранилище недоступно — не настаиваем.
  }
  for (const block of document.querySelectorAll("[data-whatsnew-version]")) {
    const v = block.dataset.whatsnewVersion;
    block.hidden = previous !== "" && !versionNewer(v, previous);
  }
  el("whatsnewTitle").textContent = t("Что нового в {0}", version);
  el("whatsnew").hidden = false;
}

el("whatsnewOk").addEventListener("click", () => {
  el("whatsnew").hidden = true;
});

// --- отчёт о проблеме ------------------------------------------------------

el("bugSend").addEventListener("click", () => {
  invoke("send_bug_report", { description: el("bugText").value });
  el("bugText").value = "";
  el("bugPreviewText").hidden = true;
});

el("bugCopy").addEventListener("click", async () => {
  const text = await invoke("bug_report", { description: el("bugText").value });
  await navigator.clipboard.writeText(text);
  el("bugCopy").textContent = t("Скопировано");
  setTimeout(() => (el("bugCopy").textContent = t("Скопировать отчет")), 1500);
});

el("bugPreview").addEventListener("click", async () => {
  const box = el("bugPreviewText");
  if (!box.hidden) {
    box.hidden = true;
    return;
  }
  box.textContent = await invoke("bug_report", { description: el("bugText").value });
  box.hidden = false;
});

// --- обновления ------------------------------------------------------------

// Приложение само смотрит, не вышла ли новая версия. Пометка появляется на
// пункте «О проекте»: узнавать об обновлении, только если сам туда зайдёшь,
// — так себе способ.
listen("solflow-update", (e) => {
  const info = e.payload;
  if (!info || !info.newer) return;
  pendingUpdate = info;
  markUpdate(info);
});

let pendingUpdate = null;

function markUpdate(info) {
  const nav = document.querySelector('.nav-item[data-page="about"]');
  if (nav) nav.classList.add("has-news");

  // Та же новость в подвале: чтобы её увидеть, не нужно никуда заходить.
  const foot = el("footVersion");
  if (foot) {
    foot.classList.add("has-news");
    el("footVersionText").textContent = t("Обновить до {0}", info.latest);
  }
  const hint = el("updateHint");
  if (!hint) return;
  hint.textContent = t("Вышла версия {0} — можно поставить", info.latest);
  el("checkUpdate").textContent = t("Обновить");
  el("checkUpdate").onclick = () => installUpdate(info);
}

// Проценты установки: файл весит десятки мегабайт, и молчащая кнопка
// выглядит как зависшая.
listen("solflow-update-progress", (e) => {
  const pct = e.payload;
  if (!updating) return;
  const text = pct >= 100 ? t("Ставлю и перезапускаю") : t("Качаю {0}%", pct);
  const hint = el("updateHint");
  if (hint) hint.textContent = text;
  el("footVersionText").textContent = text;
});

let updating = false;

async function installUpdate(info) {
  if (updating) return;
  updating = true;
  el("checkUpdate").disabled = true;
  el("updateHint").textContent = t("Качаю");
  el("footVersionText").textContent = t("Качаю");
  try {
    // Приложение перезапустится само, поэтому дальше этой строки код
    // обычно не доходит.
    await invoke("install_update");
  } catch (err) {
    updating = false;
    el("checkUpdate").disabled = false;
    el("updateHint").textContent = t("{0} — можно скачать вручную", err);
    el("checkUpdate").textContent = t("Открыть страницу");
    el("checkUpdate").onclick = () => invoke("open_link", { url: info.url });
    el("footVersionText").textContent = t("Не вышло обновить");
  }
}

// Кнопка в подвале: пока новостей нет — просто версия, по нажатию сходит и
// проверит; когда новость есть — она же ставит обновление.
let appVersion = "";

function showVersionInFoot() {
  el("footVersion").classList.remove("has-news");
  el("footVersionText").textContent = `Sol Flow ${appVersion}`;
}

el("footVersion").addEventListener("click", async () => {
  if (updating) return;
  if (pendingUpdate) {
    installUpdate(pendingUpdate);
    return;
  }
  el("footVersionText").textContent = t("Смотрю, что вышло");
  try {
    const info = await invoke("check_update");
    if (info.newer) {
      pendingUpdate = info;
      markUpdate(info);
      return;
    }
    el("footVersionText").textContent = info.latest
      ? t("У вас последняя версия")
      : t("Не удалось проверить");
  } catch {
    el("footVersionText").textContent = t("Не удалось проверить");
  }
  // Через несколько секунд возвращаем обычную подпись: подвал не место
  // для отчётов.
  setTimeout(showVersionInFoot, 4000);
});

async function checkUpdate(loud) {
  const hint = el("updateHint");
  if (loud) hint.textContent = t("Смотрю, что вышло");
  try {
    const info = await invoke("check_update");
    el("appVersion").textContent = info.current;
    if (info.latest && info.newer) {
      pendingUpdate = info;
      markUpdate(info);
    } else if (info.latest) {
      hint.textContent = t("У вас последняя версия");
    } else {
      hint.textContent = loud ? t("Не удалось проверить — нет связи?") : "";
    }
  } catch (err) {
    hint.textContent = String(err);
  }
}

el("checkUpdate").addEventListener("click", () => checkUpdate(true));

// --- запуск ----------------------------------------------------------------

// Каталог приходит из Rust уже на нужном языке, поэтому язык сообщаем
// первым и только потом просим модели и языки — иначе успеет приехать
// русский список.
const languageReady = invoke("set_ui_language", { language: UI_LANG }).catch(() => {});

invoke("ui_state");
invoke("app_version").then((version) => {
  appVersion = version;
  el("appVersion").textContent = version;
  showVersionInFoot();
  maybeShowWhatsNew(version);
});
languageReady.then(() => {
  refreshModels();
  invoke("list_languages").then((rows) => {
    languageRows = rows;
    renderFilters();
  });
});
// Замер сделан на GigaAM Q4 через Metal — единственная цифра, которую мы
// действительно мерили, поэтому названа именно она.
invoke("machine_chip").then((chip) => {
  el("chipHint").textContent =
    t("Зеленым помечена активная. У вас {0}: GigaAM считает на нем ", chip) +
    t("примерно в 115 раз быстрее речи. Скачивание идет в фоне.");
});
refreshMeetings();
refreshHistory();
invoke("get_settings")
  .then((settings) => applyTheme(settings.theme))
  .catch(() => {});
invoke("check_update").then((info) => {
  el("appVersion").textContent = info.current;
}).catch(() => {});

// Первый запуск — показываем вводный экран.
try {
  if (!localStorage.getItem("introSeen")) showIntro();
} catch (e) {
  // Хранилище недоступно: молча пропускаем, чтобы не мешать работе.
}

drawWave();

// --- различия систем в разметке -------------------------------------------

if (!IS_MAC) {
  // На Windows это не меню-бар, а трей — правим подписи разом, чтобы не
  // держать два варианта разметки. По-английски замена та же, только слова
  // другие, поэтому идём через словарь.
  const from = [t("меню-баре"), t("меню-бар")];
  const to = [t("трее"), t("трей")];
  for (const node of document.querySelectorAll(".perm-title, .muted")) {
    if (node.children.length) continue;
    let text = node.textContent;
    from.forEach((word, i) => {
      text = text.split(word).join(to[i]);
    });
    if (text !== node.textContent) node.textContent = text;
  }

  // «Универсальный доступ» — разрешение macOS: на Windows вставка работает
  // сразу, и строке в настройках там взяться неоткуда.
  el("permAccessibility")?.remove();

  // ⌃Enter и ⌘Enter на Windows — одно и то же нажатие, второй сегмент лишний.
  document.querySelector('[data-submit="cmd_enter"]')?.remove();
  const ctrlEnter = document.querySelector('[data-submit="ctrl_enter"]');
  if (ctrlEnter) ctrlEnter.textContent = "Ctrl+Enter";

  // Подпись сочетания до первого ответа из Rust.
  for (const id of ["hotkeyLabel", "hotkeyLabel2"]) {
    const node = el(id);
    if (node && node.textContent.includes("⌥")) node.textContent = t("Ctrl + Пробел");
  }
}
