// What the mockups share: the icons, the recital's running order (titles,
// artists, keys and lengths as the setlist's fence gives them), and the
// pieces drawn from data. Static: no network, no storage.
//
// Not from the vault: the covers (a song's tile shows its key on its
// colour, as the app does for a song with no cover), the sections of
// Drive To LA (a plausible form, to show the strip), every lyric line and
// the chart's bars (stand-ins). The vault is private.

const ICONS = {
  back: '<path d="M15 5l-7 7 7 7"/>',
  down: '<path d="M6 9l6 6 6-6"/>',
  play: '<path d="M7 4.5v15l13-7.5z"/>',
  pause: '<path d="M7 4.5h3.5v15H7zM13.5 4.5H17v15h-3.5z"/>',
  prev: '<path d="M18 5v14L8 12z"/><path d="M6 5v14" fill="none" stroke="currentColor" stroke-width="2"/>',
  next: '<path d="M6 5v14l10-7z"/><path d="M18 5v14" fill="none" stroke="currentColor" stroke-width="2"/>',
  list: '<path d="M4 6h10M4 12h10M4 18h7M18 8v8.5"/><circle cx="16" cy="17" r="2.2"/>',
  share: '<path d="M12 15V4M8 8l4-4 4 4M5 13v5a2 2 0 002 2h10a2 2 0 002-2v-5"/>',
  lock: '<rect x="6" y="11" width="12" height="9" rx="2"/><path d="M9 11V8a3 3 0 016 0v3"/>',
};
const SOLID = new Set(['play', 'pause', 'prev', 'next']);
const icon = (name) => `<svg class="i${SOLID.has(name) ? ' solid' : ''}" viewBox="0 0 24 24" aria-hidden="true">${ICONS[name]}</svg>`;

// JT's Senior Recital, as its setlist note lists it (the Archive left out:
// two songs that were not played; whether a guest sees them is the owner's).
const SET = [
  { act: 'Act I: “The Originals”', songs: [
    ['DTLA Cinematic Intro', 'Nights Out', 'C', 123.0, '#3d5a80'],
    ['Drive To LA', 'Nights Out', 'C→D♭', 196.5, '#2a6f97'],
    ['For You', 'Nights Out', 'A♭', 192.6, '#9c6644'],
    ["Heart Isn't Ready", 'Nights Out', 'B♭', 202.7, '#7b2cbf'],
    ['No One Knows', 'Queens of the Stone Age', '', 257.3, '#b23a48'],
    ['Hey!', 'Nights Out & Ryce', 'E', 251.0, '#588157'],
  ] },
  { act: 'Act II: “Royal Blood”', songs: [
    ['Sleeptalker', 'Royal Blood', '', 190.5, '#343a40'],
    ['Figure It Out', 'Royal Blood', '', 188.9, '#495057'],
    ['Seven Nation Army', 'The White Stripes', '', 231.4, '#9d0208'],
    ['Sad But True', 'Metallica', '', 327.2, '#212529'],
    ['Out Of The Black', 'Royal Blood', '', 327.0, '#3a3a3a'],
  ] },
  { act: 'Act III: “The Fun Part”', songs: [
    ["She's American", 'The 1975', 'D♭', 267.0, '#c9184a'],
    ['WORST PART OF YOU', 'bilmuri', 'B', 176.1, '#bc6c25'],
    ['Fire', 'JT Wright', 'D', 205.8, '#d00000'],
    ['Not Too Much Longer (Live)', 'Cody Wright', 'E', 260.5, '#1d3557'],
    ['Know Me (Extended)', 'The Band CAMINO', 'E', 304.1, '#6d597a'],
  ] },
];
const clock = (s) => `${Math.floor(s / 60)}:${String(Math.round(s % 60)).padStart(2, '0')}`;

// Drive To LA's form, for the strip: [name, colour token, bars].
const FORM = [
  ['Intro', 'intro', 4], ['Verse 1', 'verse', 16], ['Pre', 'pre', 8], ['Chorus', 'chorus', 16],
  ['Verse 2', 'verse', 16], ['Pre', 'pre', 8], ['Chorus', 'chorus', 16], ['Bridge', 'bridge', 12],
  ['Chorus in D♭', 'chorus', 24], ['Outro', 'outro', 11],
];
const NOW = 3; // the first chorus is playing, a little under half way through it

