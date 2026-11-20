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
  perc: '<path d="M7 3h10l-1.5 18h-7z"/><path d="M7.4 8h9.2M8 15h8"/>',
  acoustic: '<circle cx="9" cy="15" r="6"/><circle cx="9" cy="15" r="1.8"/><path d="M13.3 10.7 20.5 3.5"/><path d="m19 2 3 3"/>',
  calendar: '<rect x="3" y="4" width="18" height="18" rx="2"/><path d="M16 2v4M8 2v4M3 10h18"/>',
  tag: '<path d="M12.586 2.586A2 2 0 0 0 11.172 2H4a2 2 0 0 0-2 2v7.172a2 2 0 0 0 .586 1.414l8.704 8.704a2.426 2.426 0 0 0 3.42 0l6.58-6.58a2.426 2.426 0 0 0 0-3.42z"/><circle cx="7.5" cy="7.5" r=".5" fill="currentColor"/>',
  x: '<path d="M18 6 6 18"/><path d="m6 6 12 12"/>',
  chevDown: '<path d="m6 9 6 6 6-6"/>',
  chart: '<path d="M15 2H6a2 2 0 0 0-2 2v16a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2V7Z"/><path d="M14 2v4a2 2 0 0 0 2 2h4"/><path d="M8 13h2M8 17h2M14 13h2M14 17h2"/>',
  type: '<path d="M4 7V4h16v3"/><path d="M9 20h6"/><path d="M12 4v16"/>',
  message: '<path d="M7.9 20A9 9 0 1 0 4 16.1L2 22Z"/>',
  messagePlus: '<path d="M7.9 20A9 9 0 1 0 4 16.1L2 22Z"/><path d="M8 12h8"/><path d="M12 8v8"/>',
  compare: '<path d="M8 3 4 7l4 4"/><path d="M4 7h16"/><path d="m16 21 4-4-4-4"/><path d="M20 17H4"/>',
  transpose: '<path d="m3 16 4 4 4-4"/><path d="M7 20V4"/><path d="m21 8-4-4-4 4"/><path d="M17 4v16"/>',
  download: '<path d="M12 15V3"/><path d="m7 10 5 5 5-5"/><path d="M21 15v4a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2v-4"/>',
  link: '<path d="M10 13a5 5 0 0 0 7.54.54l3-3a5 5 0 0 0-7.07-7.07l-1.72 1.71"/><path d="M14 11a5 5 0 0 0-7.54-.54l-3 3a5 5 0 0 0 7.07 7.07l1.71-1.71"/>',
  layers: '<path d="m12.83 2.18a2 2 0 0 0-1.66 0L2.6 6.08a1 1 0 0 0 0 1.83l8.58 3.91a2 2 0 0 0 1.66 0l8.58-3.9a1 1 0 0 0 0-1.83Z"/><path d="m2 12 8.58 3.91a2 2 0 0 0 1.66 0L21 12"/><path d="m2 17 8.58 3.91a2 2 0 0 0 1.66 0L21 17"/>',
  pause: '<rect x="6" y="4" width="4" height="16" rx="1" fill="currentColor"/><rect x="14" y="4" width="4" height="16" rx="1" fill="currentColor"/>',
  skip: '<path d="M5 4l10 8-10 8z" fill="currentColor"/><path d="M19 5v14"/>',
  skipBack: '<path d="M19 20 9 12l10-8z" fill="currentColor"/><path d="M5 19V5"/>',
  clock: '<circle cx="12" cy="12" r="9"/><path d="M12 7v5l3 2"/>',
  branch: '<circle cx="6" cy="6" r="2.5"/><circle cx="6" cy="18" r="2.5"/><circle cx="18" cy="8" r="2.5"/><path d="M6 8.5v7"/><path d="M18 10.5c0 4-6 3.5-10.5 6"/>',
  pencil: '<path d="M21.17 6.81a1 1 0 0 0-3.98-3.98L3.84 16.17a2 2 0 0 0-.5.83l-1.32 4.35a.5.5 0 0 0 .62.62l4.35-1.32a2 2 0 0 0 .83-.5z"/>',
  grip: '<path d="M4 9h16"/><path d="M4 15h16"/>',
  mappin: '<path d="M20 10c0 4.99-5.54 10.19-7.4 11.79a1 1 0 0 1-1.2 0C9.54 20.19 4 14.99 4 10a8 8 0 0 1 16 0"/><circle cx="12" cy="10" r="3"/>',
  metronome: '<path d="M12 11.5 18 4"/><path d="M8.5 3h7l3.5 18H5z"/><path d="M7 15h10"/>',
  loop: '<path d="m17 2 4 4-4 4"/><path d="M3 11v-1a4 4 0 0 1 4-4h14"/><path d="m7 22-4-4 4-4"/><path d="M21 13v1a4 4 0 0 1-4 4H3"/>',
  bulb: '<path d="M15 14c.2-1 .7-1.7 1.5-2.5 1-.9 1.5-2.2 1.5-3.5A6 6 0 0 0 6 8c0 1 .2 2.2 1.5 3.5.7.7 1.3 1.5 1.5 2.5"/><path d="M9 18h6"/><path d="M10 22h4"/>',
  cues: '<path d="M8 6h13"/><path d="M8 12h13"/><path d="M8 18h13"/><path d="M3 6h.01"/><path d="M3 12h.01"/><path d="M3 18h.01"/>',
  arrowUp: '<path d="m5 12 7-7 7 7"/><path d="M12 19V5"/>',
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
// A track's status, and a record's: where it is on the way out.
const STAGES = [["Writing", 320], ["Recording", 25], ["Editing", 290], ["Mixing", 65], ["Mastering", 215], ["Approval", 90], ["Complete", 150], ["Released", 180], ["Archived", null]];
// The records. Crescendum's track list is the demo vault's; PNG, Montreal and
// A Journey of Immigrants are tombrooksmusic's projects on the share (their
// song folders and mixes). Every status is a placeholder.
const T = list => list.map(([title, stage]) => ({ title, stage }));
const ALBUMS = {
  cr: { title: "Crescendum", artist: "Crescendum", what: "Album", status: 3, hue: 285, when: "2h", tracks: T([
    ["Intro", 6], ["Set in Stone", 5], ["Unbreakable", 3], ["Chained Expectations", 3], ["Heavify", 2],
    ["It Knows My Name", 2], ["Kornesque", 1], ["The Ballad", 1], ["The Plague", 1], ["Empty Focus", 0]]) },
  png: { title: "PNG Worship Collective Album", artist: "PNG Worship Collective", what: "Album", status: 2, hue: 150, when: "May 26", tracks: T([
    ["All That I Am", 3], ["Lord of the Fight", 2], ["Dwelling Place", 2], ["Presence", 2], ["Life Giving Word", 1], ["He's Been Good", 2],
    ["I Know a Name", 2], ["Seek Your Face", 2], ["Hannah's Song", 2], ["Reason Why", 2], ["Goodness of God (RK version)", 3], ["Unity", 2]]) },
  mtl: { title: "Montreal Benefit Album", artist: "Various artists", what: "Benefit album", status: 1, hue: 5, when: "Aug 4", tracks: T([
    ["Courage", 3], ["Radio", 3], ["Too Young", 1], ["Where to Go", 2], ["You and I", 2]]) },
  joi: { title: "A Journey of Immigrants", artist: "Andres Jaramillo", what: "Album", status: 5, hue: 45, when: "May 8", tracks: T([
    ["Toccata", 5], ["Ancestro", 5], ["La Zacapaneca", 5], ["Five Colombian Pieces", 5], ["Luz Entre Aguas", 5], ["El Intachable", 5], ["Etudes for Immigrants", 5]]) },
};
// Folders a record can sit in, nested as deep as wanted.
// No folders by org: the org is a filter (the switcher), not a place.
const RFOLDERS = [];
// The orgs the account reaches (Task's), and whose each record is. Null: all.
const ORGS = [["fasttrackstudio", "FastTrackStudio", 175], ["tombrooksmusic", "Tom Brooks Music", 25], ["fasttrackaudio", "FastTrack Audio", 265], ["codywright", "Cody Wright", 210], ["cbu", "CBU", 55], ["days-to-praise", "Days to Praise", 320], ["rockstars-of-tomorrow", "Rockstars of Tomorrow", 110]];
const OWNER = { cr: "fasttrackstudio", png: "tombrooksmusic", mtl: "tombrooksmusic", joi: "tombrooksmusic", "t:Devour": "fasttrackaudio", "t:Fire": "codywright", "t:Drive To LA": "codywright" };
const orgName = id => (ORGS.find(o => o[0] === id) || [, id])[1];
let org = null;
const PINNED = ["jt", "mb"];
// The rigs. Presets and inputs are placeholders; one rig is loaded in the
// engine at a time.
const RIGS = [
  { name: "Drums", icon: "drums", hue: 70, preset: "Recital kit", input: "From the session" },
  { name: "Perc", icon: "perc", hue: 110, preset: "Shakers and congas", input: "SPD-SX" },
  { name: "Bass", icon: "bass", hue: 305, preset: "Royal Blood fuzz", input: "Input 2" },
  { name: "Guitar", icon: "guitar", hue: 30, preset: "CYA, Thu Nov 19", input: "Input 1 · Hi-Z", loaded: true },
  { name: "Acoustic", icon: "acoustic", hue: 190, preset: "Worship, warm", input: "Input 3 · DI" },
  { name: "Keys", icon: "keys", hue: 255, preset: "JHM Sunday", input: "Kontrol S88" },
];
const rigOf = name => RIGS.find(r => r.name === name);
// What Cody plays at each set (placeholders).
// Cody plays JHM Sunday every week, and CYA when he is on it (today).
// His part in each is a placeholder.
const MINE = { "cya@2026-11-19": "Guitar", "jhms@2026-10-18": "Guitar", "jhms@2026-10-25": "Guitar", "jhms@2026-11-01": "Guitar", "jhms@2026-11-08": "Guitar", "jhms@2026-11-15": "Guitar", "jhms@2026-11-22": "Guitar", "jhms@2026-11-29": "Guitar", "jhms@2026-12-06": "Guitar" };

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
  const lead = back && !title
    ? `<button class="back rb" data-back aria-label="Back">${ic("chevLeft", 24, 2.2)}</button>`
    : back
    ? `<button class="back" data-back aria-label="Back">${ic("chevLeft", 26, 2)}</button><span class="small">${esc(title)}</span>`
    : `<span class="title">${esc(title)}</span>`;
  return `<div class="bar"><div class="lead">${lead}</div><div class="tools">${tools.map(g => g.html ? g.html : g.joined
    ? `<div class="joined">${g.joined.map((t, i) => (i ? '<span class="hair"></span>' : "") + toolBtn(t)).join("")}</div>`
    : toolBtn(g)).join("")}</div></div>`;
}
const toolBtn = t => `<button class="tool" aria-label="${esc(t.label)}" ${t.label === "Edit" ? "data-edit" : t.filter ? "data-filters" : t.open ? `data-open="${t.open}"` : t.menu ? `data-menu="${t.menu}"` : `data-act="${esc(t.act || t.label)}"`}>${ic(t.icon, 20)}${t.badge ? `<span class="count">${t.badge}</span>` : ""}</button>`;

let toastT;
function toast(msg) { const t = document.querySelector(".toast"); t.textContent = msg; t.classList.add("show"); clearTimeout(toastT); toastT = setTimeout(() => t.classList.remove("show"), 1600); }

