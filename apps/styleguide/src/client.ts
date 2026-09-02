/**
 * The browser half of the gallery, inlined into the page. Plain DOM, no build
 * step, no framework: every view section is already in the document, so this
 * only routes, filters, and zooms.
 *
 * Component sections route exactly like view sections — same `.view` class,
 * same nav link, same hash. They just carry no shots.
 */

export const client = `
const raw = document.getElementById("gallery-data").textContent;
const data = JSON.parse(raw);
const ids = data.views.map((entry) => entry.view.id).concat((data.components || []).map((c) => c.id));
const links = new Map();
document.querySelectorAll(".nav-link").forEach((link) => links.set(link.dataset.view, link));
const filter = document.getElementById("filter");
const sizeBtn = document.getElementById("toggle-size");
const dialog = document.querySelector("dialog.lightbox");
const dialogImg = dialog.querySelector("img");
let current = ids[0];

function visibleIds() {
  return ids.filter((id) => { const link = links.get(id); return link && !link.classList.contains("hidden"); });
}

function show(id) {
  const target = ids.indexOf(id) >= 0 ? id : (visibleIds()[0] || ids[0]);
  current = target;
  document.querySelectorAll(".view").forEach((section) => {
    section.classList.toggle("active", section.dataset.view === target);
  });
  links.forEach((link, key) => link.classList.toggle("active", key === target));
  const active = links.get(target);
  if (active) { active.scrollIntoView({ block: "nearest" }); document.title = active.dataset.title + " — Exponential views"; }
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

filter.addEventListener("input", () => {
  const query = filter.value.trim().toLowerCase();
  links.forEach((link) => {
    link.classList.toggle("hidden", query !== "" && link.dataset.search.indexOf(query) < 0);
  });
  document.querySelectorAll(".group-section").forEach((section) => {
    section.classList.toggle("hidden", section.querySelectorAll(".nav-link:not(.hidden)").length === 0);
  });
  document.querySelector(".nav-empty").classList.toggle("hidden", visibleIds().length > 0);
});

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
    if (event.key === "Escape") { filter.value = ""; filter.dispatchEvent(new Event("input")); filter.blur(); }
    if (event.key === "Enter") { const first = visibleIds()[0]; if (first) location.hash = "#" + first; }
    return;
  }
  if (dialog.open) return;
  if (event.key === "/") { event.preventDefault(); filter.focus(); filter.select(); return; }
  if (event.key === "j" || event.key === "ArrowDown") { event.preventDefault(); go(1); }
  if (event.key === "k" || event.key === "ArrowUp") { event.preventDefault(); go(-1); }
});

route();
`