function strip(kind = '', names = false) {
  const cells = FORM.map(([, c, w], i) => {
    const state = i < NOW ? ' class="done"' : i === NOW ? ' class="now" ' : '';
    return `<i${state} style="--w:${w};--c:var(--s-${c});--p:42%"></i>`;
  }).join('');
  const labels = names
    ? `<div class="strip-names">${FORM.map(([n, , w], i) => `<span${i === NOW ? ' class="now"' : ''} style="--w:${w}">${i === NOW || w >= 16 ? n : ''}</span>`).join('')}</div>`
    : '';
  return `<div class="strip ${kind}" role="slider" aria-label="Where the song is, by section" aria-valuetext="Chorus, 1:07 of 3:16">${cells}</div>${labels}`;
}

function rows(playing = 1) {
  let n = 0;
  return SET.map(({ act, songs }) => {
    const total = songs.reduce((a, s) => a + s[3], 0);
    const list = songs.map(([title, artist, key, len, colour]) => {
      const on = n === playing;
      n += 1;
      return `<li class="row${on ? ' on' : ''}">
        <span class="n num">${on ? '<span class="bars" aria-label="Playing"><i></i><i></i><i></i></span>' : n}</span>
        <span class="tile" style="--c:${colour}">${key}</span>
        <span class="t"><b>${title}</b><span>${artist}</span></span>
        <span class="len num">${clock(len)}</span>
      </li>`;
    }).join('');
    return `<div class="act"><h2>${act}</h2><span class="num">${songs.length} songs, ${Math.round(total / 60)} min</span></div><ul class="rows">${list}</ul>`;
  }).join('');
}

function dock(href = 'player.html') {
  return `<a class="dock" href="${href}" aria-label="Open the player: Drive To LA, Nights Out">
    <div class="in">
      <span class="tile" style="--c:#2a6f97">C→D♭</span>
      <span style="min-width:0"><b>Drive To LA</b><span class="sub">Chorus, 1:07 of 3:16</span></span>
      <span class="tap">${icon('pause')}</span>
      <span class="tap">${icon('next')}</span>
    </div>
    ${strip('thin')}
  </a>`;
}

function wave() {
  // A stereo waveform drawn from a fixed seed: louder in the choruses.
  let seed = 7;
  const rnd = () => { seed = (seed * 16807) % 2147483647; return seed / 2147483647; };
  const total = FORM.reduce((a, f) => a + f[2], 0);
  const loud = [];
  FORM.forEach(([, c, w]) => { for (let i = 0; i < w; i++) loud.push(c === 'chorus' ? 0.92 : c === 'intro' || c === 'outro' ? 0.4 : c === 'bridge' ? 0.55 : 0.68); });
  const bars = [];
  const n = 440;
  for (let i = 0; i < n; i++) {
    const l = loud[Math.floor((i / n) * total)];
    for (const [mid, amp] of [[26, 20], [70, 20]]) {
      const h = Math.max(1.5, amp * l * (0.55 + 0.45 * rnd()));
      bars.push(`<rect x="${(i * 2).toFixed(1)}" y="${(mid - h).toFixed(1)}" width="1.2" height="${(h * 2).toFixed(1)}"/>`);
    }
  }
  return `<svg viewBox="0 0 880 96" preserveAspectRatio="none" aria-hidden="true"><g fill="currentColor" opacity="0.72">${bars.join('')}</g></svg>`;
}

document.querySelectorAll('[data-icon]').forEach((e) => { e.innerHTML = icon(e.dataset.icon); });
document.querySelectorAll('[data-rows]').forEach((e) => { e.innerHTML = rows(); });
document.querySelectorAll('[data-strip]').forEach((e) => { e.innerHTML = strip(e.dataset.strip, e.hasAttribute('data-names')); });
document.querySelectorAll('[data-dock]').forEach((e) => { e.outerHTML = dock(e.dataset.dock || 'player.html'); });
document.querySelectorAll('[data-wave]').forEach((e) => { e.innerHTML = wave(); });
document.querySelectorAll('[data-secs]').forEach((e) => {
  e.innerHTML = FORM.map(([n, c, w]) => `<i style="--w:${w};--c:var(--s-${c})">${n}</i>`).join('');
});