// ── Pages ───────────────────────────────────────────────────────────────────
let scope = "All";
const SCOPES = ["All", "Setlists", "Records", "Songs"];
const scopeOf = p => p.kind === "Set" ? "Setlists" : p.kind === "Song" ? "Songs" : "Records";
const inScope = p => scope === "All" || scopeOf(p) === scope;
// Home: what needs you now, across the live side and the studio side.
function pageProjects() {
  const mine = OCC.filter(o => daysFrom(o.date) >= 0 && MINE[o.id]).sort((a, b) => a.date - b.date);
  const next = mine[0], pl = o => PLACE[o.series.id] || [];
  const seen = new Set([next?.series.id]);
  const week = [...mine.slice(1).filter(o => daysFrom(o.date) < 8 && !seen.has(o.series.id) && seen.add(o.series.id)).map(o => ({ o, date: o.date })),
    ...REH.filter(r => daysFrom(r.date) >= 0 && daysFrom(r.date) < 8).map(r => ({ r, date: r.date }))].sort((a, b) => a.date - b.date).slice(0, 3);
  const review = STUDIO.filter(x => openOn(x.title) || /h$|Yesterday/.test(latest(x.title).when)).slice(0, 3);
  const loaded = RIGS.find(r => r.loaded);
  const recent = [
    { open: "jt", img: ART.jt, title: "JT's Senior Recital", kind: "Setlist" },
    { open: "cr", a: ALBUMS.cr, title: "Crescendum", kind: "Album" },
    { open: "t:Fire", x: studioOf("t:Fire"), title: "Fire", kind: "Single" },
    { open: "mb", h: 150, title: "Merry & Bright", kind: "Setlist" },
    { open: "png", a: ALBUMS.png, title: "PNG Worship", kind: "Album" },
  ];
  const today = new Date(TODAY);
  return `${bar({ title: "Home", tools: [{ html: orgPill() }] })}
    <div class="body">
      <div class="hdate">${DAY[today.getUTCDay()]}, ${["January","February","March","April","May","June","July","August","September","October","November","December"][today.getUTCMonth()]} ${today.getUTCDate()}</div>
      ${next ? `
      <button class="nextup hn" style="--h:${next.series.hue}" data-open="${next.id}">
        <span class="nutop">
          ${LOGO[next.series.id] ? `<img class="nulogo" src="${LOGO[next.series.id]}" alt="">` : cover({ title: next.series.name }, 64, next.series.hue)}
          <span class="mid">
            <span class="nuk">${daysFrom(next.date) === 0 ? "Tonight" : DAY[next.date.getUTCDay()]}${pl(next)[1] ? " · " + pl(next)[1] : ""}</span>
            <span class="nut">${esc(next.series.name)}</span>
            <span class="nus">${esc(fold0(next))} · ${next.songs.length} songs</span>
          </span>
        </span>
        <span class="nuwhere"><span class="nw">${ic("mappin", 13, 2)}<span><b>${esc(pl(next)[0] || "")}</b></span></span><span class="nw call">${ic("clock", 13, 2)}<span><small>Call</small><b>${esc(pl(next)[2] || "")}</b></span></span></span>
      </button>` : ""}
      ${loaded ? `<button class="rigline" style="--h:${loaded.hue}" data-tab="signal"><span class="rigicon">${ic(loaded.icon, 16, 2)}</span><span class="mid"><b>${loaded.name}</b> loaded · ${esc(loaded.preset)}</span><span class="live"></span></button>` : ""}
      ${week.length ? `<div class="dayhead dh2"><span>This week</span><button class="seeall" data-tab="setlists">Setlists ${ic("chevRight", 14)}</button></div>
      <div class="agenda">${week.map(({ o, r }) => r ? rehRow(r) : `
        <button class="ag" data-open="${o.id}">
          <span class="agd"><small>${DAY[o.date.getUTCDay()].slice(0, 3)}</small><b>${o.date.getUTCDate()}</b></span>
          ${LOGO[o.series.id] ? `<img class="aglogo" src="${LOGO[o.series.id]}" alt="">` : ""}
          <span class="mid"><span class="t1">${esc(o.series.name)}</span><span class="s1">${ic("mappin", 11, 2)}${esc(pl(o)[0] || "")}${pl(o)[2] ? ` · <b class="agcall">Call ${pl(o)[2]}</b>` : ""}</span></span>
          ${o.songs.length >= o.series.size ? `<span class="st ok">${ic("check", 13, 2.6)}</span>` : progress(o)}
        </button>`).join("")}</div>` : ""}
      ${review.length ? `<div class="dayhead dh2"><span>To review</span><button class="seeall" data-tab="sessions">Records ${ic("chevRight", 14)}</button></div>
      <div class="agenda">${review.map(x => { const v = latest(x.title), n = openOn(x.title); return `
        <button class="ag" data-rev="${esc(x.title)}">
          ${cover(x.rec ? ALBUMS[x.rec] : x, 40, x.hue)}
          <span class="mid"><span class="t1">${esc(x.title)}</span><span class="s1">${v.what} v${v.v} · ${esc(v.who)} · ${esc(v.when)}</span></span>
          ${n ? `<span class="bubble">${ic("message", 13, 2)}${n}</span>` : `<span class="newv">New</span>`}
        </button>`; }).join("")}</div>` : ""}
      <div class="dayhead">Jump back in</div>
      <div class="shelf">${recent.map(c => `
        <button class="shelfi" data-open="${c.open}">
          <span class="sc">${c.img ? `<img src="${c.img}" alt="">` : cover(c.a || c.x || { title: c.title }, 0, (c.a || c.x || {}).hue ?? c.h)}</span>
          <span class="t1">${esc(c.title)}</span><span class="s1">${c.kind}</span>
        </button>`).join("")}</div>
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
  const fold = FOLDERS.find(f => f.shows.includes(p.set));
  return `${bar({ back: true, tools: [{ joined: [{ label: "Download", icon: "download", act: "Saved for offline" }, { label: "More", icon: "more", menu: "project" }] }] })}
    <div class="body">
      ${hero({ img: ART[p.id], h: p.set === "mb" ? 150 : 20, mark: set.title[0], title: titleOf(p),
        kicker: "",
        facts: calTile(...(set.date.match(/(Jan|Feb|Mar|Apr|May|Jun|Jul|Aug|Sep|Oct|Nov|Dec)\w* (\d{4})/) || [, "", ""]).slice(1, 3).map((v, k) => k ? v : v.slice(0, 3)), "") + `<span class="fcol">${fact("setlist", `${songs(set).length} songs`)}${total(set) ? fact("clock", dur(total(set))) : ""}</span>`,
        sub: [fold?.name].filter(Boolean).join(" · "),
        buttons: playShare(`Playing ${set.title}`) })}
      ${set.acts.map(([name, list]) => `
        ${name ? `<div class="act">${esc(name)}<span>${list.length} songs${list.some(s => s.len) ? " · " + dur(list.reduce((t, s) => t + (s.len || 0), 0)) : ""}</span></div>` : ""}
        <div class="tracks">${list.map(x => `
          <div class="trow">
            ${songArt(x)}
            <button class="mid" data-play="set:${p.set}|${esc(x.title)}"><span class="t1">${esc(x.title)}</span><span class="s1">${esc(x.artist || "No artist yet")}${x.len ? " · " + dur(x.len) : ""}</span></button>
            ${x.key ? `<span class="kc" style="--h:${hue(x.key.split("→")[0])}">${esc(x.key)}</span>` : `<span class="kc empty">?</span>`}
            ${dots(`${x.title}: options`)}
          </div>`).join("")}</div>`).join("")}
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

// How far along a set is: a ring filling as songs go in, a tick when done.
function progress(o) {
  const n = o.songs.length, of = o.series.size;
  if (n >= of) return `<span class="done" aria-label="Ready">${ic("check", 15, 2.6)}</span>`;
  const c = 2 * Math.PI * 9, f = c * n / of;
  return `<span class="ring" aria-label="${n} of ${of} songs"><svg width="24" height="24" viewBox="0 0 24 24"><circle cx="12" cy="12" r="9" fill="none" stroke="#2b2b31" stroke-width="3"/><circle cx="12" cy="12" r="9" fill="none" stroke="var(--live)" stroke-width="3" stroke-linecap="round" stroke-dasharray="${f} ${c}" transform="rotate(-90 12 12)"/></svg><b>${n}/${of}</b></span>`;
}
// The set's keys in order, an outline for each place still empty.
const keys = o => `<span class="keys">${o.songs.map(x => `<i style="--h:${hue(x.key)}">${esc(x.key)}</i>`).join("")}${'<i class="gap"></i>'.repeat(Math.max(0, o.series.size - o.songs.length))}</span>`;
// The tab: the sets you are on, as a queue. All sets: every set there is.
// Folders of sets, by their tag (Architect's tags on the set notes).
const FOLDERS = [
  { tag: "church", name: "Crossroads Church", series: ["jhms", "main", "hsm", "jhm", "cya"], shows: [] },
  { tag: "cbu", name: "CBU", series: [], shows: ["mb"] },
  { tag: "recitals", name: "Recitals", series: [], shows: ["jt"] },
];
const folded = new Set();
const part = o => { const r = rigOf(MINE[o.id]); return r ? `<span class="part" style="--h:${r.hue}">${ic(r.icon, 13, 2)}${r.name}</span>` : ""; };
const prevOf = o => occsOf(o.series).filter(x => x.date < o.date && x.date.getUTCDay() === o.date.getUTCDay()).pop();
function setCard(o, withPart) {
  const pv = prevOf(o);
  return `<button class="setrow" style="--h:${o.series.hue}" data-open="${o.id}">
    ${LOGO[o.series.id] ? `<img class="slogo big" src="${LOGO[o.series.id]}" alt="">` : ""}
    <span class="mid"><span class="t1">${esc(o.series.name)}${withPart ? part(o) : ""}</span>${o.songs.length ? keys(o) : `<span class="s1">Nothing yet</span>`}</span>
    ${o.songs.length ? progress(o) : pv ? `<span class="copy" data-act="Copied ${esc(short(pv.date))}'s set">Copy ${MON[pv.date.getUTCMonth()]} ${pv.date.getUTCDate()}</span>` : ""}
  </button>`;
}
const fold0 = o => (FOLDERS.find(f => f.series.includes(o.series.id)) || {}).name || "";
// Rehearsals booked for a set: when, where, and which set they rehearse.
const REH = [{ id: "reh1", set: "jhms@2026-11-22", date: new Date(Date.UTC(2026, 10, 21)), time: "10:00 AM", room: "Room 210" }];
function rehRow(r) {
  const o = occOf(r.set);
  return `<button class="ag reh" data-open="${r.set}">
    <span class="agd"><small>${DAY[r.date.getUTCDay()].slice(0, 3)}</small><b>${r.date.getUTCDate()}</b></span>
    <span class="rehic">${ic("mic", 17, 2)}</span>
    <span class="mid"><span class="t1">Rehearsal <span class="rfor">· ${esc(o.series.name)}</span></span><span class="s1">${esc(r.room)} · ${esc(r.time)}</span></span>
    <span class="chev">${ic("chevRight", 16)}</span>
  </button>`;
}
// Book a rehearsal: for which set, what day, what time, where.
let BK = null;
function openBook() {
  const sets = OCC.filter(o => daysFrom(o.date) >= 0 && MINE[o.id]).sort((a, b) => a.date - b.date).slice(0, 4);
  BK ||= { set: sets[0].id, day: 1, time: "7:00 PM", room: "Room 210" };
  const days = Array.from({ length: 7 }, (_, k) => new Date(TODAY.getTime() + k * 864e5));
  const chip = (attr, v, on, label) => `<button class="fopt" data-bk="${attr}|${v}" aria-pressed="${on}">${label}</button>`;
  const sh = document.getElementById("sheet");
  sh.innerHTML = `<div class="bsheet" role="dialog" aria-label="Book a rehearsal"><div class="grab"></div>
    <div class="bhead"><b>Book a rehearsal</b></div>
    <div class="fsec">For</div><div class="seg">${sets.map(o => chip("set", o.id, BK.set === o.id, `${esc(o.series.name)}<small>${short(o.date)}</small>`)).join("")}</div>
    <div class="fsec">Day</div><div class="seg days">${days.map((d, k) => chip("day", k, BK.day === k, `${k === 0 ? "Today" : DAY[d.getUTCDay()].slice(0, 3)}<small>${MON[d.getUTCMonth()]} ${d.getUTCDate()}</small>`)).join("")}</div>
    <div class="fsec">Time</div><div class="seg">${["10:00 AM", "1:00 PM", "6:00 PM", "7:00 PM"].map(t => chip("time", t, BK.time === t, t)).join("")}</div>
    <div class="fsec">Where</div><div class="seg">${["Room 210", "Chapel", "Worship Center", "The Loft"].map(t => chip("room", t, BK.room === t, t)).join("")}</div>
    <button class="primary wide" data-bkgo>Book</button>
  </div>`;
  sh.classList.add("bottom"); sh.hidden = false;
}
// The tab: the sets you are on. The next one featured; the rest an agenda.
function pageSetlists() {
  const mine = OCC.filter(o => daysFrom(o.date) >= 0 && MINE[o.id]).sort((a, b) => a.date - b.date);
  const next = mine[0];
  // Coming up: the next of each event you play (not the one featured), and the rehearsals booked, by date.
  const seen = new Set([next?.series.id]);
  const events = mine.slice(1).filter(o => !seen.has(o.series.id) && seen.add(o.series.id)).map(o => ({ o, date: o.date }));
  const rest = [...events, ...REH.map(r => ({ r, date: r.date }))].sort((a, b) => a.date - b.date);
  const pl = o => PLACE[o.series.id] || [];
  const when = o => { const n = daysFrom(o.date); return n === 0 ? "Today" : n === 1 ? "Tomorrow" : DAY[o.date.getUTCDay()]; };
  const state = o => o.songs.length >= o.series.size ? `<span class="st ok">${ic("check", 13, 2.6)}</span>`
    : o.songs.length ? progress(o) : (() => { const pv = prevOf(o); return pv ? `<span class="copy2" data-act="Copied ${esc(short(pv.date))}'s set">Copy ${MON[pv.date.getUTCMonth()]} ${pv.date.getUTCDate()}</span>` : `<span class="st none">Empty</span>`; })();
  const r = next && rigOf(MINE[next.id]);
  return `${bar({ title: "Setlists", tools: [{ joined: [{ label: "All sets", icon: "folder", open: "allsets" }, { label: "Library", icon: "library", open: "library" }] }, { label: "New setlist", icon: "plus", menu: "newset" }] })}
    <div class="body">
      ${next ? `
      <div class="nextup" style="--h:${next.series.hue}">
        <button class="nuopen" data-open="${next.id}">
          <span class="nutop">
            ${LOGO[next.series.id] ? `<img class="nulogo" src="${LOGO[next.series.id]}" alt="">` : cover({ title: next.series.name }, 64, next.series.hue)}
            <span class="mid">
              <span class="nuk">Next up · ${when(next)}${pl(next)[1] ? " " + pl(next)[1] : ""}</span>
              <span class="nut">${esc(next.series.name)}</span>
              <span class="nus">${esc(fold0(next))}</span>
            </span>
          </span>
          <span class="nuwhere"><span class="nw">${ic("mappin", 15, 2)}<span><small>Where</small><b>${esc(pl(next)[0] || "—")}</b></span></span><span class="nw call">${ic("clock", 15, 2)}<span><small>Call</small><b>${esc(pl(next)[2] || "—")}</b></span></span><span class="nw">${ic("play", 13, 2)}<span><small>Starts</small><b>${esc(pl(next)[1] || "—")}</b></span></span></span>
        </button>
      </div>` : ""}
      <div class="dayhead dh2"><span>Coming up</span><button class="bookbtn" data-book>${ic("plus", 14, 2.4)}Rehearsal</button></div>
      ${rest.length ? `
      <div class="agenda">${rest.map(({ o, r }) => r ? rehRow(r) : `
        <button class="ag" data-open="${o.id}">
          <span class="agd"><small>${DAY[o.date.getUTCDay()].slice(0, 3)}</small><b>${o.date.getUTCDate()}</b></span>
          ${LOGO[o.series.id] ? `<img class="aglogo" src="${LOGO[o.series.id]}" alt="">` : ""}
          <span class="mid"><span class="t1">${esc(o.series.name)}</span><span class="s1">${ic("mappin", 11, 2)}${esc(pl(o)[0] || "")}${pl(o)[2] ? ` · <b class="agcall">Call ${pl(o)[2]}</b>` : ""}</span></span>
          ${state(o)}
        </button>`).join("")}</div>` : ""}
    </div>`;
}
// Every set there is, in folders by tag; a series is one row, its next set beside it.
function pageAllSets(root) {
  return `${bar(root ? { title: "Setlists", tools: [{ label: "Library", icon: "library", open: "library" }, { label: "New setlist", icon: "plus", menu: "newset" }] } : { back: true, title: "All sets", tools: [{ label: "New set", icon: "plus", act: "New set or series" }] })}
    <div class="body">${FOLDERS.map(f => { const open = !folded.has(f.tag), n = f.series.length + f.shows.length; return `
      <button class="folder" data-fold="${f.tag}" aria-expanded="${open}">
        <span class="fic">${ic("folder", 18)}</span><span class="fname">${esc(f.name)}</span><span class="ftag">#${f.tag}</span><span class="fn">${n}</span>
        <span class="fchev">${ic(open ? "chevDown" : "chevRight", 16)}</span>
      </button>
      ${open ? `<div class="fbody">${f.series.map(id => { const sr = seriesOf(id), nx = nextOf(sr); return `
        <button class="srow" style="--h:${sr.hue}" data-open="${sr.id}">
          ${LOGO[sr.id] ? `<img class="slogo" src="${LOGO[sr.id]}" alt="">` : '<span class="dot"></span>'}
          <span class="mid"><span class="t1">${esc(sr.name)}</span><span class="s1">${esc(sr.days.map(d => (sr.note?.[d] || DAY[d]) + "s").join(" and "))}${nx ? ` · ${daysFrom(nx.date) === 0 ? "today" : daysFrom(nx.date) === 1 ? "tomorrow" : MON[nx.date.getUTCMonth()] + " " + nx.date.getUTCDate()}` : ""}</span></span>
          ${nx ? (nx.songs.length ? progress(nx) : '<span class="s1">Not planned</span>') : ""}
        </button>`; }).join("")}${f.shows.map(id => { const st = SETS[id]; return `
        <button class="srow" data-open="${id}">
          ${ART[id] ? `<img class="slogo" src="${ART[id]}" alt="">` : `<span class="slogo">${cover({ title: st.title }, 36, id === "mb" ? 150 : 20)}</span>`}
          <span class="mid"><span class="t1">${esc(st.title)}</span><span class="s1">${esc(st.date)} · ${songs(st).length} songs</span></span>
          <span class="chev">${ic("chevRight", 16)}</span>
        </button>`; }).join("")}</div>` : ""}`; }).join("")}
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
  const fold = FOLDERS.find(f => f.series.includes(sr.id));
  let n = 0; const roomy = o.songs.length <= 6;
  const tools = [{ joined: [{ label: "Download", icon: "download", act: "Saved for offline" }, { label: "More", icon: "more", menu: "project" }] }];
  return `${bar({ back: true, tools })}
    <div class="body">
      ${hero({ img: LOGO[sr.id], h: sr.hue, mark: sr.name.split(" ")[0], title: sr.name,
        kicker: "",
        facts: `<span class="when2">${calTile(MON[o.date.getUTCMonth()], o.date.getUTCDate(), DAY[o.date.getUTCDay()].slice(0, 3))}<span class="loc">${ic("mappin", 14, 2)}<span><b>${esc(PLACE[sr.id]?.[0] || "")}</b>${PLACE[sr.id] ? `<small><span class="call">Call ${esc(PLACE[sr.id][2])}</span> · Starts ${esc(PLACE[sr.id][1])}</small>` : ""}</span></span></span>`,
        sub: `${fold ? esc(fold.name) : ""}${o.songs.length < sr.size ? ` · <span class="todo2">${esc(ready(o))}</span>` : ""}<span class="hcount">${o.songs.length} songs${o.songs.length ? " · " + dur(o.songs.reduce((t, x) => t + x.len, 0)) : ""}</span>`,
        buttons: o.songs.length ? playShare(`Playing ${sr.name}, ${short(o.date)}`) : prev ? `<button class="primary" data-act="Copied ${esc(short(prev.date))}'s set">Start from ${MON[prev.date.getUTCMonth()]} ${prev.date.getUTCDate()}</button><button class="secondary" data-open="library">${ic("library", 17)}Library</button>` : "" })}
      ${(() => {
        // A week's set in its sections: the Top Set (two songs), then the Bottom Set.
        const rows = [
          ...o.songs.map((x, i) => `
          <div class="trow roomy">
            ${songArt(x)}
            <button class="mid" data-play="occ:${o.id}|${esc(x.title)}">
              <span class="t1">${esc(x.title)}</span><span class="s1">${esc(x.artist)}</span>
              <span class="meta3">${leadsText(o, i, x)}</span>
            </button>
            <span class="rcol">${keyChip(x.key, keyIn(o, i, x), `${o.id}|${i}`)}<span class="arrsm ${arrOf(o, i) === "Normal" ? "none" : ""}" role="button" data-arrpick="${o.id}|${i}" aria-label="Arrangement: ${esc(arrOf(o, i))}">${ic("branch", 11, 2.4)}${arrOf(o, i) === "Normal" ? "" : esc(arrOf(o, i))}</span></span>
          </div>`),
          ...Array.from({ length: Math.max(0, missing) }, () => `
          <button class="trow roomy add" data-open="library">
            <span class="sart empty">${ic("plus", 18, 2)}</span>
            <span class="mid"><span class="t1">Add a song</span><span class="s1">From the Library</span></span>
            <span></span>
          </button>`),
        ];
        const len = list => dur(list.reduce((t, s) => t + (s?.len || 0), 0));
        return SECTIONS_OF(o).map(([name, from, to], k) => `
          <div class="ssec"><span>${name}</span><span></span></div>
          <div class="scard">${rows.slice(from, to).join("")}</div>`).join("");
      })()}
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
        <span class="mid"><span class="name">${esc(r.name)}<small>${esc(r.input)}</small></span>${r.loaded && next && MINE[next.id] === r.name
          ? `<span class="s1 now">${esc(next.series.name)} · 1/${next.songs.length} · ${esc(next.songs[0].title)}</span><span class="level" aria-label="Input level"><i style="width:46%"></i></span>`
          : `<span class="s1">${esc(r.preset || "Not set up")}</span>`}</span>
        ${r.loaded ? '<span class="on" aria-label="Loaded"><span class="live"></span></span>' : n ? `<span class="yours">${n} set${n > 1 ? "s" : ""}</span>` : `<span class="chev">${ic("chevRight", 18)}</span>`}
      </button>`; }).join("")}
    </div>`;
}
function lastPlayed(title) {
  const o = OCC.filter(o => daysFrom(o.date) < 0 && o.songs.some(x => x.title === title)).pop();
  if (!o) return null; const w = Math.round(-daysFrom(o.date) / 7);
  return `${o.series.name} ${w < 1 ? "this week" : w === 1 ? "last week" : w + " weeks ago"}`;
}
// Songs are notes; their tags are Architect's. Energy (1 to 5) and the
// tags are placeholders.
const ROCK = ["Royal Blood", "Metallica", "Queens of the Stone Age", "The White Stripes"];
const OWN = ["Nights Out", "Nights Out & Ryce", "JT Wright", "Cody Wright"];
const LIB = (() => {
  const seen = new Set(), out = [];
  const add = (x, tags) => { if (seen.has(x.title)) return; seen.add(x.title);
    const e = x.tempo ? Math.min(5, Math.max(1, Math.round((x.tempo - 50) / 22))) : 3;
    out.push({ ...x, tags, energy: e }); };
  POOL.forEach(x => add({ ...x, ready: x.ready || "cltm" }, ["worship", "contemporary", ...(x.tempo > 100 ? ["upbeat"] : ["ballad"]), ...(x.artist === "Chris Tomlin" ? ["hymns"] : [])]));
  songs(SETS.mb).forEach(x => add(x, ["christmas", ...(x.artist === "Chris Tomlin" ? ["worship"] : []), ...(["Silent Night", "Joy to the World (Unspeakable Joy)"].includes(x.title) ? ["hymns"] : [])]));
  songs(SETS.jt).forEach(x => add(x, ["recital", OWN.includes(x.artist) ? "originals" : "covers", ...(ROCK.includes(x.artist) ? ["rock"] : [])]));
  return out.sort((a, b) => a.title.localeCompare(b.title));
})();
const TAGS = [...new Set(LIB.flatMap(x => x.tags))].map(t => ({ t, n: LIB.filter(x => x.tags.includes(t)).length })).sort((a, b) => b.n - a.n);
const TEMPI = [["Slow", 0, 80], ["Mid", 80, 110], ["Up", 110, 140], ["Fast", 140, 999]];
const READY = [["c", "Chart"], ["l", "Lyrics"], ["s", "Stems"], ["m", "Tempo map"]];
const F = { q: "", tags: new Set(), keys: new Set(), energy: new Set(), tempo: new Set(), ready: new Set(), rested: false };
const weeksSince = title => { const o = OCC.filter(o => daysFrom(o.date) < 0 && o.songs.some(x => x.title === title)).pop(); return o ? -daysFrom(o.date) / 7 : Infinity; };
const keyOf = x => x.key ? (ALIAS[x.key.split("→")[0]] || x.key.split("→")[0]) : null;
function matches(x) {
  if (F.q && !(x.title + " " + (x.artist || "")).toLowerCase().includes(F.q)) return false;
  if ([...F.tags].some(t => !x.tags.includes(t))) return false;
  if (F.keys.size && !F.keys.has(keyOf(x) || "?")) return false;
  if (F.energy.size && !F.energy.has(x.energy)) return false;
  if (F.tempo.size && !TEMPI.some(([n, lo, hi]) => F.tempo.has(n) && x.tempo >= lo && x.tempo < hi)) return false;
  if ([...F.ready].some(k => !(x.ready || "").includes(k))) return false;
  if (F.rested && weeksSince(x.title) < 4) return false;
  return true;
}
// What is on, as chips that take themselves off.
function activeChips() {
  const c = [];
  if (F.keys.size) c.push(["keys", `Key ${[...F.keys].join(", ")}`]);
  if (F.energy.size) c.push(["energy", `Energy ${[...F.energy].sort().join(", ")}`]);
  if (F.tempo.size) c.push(["tempo", [...F.tempo].join(", ")]);
  if (F.ready.size) c.push(["ready", "Has " + READY.filter(([k]) => F.ready.has(k)).map(([, n]) => n.toLowerCase()).join(", ")]);
  if (F.rested) c.push(["rested", "Not played in 4 weeks"]);
  return c;
}
const nFilters = () => activeChips().length;
const bars = e => `<span class="energy" aria-label="Energy ${e} of 5">${[1, 2, 3, 4, 5].map(k => `<i class="${k <= e ? "on" : ""}" style="height:${4 + k * 2}px"></i>`).join("")}</span>`;
// The Library opens on its tags, All songs first; a tag opens its songs.
const cap = t => t[0].toUpperCase() + t.slice(1);
function mosaic(list) {
  const seen = new Set(), four = list.filter(x => x.artist && !seen.has(OWN.includes(x.artist) ? "own" : x.artist) && seen.add(OWN.includes(x.artist) ? "own" : x.artist)).slice(0, 4);
  return `<span class="mosaic">${four.map(x => songArt(x)).join("")}</span>`;
}
function pageLibraryHome() {
  const tile = (id, name, list, wide) => `<button class="ltile ${wide ? "wide" : ""}" data-ltile="${id}">${mosaic(list)}<span class="ln1">${esc(name)}</span><span class="ln2">${list.length} songs</span></button>`;
  return `${bar({ back: true, title: "Library", tools: [{ label: "New tag", icon: "plus", act: "New tag" }] })}
    <div class="body">
      <div class="lgrid">
        ${tile("all", "All songs", LIB, true)}
        ${TAGS.map(({ t }) => tile(t, cap(t), LIB.filter(x => x.tags.includes(t)))).join("")}
      </div>
    </div>`;
}
let libBase = null;
function pageLibrary(id) {
  const tag = id.slice(3) === "all" ? null : id.slice(3);
  if (libBase !== id) { libBase = id; F.tags = new Set(tag ? [tag] : []); }
  const list = LIB.filter(matches), on = activeChips(), within = LIB.filter(x => !tag || x.tags.includes(tag));
  return `${bar({ back: true, title: tag ? cap(tag) : "All songs", tools: [{ label: "Filter", icon: "sliders", filter: true, badge: nFilters() || "" }] })}
    <div class="body">
      <label class="search lib">${ic("search", 16)}<input type="search" id="libq" placeholder="Songs and artists" value="${esc(F.q)}" aria-label="Find a song"></label>
      <div class="tagrow">${TAGS.filter(({ t }) => t !== tag).map(({ t }) => `<button class="tagchip" data-ltag="${t}" aria-pressed="${F.tags.has(t)}">#${t}</button>`).join("")}</div>
      ${on.length ? `<div class="onrow">${on.map(([k, l]) => `<button class="onchip" data-unset="${k}">${esc(l)}${ic("x", 13, 2.4)}</button>`).join("")}<button class="clearall" data-unset="all">Clear</button></div>` : ""}
      <div class="libcount">${list.length === within.length ? `${within.length} songs` : `${list.length} of ${within.length} songs`}</div>
      <div class="list">${list.map(x => `
        <button class="song lib" data-play="lib:${esc(id)}|${esc(x.title)}">
          ${songArt(x)}
          <span class="mid"><span class="t1">${esc(x.title)}</span><span class="s1">${esc([x.artist, x.tempo ? x.tempo + " bpm" : null].filter(Boolean).join(" · "))}</span></span>
          <span class="libr">${x.key ? `<span class="kc" style="--h:${hue(x.key.split("→")[0])}">${esc(x.key)}</span>` : ""}${bars(x.energy)}</span>
        </button>`).join("") || `<div class="empty">No song matches.<button data-unset="all">Clear the filters</button></div>`}</div>
    </div>`;
}
function pageTags() {
  return `${bar({ back: true, title: "Tags", tools: [{ label: "New tag", icon: "plus", act: "New tag" }] })}
    <div class="body">
      <div class="dayhead">Songs</div>
      <div class="list">${TAGS.map(({ t, n }) => `<button class="tagrowx" data-pick="${t}"><span class="hash">#</span><span class="t1">${t}</span><span class="n">${n}</span></button>`).join("")}</div>
      <div class="dayhead">Sets</div>
      <div class="list">${FOLDERS.map(f => `<button class="tagrowx" data-open="allsets"><span class="hash">#</span><span class="t1">${f.tag}</span><span class="n">${f.series.length + f.shows.length}</span></button>`).join("")}</div>
    </div>`;
}
// The filter sheet: key, energy, tempo, what it is ready for, rest.
function openFilters() {
  const sh = document.getElementById("sheet");
  const tog = (attr, v, on, label, style = "") => `<button class="fopt" data-${attr}="${v}" aria-pressed="${on}" ${style}>${label}</button>`;
  const n = LIB.filter(matches).length;
  sh.innerHTML = `<div class="bsheet" role="dialog" aria-label="Filter songs">
    <div class="grab"></div>
    <div class="bhead"><b>Filter</b><button data-unset="all">Reset</button></div>
    <div class="fsec">Key</div>
    <div class="keygrid">${[...FIFTHS, "?"].map(k => tog("fkey", k, F.keys.has(k), k === "?" ? "None" : k, k === "?" ? "" : `style="--h:${hue(k)}"`)).join("")}</div>
    <div class="fsec">Energy</div>
    <div class="seg">${[1, 2, 3, 4, 5].map(e => tog("fenergy", e, F.energy.has(e), bars(e))).join("")}</div>
    <div class="fsec">Tempo</div>
    <div class="seg">${TEMPI.map(([nm, lo, hi]) => tog("ftempo", nm, F.tempo.has(nm), `${nm}<small>${hi > 900 ? lo + "+" : lo ? lo + "–" + hi : "under " + hi}</small>`)).join("")}</div>
    <div class="fsec">Has</div>
    <div class="seg">${READY.map(([k, nm]) => tog("fready", k, F.ready.has(k), nm)).join("")}</div>
    <div class="fsec">Rotation</div>
    <div class="seg">${tog("frested", 1, F.rested, "Not played in 4 weeks")}</div>
    <button class="primary wide" data-close>Show ${n} song${n === 1 ? "" : "s"}</button>
  </div>`;
  sh.classList.add("bottom"); sh.hidden = false;
}
// ── Studio: records, their review ──────────────────────────────────────────
// Versions and comments on everything a record has: its mixes, charts and
// lyrics. Who said what, and the versions, are placeholders.
const PEOPLE = { Cody: 30, Jake: 200, Mia: 320, Sam: 140 };
const REVIEW = [
  { id: "r1", track: "Unbreakable", kind: "audio", what: "Mix", ver: 4, who: "Cody", when: "2h", comments: [
    { at: .38, t: "1:42", who: "Jake", text: "Guitars eat the vocal in the pre-chorus", open: true },
    { at: .66, t: "2:58", who: "Mia", text: "The delay throw here is perfect", open: false },
    { at: .81, t: "3:37", who: "Sam", text: "Ride the bass up into the last chorus", open: true }] },
  { id: "r2", track: "Chained Expectations", kind: "chart", what: "Chart", ver: 2, who: "Mia", when: "5h", bar: "Bridge · bar 3", chords: ["Em", "C", "G", "D/F#"], hot: 4, comments: [
    { who: "Mia", text: "Is this D/F# or Bm? The demo walks the bass down.", open: true }] },
  { id: "r3", track: "Heavify", kind: "audio", what: "Edit", ver: 2, who: "Cody", when: "Yesterday", comments: [
    { at: .14, t: "0:37", who: "Sam", text: "Flam on the snare here", open: true }] },
  { id: "r4", track: "The Ballad", kind: "lyrics", what: "Lyrics", ver: 3, who: "Jake", when: "Yesterday", line: "Verse 2 · line 3", comments: [
    { who: "Jake", text: "Can we go back to the demo's line here?", open: true },
    { who: "Cody", text: "Trying both in tomorrow's session", open: true }] },
  { id: "r5", track: "Intro", kind: "audio", what: "Master", ver: 2, who: "Cody", when: "Mon", approved: true, comments: [
    { at: .5, t: "0:58", who: "Jake", text: "Approved", open: false }] },
];
// The songs in production, the last worked on first. A record is optional:
// a song may be on one (Crescendum) or be a single.
// Every song in production: each record's, and the singles (a single is a
// record of one).
const WHEN = { Unbreakable: "2h", "Chained Expectations": "5h", Heavify: "Yesterday", "The Ballad": "Yesterday", Intro: "Mon", "Set in Stone": "Last week", "It Knows My Name": "Nov 10", Kornesque: "Nov 8", "The Plague": "Nov 6", "Empty Focus": "Oct 30" };
const SINGLES = [
  { title: "Devour", artist: "Beautifully Violent", stage: 3, when: "Jul 6", hue: 330 },
  { title: "Fire", artist: "JT Wright", stage: 1, when: "Mon", hue: 40 },
  { title: "Drive To LA", artist: "Nights Out", stage: 4, when: "Last week", hue: 10 },
];
const STUDIO = [
  ...Object.entries(ALBUMS).flatMap(([rec, a]) => a.tracks.map((t, k) => ({ id: "t:" + t.title, title: t.title, rec, when: WHEN[t.title] || a.when, artist: a.artist, stage: t.stage, no: k + 1, hue: a.hue }))),
  ...SINGLES.map(x => ({ ...x, id: "t:" + x.title, rec: null, no: 0 })),
];
const studioOf = id => STUDIO.find(x => x.id === id);
// Every track's mix versions, newest last, each with its timestamped
// comments. The ones in REVIEW are kept; the rest are placeholders.
const lenOf = title => 170 + ([...title].reduce((a, c) => a + c.charCodeAt(0), 0) % 160);
const VERS = {};
function versionsOf(title) {
  if (VERS[title]) return VERS[title];
  const x = STUDIO.find(y => y.title === title), stage = x ? x.stage : 2;
  const what = stage >= 4 ? "Master" : stage >= 3 ? "Mix" : stage >= 2 ? "Edit" : "Rough";
  const known = REVIEW.find(r => r.track === title && r.kind === "audio");
  const n = known ? known.ver : Math.max(1, Math.min(4, stage));
  const len = lenOf(title);
  const vs = Array.from({ length: n }, (_, k) => ({ v: k + 1, what: known ? known.what : what, who: k % 2 ? "Jake" : "Cody", when: k === n - 1 ? (known ? known.when : (x ? x.when : "Mon")) : `${(n - 1 - k) * 4 + 3} days ago`, len, comments: [] }));
  if (known) vs[n - 1].comments = known.comments.map(c => ({ ...c }));
  if (n > 1) vs[n - 2].comments = [{ at: .22, t: dur(len * .22), who: "Mia", text: "Vocal a bit loud in verse 1", open: false }];
  vs.at(-1).approved = !!(known && known.approved);
  return (VERS[title] = vs);
}
const latest = title => versionsOf(title).at(-1);
const openOn = title => versionsOf(title).reduce((t, v) => t + v.comments.filter(c => c.open).length, 0);
const openCount = r => r.comments.filter(c => c.open).length;
const trackReview = title => REVIEW.filter(r => r.track === title);
// Generated art: a blurred light in the cover's colour, placed and turned by
// its title, so no two look alike. A word mark (CYA, JHM) only where one is
// asked for; never a lone letter.
const cover = (a, size, h = a.hue ?? 285, mark = "") => {
  const seed = [...(a.title || "?")].reduce((n, c) => (n * 33 + c.charCodeAt(0)) % 9973, 5);
  const x = 10 + seed % 55, y = 5 + (seed >> 3) % 50, r = seed % 360, s2 = 55 + seed % 35;
  return `<span class="cover" style="width:${size}px;height:${size}px;--h:${h};--x:${x}%;--y:${y}%;--r:${r}deg;--s:${s2}%"><i></i><u></u></span>`;
};
const avatar = who => `<span class="av" style="--h:${PEOPLE[who] ?? 0}">${who[0]}</span>`;
// A waveform, the same for the same seed.
function bigWave(seed, v, pos) {
  let x = seed * 9301 + 49297; const r = () => (x = (x * 9301 + 49297) % 233280) / 233280;
  const n = 72, bars = Array.from({ length: n }, (_, k) => 0.2 + 0.8 * Math.abs(Math.sin(k / 6 + seed)) * (0.5 + 0.5 * r()));
  return `<div class="bwave" data-scrub>
    ${bars.map((h, k) => `<i class="${k / n < pos ? "pl" : ""}" style="height:${Math.round(h * 100)}%"></i>`).join("")}
    <span class="ph" style="left:${pos * 100}%"></span>
    ${v.comments.map((c, k) => `<button class="bpin ${c.open ? "" : "gone"}" style="left:${c.at * 100}%;--h:${PEOPLE[c.who] ?? 0}" data-seek="${c.at}" aria-label="${c.who} at ${c.t}">${c.who[0]}</button>`).join("")}
  </div>`;
}
function wave(seed, n = 64, pins = []) {
  let x = seed * 9301 + 49297; const r = () => (x = (x * 9301 + 49297) % 233280) / 233280;
  const bars = Array.from({ length: n }, (_, k) => 0.25 + 0.75 * Math.abs(Math.sin(k / 5 + seed)) * (0.55 + 0.45 * r()));
  return `<span class="wave">${bars.map(h => `<i style="height:${Math.round(h * 100)}%"></i>`).join("")}${pins.map(c => `<b class="pin ${c.open ? "" : "done"}" style="left:${c.at * 100}%;--h:${PEOPLE[c.who]}"></b>`).join("")}</span>`;
}
// Real cover art where there is some; a generated one otherwise.
const ART = { jt: "covers/jt.png", show: "covers/jt.png" };
// The ministries' own logos, for their weekly sets (JHM Sunday is JHM's).
const LOGO = { main: "covers/main.png", cya: "covers/cya.png", hsm: "covers/hsm.png", jhm: "covers/jhm.png", jhms: "covers/jhm.png" };
// A date as a calendar leaf: the month on a band, the day big, the weekday
// under it. A count and a length as small facts beside it.
const calTile = (mon, day, wd) => `<span class="cal"><span class="cm">${mon}</span><span class="cd">${day}</span>${wd ? `<span class="cw">${wd}</span>` : ""}</span>`;
const fact = (icon, text) => `<span class="fact2">${ic(icon, 14, 2)}${text}</span>`;
function hero({ img, h, mark, kicker, title, sub, buttons, facts }) {
  return `<div class="tintbg" style="--h:${h}"></div>
    <div class="hero2">
      <div class="art">${img ? `<img src="${img}" alt="">` : cover({ title }, 0, h, mark)}</div>
      <div class="htxt">
        ${facts ? `<div class="hfacts">${facts}</div>` : ""}
        <div class="hbottom">
          ${kicker ? `<div class="hk">${kicker}</div>` : ""}
          <div class="ht">${esc(title)}</div>
          ${sub ? `<div class="hs">${sub}</div>` : ""}
        </div>
      </div>
    </div>`;
}
const playShare = (play, share = "Share link copied") => `<button class="primary" data-act="${esc(play)}">${ic("play", 16)}Play</button><button class="secondary" data-act="${esc(share)}">${ic("link", 17)}Share</button>`;
// A song's art: its artist's, generated where there is none (JT's own have his cover).
const strHue = t => [...(t || "?")].reduce((h, c) => (h * 31 + c.charCodeAt(0)) % 360, 7);
const songArt = x => OWN.includes(x.artist) ? `<span class="sart"><img src="covers/jt.png" alt=""></span>`
  : `<span class="sart" ${x.artist ? `data-art="${esc(x.artist)}|${esc(x.title)}"` : ""}>${cover({ title: x.title }, 44, strHue(x.artist || x.title))}</span>`;
// ── Reviewing a track: one view, in a sheet from the album or as the track's page
let RV = { title: null, v: null, pos: 0.18, playing: false, draft: "" };
function reviewBody(title, inSheet) {
  const vs = versionsOf(title), v = vs.find(y => y.v === RV.v) || vs.at(-1), x = STUDIO.find(y => y.title === title);
  const cs = [...v.comments].sort((a, b) => a.at - b.at);
  return `
    <div class="vpick">${vs.map(y => `<button class="vchip ${y === v ? "on" : ""}" data-ver="${y.v}">${y.what} v${y.v}${y.comments.some(c => c.open) ? "<i></i>" : ""}</button>`).join("")}
      ${vs.length > 1 ? `<button class="vchip ab" data-act="A/B: ${v.what} v${v.v} against v${v.v - 1}">A/B</button>` : ""}</div>
    <div class="vmeta">${avatar(v.who)}<span>${v.who} · ${esc(v.when)}</span>${v.approved ? `<span class="appr">${ic("check", 12, 2.6)}Approved</span>` : x ? stageTag(x.stage, x.id) : ""}</div>
    ${bigWave(title.length, v, RV.pos)}
    <div class="rtrans">
      <span class="rt">${dur(v.len * RV.pos)}</span>
      <button class="rplay" data-rplay aria-label="${RV.playing ? "Pause" : "Play"}">${ic(RV.playing ? "pause" : "play", 22, 2)}</button>
      <span class="rt">${dur(v.len)}</span>
    </div>
    <div class="dayhead">${cs.length ? `${cs.length} comment${cs.length > 1 ? "s" : ""}${cs.some(c => c.open) ? ` · ${cs.filter(c => c.open).length} open` : ""}` : "No comments on this version"}</div>
    <div class="clist">${cs.map(c => `
      <div class="cm2 ${c.open ? "" : "resolved"}">
        <button class="tstamp" data-seek="${c.at}">${c.t}</button>
        <span class="mid"><span class="s1"><b>${c.who}</b></span><span class="tx">${esc(c.text)}</span></span>
        ${c.open ? `<button class="rsv" aria-label="Resolve" data-resolve="${esc(title)}|${v.v}|${c.at}">${ic("check", 14, 2.4)}</button>` : `<span class="rsvd">${ic("check", 13, 2.4)}</span>`}
      </div>`).join("")}</div>
    <form class="composer" data-compose="${esc(title)}|${v.v}">
      <span class="at">${dur(v.len * RV.pos)}</span>
      <input placeholder="Comment at ${dur(v.len * RV.pos)}…" value="${esc(RV.draft)}" aria-label="Comment">
      <button class="send" aria-label="Post">${ic("arrowUp", 16, 2.4)}</button>
    </form>
    ${inSheet ? `<button class="secondary wide" data-open="t:${esc(title)}" data-close style="margin-top:12px">Open the track</button>` : ""}`;
}
function openRev(title) {
  if (RV.title !== title) RV = { title, v: null, pos: 0.18, playing: false, draft: "" };
  const x = STUDIO.find(y => y.title === title), sh = document.getElementById("sheet");
  sh.innerHTML = `<div class="bsheet review" role="dialog" aria-label="Review ${esc(title)}"><div class="grab"></div>
    <div class="bhead"><b>${esc(title)}</b><span class="bsub">${esc(x ? (x.rec ? ALBUMS[x.rec].title + " · track " + x.no : "Single · " + x.artist) : "")}</span></div>
    ${reviewBody(title, true)}</div>`;
  sh.classList.add("bottom"); sh.hidden = false;
}
const redrawReview = () => { const t = RV.title; if (!document.getElementById("sheet").hidden && document.querySelector(".bsheet.review")) openRev(t); else render(); };

// ── Choosing a song's arrangement and key in a set ─────────────────────────
function pickRef(ref) { const [id, i] = ref.split("|"), o = occOf(id); return { o, i: +i, x: o.songs[+i] }; }
function openArr(ref) {
  const { o, i, x } = pickRef(ref), cur = arrOf(o, i), sh = document.getElementById("sheet");
  sh.innerHTML = `<div class="bsheet" role="dialog" aria-label="Arrangement"><div class="grab"></div>
    <div class="bhead"><b>${esc(x.title)}</b><span class="bsub">Arrangement</span></div>
    ${ARRS.map(([a, dt]) => `<button class="mi big" data-setarr="${ref}|${a}"><span class="ic">${ic("branch", 18)}</span><span class="mid"><span class="t1">${a}</span><span class="s1">${dur((x.len || 270) + dt)}${dt ? ` · ${dt > 0 ? "+" : "−"}${dur(Math.abs(dt))}` : " · as written"}</span></span>${a === cur ? `<span class="ck">${ic("check", 17, 2.2)}</span>` : ""}</button>`).join("")}
  </div>`;
  sh.classList.add("bottom"); sh.hidden = false;
}
function openKey(ref) {
  const { o, i, x } = pickRef(ref), cur = keyIn(o, i, x), orig = x.key, sh = document.getElementById("sheet");
  sh.innerHTML = `<div class="bsheet" role="dialog" aria-label="Key"><div class="grab"></div>
    <div class="bhead"><b>${esc(x.title)}</b><span class="bsub">Key · original ${esc(orig)}</span></div>
    <div class="keypick">${FIFTHS.map(k => { const d = steps(orig, k); return `<button class="kp ${k === cur ? "on" : ""} ${k === orig ? "orig" : ""}" style="--h:${hue(k)}" data-setkey="${ref}|${k}"><b>${k}</b><small>${k === orig ? "Original" : `${d > 0 ? "+" : "−"}${Math.abs(d)}`}</small></button>`; }).join("")}</div>
    ${cur !== orig ? `<button class="secondary wide" data-setkey="${ref}|${orig}" style="margin-top:14px">Back to ${esc(orig)}, the original</button>` : ""}
  </div>`;
  sh.classList.add("bottom"); sh.hidden = false;
}
// The chart page keeps a sheet's proportions: full width when the height
// allows, else as tall as there is room for (Safari's bars take some).
function fitSheet() {
  const w = document.querySelector(".sheetwrap"), pg = w && w.querySelector(".sheetpage");
  if (!pg) return;
  const W = w.clientWidth, H = w.clientHeight, width = Math.min(W, H * 8.5 / 11);
  pg.style.width = width + "px"; pg.style.height = width * 11 / 8.5 + "px";
}
addEventListener("resize", fitSheet);
// ── Now Playing (architect_playhead's bar): above the tabs while something
// is queued; a tap opens the player.
let NP = null;
function queueOf(ctx) {
  const [kind, id] = ctx.split(":");
  if (kind === "occ") return { list: occOf(id).songs, from: `${occOf(id).series.name} · ${short(occOf(id).date)}` };
  if (kind === "set") return { list: songs(SETS[id]), from: SETS[id].title };
  return { list: LIB.filter(matches), from: "Library" };
}
function play(ref) {
  const [ctx, title] = ref.split("|"), q = queueOf(ctx);
  NP = { ...q, i: Math.max(0, q.list.findIndex(x => x.title === title)), playing: true, pos: 0.08 };
  drawNP();
}
const npSong = () => NP && NP.list[NP.i];
function drawNP() {
  const el = document.getElementById("np"), x = npSong();
  if (!x) { el.hidden = true; return; }
  el.hidden = false;
  el.innerHTML = `<button class="npmain" data-player>${songArt(x)}<span class="mid"><span class="t1">${esc(x.title)}</span><span class="s1">${esc(x.artist || NP.from)}</span></span></button>
    <button class="npb" data-np="toggle" aria-label="${NP.playing ? "Pause" : "Play"}">${ic(NP.playing ? "pause" : "play", 20, 2)}</button>
    <button class="npb" data-np="next" aria-label="Next">${ic("skip", 20, 2)}</button>
    <span class="npprog"><i style="width:${NP.pos * 100}%"></i></span>`;
  hydrateArt();
  fitSheet();
}
function openPlayer() {
  const x = npSong(), sh = document.getElementById("sheet"), len = x.len || 240;
  sh.innerHTML = `<div class="bsheet player" role="dialog" aria-label="Now playing"><div class="grab"></div>
    <div class="pfrom">Playing from <b>${esc(NP.from)}</b></div>
    <div class="partwork">${songArt(x)}</div>
    <div class="ptitle">${esc(x.title)}</div><div class="partist">${esc(x.artist || "")}${x.key ? ` · <span class="kc" style="--h:${hue(x.key.split("→")[0])}">${esc(x.key)}</span>` : ""}${x.tempo ? ` · ${x.tempo} bpm` : ""}</div>
    <div class="pscrub"><i style="width:${NP.pos * 100}%"></i><b style="left:${NP.pos * 100}%"></b></div>
    <div class="ptimes"><span>${dur(len * NP.pos)}</span><span>-${dur(len * (1 - NP.pos))}</span></div>
    <div class="ptrans">
      <button data-np="prev" aria-label="Previous">${ic("skipBack", 30, 2)}</button>
      <button class="pbig" data-np="toggle" aria-label="${NP.playing ? "Pause" : "Play"}">${ic(NP.playing ? "pause" : "play", 34, 2)}</button>
      <button data-np="next" aria-label="Next">${ic("skip", 30, 2)}</button>
    </div>
    ${NP.list.length > NP.i + 1 ? `<div class="dayhead">Up next</div>${NP.list.slice(NP.i + 1, NP.i + 5).map(y => `<div class="upn">${songArt(y)}<span class="mid"><span class="t1">${esc(y.title)}</span><span class="s1">${esc(y.artist || "")}</span></span>${y.key ? `<span class="kc" style="--h:${hue(y.key.split("→")[0])}">${esc(y.key)}</span>` : ""}</div>`).join("")}` : ""}
  </div>`;
  sh.classList.add("bottom"); sh.hidden = false; hydrateArt();
}
// The real artwork, looked up live (the iTunes Search API) and kept in the
// browser; the generated art stays where nothing matches.
const ARTS = JSON.parse(localStorage.getItem("arts") || "{}");
function hydrateArt() {
  document.querySelectorAll("[data-art]").forEach(async el => {
    const key = el.dataset.art, [artist, title] = key.split("|");
    if (!(key in ARTS)) {
      ARTS[key] = null;
      try {
        const r = await fetch(`https://itunes.apple.com/search?entity=song&limit=5&term=${encodeURIComponent(artist + " " + title)}`).then(r => r.json());
        const hit = r.results.find(x => x.artistName.toLowerCase().includes(artist.toLowerCase().split(/[ &]/)[0]));
        ARTS[key] = hit ? hit.artworkUrl100.replace("100x100", "200x200") : null;
        localStorage.setItem("arts", JSON.stringify(ARTS));
      } catch { delete ARTS[key]; return; }
    }
    if (ARTS[key]) document.querySelectorAll(`[data-art="${CSS.escape(key)}"]`).forEach(e => { e.innerHTML = `<img src="${ARTS[key]}" alt="">`; e.removeAttribute("data-art"); });
  });
}
// Who leads a song in a set: its lead vocal and lead guitar, each optional.
// Placeholders: the band's names are made up, Cody on guitar.
const LEADS = { "cya@2026-11-19": [["Mia", null], ["Jake", "Cody"], ["Mia", "Cody"], ["Sam", null]] };
function leadsOf(o, i, x) {
  if (LEADS[o.id]) return LEADS[o.id][i] || [null, null];
  const v = ["Mia", "Jake", "Sam"][(i + o.date.getUTCDate()) % 3];
  return [v, x.tempo > 100 || i % 2 ? "Cody" : null];
}
// Where each weekly event meets: the room, the time (placeholders).
// Where each weekly event meets, when the band is called, when it starts.
// Main's room is Cody's; the rest, and every time, are placeholders.
const PLACE = { main: ["Worship Center", "10:30", "8:30"], jhms: ["Room 210", "9:00", "7:45"], jhm: ["Room 210", "7:00", "6:00"], hsm: ["The Loft", "7:00", "5:45"], cya: ["Chapel", "7:30", "6:15"] };
// A week's set is two sections: the Top Set's two songs, the Bottom Set's rest.
const SECTIONS_OF = o => { const n = Math.max(o.series.size, o.songs.length); return [["Top Set", 0, 2], ["Bottom Set", 2, n]]; };
// The arrangement a set plays a song in: Normal, or a named one (placeholders).
const ARR = { "cya@2026-11-19": ["Normal", "Big Intro", "Shortened", "Normal"] };
// What a set changes on a song: its arrangement and its key. Anyone who can
// edit the set picks them; the song's own key is its original.
const CAN_EDIT = true;
let editing = false;
const OV = { "main@2026-11-22|0": { key: "C" }, "main@2026-11-22|1": { key: "E" } };
const ARRS = [["Normal", 0], ["Shortened", -70], ["Big Intro", 25], ["Acoustic", -10], ["Extended", 45]];
const SEMI = { C: 0, "C#": 1, Db: 1, D: 2, "D#": 3, Eb: 3, E: 4, F: 5, "F#": 6, Gb: 6, G: 7, "G#": 8, Ab: 8, A: 9, "A#": 10, Bb: 10, B: 11 };
const steps = (from, to) => ((SEMI[to] - SEMI[from] + 18) % 12) - 6;
const keyIn = (o, i, x) => OV[`${o.id}|${i}`]?.key || x.key;
// The key chip: the original plain; a moved one outlined, with how far.
function keyChip(orig, cur, ref) {
  const d = orig && cur && orig !== cur ? steps(orig.split("→")[0], cur) : 0;
  const tag = ref && CAN_EDIT ? `button data-keypick="${ref}"` : "span";
  // The key big in its colour; when the set moved it, how far and from what.
  return `<${tag} class="key2 ${d ? "moved" : ""}" style="--h:${hue(cur.split("→")[0])}" aria-label="Key ${esc(cur)}${d ? `, ${Math.abs(d)} ${d > 0 ? "up" : "down"} from ${esc(orig)}` : ", the original key"}">
    <b>${esc(cur)}</b>${d ? `<small>${d > 0 ? "+" : "−"}${Math.abs(d)}</small>` : ""}</${tag.split(" ")[0]}>`;
}
const arrOf = (o, i) => OV[`${o.id}|${i}`]?.arr || (ARR[o.id] || [])[i] || ["Normal", "Big Intro", "Normal", "Shortened", "Acoustic"][(i + o.date.getUTCDate()) % 5];
const arrChip = (a, ref) => ref && CAN_EDIT ? `<span class="arr ghost ${a === "Normal" ? "" : "set"}" role="button" data-arrpick="${ref}" aria-label="Arrangement: ${esc(a)}">${ic("branch", 12, 2.2)}</span>` : a === "Normal" ? "" : `<span class="arr ${a === "Normal" ? "arrnorm" : ""}">${ic("branch", 12, 2.2)}${esc(a)}</span>`;
const ME = "Cody";
const leadsText = (o, i, x) => {
  const [v, g] = leadsOf(o, i, x);
  const one = (who, role) => who === ME ? `<span class="ld me" style="--h:${(rigOf(role === "mic" ? "Keys" : "Guitar") || {}).hue ?? 30}">${ic(role, 13, 2)}You</span>` : `<span class="ld">${ic(role, 13, 2)}${who}</span>`;
  return `${v ? one(v, "mic") : ""}${g ? one(g, "guitar") : ""}`;
};
// The leads as faces before the key: an initial, a mic or a guitar on it.
const PEOPLE_HUE = { Mia: 320, Jake: 200, Sam: 140, Cody: 30 };
function leadsFaces(o, i, x) {
  const [v, g] = leadsOf(o, i, x);
  const face = (who, role) => `<span class="face" style="--h:${PEOPLE_HUE[who] ?? 0}" title="${who}, lead ${role === "mic" ? "vocal" : "guitar"}">${who[0]}<i>${ic(role, 9, 2.6)}</i></span>`;
  return `<span class="faces" aria-label="${[v && `${v} leads vocals`, g && `${g} leads guitar`].filter(Boolean).join(", ") || "No leads set"}">${v ? face(v, "mic") : ""}${g ? face(g, "guitar") : ""}</span>`;
}
function leadsLine(o, i, x) {
  const [v, g] = leadsOf(o, i, x);
  if (!v && !g) return `<span class="s1">${esc(x.artist)}</span>`;
  return `<span class="s1 leads">${v ? `<span class="ld">${ic("mic", 13, 2)}${v}</span>` : ""}${g ? `<span class="ld">${ic("guitar", 13, 2)}${g}</span>` : ""}</span>`;
}
const dots = what => `<button class="dots3" aria-label="More" data-act="${esc(what)}">${ic("more", 18)}</button>`;
let sfilter = "All";
const stageTag = (st, target) => `<${target ? `button data-status="${esc(target)}"` : "span"} class="stgtag" style="--h:${STAGES[st][1] ?? 0}" data-none="${STAGES[st][1] == null}">${STAGES[st][0]}</${target ? "button" : "span"}>`;
const statusBar = st => `<span class="sbar">${STAGES.map(([, h], k) => `<i class="${k < st ? "past" : k === st ? "now" : ""}" style="--h:${h ?? 0}"></i>`).join("")}</span>`;
// Change a track's status, or a record's.
function openStatus(target) {
  const cur = target.startsWith("rec:") ? ALBUMS[target.slice(4)].status : studioOf(target).stage;
  const name = target.startsWith("rec:") ? ALBUMS[target.slice(4)].title : studioOf(target).title;
  const sh = document.getElementById("sheet");
  sh.innerHTML = `<div class="bsheet" role="dialog" aria-label="Status"><div class="grab"></div>
    <div class="bhead"><b>${esc(name)}</b></div>
    ${STAGES.map(([nm, h], k) => `<button class="mi big" data-setstatus="${esc(target)}|${k}"><span class="sdot" style="--h:${h ?? 0}" data-none="${h == null}"></span>${nm}${k === cur ? `<span class="ck">${ic("check", 17, 2.2)}</span>` : ""}</button>`).join("")}
  </div>`;
  sh.classList.add("bottom"); sh.hidden = false;
}
function setStatus(target, k) {
  if (target.startsWith("rec:")) ALBUMS[target.slice(4)].status = k;
  else { const x = studioOf(target); x.stage = k; const a = x.rec && ALBUMS[x.rec]; if (a) a.tracks[x.no - 1].stage = k; }
}
function songRow(x) {
  const rv = trackReview(x.title), n = rv.reduce((t, r) => t + openCount(r), 0), v = rv.find(r => r.kind === "audio");
  return `<div class="srow song2">
    <button class="mid" data-open="${x.id}"><span class="t1">${esc(x.title)}</span><span class="s1">${esc(x.rec ? ALBUMS[x.rec].title : "Single")}${v ? ` · ${v.what} v${v.ver}` : ""} · ${esc(x.when)}</span></button>
    ${n ? `<span class="bubble">${ic("message", 13, 2)}${n}</span>` : ""}
    ${stageTag(x.stage, x.id)}
  </div>`;
}
// Records: one list, albums and singles alike, with folders among them.
const projOpen = list => list.reduce((t, x) => t + trackReview(x.title).reduce((u, r) => u + openCount(r), 0), 0);
const inFolder = new Set(RFOLDERS.flatMap(f => f.items));
function recRow(id) {
  const a = ALBUMS[id], x = !a && studioOf(id);
  const list = a ? STUDIO.filter(y => y.rec === id) : [x], n = projOpen(list);
  const what = a ? `${list.length} songs` : org ? `Single · ${esc(x.artist)}` : esc(x.artist);
  return `<div class="pjrow2">
    <button class="pjopen" data-open="${id}">${cover(a || x, 56, (a || x).hue)}<span class="mid"><span class="t1">${esc(a ? a.title : x.title)}</span><span class="s1">${org ? "" : `<span class="orgtag">${esc(orgName(OWNER[id]))}</span>`}${what}${n ? ` · <b class="openn">${n} open</b>` : ""}</span></span></button>
    ${stageTag(a ? a.status : x.stage, a ? "rec:" + id : id)}
  </div>`;
}
// The org switcher's button: what is shown, all orgs or one.
const orgPill = () => `<button class="orgpill" data-orgs aria-label="Organization">${org ? `<span class="omark" style="--h:${ORGS.find(o => o[0] === org)[2]}">${orgName(org)[0]}</span>${esc(orgName(org))}` : `${ic("layers", 16)}All orgs`}${ic("updown", 14)}</button>`;
const RECENT_WORK = ["cr", "t:Fire", "t:Drive To LA", "mtl", "t:Devour", "png", "joi"];
// Records: every record as its cover, where it is and what it waits on.
// Home has the review queue; this is the shelf.
// ── Setlists: the folders as a filter; the weekly events as their logos,
// each ringed by how ready its next set is; the shows as their covers.
let sfold = "all";
function ring(o, size) {
  const n = o ? o.songs.length : 0, of = o ? o.series.size : 1, f = Math.min(1, n / of), r = size / 2 - 2, c = 2 * Math.PI * r;
  const col = f >= 1 ? "var(--ok)" : f > 0 ? "var(--live)" : "#34343b";
  return `<svg class="ringsvg" width="${size}" height="${size}" viewBox="0 0 ${size} ${size}"><circle cx="${size / 2}" cy="${size / 2}" r="${r}" fill="none" stroke="#24242a" stroke-width="3"/><circle cx="${size / 2}" cy="${size / 2}" r="${r}" fill="none" stroke="${col}" stroke-width="3" stroke-linecap="round" stroke-dasharray="${c * f} ${c}" transform="rotate(-90 ${size / 2} ${size / 2})"/></svg>`;
}
function pageSetlistsTab() {
  const fs = sfold === "all" ? FOLDERS : FOLDERS.filter(f => f.tag === sfold);
  const series = fs.flatMap(f => f.series), shows = fs.flatMap(f => f.shows.map(id => ({ id, f })));
  const showDate = st => (st.date.match(/(Jan|Feb|Mar|Apr|May|Jun|Jul|Aug|Sep|Oct|Nov|Dec)\w* (\d{4})/) || [, "", ""]);
  return `${bar({ title: "Setlists", tools: [{ joined: [{ label: "Filter", icon: "sliders", menu: "sfold", badge: sfold === "all" ? "" : "1" }, { label: "Library", icon: "library", open: "library" }] }, { label: "New setlist", icon: "plus", menu: "newset" }] })}
    <div class="body">
      ${sfold === "all" ? "" : `<div class="onrow"><button class="onchip" data-sfold="all">${esc(FOLDERS.find(f => f.tag === sfold).name)}${ic("x", 13, 2.4)}</button></div>`}
      ${fs.filter(f => f.series.length).map(f => `<div class="dayhead">${esc(f.name)} · Recurring</div>
      <div class="evgrid">${f.series.map(id => { const sr = seriesOf(id), nx = nextOf(sr); return `
        <button class="ev" data-open="${sr.id}">
          <span class="evlogo">${ring(nx, 84)}${LOGO[id] ? `<img src="${LOGO[id]}" alt="">` : cover({ title: sr.name }, 64, sr.hue)}</span>
          <span class="t1">${esc(sr.name)}</span>
          <span class="s1">${nx ? (daysFrom(nx.date) === 0 ? "Today" : daysFrom(nx.date) < 7 ? DAY[nx.date.getUTCDay()].slice(0, 3) : short(nx.date)) : ""}${nx && nx.songs.length < sr.size ? ` · <b>${nx.songs.length}/${sr.size}</b>` : ""}</span>
        </button>`; }).join("")}</div>`).join("")}
      ${[["Upcoming shows", shows.filter(x => x.id === "mb")], ["Past shows", shows.filter(x => x.id !== "mb")]].filter(([, l]) => l.length).map(([head, l]) => `<div class="dayhead">${head}</div>
      <div class="showlist ${head.startsWith("Past") ? "past" : ""}">${l.map(({ id, f }) => { const st = SETS[id], [m, y] = ({ mb: ["Dec", "2026"] })[id] || showDate(st).slice(1); return `
        <button class="showc" data-open="${id}">
          <span class="showart">${ART[id] ? `<img src="${ART[id]}" alt="">` : cover({ title: st.title }, 0, id === "mb" ? 150 : 20)}</span>
          <span class="mid"><span class="t1">${esc(st.title)}</span><span class="s1">${esc(f.name)} · ${songs(st).length} songs${total(st) ? " · " + dur(total(st)) : ""}</span></span>
          <span class="cal"><span class="cm">${m.slice(0, 3)}</span><span class="cd">${y}</span></span>
        </button>`; }).join("")}</div>`).join("")}
    </div>`;
}

