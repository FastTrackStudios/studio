// Real content from the demo vault (Merry & Bright, JT's Senior Recital, Crescendum).
const FIFTHS = ["C","G","D","A","E","B","F#","Db","Ab","Eb","Bb","F"];
const ALIAS = { Gb: "F#", "C#": "Db", "G#": "Ab", "D#": "Eb", "A#": "Bb" };
const hue = k => { if (!k) return null; const i = FIFTHS.indexOf(ALIAS[k] || k); return i < 0 ? null : (i * 30 + 20) % 360; };
const S = (title, artist, key, tempo, len, ready) => ({ title, artist, key, tempo, len, ready });
const SETS = [
  { id: "jt", title: "JT's Senior Recital", date: "Recital, May 2026", acts: [
    ["The Originals", [
      S("DTLA Cinematic Intro","Nights Out","C",160,123,"cltm"), S("Drive To LA","Nights Out","C→Db",160,197,"cltm"),
      S("For You","Nights Out","Ab",105,193,"cltm"), S("Heart Isn't Ready","Nights Out","Bb",107,203,"cltm"),
      S("No One Knows","Queens of the Stone Age",null,null,257,"lt"), S("Hey!","Nights Out & Ryce","E",87,251,"cltm")]],
    ["Royal Blood", [
      S("Sleeptalker","Royal Blood",null,null,191,"lt"), S("Figure It Out","Royal Blood",null,null,189,"lt"),
      S("Seven Nation Army","The White Stripes",null,null,231,"lt"), S("Sad But True","Metallica",null,null,327,"lt"),
      S("Out Of The Black","Royal Blood",null,null,327,"lt")]],
    ["The Fun Part", [
      S("She's American","The 1975","Db",116,267,"cltm"), S("WORST PART OF YOU","bilmuri","B",183,176,"cltm"),
      S("Fire","JT Wright","D",120,206,"cltm"), S("Not Too Much Longer (Live)","Cody Wright",null,null,261,"lt"),
      S("Know Me (Extended)","The Band CAMINO",null,null,304,"clt")]]]},
  { id: "mb", title: "Merry & Bright", date: "CBU Christmas 2026", acts: [
    ["Presidents Songs", [
      S("All I Want for Christmas Is You","Mariah Carey","G",150,null,"cl"), S("Feliz Navidad",null,null,null,null,""),
      S("Joy to the World (Unspeakable Joy)","Chris Tomlin","D",128,null,"cl")]],
    ["Tree Lighting Concert Set", [
      S("Last Christmas",null,null,null,null,""), S("Rockin' Around the Christmas Tree",null,null,null,null,""),
      S("Silent Night","for KING & COUNTRY","A",83,null,"cl"), S("Sleigh Ride","Tori Kelly","B",86,null,"cl"),
      S("This Christmas","Donny Hathaway","F",93,null,"cl"), S("What Christmas Means",null,null,null,null,"")]]]},
  { id: "cr", title: "Crescendum", date: "Band set", acts: [
    [null, ["Intro","Set in Stone","Unbreakable","Chained Expectations","Heavify","It Knows My Name","Kornesque","The Ballad","The Plague","Empty Focus"]
      .map(t => S(t,"Crescendum",null,null,null,"c"))]]},
];
const RIGS = [
  { name: "Guitar", what: "Amp captures, cabs and pedals", preset: "Recital, Drive To LA", scenes: 4, input: "Input 1, Hi-Z" },
  { name: "Keys", what: "Piano, EPs, organ and pads", preset: "Recital keys", scenes: 3, input: "Kontrol S88" },
  { name: "Bass", what: "Bass amp and DI blend", preset: "Royal Blood fuzz", input: "Input 2" },
  { name: "Drums", what: "Sampled kit, every mic", preset: "Recital kit", input: "From the session" },
  { name: "E-kit", what: "Electronic kit triggers", preset: "Not set up", input: "No pads found" },
  { name: "Synth", what: "Synth rig and arps", preset: "Christmas pads", input: "MIDI" },
  { name: "Orchestra", what: "Strings, brass and winds", preset: "Silent Night strings", input: "MIDI" },
];
const RECENT = [
  { kind: "Set", title: "Merry & Bright", sub: "CBU Christmas 2026, 9 songs", when: "Mon", set: "mb" },
  { kind: "Song", title: "Drive To LA", sub: "Chart edited", when: "Today", key: "C→Db" },
  { kind: "Project", title: "Sr. Recital Showcase", sub: "Ableton set, 16 songs split", when: "Yesterday" },
  { kind: "Song", title: "This Christmas", sub: "Tempo map checked", when: "Mon", key: "F" },
  { kind: "Song", title: "Silent Night", sub: "Stems prepared", when: "Last week", key: "A" },
];

