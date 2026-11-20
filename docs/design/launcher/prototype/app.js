// Studio's launcher on Architect's phone shell. The sets, songs, keys,
// tempos and lengths are the demo vault's (JT's Senior Recital, Merry &
// Bright, Crescendum); times, places, the engine and the rigs' presets are
// placeholders.

// ── Icons (lucide) ──────────────────────────────────────────────────────────
const P = {
  panelLeft: '<rect width="18" height="18" x="3" y="3" rx="2"/><path d="M9 3v18"/>',
  arrowLeft: '<path d="m12 19-7-7 7-7"/><path d="M19 12H5"/>',
  search: '<circle cx="11" cy="11" r="8"/><path d="m21 21-4.3-4.3"/>',
  undo: '<path d="M9 14 4 9l5-5"/><path d="M4 9h10.5a5.5 5.5 0 0 1 0 11H11"/>',
  command: '<path d="M15 6v12a3 3 0 1 0 3-3H6a3 3 0 1 0 3 3V6a3 3 0 1 0-3 3h12a3 3 0 1 0-3-3"/>',
  settings: '<path d="M20 7h-9"/><path d="M14 17H5"/><circle cx="17" cy="17" r="3"/><circle cx="7" cy="7" r="3"/>',
  help: '<circle cx="12" cy="12" r="10"/><path d="M9.09 9a3 3 0 0 1 5.83 1c0 2-3 3-3 3"/><path d="M12 17h.01"/>',
  plus: '<path d="M5 12h14"/><path d="M12 5v14"/>',
  more: '<circle cx="12" cy="12" r="1"/><circle cx="19" cy="12" r="1"/><circle cx="5" cy="12" r="1"/>',
  share: '<path d="M12 2v13"/><path d="m16 6-4-4-4 4"/><path d="M4 12v8a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2v-8"/>',
  chevLeft: '<path d="m15 18-6-6 6-6"/>',
  chevRight: '<path d="m9 18 6-6-6-6"/>',
  updown: '<path d="m7 15 5 5 5-5"/><path d="m7 9 5-5 5 5"/>',
  folder: '<path d="M20 20a2 2 0 0 0 2-2V8a2 2 0 0 0-2-2h-7.9a2 2 0 0 1-1.69-.9L9.6 3.9A2 2 0 0 0 7.93 3H4a2 2 0 0 0-2 2v13a2 2 0 0 0 2 2Z"/>',
  setlist: '<path d="M21 15V6"/><path d="M18.5 18a2.5 2.5 0 1 0 0-5 2.5 2.5 0 0 0 0 5Z"/><path d="M12 12H3"/><path d="M16 6H3"/><path d="M12 18H3"/>',
  home: '<path d="M15 21v-8a1 1 0 0 0-1-1h-4a1 1 0 0 0-1 1v8"/><path d="M3 10a2 2 0 0 1 .709-1.528l7-5.999a2 2 0 0 1 2.582 0l7 5.999A2 2 0 0 1 21 10v9a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2z"/>',
  audio: '<path d="M2 10v3"/><path d="M6 6v11"/><path d="M10 3v18"/><path d="M14 8v7"/><path d="M18 5v13"/><path d="M22 10v3"/>',
  disc: '<circle cx="12" cy="12" r="10"/><circle cx="12" cy="12" r="2"/>',
  guitar: '<circle cx="8" cy="16" r="5"/><circle cx="8" cy="16" r="1.3"/><path d="M11.6 12.4 19.5 4.5"/><path d="m18 2.5 3.5 3.5"/>',
  bass: '<circle cx="7.5" cy="16.5" r="4.5"/><path d="M10.7 13.3 21 3"/><path d="m19.5 1.5 3 3"/><path d="M5.5 16.5h4"/>',
  keys: '<rect x="2" y="5" width="20" height="14" rx="2"/><path d="M8 13v6M14 13v6"/><path d="M6.5 5v8h3V5M12.5 5v8h3V5M18 5v8h2.5"/>',
  drums: '<ellipse cx="12" cy="9" rx="9" ry="3"/><path d="M3 9v7c0 1.7 4 3 9 3s9-1.3 9-3V9"/><path d="m17 2-4 6M7 2l4 6"/>',
  pads: '<rect x="3" y="3" width="8" height="8" rx="2"/><rect x="13" y="3" width="8" height="8" rx="2"/><rect x="3" y="13" width="8" height="8" rx="2"/><rect x="13" y="13" width="8" height="8" rx="2"/>',
  wave: '<path d="M2 12c1.5-5 3-5 4.5 0s3 5 4.5 0 3-5 4.5 0 3 5 4.5 0"/><path d="M2 20h20"/>',
  strings: '<path d="M9 18V5l12-2v13"/><circle cx="6" cy="18" r="3"/><circle cx="18" cy="16" r="3"/>',
  tuner: '<path d="M12 21a9 9 0 1 1 9-9"/><path d="m12 12 4-6"/><circle cx="12" cy="12" r="1.5"/>',
  rig: '<path d="M22 12h-4l-3 9L9 3l-3 9H2"/>',
  library: '<path d="m16 6 4 14"/><path d="M12 6v14"/><path d="M8 8v12"/><path d="M4 4v16"/>',
  play: '<path d="M6 3l14 9-14 9z" fill="currentColor"/>',
  mic: '<path d="M12 2a3 3 0 0 0-3 3v7a3 3 0 0 0 6 0V5a3 3 0 0 0-3-3Z"/><path d="M19 10v2a7 7 0 0 1-14 0v-2"/><path d="M12 19v3"/>',
  drive: '<path d="M22 12H2"/><path d="M5.45 5.11 2 12v6a2 2 0 0 0 2 2h16a2 2 0 0 0 2-2v-6l-3.45-6.89A2 2 0 0 0 16.76 4H7.24a2 2 0 0 0-1.79 1.11z"/>',
  moon: '<path d="M12 3a6 6 0 0 0 9 9 9 9 0 1 1-9-9Z"/>',
  scan: '<path d="M3 12a9 9 0 0 1 9-9 9.75 9.75 0 0 1 6.74 2.74L21 8"/><path d="M21 3v5h-5"/><path d="M21 12a9 9 0 0 1-9 9 9.75 9.75 0 0 1-6.74-2.74L3 16"/><path d="M8 16H3v5"/>',
  check: '<path d="M20 6 9 17l-5-5"/>',
  open: '<path d="m6 14 1.5-2.9A2 2 0 0 1 9.24 10H20a2 2 0 0 1 1.94 2.5l-1.54 6a2 2 0 0 1-1.95 1.5H4a2 2 0 0 1-2-2V5a2 2 0 0 1 2-2h3.9a2 2 0 0 1 1.69.9l.81 1.2a2 2 0 0 0 1.67.9H18a2 2 0 0 1 2 2v2"/>',
  import: '<path d="M12 3v12"/><path d="m8 11 4 4 4-4"/><path d="M8 5H4a2 2 0 0 0-2 2v10a2 2 0 0 0 2 2h16a2 2 0 0 0 2-2V7a2 2 0 0 0-2-2h-4"/>',
  pin: '<path d="M12 17v5"/><path d="M9 10.76a2 2 0 0 1-1.11 1.79l-1.78.9A2 2 0 0 0 5 15.24V16a1 1 0 0 0 1 1h12a1 1 0 0 0 1-1v-.76a2 2 0 0 0-1.11-1.79l-1.78-.9A2 2 0 0 1 15 10.76V7a1 1 0 0 1 1-1 2 2 0 0 0 0-4H8a2 2 0 0 0 0 4 1 1 0 0 1 1 1z"/>',
  trash: '<path d="M3 6h18"/><path d="M19 6v14a2 2 0 0 1-2 2H7a2 2 0 0 1-2-2V6"/><path d="M8 6V4a2 2 0 0 1 2-2h4a2 2 0 0 1 2 2v2"/>',
  sliders: '<path d="M4 21v-7"/><path d="M4 10V3"/><path d="M12 21v-9"/><path d="M12 8V3"/><path d="M20 21v-5"/><path d="M20 12V3"/><path d="M2 14h4"/><path d="M10 8h4"/><path d="M18 16h4"/>',
};
const ic = (name, size = 20, w = 1.8) => `<svg width="${size}" height="${size}" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="${w}" stroke-linecap="round" stroke-linejoin="round">${P[name]}</svg>`;