// ── Records: each record as the work it is — its tracks by status, what
// waits on review, when it is due.
const DUE = { mtl: "Jun 15", cr: "Jan 30", png: "Feb 14" };
function recRow2(id) {
  const a = ALBUMS[id], x = !a && studioOf(id), list = a ? STUDIO.filter(y => y.rec === id).sort((p, q) => p.no - q.no) : [x];
  const n = projOpen(list), st = a ? a.status : x.stage, v = !a && latest(x.title);
  const counts = STAGES.map((sg, k) => [sg, list.filter(y => y.stage === k).length]).filter(([, c]) => c);
  return `<button class="rrow" data-open="${id}">
    <span class="rtop">
      ${ART[id] ? `<img class="rart" src="${ART[id]}" alt="">` : `<span class="rart">${cover(a || x, 52, (a || x).hue)}</span>`}
      <span class="mid"><span class="t1">${esc(a ? a.title : x.title)}</span><span class="s1">${esc(a ? a.artist : x.artist)} · ${a ? `${list.length} songs` : `Single · ${v.what} v${v.v}`}</span></span>
      ${stageTag(st)}
    </span>
    ${a ? `<span class="rbar">${list.map(y => `<i style="--h:${STAGES[y.stage][1] ?? 0}" data-none="${STAGES[y.stage][1] == null}" title="${esc(y.title)}: ${STAGES[y.stage][0]}"></i>`).join("")}</span>
    <span class="rlegend">${counts.map(([[nm, h], c]) => `<span><i style="--h:${h ?? 0}" data-none="${h == null}"></i>${c} ${nm.toLowerCase()}</span>`).join("")}</span>` : ""}
    ${n || DUE[id] ? `<span class="rmeta">${n ? `<span class="ropen">${ic("message", 12, 2.2)}${n} open</span>` : ""}${DUE[id] ? `<span class="rdue">${ic("calendar", 12, 2)}Due ${DUE[id]}</span>` : ""}</span>` : ""}
  </button>`;
}
// Records: the newest versions to listen to first, each playable where it
// sits (Samply's feed); then the records themselves, a shelf.
let RPLAY = null;
function pageSessions() {
  const ids = RECENT_WORK.filter(id => !org || OWNER[id] === org);
  // One newest version from each record in turn, so the feed is not one album.
  const byRec = ids.map(id => STUDIO.filter(x => (x.rec || x.id) === id));
  const feed = []; for (let k = 0; feed.length < 5 && k < 12; k++) byRec.forEach(l => { if (l[k] && feed.length < 5) feed.push({ x: l[k], v: latest(l[k].title) }); });
  return `${bar({ title: "Records", tools: [{ label: "New record", icon: "plus", act: "New album, EP or single" }] })}
    <div class="body">
      <div class="dayhead">Records</div>
      <div class="rshelf">${ids.map(id => { const a = ALBUMS[id], x = !a && studioOf(id); return `
        <button class="rsh" data-open="${id}">
          <span class="rshc">${ART[id] ? `<img src="${ART[id]}" alt="">` : cover(a || x, 0, (a || x).hue)}</span>
          <span class="t1">${esc(a ? a.title : x.title)}</span>
          <span class="s1">${a ? `${a.tracks.length} songs` : "Single"}</span>
          ${stageTag(a ? a.status : x.stage)}
        </button>`; }).join("")}</div>
      <div class="dayhead">Latest versions</div>
      <div class="feed">${feed.map(({ x, v }) => { const n = v.comments.filter(c => c.open).length, on = RPLAY === x.title; return `
        <div class="fd ${on ? "on" : ""}">
          <button class="fdplay" data-fplay="${esc(x.title)}" aria-label="${on ? "Pause" : "Play"} ${esc(x.title)}">${ic(on ? "pause" : "play", 16, 2)}</button>
          <button class="fdmid" data-rev="${esc(x.title)}">
            <span class="fdtop"><span class="t1">${esc(x.title)}</span><span class="fdv">${v.what} v${v.v}</span></span>
            ${wave(x.title.length, 48, v.comments)}
            <span class="s1">${esc(x.rec ? ALBUMS[x.rec].title : "Single")} · ${esc(v.who)} · ${esc(v.when)}${n ? ` · <b class="openn">${n} open</b>` : ""}</span>
          </button>
        </div>`; }).join("")}</div>
    </div>`;
}
// Pick the org: all of them, or one (Architect's workspace switcher).
function openOrgs() {
  const sh = document.getElementById("sheet");
  const n = id => RECENT_WORK.filter(r => !id || OWNER[r] === id).length;
  sh.innerHTML = `<div class="bsheet" role="dialog" aria-label="Organization"><div class="grab"></div>
    <div class="bhead"><b>Organization</b></div>
    <button class="mi big" data-org=""><span class="omark all">${ic("layers", 15)}</span>All organizations<span class="nt">${n(null)}</span>${org ? "" : `<span class="ck">${ic("check", 17, 2.2)}</span>`}</button>
    <div class="md"></div>
    ${ORGS.map(([id, name, h]) => `<button class="mi big" data-org="${id}"><span class="omark" style="--h:${h}">${name[0]}</span>${esc(name)}<span class="nt">${n(id) || ""}</span>${org === id ? `<span class="ck">${ic("check", 17, 2.2)}</span>` : ""}</button>`).join("")}
  </div>`;
  sh.classList.add("bottom"); sh.hidden = false;
}
function pageRFolder(id) {
  const f = RFOLDERS.find(x => x.id === id);
  return `${bar({ back: true, title: f.name, tools: [{ label: "New record", icon: "plus", act: `New record in ${f.name}` }] })}
    <div class="body"><div class="list">${f.items.map(recRow).join("")}</div></div>`;
}
// Inside a song: its Song tab. The stage it is at, the version to hear,
// what is open on it, and its record if it has one.
function pageSong(id) {
  const x = studioOf(id), a = x.rec && ALBUMS[x.rec];
  if (RV.title !== x.title) RV = { title: x.title, v: null, pos: 0.18, playing: false, draft: "" };
  return `${bar({ back: true, tools: [{ joined: [{ label: "Open in Session", icon: "open", act: `Opened ${x.title} in Session` }, { label: "More", icon: "more", menu: "project" }] }] })}
    <div class="body">
      <div class="trkhead">${a ? cover(a, 56, a.hue) : cover(x, 56, x.hue)}<span class="mid"><span class="hk">${esc(a ? `${a.title} · track ${x.no}` : "Single")}</span><span class="t">${esc(x.title)}</span><span class="s1">${esc(x.artist)}</span></span></div>
      ${reviewBody(x.title, false)}
    </div>`;
}
// Inside a record, its Album tab.
function pageAlbum(id) {
  const a = ALBUMS[id], list = STUDIO.filter(x => x.rec === id).sort((p, q) => p.no - q.no);
  const total = list.reduce((t, x) => t + latest(x.title).len, 0), open = list.reduce((t, x) => t + openOn(x.title), 0);
  return `${bar({ back: true, tools: [{ joined: [{ label: "Download", icon: "download", act: "Saved for offline" }, { label: "More", icon: "more", menu: "project" }] }] })}
    <div class="body">
      ${hero({ img: ART[id], h: a.hue ?? 285, title: a.title, kicker: `${esc(a.what)}`,
        sub: `${esc(a.artist)} · ${stageTag(a.status, "rec:" + id)}<span class="hcount">${list.length} tracks · ${dur(total)}${open ? ` · <span class="openn">${open} open</span>` : ""}</span>` })}
      <div class="scard">${list.map(x => { const v = latest(x.title), n = openOn(x.title); return `
        <div class="trow roomy rec">
          <span class="tno">${x.no}</span>
          <button class="mid" data-rev="${esc(x.title)}">
            <span class="t1">${esc(x.title)}</span>
            <span class="s1">${esc(v.who)} · ${esc(v.when)}</span>
            <span class="meta3">${n ? `<span class="ld cmt">${ic("message", 12, 2)}${n} open</span>` : `<span class="ld">${dur(v.len)}</span>`}</span>
          </button>
          <span class="rcol"><button class="vtag" data-rev="${esc(x.title)}">${v.what} v${v.v}</button>${stageTag(x.stage, x.id)}</span>
        </div>`; }).join("")}</div>
    </div>`;
}
// Inside a record, its Review tab: every version and comment, newest first.
let rfilter = "All", rtrack = null;
function pageReview(id) {
  const song = studioOf(id); if (song) rtrack = null;
  const a = song ? { title: song.title } : ALBUMS[id];
  const list = REVIEW.filter(r => (!song || r.track === song.title) && (!rtrack || r.track === rtrack) && (rfilter === "All" || (rfilter === "Open" ? openCount(r) : r.kind === rfilter.toLowerCase())));
  const card = r => `
    <div class="rv">
      <div class="rvhead">${avatar(r.who)}<span class="mid"><span class="t1">${esc(r.track)}</span><span class="s1">${r.who} · ${r.what} v${r.ver} · ${r.when}</span></span>
        ${r.approved ? `<span class="appr">${ic("check", 13, 2.6)}Approved</span>` : `<span class="vers">${Array.from({ length: r.ver }, (_, k) => `<i class="${k === r.ver - 1 ? "on" : ""}">v${k + 1}</i>`).join("")}</span>`}</div>
      ${r.kind === "audio" ? `<div class="rvwave"><button class="pl" aria-label="Play" data-act="Playing ${esc(r.track)} ${r.what} v${r.ver}">${ic("play", 14)}</button>${wave(r.track.length, 60, r.comments)}</div>`
        : r.kind === "chart" ? `<div class="rvchart"><span class="where">${esc(r.bar)}</span><div class="bars4">${r.chords.map((c, k) => `<span class="${k === r.hot - 1 ? "hot" : ""}">${esc(c)}</span>`).join("")}</div></div>`
        : `<div class="rvlyr"><span class="where">${esc(r.line)}</span><span class="ln"></span><span class="ln hot"></span><span class="ln short"></span></div>`}
      <div class="cms">${r.comments.map(c => `
        <div class="cm ${c.open ? "" : "resolved"}">${avatar(c.who)}<span class="mid"><span class="s1"><b>${c.who}</b>${c.t ? ` <span class="ts">${c.t}</span>` : ""}</span><span class="tx">${esc(c.text)}</span></span>
          ${c.open ? `<button class="rsv" aria-label="Resolve" data-act="Resolved">${ic("check", 14, 2.4)}</button>` : ""}</div>`).join("")}</div>
      <button class="reply" data-act="Reply to ${esc(r.track)}">Reply${r.kind === "audio" ? " at the playhead" : ""}…</button>
    </div>`;
  return `${bar({ back: true, title: rtrack || a.title, tools: [{ label: "Compare versions", icon: "compare", act: "A/B the last two versions" }] })}
    <div class="body">
      <div class="tagrow rvchips">${(rtrack ? [`× ${rtrack}`] : []).concat(["All", "Open", "Audio", "Chart", "Lyrics"]).map(f => f.startsWith("×") ? `<button class="tagchip on" data-rtrack="">${esc(f)}</button>` : `<button class="tagchip" data-rfilter="${f}" aria-pressed="${rfilter === f}">${f}</button>`).join("")}</div>
      ${list.map(card).join("") || `<div class="empty">Nothing here yet.</div>`}
    </div>`;
}

