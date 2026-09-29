// Pointer and keyboard resize; the preferred width survives app restarts.
(() => {
  const pane = document.getElementById("sidebar"), handle = document.getElementById("sidebarResize");
  if (!pane || !handle) return;
  const key = "solflow-sidebar-width", minimum = 208, defaultWidth = 248;
  const maximum = () => Math.max(minimum, Math.min(480, innerWidth - 420));
  let preferred = defaultWidth, pointer = null, startX = 0, startWidth = 0;
  try { const saved = Number(localStorage.getItem(key)); if (Number.isFinite(saved) && saved >= minimum) preferred = Math.min(saved, 480); } catch (_) {}
  function apply() {
    const width = Math.round(Math.max(minimum, Math.min(preferred, maximum())));
    pane.style.setProperty("--sidebar-width", width + "px");
    handle.setAttribute("aria-valuemin", minimum); handle.setAttribute("aria-valuemax", maximum()); handle.setAttribute("aria-valuenow", width);
  }
  function save() { try { localStorage.setItem(key, String(preferred)); } catch (_) {} }
  function finish(event) {
    if (pointer !== event.pointerId) return;
    pointer = null; document.body.classList.remove("sidebar-dragging"); save();
  }
  handle.addEventListener("pointerdown", event => {
    if (event.button !== 0 || pointer !== null) return;
    event.preventDefault(); pointer = event.pointerId; startX = event.clientX; startWidth = pane.getBoundingClientRect().width;
    handle.setPointerCapture(pointer); handle.focus(); document.body.classList.add("sidebar-dragging");
  });
  handle.addEventListener("pointermove", event => {
    if (event.pointerId !== pointer) return;
    preferred = Math.max(minimum, Math.min(maximum(), startWidth + event.clientX - startX)); apply();
  });
  ["pointerup", "pointercancel", "lostpointercapture"].forEach(name => handle.addEventListener(name, finish));
  handle.addEventListener("dblclick", () => { preferred = defaultWidth; apply(); save(); });
  handle.addEventListener("keydown", event => {
    if (!["ArrowLeft", "ArrowRight", "Home", "End"].includes(event.key)) return;
    event.preventDefault();
    preferred = event.key === "Home" ? defaultWidth : event.key === "End" ? maximum() : pane.getBoundingClientRect().width + (event.key === "ArrowRight" ? 1 : -1) * (event.shiftKey ? 40 : 16);
    preferred = Math.max(minimum, Math.min(maximum(), preferred)); apply(); save();
  });
  addEventListener("resize", apply); apply();
})();