// ── Content ─────────────────────────────────────────────────────────────────
const FIFTHS = ["C","G","D","A","E","B","F#","Db","Ab","Eb","Bb","F"];
const ALIAS = { Gb: "F#", "C#": "Db", "G#": "Ab", "D#": "Eb", "A#": "Bb" };
const hue = k => { if (!k) return null; const i = FIFTHS.indexOf(ALIAS[k] || k); return i < 0 ? null : (i * 30 + 20) % 360; };
const S = (title, artist, key, tempo, len, ready) => ({ title, artist, key, tempo, len, ready });
const SETS = {
  jt: { title: "JT's Senior Recital", date: "Recital, May 2026", acts: [
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
  mb: { title: "Merry & Bright", date: "CBU Christmas 2026", acts: [
    ["Presidents Songs", [
      S("All I Want for Christmas Is You","Mariah Carey","G",150,null,"cl"), S("Feliz Navidad",null,null,null,null,""),
      S("Joy to the World (Unspeakable Joy)","Chris Tomlin","D",128,null,"cl")]],
    ["Tree Lighting Concert Set", [
      S("Last Christmas",null,null,null,null,""), S("Rockin' Around the Christmas Tree",null,null,null,null,""),
      S("Silent Night","for KING & COUNTRY","A",83,null,"cl"), S("Sleigh Ride","Tori Kelly","B",86,null,"cl"),
      S("This Christmas","Donny Hathaway","F",93,null,"cl"), S("What Christmas Means",null,null,null,null,"")]]]}
};
// A song's sections, for its ribbon (placeholder shapes).
const SECTIONS = { dtla: "i v c v c b c o", xmas: "i v v c v c b c c o", silent: "i v v c v c o", fire: "i v c v c b c c o" };
const PROJECTS = [
  { id: "jt", kind: "Set", fmt: "Studio", set: "jt", group: "Today", when: "4:12 PM", where: "THEBATTLESHIP", stopped: [15, "Know Me (Extended)", "2:41"] },
  { id: "dtla", kind: "Song", fmt: "Studio", title: "Drive To LA", artist: "Nights Out", key: "C→Db", tempo: 160, len: 197, group: "Today", when: "2:05 PM", note: "Chart edited" },
  { id: "show", kind: "Production", fmt: "Ableton Live", short: "Live", title: "Sr. Recital Showcase", set: "jt", group: "Yesterday", when: "Yesterday", note: "16 songs split into a setlist" },
  { id: "mb", kind: "Set", fmt: "Studio", set: "mb", group: "This week", when: "Mon" },
  { id: "xmas", kind: "Song", fmt: "Studio", title: "This Christmas", artist: "Donny Hathaway", key: "F", tempo: 93, group: "This week", when: "Mon", note: "Tempo map checked" },
  { id: "silent", kind: "Song", fmt: "Studio", title: "Silent Night", artist: "for KING & COUNTRY", key: "A", tempo: 83, group: "This week", when: "Sun", note: "Stems prepared" },
  { id: "fire", kind: "Production", fmt: "Pro Tools", short: "PT", title: "Fire, tracking", artist: "JT Wright", key: "D", tempo: 120, group: "Earlier", when: "Nov 4", note: "Pro Tools session, 38 tracks" },
  { id: "cr", kind: "Album", album: "cr", group: "Earlier", when: "Nov 2" },
];
// Records: what is made in the studio. The track list is the vault's; the
// stages are placeholders.
const STAGES = [["Demo", null], ["Tracking", 235], ["Editing", 295], ["Mixing", 65], ["Mastered", 150]];
const ALBUMS = { cr: { title: "Crescendum", artist: "Crescendum", what: "Studio album", tracks: [
  ["Intro", 4], ["Set in Stone", 4], ["Unbreakable", 3], ["Chained Expectations", 3], ["Heavify", 2],
  ["It Knows My Name", 2], ["Kornesque", 1], ["The Ballad", 1], ["The Plague", 1], ["Empty Focus", 0]].map(([title, stage]) => ({ title, stage })) } };
const PINNED = ["jt", "mb"];
// The rigs. Presets and inputs are placeholders; one rig is loaded in the
// engine at a time.
const RIGS = [
  { name: "Guitar", icon: "guitar", hue: 30, preset: "CYA, Thu Nov 19", input: "Input 1 · Hi-Z", loaded: true },
  { name: "Keys", icon: "keys", hue: 255, preset: "JHM Sunday", input: "Kontrol S88" },
  { name: "Bass", icon: "bass", hue: 305, preset: "Royal Blood fuzz", input: "Input 2" },
  { name: "Drums", icon: "drums", hue: 70, preset: "Recital kit", input: "From the session" },
  { name: "E-kit", icon: "pads", hue: 195, preset: null, input: "No pads found" },
  { name: "Synth", icon: "wave", hue: 330, preset: "Christmas pads", input: "MIDI" },
  { name: "Orchestra", icon: "strings", hue: 145, preset: "Silent Night strings", input: "MIDI" },
];
const rigOf = name => RIGS.find(r => r.name === name);
// What Cody plays at each set (placeholders).
const MINE = { "cya@2026-11-19": "Guitar", "jhms@2026-11-22": "Keys", "main@2026-11-22": "Guitar", "hsm@2026-11-24": "Guitar", "main@2026-11-25": "Keys" };

const esc = s => String(s ?? "").replace(/[&<>"]/g, c => ({ "&": "&amp;", "<": "&lt;", ">": "&gt;", '"': "&quot;" }[c]));
const dur = s => { if (s == null) return ""; s = Math.round(s); const h = Math.floor(s / 3600), m = Math.floor(s % 3600 / 60), x = String(s % 60).padStart(2, "0"); return h ? `${h}:${String(m).padStart(2, "0")}:${x}` : `${m}:${x}`; };
const songs = set => set.acts.flatMap(a => a[1]);
const total = set => songs(set).reduce((t, s) => t + (s.len || 0), 0);
const titleOf = p => p.title || (p.set ? SETS[p.set] : ALBUMS[p.album]).title;

// ── Pieces ──────────────────────────────────────────────────────────────────
function keyTile(key, cls = "") {
  if (!key) return `<span class="tile nokey ${cls}">?</span>`;
  if (key.includes("→")) { const [a, b] = key.split("→"); return `<span class="tile key mod ${cls}" style="--h:${hue(a)};--h2:${hue(b)}" aria-label="${esc(a)} to ${esc(b)}">${esc(key)}</span>`; }
  return `<span class="tile key ${cls}" style="--h:${hue(key)}">${esc(key)}</span>`;
}
function projTile(p) {
  if (p.kind === "Album") { const a = ALBUMS[p.album]; return `${a.what} · ${a.tracks.length} tracks · ${stageSummary(a)}`; }
  if (p.kind === "Production") return `<span class="tile fmt" aria-label="${esc(p.fmt)}">${esc(p.short)}</span>`;
  if (p.kind === "Song") return keyTile(p.key);
  if (p.kind === "Album") return `<span class="tile disc" aria-label="Album">${ic("disc", 21)}</span>`;
  return `<span class="tile" aria-label="Set">${ic("setlist", 20)}</span>`;
}
// The set's songs side by side, each as long as it lasts (a song with no
// length yet takes an average one), acts apart.
function setRibbon(set, here = -1) {
  const all = songs(set), known = all.filter(s => s.len), avg = known.length ? total(set) / known.length : 1;
  let i = 0;
  return `<div class="ribbon" aria-hidden="true">${set.acts.map((a, ai) => (ai ? '<span class="gap"></span>' : "") + a[1].map(s => {
    const h = s.key ? hue(s.key.split("→")[0]) : null, idx = i++;
    return `<i class="${h == null ? "nokey" : ""} ${idx === here ? "here" : ""}" style="flex:${(s.len || avg).toFixed(0)} 1 0;${h != null ? `--h:${h}` : ""}"></i>`;
  }).join("")).join("")}</div>`;
}
function songRibbon(p) {
  const h = hue((p.key || "C").split("→")[0]), w = { i: 2, v: 4, c: 4, b: 3, o: 2 };
  return `<div class="ribbon" aria-hidden="true">${(SECTIONS[p.id] || "i v c v c o").split(" ").map(s => `<i class="sec ${s === "c" ? "ch" : ""}" style="flex:${w[s]} 1 0;--h:${h}"></i>`).join("")}</div>`;
}
// A record's tracks side by side, coloured by how far each has come.
const stageRibbon = a => `<div class="ribbon" aria-hidden="true">${a.tracks.map(t => `<i class="${STAGES[t.stage][1] == null ? "nokey" : ""}" style="flex:1 1 0;--h:${STAGES[t.stage][1]}"></i>`).join("")}</div>`;
const stageSummary = a => STAGES.map(([n], k) => [n, a.tracks.filter(t => t.stage === k).length]).filter(([, c]) => c).reverse().map(([n, c]) => `${c} ${n.toLowerCase()}`).join(" · ");
const ribbon = (p, here) => p.set ? setRibbon(SETS[p.set], here) : p.album ? stageRibbon(ALBUMS[p.album]) : songRibbon(p);
function sub(p) {
  if (p.kind === "Set") { const s = SETS[p.set]; return [`${songs(s).length} songs`, total(s) ? dur(total(s)) : null, s.date].filter(Boolean).join(" · "); }
  if (p.kind === "Production") return [p.fmt, p.note].filter(Boolean).join(" · ");
  return [p.artist, p.tempo ? `${p.tempo} bpm` : null, p.note].filter(Boolean).join(" · ");
}

// The page's bar (page.rs TouchBar): the brand on a root page, or the way
// back and the title small; the toolbar on the right.
function bar({ back, title, tools = [] }) {
  const lead = back
    ? `<button class="back" data-back aria-label="Back">${ic("chevLeft", 26, 2)}</button><span class="small">${esc(title)}</span>`
    : `<span class="title">${esc(title)}</span>`;
  return `<div class="bar"><div class="lead">${lead}</div><div class="tools">${tools.map(g => g.html ? g.html : g.joined
    ? `<div class="joined">${g.joined.map((t, i) => (i ? '<span class="hair"></span>' : "") + toolBtn(t)).join("")}</div>`
    : toolBtn(g)).join("")}</div></div>`;
}
const toolBtn = t => `<button class="tool" aria-label="${esc(t.label)}" ${t.open ? `data-open="${t.open}"` : t.menu ? `data-menu="${t.menu}"` : `data-act="${esc(t.act || t.label)}"`}>${ic(t.icon, 20)}${t.badge ? `<span class="count">${t.badge}</span>` : ""}</button>`;

let toastT;
function toast(msg) { const t = document.querySelector(".toast"); t.textContent = msg; t.classList.add("show"); clearTimeout(toastT); toastT = setTimeout(() => t.classList.remove("show"), 1600); }

// ── Pages ───────────────────────────────────────────────────────────────────
let scope = "All";
const SCOPES = ["All", "Setlists", "Sessions", "Songs"];
const scopeOf = p => p.kind === "Set" ? "Setlists" : p.kind === "Song" ? "Songs" : "Sessions";
const inScope = p => scope === "All" || scopeOf(p) === scope;
function pageProjects() {
  const [first, ...rest] = PROJECTS.filter(inScope);
  const groups = [...new Set(rest.map(p => p.group))];
  const hero = first ? `
    <button class="hero" data-open="${first.id}">
      <span class="kicker"><span class="dot"></span>Last open · ${esc(first.when)} on ${esc(first.where || "this iPhone")}</span>
      <div class="t">${esc(titleOf(first))}</div>
      <div class="s">${first.stopped ? `Stopped in <b>${esc(first.stopped[1])}</b> at ${esc(first.stopped[2])}` : esc(sub(first))}</div>
      ${ribbon(first, first.stopped ? first.stopped[0] : -1)}
      <span class="go">
        <span class="primary" data-act="Opened ${esc(titleOf(first))} in Session">Open</span>
        <span class="secondary" data-act="${first.set ? "Playing the set from " + esc(first.stopped?.[1] || "the top") : "Rehearsing " + esc(titleOf(first))}">${ic("play", 15)}${first.set ? "Play the set" : "Rehearse"}</span>
      </span>
    </button>` : "";
  return `${bar({ title: "Home", tools: [{ joined: [{ label: "New project", icon: "plus", act: "New project" }, { label: "More", icon: "more", menu: "projects" }] }] })}
    <div class="body">
            <div class="chips" role="group" aria-label="Show">${SCOPES.map(s => `<button class="chip" data-scope="${s}" aria-pressed="${s === scope}">${s}${s === "All" ? "" : ` <b>${PROJECTS.filter(p => scopeOf(p) === s).length}</b>`}</button>`).join("")}</div>
      ${hero}
      ${groups.map(g => `<h2>${esc(g)}</h2><div class="list">${rest.filter(p => p.group === g).map(row).join("")}</div>`).join("")}
    </div>`;
}
const row = p => `
  <button class="proj" data-open="${p.id}">
    ${projTile(p)}
    <span class="mid"><span class="t1">${esc(titleOf(p))}</span><span class="s1">${esc(sub(p))}</span></span>
    <span class="when">${esc(p.when)}</span>
    <span></span>${ribbon(p)}
  </button>`;

function pageProject(id) {
  const p = PROJECTS.find(x => x.id === id);
  const set = p.set ? SETS[p.set] : null; let n = 0;
  const tools = [{ joined: [{ label: "Share", icon: "share", act: "Share link copied" }, { label: "More", icon: "more", menu: "project" }] }];
  if (!set) return `${bar({ back: true, title: titleOf(p), tools })}
    <div class="body">
      <p class="meta">${esc(sub(p))}</p>
      <div class="big-ribbon">${songRibbon(p)}</div>
      <div class="acts"><span>Intro</span><span>Verse</span><span>Chorus</span><span>Bridge</span><span>Outro</span></div>
      <div class="go2"><button class="primary" data-act="Opened ${esc(p.title)} in Session">Open in Session</button><button class="secondary" data-act="Rehearsing ${esc(p.title)}">${ic("mic", 16)}Rehearse</button></div>
      <div class="facts">
        <div class="fact"><span class="k">Key</span><span>${esc(p.key)}</span></div>
        <div class="fact"><span class="k">Tempo</span><span>${p.tempo} bpm</span></div>
        <div class="fact"><span class="k">Format</span><span>${esc(p.fmt)}</span></div>
        <div class="fact"><span class="k">Edited</span><span>${esc(p.when)}</span></div>
      </div>
    </div>`;
  return `${bar({ back: true, title: titleOf(p), tools })}
    <div class="body">
      <p class="meta">${[p.kind === "Production" ? p.fmt : null, `${songs(set).length} songs`, total(set) ? dur(total(set)) : null, set.date].filter(Boolean).join(" · ")}</p>
      <div class="big-ribbon">${setRibbon(set, p.stopped ? p.stopped[0] : -1)}</div>
      ${set.acts[0][0] ? `<div class="acts">${set.acts.map(a => `<span style="flex:${a[1].length}">${esc(a[0])}</span>`).join("")}</div>` : '<div style="height:12px"></div>'}
      <div class="go2">
        <button class="primary" data-act="Playing ${esc(set.title)}">${ic("play", 15)}Play the set</button>
        <button class="secondary" data-act="Choose a song to rehearse">${ic("mic", 16)}Rehearse</button>
      </div>
      ${set.acts.map(([name, list]) => `
        ${name ? `<div class="act">${esc(name)}<span>${list.length} songs${list.some(s => s.len) ? " · " + dur(list.reduce((t, s) => t + (s.len || 0), 0)) : ""}</span></div>` : '<div style="height:14px"></div>'}
        <div class="list">${list.map(s => `
          <button class="song" data-act="Rehearsing ${esc(s.title)}">
            <span class="n">${++n}</span>${keyTile(s.key)}
            <span class="mid"><span class="t1">${esc(s.title)}</span><span class="s1">${esc([s.artist, s.tempo ? `${s.tempo} bpm` : null].filter(Boolean).join(" · ") || "Not prepared yet")}</span></span>
            <span class="len">${dur(s.len)}</span>
          </button>`).join("")}</div>`).join("")}
      <h2>Plays through</h2>
      <div class="facts">
        <div class="fact"><span class="k">Audio</span><span>Scarlett 18i20</span></div>
        <div class="fact"><span class="k">Rigs</span><span>Guitar, Keys</span></div>
        <div class="fact"><span class="k">Lights</span><span>Room 138</span></div>
      </div>
    </div>`;
}
// ── The weekly events: a series meets on its days, and each meeting's set
// has no name, only its date. The days are Cody's; the songs in the weekly
// sets are placeholders (a worship rotation), the shows are the vault's.
const TODAY = new Date(Date.UTC(2026, 10, 19)); // Thu Nov 19 2026
const DAY = ["Sunday", "Monday", "Tuesday", "Wednesday", "Thursday", "Friday", "Saturday"];
const MON = ["Jan","Feb","Mar","Apr","May","Jun","Jul","Aug","Sep","Oct","Nov","Dec"];
const SERIES = [
  { id: "jhms", name: "JHM Sunday", days: [0], hue: 285, size: 5 },
  { id: "main", name: "Main", days: [0, 3], hue: 45, size: 5, note: { 3: "Wednesday night" } },
  { id: "hsm", name: "HSM", days: [2], hue: 215, size: 4 },
  { id: "jhm", name: "JHM", days: [3], hue: 150, size: 4 },
  { id: "cya", name: "CYA", days: [4], hue: 345, size: 4 },
];
const POOL = [
  S("Goodness of God","Bethel Music","Ab",63,296), S("Gratitude","Brandon Lake","B",75,330), S("Build My Life","Housefires","G",68,290),
  S("Way Maker","Sinach","E",68,330), S("Firm Foundation (He Won't)","Cody Carnes","Bb",70,312), S("Holy Forever","Chris Tomlin","A",72,330),
  S("Praise","Elevation Worship","Bb",127,290), S("Trust in God","Elevation Worship","Ab",72,300), S("Jireh","Elevation Worship","C",64,330),
  S("Living Hope","Phil Wickham","Db",72,330), S("This Is Amazing Grace","Phil Wickham","A",103,280), S("See A Victory","Elevation Worship","B",72,330),
  S("King of Kings","Hillsong Worship","D",68,300), S("Graves Into Gardens","Elevation Worship","B",72,300), S("Same God","Elevation Worship","Db",72,330),
  S("Joy to the World (Unspeakable Joy)","Chris Tomlin","D",128,250),
];
const iso = d => d.toISOString().slice(0, 10);
const short = d => `${DAY[d.getUTCDay()].slice(0, 3)} ${MON[d.getUTCMonth()]} ${d.getUTCDate()}`;
const daysFrom = d => Math.round((d - TODAY) / 864e5);
const when = d => { const n = daysFrom(d); return n === 0 ? "Today" : n === 1 ? "Tomorrow" : n === -1 ? "Yesterday" : n > 0 && n < 7 ? DAY[d.getUTCDay()] : short(d); };
// How far along this week's sets are (the rest of the future is empty).
const PLANNED = { "cya@2026-11-19": 4, "jhms@2026-11-22": 4, "main@2026-11-22": 5, "hsm@2026-11-24": 2, "main@2026-11-25": 3, "jhm@2026-11-25": 0 };
const OCC = [];
SERIES.forEach((sr, si) => {
  for (let d = new Date(Date.UTC(2026, 9, 18)); d <= new Date(Date.UTC(2026, 11, 6)); d = new Date(d.getTime() + 864e5)) {
    if (!sr.days.includes(d.getUTCDay())) continue;
    const id = `${sr.id}@${iso(d)}`, n = OCC.filter(o => o.series === sr).length;
    const filled = daysFrom(d) < 0 ? sr.size : (PLANNED[id] ?? 0);
    const list = Array.from({ length: filled }, (_, k) => POOL[(n * 5 + k * 3 + si * 7 + (d.getUTCDay() === 3 ? 2 : 0)) % POOL.length]);
    OCC.push({ id, series: sr, date: new Date(d), songs: list });
  }
});
const seriesOf = id => SERIES.find(s => s.id === id);
const occOf = id => OCC.find(o => o.id === id);
const occsOf = sr => OCC.filter(o => o.series === sr);
const nextOf = sr => occsOf(sr).find(o => daysFrom(o.date) >= 0);
const everyDays = sr => "Every " + sr.days.map(d => sr.note?.[d] || DAY[d]).join(" and ");
const occName = o => o.series.note?.[o.date.getUTCDay()] ? `${o.series.name}, ${o.series.note[o.date.getUTCDay()]}` : o.series.name;
const ready = o => o.songs.length >= o.series.size ? "Ready" : o.songs.length ? `${o.songs.length} of ${o.series.size} songs` : "Not planned";
function occRibbon(o) {
  const slots = [...o.songs.map(s => `<i style="flex:${s.len} 1 0;--h:${hue(s.key)}"></i>`), ...Array.from({ length: Math.max(0, o.series.size - o.songs.length) }, () => '<i class="empty" style="flex:300 1 0"></i>')];
  return `<div class="ribbon" aria-hidden="true">${slots.join("")}</div>`;
}
const swatch = (sr, label) => `<span class="tile sw" style="--h:${sr.hue}">${label}</span>`;
const dateTile = o => `<span class="tile date" style="--h:${o.series.hue}"><small>${MON[o.date.getUTCMonth()]}</small>${o.date.getUTCDate()}</span>`;
const occRow = (o, named) => `
  <button class="proj" data-open="${o.id}">
    ${named ? swatch(o.series, DAY[o.date.getUTCDay()].slice(0, 3)) : dateTile(o)}
    <span class="mid"><span class="t1">${esc(named ? occName(o) : short(o.date))}</span><span class="s1 ${o.songs.length < o.series.size && daysFrom(o.date) >= 0 ? "todo" : ""}">${esc(named ? `${when(o.date)} · ${ready(o)}` : `${ready(o)}${o.songs.length ? " · " + o.songs.slice(0, 2).map(s => s.title).join(", ") : ""}`)}</span></span>
    <span class="when">${!named && Math.abs(daysFrom(o.date)) <= 1 ? esc(when(o.date)) : ""}</span>
    <span></span>${occRibbon(o)}
  </button>`;

function pageSetlists() {
  const week = OCC.filter(o => daysFrom(o.date) >= 0 && daysFrom(o.date) < 7).sort((a, b) => a.date - b.date);
  return `${bar({ title: "Setlists", tools: [{ label: "Library", icon: "library", open: "library" }, { joined: [{ label: "New show", icon: "plus", act: "New show" }, { label: "More", icon: "more", menu: "projects" }] }] })}
    <div class="body">
      <h2 style="margin-top:4px">This week</h2>
      <div class="list">${week.map(o => occRow(o, true)).join("")}</div>
      <h2>Every week</h2>
      <div class="list">${SERIES.map(sr => { const nx = nextOf(sr); return `
        <button class="proj" data-open="${sr.id}">
          ${swatch(sr, sr.name.split(" ")[0].slice(0, 4))}
          <span class="mid"><span class="t1">${esc(sr.name)}</span><span class="s1">${esc(everyDays(sr))}</span></span>
          <span class="when">${nx ? esc(when(nx.date)) : ""}</span>
        </button>`; }).join("")}</div>
      <h2>Shows</h2>
      <div class="list">${PROJECTS.filter(p => p.kind === "Set").map(row).join("")}</div>
    </div>`;
}
function pageSeries(id) {
  const sr = seriesOf(id), all = occsOf(sr);
  const up = all.filter(o => daysFrom(o.date) >= 0), past = all.filter(o => daysFrom(o.date) < 0).reverse();
  const todo = up.find(o => o.songs.length < sr.size);
  return `${bar({ back: true, title: sr.name, tools: [{ joined: [{ label: "Plan a set", icon: "plus", act: todo ? `Planning ${short(todo.date)}` : "Plan a set" }, { label: "More", icon: "more", menu: "series" }] }] })}
    <div class="body">
      <p class="meta">${esc(everyDays(sr))} · ${sr.size} songs a set · ${past.length} sets so far</p>
      ${todo ? `<button class="primary wide" data-open="${todo.id}">${ic("plus", 17, 2.2)}Plan ${esc(short(todo.date))}</button>` : ""}
      <h2>Coming up</h2>
      <div class="list">${up.map(o => occRow(o, false)).join("")}</div>
      <h2>Before</h2>
      <div class="list">${past.map(o => occRow(o, false)).join("")}</div>
    </div>`;
}
function pageOcc(id) {
  const o = occOf(id), sr = o.series, missing = sr.size - o.songs.length;
  const prev = occsOf(sr).filter(x => x.date < o.date && x.date.getUTCDay() === o.date.getUTCDay()).pop();
  let n = 0;
  return `${bar({ back: true, title: `${sr.name} · ${short(o.date)}`, tools: [{ joined: [{ label: "Share", icon: "share", act: "Share link copied" }, { label: "More", icon: "more", menu: "project" }] }] })}
    <div class="body">
      <p class="meta">${esc(DAY[o.date.getUTCDay()])}, ${MON[o.date.getUTCMonth()]} ${o.date.getUTCDate()} · ${esc(ready(o))}${o.songs.length ? " · " + dur(o.songs.reduce((t, s) => t + s.len, 0)) : ""}</p>
      <div class="big-ribbon">${occRibbon(o)}</div>
      <div style="height:16px"></div>
      ${o.songs.length ? `<div class="go2">
        <button class="primary" data-act="Playing ${esc(sr.name)}, ${esc(short(o.date))}">${ic("play", 15)}Play the set</button>
        <button class="secondary" data-act="Choose a song to rehearse">${ic("mic", 16)}Rehearse</button>
      </div>
      ${MINE[o.id] ? `<div class="assign solo" style="--h:${rigOf(MINE[o.id]).hue}"><div class="who">${rigIcon(rigOf(MINE[o.id]), 20)}<span class="mid"><span class="t1">You're on ${MINE[o.id]}</span><span class="s1">A preset for each song, in order</span></span></div>${openInRig(o, rigOf(MINE[o.id]))}</div>` : ""}` : prev ? `<button class="secondary wide" data-act="Copied ${esc(short(prev.date))}'s set">Start from ${esc(short(prev.date))}'s set</button>` : ""}
      <div class="list" style="margin-top:14px">
        ${o.songs.map(s => `
          <button class="song" data-act="Rehearsing ${esc(s.title)}">
            <span class="n">${++n}</span>${keyTile(s.key)}
            <span class="mid"><span class="t1">${esc(s.title)}</span><span class="s1">${esc(s.artist)} · ${s.tempo} bpm</span></span>
            <span class="len">${dur(s.len)}</span>
          </button>`).join("")}
        ${Array.from({ length: Math.max(0, missing) }, () => `
          <button class="song slot" data-open="library">
            <span class="n">${++n}</span><span class="tile add">${ic("plus", 18, 2)}</span>
            <span class="mid"><span class="t1">Add a song</span><span class="s1">From the Library</span></span><span></span>
          </button>`).join("")}
      </div>
    </div>`;
}
const rigIcon = (r, size = 24) => `<span class="rigicon" style="--h:${r.hue}">${ic(r.icon, size, 1.7)}</span>`;
function openInRig(o, r) {
  return `<button class="inrig" style="--h:${r.hue}" data-act="Opened ${esc(occName(o))}, ${esc(short(o.date))} in the ${r.name} rig: ${o.songs.length} songs, a preset each">${ic(r.icon, 17, 1.8)}Open in ${r.name}</button>`;
}
const mySets = () => OCC.filter(o => MINE[o.id] && daysFrom(o.date) >= 0).sort((a, b) => a.date - b.date);
function pageRigs() {
  const next = mySets()[0], nr = next && rigOf(MINE[next.id]);
  // Today's set (or the next), opened in your rig in one tap; the chevron
  // lists the rest of yours.
  const badge = next ? `<div class="joined quick" style="--h:${nr.hue}">
      <button class="qopen" aria-label="Open ${esc(occName(next))} in the ${nr.name} rig" data-act="Opened ${esc(occName(next))}, ${esc(short(next.date))} in the ${nr.name} rig: ${next.songs.length} songs, a preset each">${ic(nr.icon, 16, 1.9)}<span>${esc(next.series.name)} · ${esc(when(next.date))}</span></button>
      <span class="hair"></span>
      <button class="tool" aria-label="Your sets" data-menu="mine">${ic("updown", 16)}</button>
    </div>` : "";
  const mine = mySets();
  return `${bar({ title: "Signal", tools: [{ html: badge }] })}
    <div class="body rigs">${RIGS.map(r => { const n = mine.filter(o => MINE[o.id] === r.name).length; return `
      <button class="rigrow ${r.loaded ? "loaded" : ""} ${r.preset ? "" : "off"}" style="--h:${r.hue}" data-act="Opened the ${esc(r.name)} rig">
        ${rigIcon(r, 26)}
        <span class="mid"><span class="name">${esc(r.name)}</span><span class="s1">${esc(r.preset || "Not set up")} · ${esc(r.input)}</span></span>
        ${r.loaded ? '<span class="on"><span class="live"></span>Loaded</span>' : n ? `<span class="yours">${n} set${n > 1 ? "s" : ""}</span>` : `<span class="chev">${ic("chevRight", 18)}</span>`}
      </button>`; }).join("")}
    </div>`;
}
function lastPlayed(title) {
  const o = OCC.filter(o => daysFrom(o.date) < 0 && o.songs.some(x => x.title === title)).pop();
  if (!o) return null; const w = Math.round(-daysFrom(o.date) / 7);
  return `${o.series.name} ${w < 1 ? "this week" : w === 1 ? "last week" : w + " weeks ago"}`;
}
function pageLibrary() {
  const seen = new Set();
  const list = [...POOL, ...Object.values(SETS).flatMap(songs)].filter(s => !seen.has(s.title) && seen.add(s.title));
  return `${bar({ back: true, title: "Library", tools: [{ label: "Filter", icon: "sliders", act: "Filter songs" }] })}
    <div class="body">
      <p class="meta">${list.length} songs · what each is ready for, and where it was last played</p>
      <div class="legend"><span><i></i>Chart</span><span><i></i>Lyrics</span><span><i></i>Stems</span><span><i></i>Tempo map</span></div>
      <div class="list">${list.map(s => { const lp = lastPlayed(s.title); return `
        <button class="song" style="grid-template-columns:36px 1fr auto" data-act="Opened ${esc(s.title)}">
          ${keyTile(s.key)}
          <span class="mid"><span class="t1">${esc(s.title)}</span><span class="s1">${esc([s.artist || "No artist yet", lp].filter(Boolean).join(" · "))}</span></span>
          <span class="len">${dur(s.len)}<span class="dots">${"cltm".split("").map(k => `<i class="${(s.ready ?? "cltm").includes(k) ? "y" : ""}"></i>`).join("")}</span></span>
        </button>`; }).join("")}</div>
    </div>`;
}
function pageSessions() {
  const a = ALBUMS.cr;
  return `${bar({ title: "Sessions", tools: [{ joined: [{ label: "New session", icon: "plus", act: "New session" }, { label: "More", icon: "more", menu: "projects" }] }] })}
    <div class="body">
      <h2 style="margin-top:4px">Records</h2>
      <button class="hero" data-open="cr">
        <span class="kicker">${ic("disc", 14)}${esc(a.what)} · ${esc(a.artist)}</span>
        <div class="t">${esc(a.title)}</div>
        <div class="s">${a.tracks.length} tracks · ${esc(stageSummary(a))}</div>
        ${stageRibbon(a)}
        <span class="stages">${STAGES.map(([n, h]) => `<span><i style="${h == null ? "background:#3a3a40" : `background:oklch(.62 .13 ${h})`}"></i>${n}</span>`).join("")}</span>
      </button>
      <h2>Sessions on THEBATTLESHIP</h2>
      <div class="list">${PROJECTS.filter(p => p.kind === "Production").map(row).join("")}</div>
    </div>`;
}
function pageAlbum(id) {
  const a = ALBUMS[id];
  return `${bar({ back: true, title: a.title, tools: [{ joined: [{ label: "Share", icon: "share", act: "Share link copied" }, { label: "More", icon: "more", menu: "project" }] }] })}
    <div class="body">
      <p class="meta">${esc(a.what)} · ${a.tracks.length} tracks · ${esc(stageSummary(a))}</p>
      <div class="big-ribbon">${stageRibbon(a)}</div>
      <div class="stages" style="margin:8px 0 18px">${STAGES.map(([n, h]) => `<span><i style="${h == null ? "background:#3a3a40" : `background:oklch(.62 .13 ${h})`}"></i>${n}</span>`).join("")}</div>
      <div class="go2"><button class="primary" data-act="Opened ${esc(a.title)} in Session">Open in Session</button><button class="secondary" data-act="Playing the latest mixes">${ic("play", 15)}Listen</button></div>
      <div class="list" style="margin-top:14px">${a.tracks.map((t, k) => `
        <button class="song" data-act="Opened ${esc(t.title)}">
          <span class="n">${k + 1}</span><span class="tile" style="width:36px;height:36px">${ic("audio", 18)}</span>
          <span class="mid"><span class="t1">${esc(t.title)}</span><span class="s1">${esc(a.artist)}</span></span>
          <span class="stage" style="${STAGES[t.stage][1] == null ? "" : `--h:${STAGES[t.stage][1]}`}" data-none="${STAGES[t.stage][1] == null}">${STAGES[t.stage][0]}</span>
        </button>`).join("")}</div>
    </div>`;
}
const PAGES = { home: pageProjects, sessions: pageSessions, setlists: pageSetlists, signal: pageRigs };
const TABS = [["home", "Home", "home"], ["sessions", "Sessions", "audio"], ["setlists", "Setlists", "setlist"], ["signal", "Signal", "rig"]];

// ── The sidebar (the left drawer) ───────────────────────────────────────────
function drawer() {
  const meter = [1,1,1,1,1,1,0,0,0,0,0,0].map(o => `<i class="${o ? "on" : ""}"></i>`).join("");
  return `
    <div class="scroll">
      <button class="switcher" data-act="Switch organization">
        <span class="mark">F</span><span class="nm"><b>FastTrackStudio</b><span>Organization · 3 vaults</span></span>
        <span style="color:var(--ink-3)">${ic("updown", 16)}</span>
      </button>
      <div class="sidetabs">${TABS.map(([id, label, icon]) => `<button class="sidetab" data-tab="${id}" aria-current="${tab === id ? "page" : "false"}">${ic(icon, 19)}${label}<span class="n">${{ home: "", sessions: 3, setlists: SERIES.length + 2, signal: RIGS.length }[id]}</span></button>`).join("")}
        <button class="sidetab" data-open="library">${ic("library", 19)}Library<span class="n">${POOL.length + 21}</span></button></div>

      <div class="sidehead">Engine</div>
      <div class="engine">
        <div class="row1"><span class="live"></span>Running on THEBATTLESHIP</div>
        <div class="row2">Scarlett 18i20 · 48 kHz · 128 samples</div>
        <div class="meter" aria-label="Load 48%">${meter}</div>
      </div>

      <div class="sidehead">Every week</div>
      ${SERIES.map(sr => { const nx = nextOf(sr); return `<button class="siderow" data-open="${sr.id}"><span class="sw" style="background:oklch(.62 .12 ${sr.hue})"></span><span class="lbl">${esc(sr.name)}</span><span class="nt">${nx ? esc(when(nx.date)) : ""}</span></button>`; }).join("")}

      <div class="sidehead">Shows</div>
      ${PINNED.map(id => { const p = PROJECTS.find(x => x.id === id); return `<button class="siderow" data-open="${id}">${ribbon(p)}<span class="lbl">${esc(titleOf(p))}</span><span class="nt">${songs(SETS[p.set]).length}</span></button>`; }).join("")}

      <div class="sidehead">Loaded rig<button data-tab="signal">All</button></div>
      ${RIGS.filter(r => r.loaded).map(r => `<button class="siderow" data-act="Opened the ${esc(r.name)} rig"><span class="ic"><span class="live"></span></span><span class="lbl">${esc(r.name)}</span><span class="nt">${esc(r.preset)}</span></button>`).join("")}

      <div class="sidehead">Locations<button data-act="Scanning 4 locations">Scan</button></div>
      ${[["Studio vault", "86"], ["Ableton projects", "41"], ["Pro Tools sessions", "23"], ["REAPER projects", "12"]].map(([l, n]) => `<button class="siderow" data-act="Opened ${l}"><span class="ic">${ic("drive", 17)}</span><span class="lbl">${l}</span><span class="nt">${n}</span></button>`).join("")}
    </div>
    <div class="footrows">
      <button data-act="Theme: dark">${ic("moon", 18)}Theme</button>
      <button data-act="Settings">${ic("settings", 18)}Settings</button>
    </div>
    <div class="account"><button data-act="Account">
      <span class="avatar">CW</span><span class="who"><b>Cody Wright</b><span>Signed in · 2 devices</span></span>
      <span style="color:var(--ink-3)">${ic("updown", 15)}</span>
    </button></div>`;
}

// ── The ⋯ menus ─────────────────────────────────────────────────────────────
const MENUS = {
  mine: () => [["head", "Your sets"], ...mySets().map(o => ["item", `${occName(o)}`, rigOf(MINE[o.id]).icon, `${when(o.date)} · ${MINE[o.id]}`])],
  projects: () => [
    ["item", "New project", "plus"], ["item", "Open a file…", "open"],
    ["div"], ["head", "Bring in"],
    ["item", "Ableton Live set", "import"], ["item", "Pro Tools session", "import"], ["item", "REAPER or Logic project", "import"],
    ["div"], ["item", "Scan for projects", "scan", "4 places"],
    ["div"], ["head", "Sort"], ["check", "Last opened", true], ["check", "Name", false], ["check", "Show ribbons", true],
  ],
  series: () => [
    ["item", "Plan a set", "plus"], ["item", "Change the days", "settings"], ["item", "Songs a set", "setlist", "usual count"],
    ["div"], ["item", "Pin to the sidebar", "pin"], ["item", "Share…", "share"],
  ],
  project: () => [
    ["item", "Open in Session", "open"], ["item", "Rehearse", "mic"], ["item", "Pin to the sidebar", "pin"],
    ["div"], ["item", "Share…", "share"], ["item", "Show in the vault", "folder"],
    ["div"], ["item", "Remove from recent", "trash", null, true],
  ],
};
function openMenu(name, anchor) {
  const m = document.getElementById("menu");
  m.innerHTML = MENUS[name]().map(([k, a, b, c, d]) => k === "div" ? '<div class="md"></div>'
    : k === "head" ? `<div class="mh">${a}</div>`
    : k === "check" ? `<button class="mi" data-act="${a}"><span class="ic"></span>${a}${b ? `<span class="ck">${ic("check", 17, 2.2)}</span>` : ""}</button>`
    : `<button class="mi" data-act="${a}" ${d ? 'style="color:#f87171"' : ""}><span class="ic" ${d ? 'style="color:#f87171"' : ""}>${ic(b, 18)}</span>${a}${c ? `<span class="nt">${c}</span>` : ""}</button>`).join("");
  const r = anchor.getBoundingClientRect(), ph = document.getElementById("phone").getBoundingClientRect();
  m.style.top = `${r.bottom - ph.top + 8}px`; m.style.right = `${ph.right - r.right}px`;
  m.hidden = false;
  const c = document.createElement("div"); c.className = "menu-catch"; c.onclick = closeMenu; m.before(c);
}
function closeMenu() { document.getElementById("menu").hidden = true; document.querySelector(".menu-catch")?.remove(); }

// ── Open anything (the round button) ────────────────────────────────────────
function openPalette() {
  const sh = document.getElementById("sheet");
  sh.innerHTML = `<div class="panel"><label class="field">${ic("search", 18)}<input placeholder="Projects, songs, rigs, actions" aria-label="Open anything"><button class="cancel" data-close>Cancel</button></label><div class="results"></div></div>`;
  sh.hidden = false;
  const input = sh.querySelector("input"), res = sh.querySelector(".results");
  const ITEMS = [
    ...PROJECTS.map(p => ({ t: titleOf(p), k: p.kind, tile: projTile(p), open: p.id })),
    ...Object.values(SETS).flatMap(songs).filter((s, i, a) => a.findIndex(x => x.title === s.title) === i).map(s => ({ t: s.title, k: "Song", tile: keyTile(s.key), act: "Opened " + s.title })),
    ...RIGS.map(r => ({ t: r.name + " rig", k: "Rig", tile: `<span class="tile">${ic("rig", 17)}</span>`, act: "Opened the " + r.name + " rig" })),
    ...["New project", "Scan for projects", "Tuner", "Audio settings"].map(a => ({ t: a, k: "Action", tile: `<span class="tile">${ic("command", 16)}</span>`, act: a })),
  ];
  const draw = () => { const q = input.value.toLowerCase().trim(); const list = (q ? ITEMS.filter(i => i.t.toLowerCase().includes(q)) : ITEMS.slice(0, 6)).slice(0, 12);
    res.innerHTML = (q ? "" : '<div class="mh">Recent</div>') + (list.map(i => `<button class="res" ${i.open ? `data-open="${i.open}"` : `data-act="${esc(i.act)}"`} data-close>${i.tile}<span class="t1">${esc(i.t)}</span><span class="k">${i.k}</span></button>`).join("") || `<div class="mh">Nothing called “${esc(q)}”</div>`); };
  input.addEventListener("input", draw); draw();
  setTimeout(() => input.focus(), 50);
}
const closePalette = () => { document.getElementById("sheet").hidden = true; };

// ── Navigation ──────────────────────────────────────────────────────────────
let tab = "home", stack = [];
const stage = document.getElementById("stage");
const known = id => id === "library" || seriesOf(id) || occOf(id) || PROJECTS.find(p => p.id === id);
const pageFor = id => id === "library" ? pageLibrary() : seriesOf(id) ? pageSeries(id) : occOf(id) ? pageOcc(id) : ALBUMS[id] ? pageAlbum(id) : pageProject(id);
// Where a page lives when it is opened from outside a tab's own pages.
const tabFor = id => { const p = PROJECTS.find(x => x.id === id); return p ? (p.kind === "Set" ? "setlists" : p.kind === "Song" ? "home" : "sessions") : "setlists"; };
function render() {
  const top = stack.at(-1), under = stack.at(-2);
  stage.innerHTML = `<section class="page" id="root">${under ? pageFor(under) : PAGES[tab]()}</section>` + (top ? `<section class="page" id="top">${pageFor(top)}</section>` : "");
  if (top) stage.querySelector("#root").classList.add("under");
  document.getElementById("tabs").innerHTML = TABS.map(([id, label, icon]) => `<button class="tab" data-tab="${id}" aria-label="${label}" aria-current="${tab === id ? "page" : "false"}">${ic(icon, 22)}</button>`).join("");
  document.getElementById("drawer").innerHTML = drawer();
  history.replaceState(null, "", "#/" + [tab, ...stack].join("/") + (side ? "!side" : ""));
}
function showTab(t) { tab = t; stack = []; setSide(false); render(); }
function push(id) {
  closePalette();
  if (!known(id)) return;
  if (side || tab === "signal") { setSide(false); stack = []; tab = tabFor(id); }
  stack.push(id); render();
  const top = stage.querySelector("#top"), root = stage.querySelector("#root");
  root.classList.remove("under"); top.classList.add("enter");
  requestAnimationFrame(() => requestAnimationFrame(() => { top.classList.remove("enter"); root.classList.add("under"); }));
}
function pop() {
  if (!stack.length) return;
  const top = stage.querySelector("#top"), root = stage.querySelector("#root");
  root.classList.remove("under"); top.classList.add("enter");
  setTimeout(() => { stack.pop(); render(); }, 280);
}
let side = false;
function setSide(open) { side = open; document.getElementById("phone").classList.toggle("side-open", open); history.replaceState(null, "", location.hash.replace("!side", "") + (open ? "!side" : "")); }

document.querySelector(".round").innerHTML = ic("search", 22, 1.9);

document.addEventListener("click", e => {
  const el = sel => e.target.closest(sel);
  if (el("[data-scope]")) { scope = el("[data-scope]").dataset.scope; return render(); }
  if (el("[data-menu]")) { e.stopPropagation(); return openMenu(el("[data-menu]").dataset.menu, el("[data-menu]")); }
  if (el(".menu .mi")) { closeMenu(); return toast(el(".mi").dataset.act); }
  if (el("[data-palette]")) return openPalette();
  if (el("#scrim")) return setSide(false);
  // A button inside the hero card says what it does; the card itself opens.
  const a = el("[data-act]"), o = el("[data-open]");
  if (a && (!o || o.contains(a) && a !== o)) { if (el("[data-close]")) closePalette(); return toast(a.dataset.act); }
  if (o) return push(o.dataset.open);
  if (el("[data-tab]")) return showTab(el("[data-tab]").dataset.tab);
  if (el("[data-back]")) return pop();
  if (el("[data-close]")) return closePalette();
  if (e.target.id === "sheet") return closePalette();
});

// A swipe in from the left edge brings the sidebar; a swipe left puts it away.
let sx = null, sy = null;
addEventListener("touchstart", e => { const t = e.touches[0], ph = document.getElementById("phone").getBoundingClientRect(); sx = (side || t.clientX - ph.left < 24) ? t.clientX : null; sy = t.clientY; }, { passive: true });
addEventListener("touchmove", e => { if (sx == null) return; const t = e.touches[0], dx = t.clientX - sx; if (Math.abs(t.clientY - sy) > Math.abs(dx)) { sx = null; return; } if (!side && dx > 50) { setSide(true); sx = null; } else if (side && dx < -50) { setSide(false); sx = null; } }, { passive: true });

// #/projects, #/projects/jt, #/rigs, …; "!side" with the sidebar out.
const h = (location.hash || "#/home").replace("#/projects", "#/home"), wantSide = h.includes("!side");
const [, first, ...rest] = h.replace("!side", "").split("/");
tab = PAGES[first] ? first : "home";
stack = rest.filter(known);
render();
if (wantSide) setSide(true);