// ── Live: a set's chart and lyrics, a song at a time ───────────────────────
const SCALE = { C: "C D E F G A B", G: "G A B C D E F#", D: "D E F# G A B C#", A: "A B C# D E F# G#", E: "E F# G# A B C# D#", B: "B C# D# E F# G# A#", "F#": "F# G# A# B C# D# E#", Db: "Db Eb F Gb Ab Bb C", Ab: "Ab Bb C Db Eb F G", Eb: "Eb F G Ab Bb C D", Bb: "Bb C D Eb F G A", F: "F G A Bb C D E" };
const chord = (key, deg) => { const sc = (SCALE[ALIAS[key] || key] || SCALE.C).split(" "); return sc[deg - 1] + ([2, 3, 6].includes(deg) ? "m" : ""); };
// A worship-shaped chart in the song's key (placeholder chords).
const FORM = [["Intro", [1, 5, 6, 4]], ["Verse 1", [1, 1, 4, 4, 6, 5, 4, 4]], ["Chorus", [4, 1, 5, 6, 4, 1, 5, 5]], ["Verse 2", [1, 1, 4, 4, 6, 5, 4, 4]], ["Chorus", [4, 1, 5, 6, 4, 1, 5, 5]], ["Bridge", [6, 4, 1, 5, 6, 4, 1, 5]], ["Chorus", [4, 1, 5, 6, 4, 1, 5, 5]]];
let songIx = 0;
const setSongs = id => occOf(id) ? occOf(id).songs : SETS[PROJECTS.find(p => p.id === id).set].acts.flatMap(a => a[1]);
// In Chart and Lyrics the song on show is the one playing: one line names
// it, the progress bar (an existing component) sits under it, and the
// transport at the foot moves through sections and songs.
const SECS = ["Intro", "Verse 1", "Chorus", "Verse 2", "Chorus", "Bridge", "Chorus", "Outro"];
// Each section's length in bars, and its colour by kind (a placeholder form).
const BARS = [4, 8, 8, 8, 8, 8, 8, 4];
const SEC_HUE = { Intro: 250, Verse: 200, Chorus: 330, Bridge: 60, Outro: 250 };
const secHue = n => SEC_HUE[n.split(" ")[0]] ?? 0;
let barIx = 3; // the bar playing inside the section
// The song's sections side by side, each as long as it lasts, the ones played
// full, the one playing filling; under it the section's bars, one tick each.
function progressBars() {
  const total = BARS.reduce((a, b) => a + b, 0);
  const song = SECS.map((n, k) => { const fill = k < secIx ? 100 : k === secIx ? (barIx + 0.5) / BARS[k] * 100 : 0;
    return `<span class="pseg ${k === secIx ? "now" : ""}" style="flex:${BARS[k]};--h:${secHue(n)}"><i style="width:${fill}%"></i><b>${esc(n.replace("Verse ", "V").replace("Chorus", "C").replace("Bridge", "B").replace("Intro", "In").replace("Outro", "Out"))}</b></span>`; }).join("");
  const n = BARS[secIx];
  const bars = Array.from({ length: n }, (_, k) => `<span class="pbar ${k < barIx ? "played" : k === barIx ? "now" : ""}"></span>`).join("");
  return `<div class="prog" aria-label="Song progress">${song}</div>
    <div class="secbar" aria-label="${SECS[secIx]}, bar ${barIx + 1} of ${n}" style="--h:${secHue(SECS[secIx])}"><span class="sname">${SECS[secIx]}</span><span class="pbars">${bars}</span><span class="scount">${barIx + 1}/${n}</span></div>`;
}
let secIx = 1, livePlaying = false;
function songHead(id) {
  const ss = setSongs(id), x = ss[songIx] || ss[0];
  return `<div class="nowline"><button class="back sm" data-back aria-label="Back">${ic("chevLeft", 22, 2.2)}</button><span class="nn">${songIx + 1}/${ss.length}</span><span class="t1">${esc(x.title)}</span><span class="nk">${esc([x.key, x.tempo ? x.tempo + " bpm" : null].filter(Boolean).join(" · "))}</span></div>
    ${progressBars()}`;
}
// The transport: the click, the previous section, play, the next section, loop.
let clickOn = false, loopOn = false;
function transport() {
  const b = (k, icon, label, on, big) => `<button class="tp ${big ? "big" : ""} ${on ? "on" : ""}" data-tp="${k}" aria-label="${label}" aria-pressed="${!!on}">${ic(icon, big ? 22 : 20, 2)}</button>`;
  return `${b("click", "metronome", "Click", clickOn)}${b("sec-1", "chevLeft", "Previous section")}
    ${b("play", livePlaying ? "pause" : "play", livePlaying ? "Pause" : "Play", false, true)}
    ${b("sec1", "chevRight", "Next section")}${b("loop", "loop", loopOn ? `Looping ${SECS[secIx]}` : "Loop the section", loopOn)}`;
}
function liveTitle(id) { return occOf(id) ? `${occOf(id).series.name} · ${short(occOf(id).date)}` : titleOf(PROJECTS.find(p => p.id === id)); }
function pageChart(id) {
  // The page fills the middle, letterboxed on black; above it the song and
  // the progress bar, below it the transport and the tabs.
  return `
    <div class="body sheetview">${songHead(id)}
      <div class="sheetwrap"><div class="sheetpage"><b>Chart</b><span>The chart page (keyflow) fills this page, as wide as the phone</span></div></div>
    </div>`;
}
function pageLyrics(id) {
  // The page fills the middle, letterboxed on black; above it the song and
  // the progress bar, below it the transport and the tabs.
  return `
    <div class="body sheetview">${songHead(id)}
      <div class="sheetwrap"><div class="sheetpage"><b>Lyrics</b><span>The lyrics page fills this page, as wide as the phone</span></div></div>
    </div>`;
}
const PAGES = { home: pageProjects, sessions: pageSessions, setlists: pageSetlistsTab, signal: pageRigs };
const TABS = [["home", "Home", "home"], ["sessions", "Records", "disc"], ["setlists", "Setlists", "setlist"], ["signal", "Signal", "rig"]];

