/**
 * The browser half of the gallery, inlined into the page. Plain DOM, no build
 * step, no framework: every view section is already in the document, so this
 * only routes, filters, switches mode, and zooms.
 *
 * Component sections route exactly like view sections — same `.view` class,
 * same nav link, same hash. They just carry no shots.
 *
 * EXP-1019: the page has FOUR sections (`style` / `general` / `special` /
 * `views`, the contract's order) and everything below acts WITHIN the current
 * one — the filter, `j`/`k`, the summary. `data-mode` on the body and on every
 * `.view` names the section; the vocabulary below still says "mode" because
 * that is the DOM contract the page has always carried. A hash always wins:
 * `#pill` switches to General components first and then shows the entry, so a
 * deep link never lands on a hidden section.
 */

export const client = `
const raw = document.getElementById("gallery-data").textContent;
const data = JSON.parse(raw);
const modes = data.modes || {};
const ids = data.views.map((entry) => entry.view.id).concat((data.components || []).map((c) => c.id));
const links = new Map();
document.querySelectorAll(".nav-link").forEach((link) => links.set(link.dataset.view, link));
const filter = document.getElementById("filter");
const sizeBtn = document.getElementById("toggle-size");
const dialog = document.querySelector("dialog.lightbox");
const dialogImg = dialog.querySelector("img");
const modeButtons = Array.from(document.querySelectorAll(".mode-btn"));
const MODES = modeButtons.map((button) => button.dataset.mode);
const STORAGE_KEY = "styleguide-section";
let mode = "views";
let current = ids[0];

function modeOf(id) { return modes[id] || "views"; }

function setMode(next) {
  if (MODES.indexOf(next) < 0) return;
  mode = next;
  document.body.dataset.mode = next;
  modeButtons.forEach((button) => button.setAttribute("aria-pressed", String(button.dataset.mode === next)));
  try { localStorage.setItem(STORAGE_KEY, next); } catch (error) { /* private mode */ }
  applyFilter();
}

function visibleIds() {
  return ids.filter((id) => {
    if (modeOf(id) !== mode) return false;
    const link = links.get(id);
    return link && !link.classList.contains("hidden");
  });
}

function show(id) {
  const known = ids.indexOf(id) >= 0;
  if (known && modeOf(id) !== mode) setMode(modeOf(id));
  const target = known ? id : (visibleIds()[0] || ids[0]);
  current = target;
  document.querySelectorAll(".view").forEach((section) => {
    section.classList.toggle("active", section.dataset.view === target);
  });
  links.forEach((link, key) => link.classList.toggle("active", key === target));
  const active = links.get(target);
  if (active) { active.scrollIntoView({ block: "nearest" }); document.title = active.dataset.title + " — Exponential styleguide"; }
  window.scrollTo(0, 0);
}

function route() { show(decodeURIComponent(location.hash.replace(/^#/, ""))); }
addEventListener("hashchange", route);

function go(delta) {
  const order = visibleIds();
  if (order.length === 0) return;
  const at = order.indexOf(current);
  const next = order[Math.min(order.length - 1, Math.max(0, (at < 0 ? 0 : at) + delta))];
  if (next && next !== current) location.hash = "#" + next;
}

function applyFilter() {
  const query = filter.value.trim().toLowerCase();
  links.forEach((link, id) => {
    const hit = query === "" || link.dataset.search.indexOf(query) >= 0;
    link.classList.toggle("hidden", !(hit && modeOf(id) === mode));
  });
  document.querySelectorAll(".group-section").forEach((section) => {
    section.classList.toggle("hidden", section.querySelectorAll(".nav-link:not(.hidden)").length === 0);
  });
  document.querySelector(".nav-empty").classList.toggle("hidden", visibleIds().length > 0);
}

filter.addEventListener("input", applyFilter);

modeButtons.forEach((button) => button.addEventListener("click", () => {
  setMode(button.dataset.mode);
  const first = visibleIds()[0];
  if (first) location.hash = "#" + first;
}));

sizeBtn.addEventListener("click", () => {
  const actual = document.body.classList.toggle("actual");
  sizeBtn.setAttribute("aria-pressed", String(actual));
  sizeBtn.textContent = actual ? "Actual size" : "Fit to height";
});

document.addEventListener("click", (event) => {
  const img = event.target.closest && event.target.closest("figure.shot img");
  if (!img) return;
  dialogImg.src = img.currentSrc || img.src;
  dialogImg.style.width = (img.naturalWidth || img.width) + "px";
  dialog.showModal();
});
dialog.addEventListener("click", () => dialog.close());

addEventListener("keydown", (event) => {
  if (event.metaKey || event.ctrlKey || event.altKey) return;
  if (event.target === filter) {
    if (event.key === "Escape") { filter.value = ""; applyFilter(); filter.blur(); }
    if (event.key === "Enter") { const first = visibleIds()[0]; if (first) location.hash = "#" + first; }
    return;
  }
  if (dialog.open) return;
  if (event.key === "/") { event.preventDefault(); filter.focus(); filter.select(); return; }
  const digit = MODES[["1", "2", "3", "4"].indexOf(event.key)];
  if (digit) {
    event.preventDefault();
    setMode(digit);
    const first = visibleIds()[0];
    if (first) location.hash = "#" + first;
    return;
  }
  if (event.key === "j" || event.key === "ArrowDown") { event.preventDefault(); go(1); }
  if (event.key === "k" || event.key === "ArrowUp") { event.preventDefault(); go(-1); }
});

let stored = null;
try { stored = localStorage.getItem(STORAGE_KEY); } catch (error) { /* private mode */ }
setMode(location.hash ? modeOf(decodeURIComponent(location.hash.replace(/^#/, ""))) : (stored || "views"));
route();
`