const esc = s => String(s ?? "").replace(/[&<>"]/g, c => ({ "&": "&amp;", "<": "&lt;", ">": "&gt;", '"': "&quot;" }[c]));
const dur = s => { if (s == null) return ""; s = Math.round(s); const h = Math.floor(s / 3600), m = Math.floor(s % 3600 / 60), x = String(s % 60).padStart(2, "0"); return h ? `${h}:${String(m).padStart(2, "0")}:${x}` : `${m}:${x}`; };
const songs = set => set.acts.flatMap(a => a[1]);
const total = set => songs(set).reduce((t, s) => t + (s.len || 0), 0);
function tile(key) {
  if (!key) return `<span class="tile none" aria-label="No key yet">?</span>`;
  if (key.includes("→")) { const [a, b] = key.split("→"); return `<span class="tile mod" style="--h:${hue(a)};--h2:${hue(b)}" aria-label="${esc(a)} to ${esc(b)}">${esc(key)}</span>`; }
  return `<span class="tile" style="--h:${hue(key)}">${esc(key)}</span>`;
}
function strip(set, here = -1) {
  let i = 0;
  return `<div class="strip" aria-label="${esc(set.title)}, ${songs(set).length} songs">` + set.acts.map((a, ai) =>
    (ai ? '<span class="gap"></span>' : "") + a[1].map(s => {
      const k = s.key, idx = i++, h = k ? hue(k.split("→")[0]) : null;
      return `<span class="tile ${k ? "" : "none"} ${idx === here ? "here" : ""}" ${h != null ? `style="--h:${h}"` : ""} title="${esc(s.title)}">${esc(k ? k.split("→")[0] : String(idx + 1))}</span>`;
    }).join("")).join("") + `</div>`;
}
const bar = (avatar = true) => `<div class="top"><span></span>${avatar ? '<span class="avatar" aria-label="Signed in as Ada">A</span>' : ""}</div>`;
let toastT;
function toast(msg) { const t = document.querySelector(".toast"); t.textContent = msg; t.classList.add("show"); clearTimeout(toastT); toastT = setTimeout(() => t.classList.remove("show"), 1600); }

function renderHome() {
  const jt = SETS[0];
  document.getElementById("home").innerHTML = `${bar()}
    <h1>Home</h1>
    <div class="resume">
      <p class="resume-title">${esc(jt.title)}</p>
      <div class="resume-where">Stopped in <b>Know Me (Extended)</b> at 2:41</div>
      ${strip(jt, 15)}
      <div class="buttons">
        <button class="primary" data-act="Resumed JT's Senior Recital">Resume</button>
        <button class="secondary" data-open="jt">Set list</button>
      </div>
    </div>
    <h2>Recent</h2>
    <div class="group">${RECENT.map(r => `
      <button class="row" ${r.set ? `data-open="${r.set}"` : `data-act="Opened ${esc(r.title)}"`}>
        ${r.key ? tile(r.key) : `<span class="tile none">${r.kind === "Set" ? "♫" : "▤"}</span>`}
        <span><span class="t">${esc(r.title)}</span><span class="s">${esc(r.kind)}, ${esc(r.sub)}</span></span>
        <span class="m">${esc(r.when)}</span>
      </button>`).join("")}</div>`;
}
function renderSession() {
  document.getElementById("session").innerHTML = `${bar()}
    <h1>Session</h1>
    <div class="group">${SETS.map(s => `
      <button class="row" data-open="${s.id}">
        <span class="tile none">${songs(s).length}</span>
        <span><span class="t">${esc(s.title)}</span><span class="s">${esc(s.date)}${total(s) ? `, ${dur(total(s))}` : ""}</span></span>
        <span class="chev">›</span>
      </button>`).join("")}</div>
    <p class="device">Plays through Scarlett 18i20. Lights on Room 138.</p>`;
}
let openSet = null;
function renderSet(id) {
  const set = SETS.find(s => s.id === id); let n = 0;
  document.getElementById("set").innerHTML = `
    <div class="top"><button class="back" data-back>‹ Session</button><span></span></div>
    <h1>${esc(set.title)}</h1>
    <p class="meta">${songs(set).length} songs${total(set) ? `, ${dur(total(set))}` : ""}</p>
    ${strip(set)}
    ${set.acts.map(([name, list]) => `
      ${name ? `<div class="act">${esc(name)}</div>` : '<div style="height:14px"></div>'}
      <div class="group">${list.map(s => `
        <button class="row song" data-act="Rehearsing ${esc(s.title)}">
          <span class="n">${++n}</span>${tile(s.key)}
          <span><span class="t">${esc(s.title)}</span><span class="s">${esc([s.artist, s.tempo ? `${s.tempo} bpm` : null].filter(Boolean).join(", ") || "Not prepared yet")}</span></span>
          <span class="m">${dur(s.len)}</span>
        </button>`).join("")}</div>`).join("")}`;
  document.getElementById("dock").innerHTML = `
    <button class="secondary" data-act="Choose a song to rehearse">Rehearse</button>
    <button class="primary" data-act="Playing ${esc(set.title)}">Play the set</button>`;
}
function renderSignal() {
  const card = r => `
    <button class="rig" data-act="Opened the ${esc(r.name)} rig">
      <span class="name">${esc(r.name)}</span><span class="what">${esc(r.what)}</span>
      <span class="state">${esc(r.preset)}<br>${esc(r.input)}</span>
    </button>`;
  document.getElementById("signal").innerHTML = `${bar()}
    <h1>Signal</h1>
    <div class="rigs-main">${RIGS.slice(0, 2).map(card).join("")}</div>
    <h2>More rigs</h2>
    <div class="group">${RIGS.slice(2).map(r => `
      <button class="row plain" data-act="Opened the ${esc(r.name)} rig">
        <span><span class="t">${esc(r.name)}</span><span class="s">${esc(r.preset)}</span></span><span class="chev">›</span>
      </button>`).join("")}</div>`;
}
let q = "";
function renderLibrary() {
  const all = SETS.flatMap(s => songs(s).map(x => ({ ...x, from: s.title })));
  const seen = new Set();
  const list = all.filter(s => !seen.has(s.title) && seen.add(s.title)).filter(s => !q || (s.title + " " + (s.artist || "")).toLowerCase().includes(q));
  const el = document.getElementById("library");
  el.innerHTML = `${bar()}
    <h1>Library</h1>
    <label class="search"><svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><circle cx="11" cy="11" r="7"/><path d="m20 20-4-4"/></svg><input type="search" placeholder="Songs and artists" value="${esc(q)}" aria-label="Find a song"></label>
    <div class="legend"><span><i></i>Chart</span><span><i></i>Lyrics</span><span><i></i>Stems</span><span><i></i>Tempo map</span></div>
    <div class="group">${list.map(s => `
      <button class="row" data-act="Opened ${esc(s.title)}">
        ${tile(s.key)}
        <span><span class="t">${esc(s.title)}</span><span class="s">${esc(s.artist || "No artist yet")}</span></span>
        <span class="m">${dur(s.len)}<span class="dots" aria-label="${["Chart","Lyrics","Stems","Tempo map"].filter((_, i) => s.ready.includes("cltm"[i])).join(", ") || "Nothing ready"}">${"cltm".split("").map(k => `<i class="${s.ready.includes(k) ? "y" : ""}"></i>`).join("")}</span></span>
      </button>`).join("") || `<div class="row plain"><span class="s">No song matches “${esc(q)}”.</span></div>`}</div>`;
  const input = el.querySelector("input");
  input.addEventListener("input", e => { q = e.target.value.toLowerCase(); const p = e.target.selectionStart; renderLibrary(); const i = el.querySelector("input"); i.focus(); i.setSelectionRange(p, p); });
}

let tab = "home";
function showTab(t) {
  tab = t; openSet = null;
  document.querySelectorAll(".tab").forEach(b => b.setAttribute("aria-current", b.dataset.tab === t ? "page" : "false"));
  document.querySelectorAll(".screen").forEach(s => { s.hidden = s.id !== t; s.classList.remove("under", "enter"); });
  document.getElementById("dock").hidden = true;
  history.replaceState(null, "", "#/" + t);
}
function push(id) {
  openSet = id; renderSet(id);
  if (tab !== "session") showTab("session");
  const under = document.getElementById("session"), over = document.getElementById("set");
  over.hidden = false; over.scrollTop = 0; over.classList.add("enter");
  requestAnimationFrame(() => requestAnimationFrame(() => { over.classList.remove("enter"); under.classList.add("under"); }));
  setTimeout(() => { under.hidden = true; }, 300);
  document.getElementById("dock").hidden = false;
  history.replaceState(null, "", "#/session/" + id);
}
function pop() {
  const under = document.getElementById("session"), over = document.getElementById("set");
  under.hidden = false; under.classList.add("under");
  requestAnimationFrame(() => { under.classList.remove("under"); over.classList.add("enter"); });
  document.getElementById("dock").hidden = true;
  setTimeout(() => { over.hidden = true; over.classList.remove("enter"); }, 280);
  openSet = null; history.replaceState(null, "", "#/session");
}
document.addEventListener("click", e => {
  const t = e.target.closest("[data-tab]"); if (t) return showTab(t.dataset.tab);
  const o = e.target.closest("[data-open]"); if (o) return push(o.dataset.open);
  if (e.target.closest("[data-back]")) return pop();
  const a = e.target.closest("[data-act]"); if (a) toast(a.dataset.act);
});
renderHome(); renderSession(); renderSignal(); renderLibrary();
const [, first, second] = (location.hash || "#/home").split("/");
showTab(["home", "session", "signal", "library"].includes(first) ? first : "home");
if (first === "session" && second) push(second);