// ── The sidebar (the left drawer) ───────────────────────────────────────────
function drawer() {
  const meter = [1,1,1,1,1,1,0,0,0,0,0,0].map(o => `<i class="${o ? "on" : ""}"></i>`).join("");
  return `
    <div class="scroll">
      <button class="switcher" data-orgs>
        ${org ? `<span class="mark" style="background:oklch(.45 .1 ${ORGS.find(o => o[0] === org)[2]})">${orgName(org)[0]}</span>` : `<span class="mark">${ic("layers", 16)}</span>`}<span class="nm"><b>${esc(org ? orgName(org) : "All organizations")}</b><span>${org ? "Organization" : `${ORGS.length} organizations`}</span></span>
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
  sfold: () => [["head", "Show"], ["check", "All folders", sfold === "all"], ...FOLDERS.map(f => ["check", f.name, sfold === f.tag])],
  // A new set: the next week of an event, or a one-off show.
  newset: () => [["head", "Next week of"], ...SERIES.map(sr => { const nx = OCC.find(o => o.series === sr && daysFrom(o.date) >= 0 && !o.songs.length); return ["item", sr.name, "calendar", nx ? short(nx.date) : "", false, `New ${sr.name} set${nx ? " for " + short(nx.date) : ""}`]; }),
    ["div"], ["item", "A show", "plus", "named, one date", false, "New show"], ["item", "A new weekly event", "loop", "", false, "New weekly event"]],
  mine: () => [["head", "Open in your rig"], ...mySets().map(o => ["item", `${occName(o)}`, rigOf(MINE[o.id]).icon, `${when(o.date)} · ${MINE[o.id]}`, false, `Opened ${occName(o)}, ${short(o.date)} in the ${MINE[o.id]} rig`]), ["div"], ["item", "Tuner", "tuner"]],
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
  m.innerHTML = MENUS[name]().map(([k, a, b, c, d, says]) => k === "div" ? '<div class="md"></div>'
    : k === "head" ? `<div class="mh">${a}</div>`
    : k === "check" ? `<button class="mi" data-act="${a}"><span class="ic"></span>${a}${b ? `<span class="ck">${ic("check", 17, 2.2)}</span>` : ""}</button>`
    : `<button class="mi" data-act="${esc(says || a)}" ${d ? 'style="color:#f87171"' : ""}><span class="ic" ${d ? 'style="color:#f87171"' : ""}>${ic(b, 18)}</span>${a}${c ? `<span class="nt">${c}</span>` : ""}</button>`).join("");
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
const closePalette = () => { const sh = document.getElementById("sheet"); sh.hidden = true; sh.classList.remove("bottom"); };

// ── Navigation ──────────────────────────────────────────────────────────────
let tab = "home", stack = [];
const stage = document.getElementById("stage");
const known = id => id.startsWith("lt:") || ALBUMS[id] || RFOLDERS.some(f => f.id === id) || studioOf(id) || id === "library" || id === "allsets" || id === "tags" || seriesOf(id) || occOf(id) || PROJECTS.find(p => p.id === id);
const pageFor = id => id.startsWith("lt:") ? pageLibrary(id) : RFOLDERS.some(f => f.id === id) ? pageRFolder(id) : studioOf(id) ? pageSong(id) : id === "library" ? pageLibraryHome() : id === "allsets" ? pageAllSets() : id === "tags" ? pageTags() : seriesOf(id) ? pageSeries(id) : occOf(id) ? pageOcc(id) : ALBUMS[id] ? pageAlbum(id) : pageProject(id);
// Where a page lives when it is opened from outside a tab's own pages.
const tabFor = id => { if (studioOf(id) || id.startsWith("f:") || ALBUMS[id]) return "sessions"; const p = PROJECTS.find(x => x.id === id); return p ? (p.kind === "Set" ? "setlists" : p.kind === "Song" ? "home" : "sessions") : "setlists"; };
// The note the bottom slots follow: a set (live) or a record (studio).
const noteMode = id => studioOf(id) ? "song" : ALBUMS[id] ? "studio" : (occOf(id) || PROJECTS.find(p => p.id === id && p.kind === "Set")) ? "live" : null;
const modeNote = () => [...stack].reverse().find(noteMode);
const FAV = {
  live: { tabs: [["setlist", "Setlist", "setlist"], ["chart", "Chart", "chart"], ["lyrics", "Lyrics", "type"], ["more", "More", "more"]], primary: ["Play the set", "play"] },
  studio: { tabs: [["album", "Album", "disc"], ["review", "Review", "message"]], primary: ["Comment at the playhead", "messagePlus"] },
  song: { tabs: [["song", "Song", "audio"], ["review", "Review", "message"]], primary: ["Comment at the playhead", "messagePlus"] },
};
let view = null;
function topPage(id) {
  const m = noteMode(id);
  if (m === "live" && view === "chart") return pageChart(id);
  if (m === "live" && view === "lyrics") return pageLyrics(id);
  if ((m === "studio" || m === "song") && view === "review") return pageReview(id);
  return pageFor(id);
}
function render() {
  const top = stack.at(-1), under = stack.at(-2);
  stage.innerHTML = `<section class="page" id="root">${under ? pageFor(under) : PAGES[tab]()}</section>` + (top ? `<section class="page" id="top">${topPage(top)}</section>` : "");
  if (top) stage.querySelector("#root").classList.add("under");
  // A cover's tint sits behind the whole page, its bar too.
  stage.querySelectorAll(".page").forEach(pg => { const t = pg.querySelector(".body > .tintbg"); if (t) pg.prepend(t); });
  hydrateArt();
  const note = modeNote(), m = note && noteMode(note), fav = m && FAV[m];
  const cur = fav ? (view || fav.tabs[0][0]) : tab;
  document.getElementById("tabs").innerHTML = (fav ? fav.tabs : TABS).map(([id, label, icon]) => `<button class="tab ${fav ? "fav" : ""}" ${fav ? `data-view="${id}"` : `data-tab="${id}"`} aria-label="${label}" aria-current="${cur === id ? "page" : "false"}">${ic(icon, 22)}${fav ? `<span>${label}</span>` : ""}</button>`).join("");
  const songView = m === "live" && (view === "chart" || view === "lyrics");
  // A track is its own player: no Now Playing bar over its review.
  document.getElementById("phone").classList.toggle("trackview", m === "song");
  const tpEl = document.getElementById("transport");
  tpEl.hidden = !songView; if (songView) tpEl.innerHTML = transport();
  document.getElementById("phone").classList.toggle("songview", songView);
  const round = document.querySelector(".round");
  const rig = m === "live" && rigOf(MINE[note]);
  const prim = rig ? [`Open in ${rig.name}`, rig.icon] : fav && fav.primary;
  round.outerHTML = fav ? `<button class="round prim" ${rig ? `style="--h:${rig.hue}" data-rig` : ""} data-act="${prim[0]}" aria-label="${prim[0]}">${ic(prim[1], 22, 1.9)}</button>` : `<button class="round" data-palette aria-label="Open anything">${ic("search", 22, 1.9)}</button>`;
  document.getElementById("drawer").innerHTML = drawer();
  history.replaceState(null, "", "#/" + [tab, ...stack].join("/") + (view ? "~" + view : "") + (side ? "!side" : ""));
}
function setView(v) {
  const note = modeNote(); if (!note) return;
  if (v === "more") return openMore();
  while (stack.at(-1) !== note) stack.pop();
  view = ["setlist", "album", "song"].includes(v) ? null : v;
  if (v !== "review") rtrack = null;
  stage.scrollTop = 0; render();
}
function openMore() {
  const sh = document.getElementById("sheet");
  sh.innerHTML = `<div class="bsheet" role="dialog" aria-label="More"><div class="grab"></div>
    ${[["Arrangement", "branch"], ["Mixer", "sliders"], ["Lighting Vis", "bulb"], ["Lighting Cue List", "cues"], ["Notes", "message"]].map(([l, i]) => `<button class="mi big" data-act="${l}" data-close>${ic(i, 20)}${l}</button>`).join("")}
  </div>`;
  sh.classList.add("bottom"); sh.hidden = false;
}
function showTab(t) { editing = false; tab = t; stack = []; view = null; setSide(false); render(); }
function push(id) {
  closePalette();
  if (!known(id)) return;
  if (side || tab === "signal") { setSide(false); stack = []; tab = tabFor(id); }
  if (noteMode(id) && id !== modeNote()) { view = null; songIx = 0; rtrack = null; }
  stack.push(id); render();
  const top = stage.querySelector("#top"), root = stage.querySelector("#root");
  root.classList.remove("under"); top.classList.add("enter");
  requestAnimationFrame(() => requestAnimationFrame(() => { top.classList.remove("enter"); root.classList.add("under"); }));
}
function pop() {
  if (!stack.length) return;
  const top = stage.querySelector("#top"), root = stage.querySelector("#root");
  root.classList.remove("under"); top.classList.add("enter");
  setTimeout(() => { stack.pop(); editing = false; if (!modeNote()) view = null; render(); }, 280);
}
let side = false;
function setSide(open) { side = open; document.getElementById("phone").classList.toggle("side-open", open); history.replaceState(null, "", location.hash.replace("!side", "") + (open ? "!side" : "")); }


// The Library's filters: a change redraws the page under the sheet, and the sheet.
const refilter = () => { render(); if (!document.getElementById("sheet").hidden && document.querySelector(".bsheet")) openFilters(); };
const flip = (set, v) => set.has(v) ? set.delete(v) : set.add(v);
document.addEventListener("click", e => {
  const el = sel => e.target.closest(sel);
  if (el("[data-edit]")) { editing = !editing; return render(); }
  if (el("[data-arrpick]")) return openArr(el("[data-arrpick]").dataset.arrpick);
  if (el("[data-keypick]")) return openKey(el("[data-keypick]").dataset.keypick);
  if (el("[data-setarr]")) { const [id, i, a] = el("[data-setarr]").dataset.setarr.split("|"); (OV[`${id}|${i}`] ||= {}).arr = a; closePalette(); return render(); }
  if (el("[data-setkey]")) { const [id, i, k] = el("[data-setkey]").dataset.setkey.split("|"); (OV[`${id}|${i}`] ||= {}).key = k; closePalette(); return render(); }
  if (el("[data-book]")) { BK = null; return openBook(); }
  if (el("[data-bk]")) { const [k, v] = el("[data-bk]").dataset.bk.split("|"); BK[k] = k === "day" ? +v : v; return openBook(); }
  if (el("[data-bkgo]")) { REH.push({ id: "reh" + (REH.length + 1), set: BK.set, date: new Date(TODAY.getTime() + BK.day * 864e5), time: BK.time, room: BK.room }); closePalette(); toast("Rehearsal booked"); return render(); }
  if (el("[data-rev]")) return openRev(el("[data-rev]").dataset.rev);
  if (el("[data-ver]")) { RV.v = +el("[data-ver]").dataset.ver; return redrawReview(); }
  if (el("[data-seek]")) { RV.pos = +el("[data-seek]").dataset.seek; return redrawReview(); }
  if (el("[data-rplay]")) { RV.playing = !RV.playing; return redrawReview(); }
  if (el("[data-resolve]")) { const [t, vv, at] = el("[data-resolve]").dataset.resolve.split("|"); const c = versionsOf(t).find(y => y.v === +vv).comments.find(c => c.at === +at); if (c) c.open = false; return redrawReview(); }
  if (el("[data-scrub]") && !el("[data-seek]")) { const w = el("[data-scrub]").getBoundingClientRect(); RV.pos = Math.max(0, Math.min(1, (e.clientX - w.left) / w.width)); return redrawReview(); }
  if (el("[data-sfold]")) { sfold = el("[data-sfold]").dataset.sfold; return render(); }
  if (el("[data-fplay]")) { const t = el("[data-fplay]").dataset.fplay; RPLAY = RPLAY === t ? null : t; return render(); }
  if (el("[data-rfil]")) { rfil = el("[data-rfil]").dataset.rfil; return render(); }
  if (el("[data-filters]")) return openFilters();
  if (el("[data-play]")) return play(el("[data-play]").dataset.play);
  if (el("[data-player]")) return openPlayer();
  if (el("[data-np]")) { const k = el("[data-np]").dataset.np;
    if (k === "toggle") NP.playing = !NP.playing;
    if (k === "next" && NP.i < NP.list.length - 1) { NP.i++; NP.pos = 0; }
    if (k === "prev") { if (NP.pos > 0.05 || !NP.i) NP.pos = 0; else NP.i--; }
    drawNP(); if (!document.getElementById("sheet").hidden && document.querySelector(".player")) openPlayer(); return; }
  if (el("[data-ltile]")) return push("lt:" + el("[data-ltile]").dataset.ltile);
  if (el("[data-tp]")) { const k = el("[data-tp]").dataset.tp, n = setSongs(modeNote()).length;
    if (k === "play") livePlaying = !livePlaying;
    if (k === "click") { clickOn = !clickOn; return render(); }
    if (k === "loop") { loopOn = !loopOn; return render(); }
    if (k === "song1" && songIx < n - 1) { songIx++; secIx = 0; }
    if (k === "song-1" && songIx) { songIx--; secIx = 0; }
    if (k.startsWith("sec") || k.startsWith("song")) barIx = 0;
    if (k === "sec1") { if (secIx < SECS.length - 1) secIx++; else if (songIx < n - 1) { songIx++; secIx = 0; } }
    if (k === "sec-1") { if (secIx) secIx--; else if (songIx) { songIx--; secIx = SECS.length - 1; } }
    return render(); }
  if (el("[data-song]")) { const d = +el("[data-song]").dataset.song, n = setSongs(modeNote()).length; songIx = Math.max(0, Math.min(n - 1, songIx + d)); return render(); }
  if (el("[data-rfilter]")) { rfilter = el("[data-rfilter]").dataset.rfilter; return render(); }
  if (el("[data-rtrack]")) { rtrack = null; return render(); }
  if (el("[data-track]")) { rtrack = el("[data-track]").dataset.track; rfilter = "All"; return setView("review"); }
  if (el("[data-orgs]")) { setSide(false); return openOrgs(); }
  if (el("[data-org]")) { org = el("[data-org]").dataset.org || null; closePalette(); return render(); }
  if (el("[data-setstatus]")) { const [tg, k] = el("[data-setstatus]").dataset.setstatus.split("|"); setStatus(tg, +k); closePalette(); return render(); }
  if (el("[data-status]")) { e.stopPropagation(); return openStatus(el("[data-status]").dataset.status); }
  if (el("[data-sfilter]")) { sfilter = el("[data-sfilter]").dataset.sfilter; return render(); }
  if (el("[data-view]")) return setView(el("[data-view]").dataset.view);
  if (el("[data-ltag]")) { flip(F.tags, el("[data-ltag]").dataset.ltag); return refilter(); }
  if (el("[data-pick]")) { F.tags = new Set([el("[data-pick]").dataset.pick]); stack.pop(); return render(); }
  if (el("[data-fkey]")) { flip(F.keys, el("[data-fkey]").dataset.fkey); return refilter(); }
  if (el("[data-fenergy]")) { flip(F.energy, +el("[data-fenergy]").dataset.fenergy); return refilter(); }
  if (el("[data-ftempo]")) { flip(F.tempo, el("[data-ftempo]").dataset.ftempo); return refilter(); }
  if (el("[data-fready]")) { flip(F.ready, el("[data-fready]").dataset.fready); return refilter(); }
  if (el("[data-frested]")) { F.rested = !F.rested; return refilter(); }
  if (el("[data-unset]")) { const k = el("[data-unset]").dataset.unset;
    if (k === "all") { F.keys.clear(); F.energy.clear(); F.tempo.clear(); F.ready.clear(); F.rested = false; F.tags = new Set(libBase && libBase !== "lt:all" ? [libBase.slice(3)] : []); }
    else if (k === "rested") F.rested = false; else F[k].clear();
    return refilter(); }
  if (el("[data-fold]")) { flip(folded, el("[data-fold]").dataset.fold); return render(); }
  if (el("[data-scope]")) { scope = el("[data-scope]").dataset.scope; return render(); }
  if (el("[data-menu]")) { e.stopPropagation(); return openMenu(el("[data-menu]").dataset.menu, el("[data-menu]")); }
  if (el(".menu .mi")) { const a = el(".mi").dataset.act, f = a === "All folders" ? "all" : FOLDERS.find(x => x.name === a)?.tag; closeMenu(); if (f) { sfold = f; return render(); } return toast(a); }
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

// Posting a comment at the playhead.
document.addEventListener("submit", e => {
  const f = e.target.closest("[data-compose]"); if (!f) return; e.preventDefault();
  const text = f.querySelector("input").value.trim(); if (!text) return;
  const [t, vv] = f.dataset.compose.split("|"), v = versionsOf(t).find(y => y.v === +vv);
  v.comments.push({ at: RV.pos, t: dur(v.len * RV.pos), who: "Cody", text, open: true });
  RV.draft = ""; redrawReview();
});
document.addEventListener("input", e => {
  if (e.target.id !== "libq") return;
  F.q = e.target.value.toLowerCase(); const at = e.target.selectionStart; render();
  const q = document.getElementById("libq"); q.focus(); q.setSelectionRange(at, at);
});

// A swipe in from the left edge goes back; on a top page it brings the sidebar. A swipe left puts the sidebar away.
let sx = null, sy = null;
addEventListener("touchstart", e => { const t = e.touches[0], ph = document.getElementById("phone").getBoundingClientRect(); sx = (side || t.clientX - ph.left < 24) ? t.clientX : null; sy = t.clientY; }, { passive: true });
addEventListener("touchmove", e => { if (sx == null) return; const t = e.touches[0], dx = t.clientX - sx; if (Math.abs(t.clientY - sy) > Math.abs(dx)) { sx = null; return; } if (!side && dx > 50) { if (stack.length) pop(); else setSide(true); sx = null; } else if (side && dx < -50) { setSide(false); sx = null; } }, { passive: true });

// #/projects, #/projects/jt, #/rigs, …; "!side" with the sidebar out.
const H0 = decodeURIComponent(location.hash);
const h = (location.hash || "#/home").replace("#/projects", "#/home").replace("!filters", "").replace("!player", "").replace(/!(key|arr)=[^!]+/, "").replace("!edit", "").replace("!book", "").replace(/!rev=[^!]+/, ""), wantSide = h.includes("!side"), wantFilters = location.hash.includes("!filters"), wantPlayer = location.hash.includes("!player");
const [, first, ...rest] = h.replace("!side", "").split("~")[0].split("/");
view = h.replace("!side", "").split("~")[1] || null;
tab = PAGES[first] ? first : "home";
stack = rest.filter(known);
render();
if (wantSide) setSide(true);
if (wantFilters) openFilters();
NP = { ...queueOf("occ:cya@2026-11-19"), i: 0, playing: false, pos: 0.35 }; drawNP();
if (wantPlayer) openPlayer();
const rq = H0.match(/!rev=([^!]+)/); if (rq) openRev(rq[1]);
if (H0.includes("!book")) openBook();
if (H0.includes("!edit")) { editing = true; render(); }
const pk = H0.match(/!(key|arr)=([^!]+)/); if (pk) (pk[1] === "key" ? openKey : openArr)(pk[2]);
setInterval(() => { if (RV.playing) { RV.pos = Math.min(1, RV.pos + 0.006); const ph = document.querySelector(".bwave .ph"); if (ph) { ph.style.left = RV.pos * 100 + "%"; document.querySelectorAll(".bwave i").forEach((b, k, all) => b.classList.toggle("pl", k / all.length < RV.pos)); } } }, 1000);
// The playhead moves while playing.
setInterval(() => { if (NP && NP.playing) { NP.pos = Math.min(1, NP.pos + 0.004); const b = document.querySelector(".npprog i"); if (b) b.style.width = NP.pos * 100 + "%"; } }, 1000);
// In a browser tab Safari's own toolbar sits under the page, so the home
// indicator's inset is not ours to keep clear: the tabs sit low.
if (!(navigator.standalone || matchMedia("(display-mode: standalone)").matches)) document.documentElement.classList.add("browser");
