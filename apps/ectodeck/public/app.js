'use strict';
/* Z10 RGB editor. The server is the keyboard; this page keeps an unsaved draft
   per profile and renders an on-screen preview of the draft's lighting. */

// ---------------------------------------------------------------- data tables
// What is printed on each key. `top` is the shifted legend above the main one,
// `sub` the alternate function printed below, as on the keyboard itself.
// Each cap's legend. `main` is the key's own legend and `main2` a second line
// of the same words (PRINT / SCREEN). `top` is the shifted symbol, printed
// beside the main one, and `sub` is a second function (PF1, HOME), printed at
// the bottom of the cap.
const MUTE = Symbol('mute');   // F1's second function, printed as a muted speaker
const LEGEND = {
  ESC: 'ESC', PrintScreen: { main: 'PRINT', main2: 'SCREEN' }, ScrollLock: { main: 'SCROLL', main2: 'LOCK' },
  PauseBreak: { main: 'PAUSE', main2: 'BREAK' },
  GraveAccent: { top: '~', main: '`' },
  1: { top: '!', main: '1', sub: 'PF1' }, 2: { top: '@', main: '2', sub: 'PF2' },
  3: { top: '#', main: '3', sub: 'PF3' }, 4: { top: '$', main: '4', sub: 'PF4' },
  5: { top: '%', main: '5', sub: 'PF5' }, 6: { top: '^', main: '6' },
  7: { top: '&', main: '7' }, 8: { top: '*', main: '8' }, 9: { top: '(', main: '9' },
  0: { top: ')', main: '0' }, Minus: { top: '_', main: '-', sub: 'PF\u2212' }, Equals: { top: '+', main: '=', sub: 'PF+' },
  Backspace: 'BACKSPACE', Insert: 'INSERT', Home: 'HOME', PageUp: { main: 'PG', main2: 'UP' },
  Tab: 'TAB', LBracket: { top: '{', main: '[' }, RBracket: { top: '}', main: ']' },
  BackSlash: { top: '|', main: '\\' },
  Delete: 'DELETE', End: 'END', PageDown: { main: 'PG', main2: 'DN' },
  CapsLock: { main: 'CAPS', main2: 'LOCK' }, Semicolon: { top: ':', main: ';' },
  SingleQuotation: { top: '"', main: "'" }, Enter: 'ENTER',
  LSHIFT: 'SHIFT', RSHIFT: 'SHIFT', Comma: { top: '<', main: ',' },
  Period: { top: '>', main: '.' }, Slash: { top: '?', main: '/' },
  LCONTROL: 'CTRL', RCONTROL: 'CTRL', FN: 'FN', LALT: 'ALT', RALT: 'ALT',
  WIN: '\u229e', Menu: '\u2261', Space: '',
  ArrowUp: '\u25b2', ArrowDown: '\u25bc', ArrowLeft: '\u25c0', ArrowRight: '\u25b6',
  NumLock: { main: 'NUM', main2: 'LOCK' }, KeypadDivide: '/', KeypadMultiply: '*',
  KeypadMinus: '-', KeypadPlus: '+', KeypadEnter: 'ENTER',
  Keypad7: { main: '7', sub: 'HOME' }, Keypad8: { main: '8', sub: '\u25b2' },
  Keypad9: { main: '9', sub: 'PG UP' }, Keypad4: { main: '4', sub: '\u25c0' },
  Keypad5: '5', Keypad6: { main: '6', sub: '\u25b6' },
  Keypad1: { main: '1', sub: 'END' }, Keypad2: { main: '2', sub: '\u25bc' },
  Keypad3: { main: '3', sub: 'PG DN' }, Keypad0: { main: '0', sub: 'INS' },
  KeypadPeriod: { main: '.', sub: 'DEL' },
  // the media symbols are printed on the deck below these keys, not on them
  MultimediaStop: '', MultimediaPreviousTrack: '', MultimediaPlayPause: '', MultimediaNextTrack: '',
};
for (let i = 1; i <= 12; i++) LEGEND['F' + i] = 'F' + i;
LEGEND.F1 = { main: 'F1', sub: MUTE };
LEGEND.F12 = { main: 'F12', sub: 'G/N' };

const FULLNAME = {
  ESC: 'Escape', GraveAccent: 'Grave accent', LBracket: 'Left bracket', RBracket: 'Right bracket', BackSlash: 'Backslash',
  SingleQuotation: 'Quote', LSHIFT: 'Left Shift', RSHIFT: 'Right Shift', LCONTROL: 'Left Ctrl', RCONTROL: 'Right Ctrl',
  LALT: 'Left Alt', RALT: 'Right Alt', FN: 'Fn', WIN: 'Win', GameMode: 'Game mode button',
  MultimediaStop: 'Media stop', MultimediaPreviousTrack: 'Previous track', MultimediaPlayPause: 'Play/pause',
  MultimediaNextTrack: 'Next track', KeypadDivide: 'Keypad /', KeypadMultiply: 'Keypad *', KeypadMinus: 'Keypad -',
  KeypadPlus: 'Keypad +', KeypadEnter: 'Keypad Enter', KeypadPeriod: 'Keypad .', ArrowUp: 'Up arrow',
  ArrowDown: 'Down arrow', ArrowLeft: 'Left arrow', ArrowRight: 'Right arrow', PrintScreen: 'Print Screen',
  ScrollLock: 'Scroll Lock', PauseBreak: 'Pause/Break', CapsLock: 'Caps Lock', NumLock: 'Num Lock',
  PageUp: 'Page Up', PageDown: 'Page Down', Space: 'Space', Menu: 'Menu',
};

// The board is drawn from Grant's Inkscape drawing, built into board.svg by
// tools/build_board.py. board.json says where the drawing puts the parts the
// editor draws itself: the drawing area, the display's glass, and the keys that
// are not in EVGA's layout. Every key is laid out in the same pixel space as
// EVGA's own top-down photo, which is also the space layout.json uses.
const GAP = 3;                       // drawn gap between neighbouring keys
let SPACE, GLASS, BEZEL;

// The keys EVGA's layout leaves out: the mode button at the far left of the
// function row, and the four media keys over the numpad. The drawing gives the
// cap each one shows, so the key is that cap grown by half the gap on each side.
function extraKeys(g) {
  const key = (pos, id, [x0, y0, x1, y1]) =>
    ({ pos, id, x: x0 - GAP / 2, y: y0 - GAP / 2, w: x1 - x0 + GAP, h: y1 - y0 + GAP });
  return [
    key(0, 'GameMode', g.gpc),
    key(17, 'MultimediaStop', g.media[0]),
    key(18, 'MultimediaPreviousTrack', g.media[1]),
    key(19, 'MultimediaPlayPause', g.media[2]),
    key(20, 'MultimediaNextTrack', g.media[3]),
  ];
}

// USB HID keyboard usages.
const HID = { 0: 'None', 40: 'Enter', 41: 'Escape', 42: 'Backspace', 43: 'Tab', 44: 'Space', 45: '-', 46: '=',
  47: '[', 48: ']', 49: '\\', 51: ';', 52: "'", 53: '`', 54: ',', 55: '.', 56: '/', 57: 'Caps Lock',
  70: 'Print Screen', 71: 'Scroll Lock', 72: 'Pause', 73: 'Insert', 74: 'Home', 75: 'Page Up', 76: 'Delete',
  77: 'End', 78: 'Page Down', 79: 'Right arrow', 80: 'Left arrow', 81: 'Down arrow', 82: 'Up arrow', 83: 'Num Lock',
  84: 'Keypad /', 85: 'Keypad *', 86: 'Keypad -', 87: 'Keypad +', 88: 'Keypad Enter', 99: 'Keypad .', 101: 'Menu' };
for (let i = 0; i < 26; i++) HID[4 + i] = String.fromCharCode(65 + i);
for (let i = 1; i <= 9; i++) { HID[29 + i] = String(i); HID[88 + i] = 'Keypad ' + i; }
HID[39] = '0'; HID[98] = 'Keypad 0';
for (let i = 1; i <= 12; i++) HID[57 + i] = 'F' + i;
for (let i = 13; i <= 24; i++) HID[91 + i] = 'F' + i;
const MODS = [[1, 'Left Ctrl'], [2, 'Left Shift'], [4, 'Left Alt'], [8, 'Left Super'], [16, 'Right Ctrl'], [32, 'Right Shift'], [64, 'Right Alt'], [128, 'Right Super']];
const CONSUMER = { 181: 'Next track', 182: 'Previous track', 183: 'Stop', 205: 'Play/pause', 226: 'Mute',
  233: 'Volume up', 234: 'Volume down', 387: 'Media player', 394: 'Mail', 402: 'Calculator', 404: 'My computer',
  545: 'Web search', 547: 'Web home', 548: 'Web back', 549: 'Web forward', 550: 'Web stop', 551: 'Web refresh', 554: 'Web favourites' };
// In a macro the keyboard numbers the modifiers 112 to 119, which collides with
// F21..F24 in the table above, so macros get their own list.
const MACRO_KEYS = {};
for (const [h, n] of Object.entries(HID)) if (+h && +h < 104) MACRO_KEYS[+h] = n;
Object.assign(MACRO_KEYS, { 112: 'Left Ctrl', 113: 'Left Shift', 114: 'Left Alt', 115: 'Left Super',
  116: 'Right Ctrl', 117: 'Right Shift', 118: 'Right Alt', 119: 'Right Super' });

const CODE_TO_HID = { Enter: 40, Escape: 41, Backspace: 42, Tab: 43, Space: 44, Minus: 45, Equal: 46, BracketLeft: 47,
  BracketRight: 48, Backslash: 49, Semicolon: 51, Quote: 52, Backquote: 53, Comma: 54, Period: 55, Slash: 56,
  CapsLock: 57, PrintScreen: 70, ScrollLock: 71, Pause: 72, Insert: 73, Home: 74, PageUp: 75, Delete: 76, End: 77,
  PageDown: 78, ArrowRight: 79, ArrowLeft: 80, ArrowDown: 81, ArrowUp: 82, NumLock: 83, NumpadDivide: 84,
  NumpadMultiply: 85, NumpadSubtract: 86, NumpadAdd: 87, NumpadEnter: 88, Numpad0: 98, NumpadDecimal: 99, ContextMenu: 101 };
for (let i = 0; i < 26; i++) CODE_TO_HID['Key' + String.fromCharCode(65 + i)] = 4 + i;
for (let i = 1; i <= 9; i++) { CODE_TO_HID['Digit' + i] = 29 + i; CODE_TO_HID['Numpad' + i] = 88 + i; }
CODE_TO_HID.Digit0 = 39;
for (let i = 1; i <= 12; i++) CODE_TO_HID['F' + i] = 57 + i;
for (let i = 13; i <= 24; i++) CODE_TO_HID['F' + i] = 91 + i;
const CODE_TO_MOD = { ControlLeft: 1, ShiftLeft: 2, AltLeft: 4, MetaLeft: 8, ControlRight: 16, ShiftRight: 32, AltRight: 64, MetaRight: 128 };

const EFFECTS = [
  { mode: 'off', name: 'Off' },
  { mode: 'static', name: 'Static' },
  { mode: 'breathing', name: 'Breathing', speed: [1000, 5000] },
  { mode: 'pulse', name: 'Pulse', speed: [6000, 10000] },
  { mode: 'spiral', name: 'Spiral', speed: [1000, 5000] },
  { mode: 'wave', name: 'Wave', speed: [1000, 5000] },
  { mode: 'stars', name: 'Stars', speed: [500, 4000] },
  { mode: 'trigger', name: 'Reactive', speed: [50, 1000], key: 'shift' },
  { mode: 'custom', name: 'Per key' },
];
const ZONES = [[0x80, 'Whole keyboard'], [0x02, 'F-keys'], [0x04, 'Number row'], [0x10, 'WASD'], [0x08, 'Arrows'], [0x01, 'Numpad']];
const LCD_APPS = [[0x20, 'Clock'], [0x04, 'Profile number'], [0x08, 'Keyboard mode'], [0x01, 'Rollover setting'], [0x02, 'EVGA Precision X1 stats']];
const GAME_KEYS = [[0x40, 'Windows key'], [0x01, 'Alt+Tab'], [0x08, 'Alt+F4'], [0x04, 'Ctrl+Esc'], [0x02, 'Tab'], [0x10, 'Left Shift'], [0x20, 'Menu key']];

// ---------------------------------------------------------------- state
const S = {
  dev: null, layout: [], zones: {}, keys: [], byPos: {},
  editing: 1, drafts: {}, view: 'keys', sel: null, busy: false,
  flashes: {}, stars: {}, t0: performance.now(),
  paint: '#ff2d00', custom: {}, macro: null, mdrafts: {}, previewTimer: null,
  rec: null, ins: null, selStep: null, editWait: null, menu: null,
  recordWaits: (() => { try { return localStorage.getItem('z10:recordWaits') !== '0'; } catch (e) { return true; } })(),
};
const $ = (s, el = document) => el.querySelector(s);
const clone = o => JSON.parse(JSON.stringify(o));
const hex = c => '#' + c.map(v => v.toString(16).padStart(2, '0')).join('');
const unhex = h => [1, 3, 5].map(i => parseInt(h.slice(i, i + 2), 16));
const draft = () => S.drafts[S.editing];
const saved = n => S.dev.profiles[n];
const isDirty = n => JSON.stringify(S.drafts[n]) !== JSON.stringify(pick(saved(n)));
const pick = p => ({ name: p.name, lighting: p.lighting, keys: p.keys, fnkeys: p.fnkeys });

function el(tag, attrs = {}, ...kids) {
  const e = document.createElement(tag);
  for (const [k, v] of Object.entries(attrs)) {
    if (k === 'class') e.className = v;
    else if (k.startsWith('on')) e.addEventListener(k.slice(2), v);
    else if (v === true) e.setAttribute(k, '');
    else if (v !== false && v != null) e.setAttribute(k, v);
  }
  for (const k of kids.flat()) if (k != null && k !== false) e.append(k.nodeType ? k : document.createTextNode(k));
  return e;
}

// In the Macro Editor window the messages sit in the workspace's foot, which is
// rebuilt as the macro changes, so the one element is kept and moved along.
const STATUS = document.getElementById('status');
function status(msg, kind = '') {
  const s = STATUS;
  s.textContent = msg; s.className = 'status ' + kind;
}

// ---------------------------------------------------------------- live preview
// The keyboard shows the effect while the editor sends it frames. Nothing is
// stored, so leaving without saving gives the profile back untouched.

function customColors(n) {
  const key = `custom:${n ?? S.editing}`;
  return S.custom[key] || (S.custom[key] = loadCustom(key));
}
function loadCustom(key) {
  try {
    const raw = localStorage.getItem('z10:' + key);
    if (raw) return JSON.parse(raw);
  } catch (e) { /* private window, or storage cleared */ }
  return Array.from({ length: 116 }, () => [0, 0, 0]);
}
function saveCustom() {
  try { localStorage.setItem('z10:custom:' + S.editing, JSON.stringify(customColors())); }
  catch (e) { /* not fatal, the colours just will not survive a reload */ }
}

function setLive(on) { $('#live').hidden = !on; }

// The display is one continuous grid of 160 x 32 pixels, cut into 20 x 4 cells
// of 8 x 8 that touch, with each glyph drawn 5 wide and 7 tall in columns 1 to 5
// of its cell. That is the panel the z10-lcd work measured off the keyboard, and
// the font is the Adafruit GFX classic 5x7 the keyboard's controller uses.
// It is a positive display: the backlight shines through every pixel that is
// off, and a pixel that is on blocks it, so text reads dark on the lit colour.
const LCD = { cols: 20, rows: 4, cw: 8, ch: 8 };
const PW = LCD.cols * LCD.cw, PH = LCD.rows * LCD.ch;

function panelBits(rows) {
  const lit = new Uint8Array(PW * PH);
  for (let r = 0; r < LCD.rows; r++) {
    const text = (rows[r] || '').padEnd(LCD.cols).slice(0, LCD.cols);
    for (let c = 0; c < LCD.cols; c++) {
      const ch = text[c];
      if (ch === '\u2588') {   // the solid block lights the whole cell
        for (let y = 0; y < LCD.ch; y++) for (let x = 0; x < LCD.cw; x++)
          lit[(r * LCD.ch + y) * PW + c * LCD.cw + x] = 1;
        continue;
      }
      const g = S.font[ch];
      if (!g) continue;
      for (let x = 0; x < 5; x++) for (let y = 0; y < 7; y++)
        if ((g[x] >> y) & 1) lit[(r * LCD.ch + y) * PW + c * LCD.cw + 1 + x] = 1;
    }
  }
  return lit;
}

// The panel is drawn offscreen and handed to CSS as an image, at exactly the
// size it is shown in real screen pixels, so it is never scaled: shrinking a
// finer picture to fit turned the pixel grid into uneven stripes, most of all
// in WebKit and at the smaller sizes a desktop window shows the keyboard at.
// Drawing into a canvas in the page instead left the element compositing at
// the wrong scale. The picture is rebuilt when the text, the backlight colour
// or the panel's size changes.
const PITCH = 4;           // pixels per LCD pixel before the panel has a size

function drawPanel() {
  const panel = document.querySelector('.screen .panel');
  if (!panel || !S.font) return;
  const r = panel.getBoundingClientRect(), dpr = devicePixelRatio || 1;
  const cv = document.createElement('canvas');
  cv.width = Math.round(r.width * dpr) || PW * PITCH;
  cv.height = Math.round(r.height * dpr) || PH * PITCH;
  const cx = cv.width / PW, cy = cv.height / PH;
  // a gap round each pixel where it is big enough to show one; smaller, the gaps
  // would beat against the screen's own pixels and stripe the characters
  const dot = cx >= 2.5 ? 0.84 : 1;
  const ctx = cv.getContext('2d');
  const back = S.lcdLive ? (S.lcdColor || [90, 90, 90]) : null;
  // The panel is negative: the field stays dark and only the pixels that are on
  // light up in the backlight colour, so the text glows out of black.
  ctx.fillStyle = back ? shade(back, 0.05) : '#080a0c';
  ctx.fillRect(0, 0, cv.width, cv.height);
  // it sits behind glass, so even a lit pixel is well short of full brightness
  const off = back ? shade(back, 0.1) : '#12161a';
  const on = back ? shade(back, 0.72) : '#2a3138';
  const lit = S.lcdLive ? panelBits(S.lcdRows || []) : new Uint8Array(PW * PH);
  for (let y = 0; y < PH; y++) for (let x = 0; x < PW; x++) {
    const bright = lit[y * PW + x];
    ctx.fillStyle = bright ? on : off;
    // a lit pixel blooms a little into the ones around it, as it does on the panel
    if (bright && back) { ctx.shadowColor = shade(back, 0.45); ctx.shadowBlur = cx * 0.6; }
    else ctx.shadowBlur = 0;
    ctx.fillRect(x * cx, y * cy, cx * dot, cy * dot);
  }
  ctx.shadowBlur = 0;
  panel.style.backgroundImage = `url(${cv.toDataURL()})`;
}

const shade = (c, f) => `rgb(${c.map(v => Math.min(255, Math.round(v * f))).join(',')})`;

// The keyboard will not read its own display back, so the z10-lcd service writes
// each frame it sends to a file and the server passes it on. If nothing is
// driving the display, the panel is drawn unlit rather than showing stale text.
// The panel is drawn at its shown size, so draw it again when that changes:
// with the window, and with the page, which narrows as the rail opens. While
// the rail animates the size changes every frame, so it is drawn once it
// settles. Hidden, the panel has no size and is left as it was.
let panelSize = '', panelWait = 0;
new ResizeObserver(() => {
  clearTimeout(panelWait);
  panelWait = setTimeout(() => {
    const r = document.querySelector('.screen .panel')?.getBoundingClientRect();
    if (!r || !r.width) return;
    const k = `${Math.round(r.width * devicePixelRatio)}x${Math.round(r.height * devicePixelRatio)}`;
    if (k !== panelSize) { panelSize = k; drawPanel(); }
  }, 60);
}).observe(document.getElementById('board'));

async function pollLcd() {
  let lcd;
  try { lcd = await api('/api/lcd'); } catch (e) { lcd = { live: false, rows: [] }; }
  S.lcdLive = !!lcd.live;
  S.lcdRows = lcd.rows || [];
  drawPanel();
}

// Only one editor tab drives the preview: the one used last. A tab claims it
// when it opens and whenever it is clicked, typed in or brought to the front;
// the server ignores whatever the other tabs send meanwhile.
const TAB = Math.random().toString(36).slice(2);
let claim = true;
function sendPreview(body) {
  const r = api('/api/preview', { ...body, tab: TAB, claim });
  claim = false;
  return r;
}
for (const ev of ['pointerdown', 'keydown', 'focus'])
  addEventListener(ev, () => {
    if (claim || document.hidden) return;
    claim = true;
    if (S.previewTimer) pushPreview();
  }, true);

// Stars and Reactive have no preview frame the keyboard understands (Unleash
// shows neither live), so the keyboard gets each key's colour from the same
// animation that draws the keyboard on screen, about 14 times a second.
const STREAMED = ['stars', 'trigger'];
function streamedColors() {
  const c = S.lastColors || {};
  return Array.from({ length: 116 }, (_, i) => { const v = c[i]; return v ? [0, 1, 2].map(j => Math.round(v[j] * v[3])) : [0, 0, 0]; });
}
setInterval(() => {
  if (!S.dev || S.view !== 'lighting' || !S.previewTimer || !STREAMED.includes(draft().lighting.mode)) return;
  sendPreview({ colors: streamedColors() }).catch(() => {});
}, 70);

async function pushPreview() {
  if (!S.dev || S.view !== 'lighting') return;
  const L = draft().lighting;
  const body = L.mode === 'custom' ? { colors: customColors() } : STREAMED.includes(L.mode) ? { colors: streamedColors() } : { lighting: L };
  try {
    const r = await sendPreview(body);
    setLive(r.mode);
    if (r.error) status(`The keyboard stopped showing the preview: ${r.error}`, 'bad');
  } catch (e) {
    setLive(false);
    stopPreview();
    status(`Preview unavailable: ${e.message}`, 'bad');
  }
}

function startPreview() {
  claim = true;   // only ever started by opening the page or by using it
  if (!S.previewTimer) S.previewTimer = setInterval(pushPreview, 2000);
  pushPreview();
}

function stopPreview() {
  if (S.previewTimer) { clearInterval(S.previewTimer); S.previewTimer = null; }
  setLive(false);
  sendPreview({ stop: true }).catch(() => {});
}

// ---------------------------------------------------------------- loading
// every call goes through bridge.js, which hands it to Ectodeck's core
const api = (path, body) => window.__api(path, body);

async function load() {
  const [layout, zones, font, board] = await Promise.all([
    fetch('layout.json').then(r => r.json()),
    fetch('zones.json').then(r => r.json()),
    fetch('lcdfont.json').then(r => r.json()),
    fetch('board.json').then(r => r.json()),
  ]);
  const [vx0, vy0, vx1, vy1] = board.view;
  SPACE = { x0: vx0, y0: vy0, x1: vx1, y1: vy1 };
  GLASS = board.glass;
  BEZEL = board.bezel;
  // The spacebar stands taller than its row and overhangs the front of its
  // well; the drawing gives how far down it reaches.
  const space = layout.find(k => k.id === 'Space');
  if (space) space.h = board.space[3] + GAP / 2 - space.y;
  S.board = board;
  S.layout = layout.concat(extraKeys(board));
  S.font = font;
  S.zones = { 0x10: zones.WASD, 0x08: zones.Arrows, 0x04: zones.Numbers, 0x02: zones.FKeys, 0x01: zones.Numpad };
  buildBoard();
  await refresh();
}

async function refresh() {
  try {
    S.dev = await api('/api/state');
  } catch (e) {
    $('#devline').textContent = 'Keyboard not connected';
    $('#conn').classList.add('off');
    const boot = $('#boot');
    boot.classList.add('bad');
    boot.replaceChildren(el('div', {}, el('b', {}, 'The editor cannot reach the keyboard'),
      el('span', {}, `${e.message}. Check that it is plugged in, then reload this page.`)));
    return;
  }
  $('#boot').remove();
  document.querySelector('.mboot')?.remove();
  $('#conn').classList.remove('wait');
  for (const n of [1, 2, 3, 4, 5]) if (!S.drafts[n]) S.drafts[n] = clone(pick(saved(n)));
  S.editing = S.dev.active || 1;
  // printed as EVGA's software prints it: both bytes in decimal, so 0x0111 is v1.17.
  // EVGA never used a second byte of 100 or more; our own builds of 1.17 report
  // 171, 172 and so on (0x01AB is v1.171, unofficial build 1).
  const fw = S.dev.firmware, minor = fw & 0xff;
  const line = $('#devline');
  line.textContent = `Connected, firmware v${fw >> 8}.${String(minor).padStart(2, '0')}`;
  if (minor >= 100) {
    const build = minor > 170 ? ` ${minor - 170}` : '';
    line.append(el('span', { class: 'unoff' }, `Unofficial build${build}`));
    line.title = minor > 170
      ? `EVGA's firmware v1.${Math.floor(minor / 10)} with our changes, build${build}. EVGA did not make this build.`
      : 'This number is not one EVGA released, so the keyboard is running modified firmware.';
  }
  buildHidMap();
  settleWindow();
  renderAll();
  pollLcd();
  setInterval(pollLcd, 2000);
  if (S.view === 'lighting') startPreview();
}

// Ectodeck has two windows over this one page: the main window, with Keys,
// Lighting and Settings, and the Macro Editor. Each shows its own part, and
// each passes on what it saves so the other stays current.
function settleWindow() {
  document.body.classList.add('win-' + window.__window);
  if (window.__window === 'macros') {
    document.title = 'EVGA Z10 RGB / Macro Editor';
    S.view = 'keys'; S.macrosOpen = true;
    const pickMacro = () => {
      const n = +(location.hash.split('/')[1] || 0);
      if (n && S.dev.macros[n]) { S.macro = n; S.selStep = null; S.ins = null; S.scrollToMacro = true; }
      renderMacros();
    };
    pickMacro();
    addEventListener('hashchange', pickMacro);
    window.__hear('profiles', profiles => {
      for (const [n, p] of Object.entries(profiles)) {
        const clean = S.drafts[n] && !isDirty(+n);
        S.dev.profiles[n] = p;
        if (clean) S.drafts[n] = clone(pick(p));
      }
      renderMacros();
    });
  } else {
    document.title = 'Ectodeck';
    window.__prepareMacroEditor();
    window.__hear('macros', macros => { S.dev.macros = macros; renderAll(); });
  }
}

function showBanner(text, bad) {
  const b = el('p', { class: 'banner' + (bad ? ' bad' : '') }, text);
  $('.page').prepend(b);
}

// Browser key code -> key position, from the factory keymap.
function buildHidMap() {
  S.hidToPos = {}; S.modToPos = {};
  saved(254).keys.forEach(([f, a, b], pos) => {
    if (f !== 0) return;
    if (b) S.hidToPos[b] = pos; else if (a) S.modToPos[a] = pos;
  });
}
function posForEvent(e) {
  if (CODE_TO_MOD[e.code]) return S.modToPos[CODE_TO_MOD[e.code]];
  return S.hidToPos[CODE_TO_HID[e.code]];
}

// ---------------------------------------------------------------- board
function keyFace(id) {
  if (id === 'GameMode')
    return el('span', { class: 'leg printed' }, el('img', { class: 'emark', src: 'evga-e.svg', alt: '' }));
  const L = LEGEND[id] ?? id;
  const spec = typeof L === 'string' ? { main: L } : L;
  const main = spec.main ?? '';
  // Two sizes only: a single character (a letter, digit, symbol or arrow) is set
  // large, and anything longer is a word and set small, wherever it sits.
  const size = t => ([...t].length === 1 ? ' glyph' + (MARKS.includes(t) ? ' mark' : '') : ' word');
  // The key's own legend at the top of the cap, a shifted symbol beside it,
  // and a second function at the bottom.
  const head = [el('span', { class: 'main' + size(main) }, main)];
  if (spec.top) head.push(el('span', { class: 'top' + size(spec.top) }, spec.top));
  const parts = [el('span', { class: 'head' }, head)];
  if (spec.main2) parts.push(el('span', { class: 'main' + size(spec.main2) }, spec.main2));
  if (spec.sub === MUTE) parts.push(el('span', { class: 'sub icon mute', role: 'img', 'aria-label': 'mute' }));
  else if (spec.sub) parts.push(el('span', { class: 'sub' + size(spec.sub) }, spec.sub));
  return el('span', { class: 'leg' + (CENTRED.has(id) ? ' centred' : '') }, parts);
}
// Thin punctuation, which at a digit's size looks smaller and lighter than the
// digits beside it, so it is set a step larger and bolder to match them.
const MARKS = '`~-_=+^*\'";:,.';
// The few caps whose legend sits in the middle rather than at the top.
const CENTRED = new Set(['E1', 'E2', 'E3', 'E4', 'E5', 'E6', 'E7', 'Tab', 'CapsLock',
                         'LSHIFT', 'RSHIFT', 'Enter', 'KeypadEnter', 'Backspace'])

function buildBoard() {
  const board = $('#board');
  const { x0, y0, x1, y1 } = SPACE;
  const W = x1 - x0, H = y1 - y0;
  board.style.aspectRatio = `${W} / ${H}`;
  board.style.setProperty('--ar', String(W / H));
  S.geom = { x0, y0, W, H };
  const at = (x, y, w, h) => `left:${(x - x0) / W * 100}%;top:${(y - y0) / H * 100}%;`
    + `width:${w / W * 100}%;height:${h / H * 100}%`;
  board.append(el('img', { class: 'art', src: 'board.svg', alt: '', draggable: 'false' }));
  // Labels printed on the deck, where EVGA's photo has them: under each slider,
  // a minus at one end, the name, and a plus at the other; and G/PC under the
  // mode button.
  const printed = (x, y, w, h, ...words) => board.append(el('div', { class: 'print', style: at(x, y, w, h) },
    words.map(t => el('span', {}, t))));
  const [[dx0, , dx1, dy1], [vx0, , vx1]] = S.board.sliders;
  printed(dx0 + 1.5, dy1 + 3.2, dx1 - dx0 - 3, 4, '\u2212', 'DIMMER', '+');
  printed(vx0 + 1.5, dy1 + 3.2, vx1 - vx0 - 3, 4, '\u2212', 'VOLUME', '+');
  const [px0, , px1, py1] = S.board.gpc;
  printed(px0 - 4, py1 + 2.4, px1 - px0 + 8, 4, 'G/PC');
  for (const k of S.layout) {
    const b = el('button', {
      class: 'key' + (k.led ? '' : ' nolight')
        + (k.id.startsWith('Multimedia') ? ' molded' : ''),
      'data-pos': k.pos, 'aria-label': FULLNAME[k.id] || k.id,
      style: `left:${(k.x - x0 + GAP / 2) / W * 100}%;top:${(k.y - y0 + GAP / 2) / H * 100}%;`
           + `width:${(k.w - GAP) / W * 100}%;height:${(k.h - GAP) / H * 100}%`,
      onclick: () => selectKey(k.pos),
    }, el('span', { class: 'halo' }), el('span', { class: 'glow' }),
       keyFace(k.id), el('span', { class: 'remap' }));
    board.append(b);
    k.el = b; k.cx = k.x + k.w / 2; k.cy = k.y + k.h / 2;
    S.byPos[k.pos] = k;
  }
  // The drawing again, with its wells cut out. It comes after the keys, so it
  // covers the light they throw everywhere except down in the wells, which is
  // where that light shows on the keyboard.
  board.append(el('img', { class: 'art top', src: 'board-top.svg', alt: '', draggable: 'false' }));
  // the display, drawn into the glass of the drawing
  const [gx0, gy0, gx1, gy1] = GLASS;
  board.append(el('div', { class: 'screen', style: at(gx0, gy0, gx1 - gx0, gy1 - gy0) },
    el('span', { class: 'panel' })));
  // The screen and its bezel sit behind one pane of glossy glass.
  const [bx0, by0, bx1, by1] = BEZEL;
  board.append(el('div', { class: 'glass', style: at(bx0, by0, bx1 - bx0, by1 - by0) }));
  const cx = S.layout.map(k => k.cx), cy = S.layout.map(k => k.cy);
  S.center = [(Math.min(...cx) + Math.max(...cx)) / 2, (Math.min(...cy) + Math.max(...cy)) / 2];
  S.span = [Math.max(...cx) - Math.min(...cx), Math.max(...cy) - Math.min(...cy), Math.min(...cx), Math.min(...cy)];
}

function zoneSet(zone) {
  if (zone & 0x80) return null; // everything
  const s = new Set();
  for (const [bit, list] of Object.entries(S.zones)) if (zone & bit) list.forEach(p => s.add(p));
  return s;
}

// ---------------------------------------------------------------- preview
const mix = (a, b, t) => a.map((v, i) => v + (b[i] - v) * t);
function ramp(colors, u) { // u in [0,1) around a loop of colours
  const n = colors.length, x = ((u % 1) + 1) % 1 * n, i = Math.floor(x);
  return mix(colors[i], colors[(i + 1) % n], x - i);
}
function effectColors(L, t) {
  const mode = L.mode, out = {};
  const e = L[mode === 'off' ? 'static' : mode] || {};
  const inZone = zoneSet(e.zone ?? 0x80);
  const cols = (e.colors || []).slice(0, 7).map(c => c.color);
  const bri = (e.colors?.[0]?.brightness ?? e.brightness ?? 255) / 255;
  for (const k of S.layout) {
    if (!k.led) continue;
    if (inZone && !inZone.has(k.pos)) { out[k.pos] = [0, 0, 0, 0]; continue; }
    let c = [0, 0, 0], a = 0;
    if (mode === 'static') { c = e.color; a = e.brightness / 255; }
    else if (mode === 'breathing') {
      const u = (t % (2 * e.cycle)) / e.cycle, which = u < 1 ? 0 : 1;
      c = e.colors[which].color; a = Math.sin(Math.PI * (u % 1)) ** 2 * bri;
    } else if (mode === 'pulse') {
      const n = Math.max(1, Math.min(e.count || 7, 7)), step = e.cycle / n;
      const i = Math.floor(t / step) % n, u = (t % step) / step;
      c = cols[i] || [255, 255, 255];
      const on = Math.min(1, u * 6) * Math.min(1, (1 - u) * 6);
      c = mix(e.background.color, c, on); a = bri;
    } else if (mode === 'spiral') {
      const ang = Math.atan2(k.cy - S.center[1], (k.cx - S.center[0]) / 2.2) / (2 * Math.PI);
      const dir = e.direction === 1 ? -1 : 1;
      c = ramp(cols.length ? cols : [[255, 0, 0], [0, 255, 0], [0, 0, 255]], ang + dir * t / e.cycle); a = bri;
    } else if (mode === 'wave') {
      const d = e.direction > 3 ? 0 : e.direction; // 0 up 1 down 2 right 3 left
      const along = d >= 2 ? (k.cx - S.span[2]) / S.span[0] : (k.cy - S.span[3]) / S.span[1];
      const sign = (d === 1 || d === 2) ? -1 : 1;
      c = ramp(cols, sign * along + t / e.cycle); a = bri;
    } else if (mode === 'stars') {
      const s = S.stars[k.pos];
      if (s) { const u = (t - s.t) / e.cycle; if (u < 1) { c = s.c; a = Math.sin(Math.PI * u) * e.brightness / 255; } else delete S.stars[k.pos]; }
    } else if (mode === 'trigger') {
      const f = S.flashes[k.pos];
      if (f) { const u = (t - f.t) / Math.max(250, e.shift * 3); if (u < 1) { c = f.c; a = (1 - u) * bri; } }
    }
    out[k.pos] = [...c, a];
  }
  return out;
}

function frame(now) {
  const t = now - S.t0;
  if (S.dev) {
    const L = draft().lighting;
    if (L.mode === 'stars' && Math.random() < 0.35) {
      const k = S.layout[Math.floor(Math.random() * S.layout.length)];
      S.stars[k.pos] ??= { t, c: [0, 1, 2].map(() => 80 + Math.floor(Math.random() * 176)) };
    }
    if (S.demoTyping && L.mode === 'trigger' && Math.random() < 0.08) {
      const k = S.layout[Math.floor(Math.random() * S.layout.length)];
      flash(k.pos, t);
    }
    const colors = L.mode === 'custom'
      ? Object.fromEntries(customColors().map((c, i) =>
          [i, S.byPos[i] && S.byPos[i].led ? [...c, Math.max(...c) ? 1 : 0] : [0, 0, 0, 0]]))
      : effectColors(L, t);
    S.lastColors = colors;
    for (const k of S.layout) {
      const v = colors[k.pos];
      if (!v) continue;
      k.el.style.setProperty('--c', `rgb(${v[0] | 0},${v[1] | 0},${v[2] | 0})`);
      k.el.style.setProperty('--a', v[3].toFixed(3));
    }
  }
  requestAnimationFrame(frame);
}

function flash(pos, t = performance.now() - S.t0) {
  const e = draft().lighting.trigger;
  const cols = e.colors.slice(0, 7).map(c => c.color);
  S.flashIdx = ((S.flashIdx ?? -1) + 1) % cols.length;
  const c = cols[S.flashIdx];
  const k = S.byPos[pos]; if (!k) return;
  const hit = (e.submode > 3 ? 0 : e.submode) === 2 ? S.layout.filter(o => Math.abs(o.cx - k.cx) < 34 && Math.abs(o.cy - k.cy) < 34) : [k];
  for (const o of hit) S.flashes[o.pos] = { t, c };
}

// ---------------------------------------------------------------- key labels
function describe(def) {
  const [f, a, b, c] = def;
  if (f === 0) {
    const mods = MODS.filter(([m]) => a & m).map(([, n]) => n);
    const keys = [b, c].filter(Boolean).map(h => HID[h] || `Key ${h}`);
    return [...mods, ...keys].join(' + ') || 'Nothing';
  }
  if (f === 2) return CONSUMER[a | b << 8] || `Media code ${a | b << 8}`;
  if (f === 3) return `Macro: ${S.dev.macros[a]?.name || 'number ' + a}`;
  if (f === 4) return 'Fn';
  if (f === 5) return 'Game mode toggle';
  if (f === 6) return a ? 'Scroll down' : 'Scroll up';
  if (f === 7) return 'Left click';
  if (f === 8) return 'Right click';
  if (f === 9) return 'Middle click';
  if (f >= 17 && f <= 21) return `Switch to profile ${f - 16}`;
  if (f === 22) return 'Next profile';
  if (f === 23) return 'Previous profile';
  if (f === 255) return 'Disabled';
  return `Function ${f}`;
}
// A remapped key's new meaning, printed on it like a second legend. A key
// shows its own name, a shortcut shows as Ctrl+C, and a macro shows its name
// without the words it shares with the other macros on the board, so four
// macros called "KDE Switch Desktop Left" and so on read Left, Right, Up, Down.
const MOD_SHORT = { 1: 'Ctrl', 2: 'Shift', 4: 'Alt', 8: 'Super', 16: 'Ctrl', 32: 'Shift', 64: 'Alt', 128: 'Super' };
function shortLabel(def, macroNames = []) {
  const [f, a, b, c] = def;
  if (f === 0) {
    const mods = [...new Set(MODS.filter(([m]) => a & m).map(([m]) => MOD_SHORT[m]))];
    const keys = [b, c].filter(Boolean).map(h => (HID[h] || `Key ${h}`).replace(/^Keypad /, 'Num '));
    return [...mods, ...keys].join('+') || 'Nothing';
  }
  if (f === 3) return distinctPart(S.dev.macros[a]?.name || `Macro ${a}`, macroNames);
  if (f === 255) return 'Off';
  return describe(def);
}
// the words at the end of a name that tell it apart from the others
function distinctPart(name, names) {
  const w = name.split(/\s+/);
  let keep = 0;
  for (const other of names) {
    if (other === name) continue;
    const o = other.split(/\s+/);
    let n = 0;
    while (n < w.length - 1 && n < o.length && w[n] === o[n]) n++;
    keep = Math.max(keep, n);
  }
  return w.slice(keep).join(' ');
}

// ---------------------------------------------------------------- render
function renderAll() {
  renderProfiles(); renderBoardLabels(); renderView(); renderActions();
}

// A small colour sample for a profile button. Every mode is handled, including
// per key, which has no stored colour list of its own.
function profileSwatch(n, L) {
  if (L.mode === 'off') return '#000';
  if (L.mode === 'static') return hex(L.static.color);
  if (L.mode === 'custom') {
    const lit = customColors(n).filter(c => Math.max(...c)).slice(0, 4).map(hex);
    return lit.length ? `linear-gradient(90deg, ${lit.join(',')})` : '#000';
  }
  const e = L[L.mode];
  const cols = e && e.colors ? e.colors.slice(0, 4).map(c => hex(c.color)) : [];
  return cols.length ? `linear-gradient(90deg, ${cols.join(',')})` : 'linear-gradient(90deg,#888,#fff)';
}

function renderProfiles() {
  const nav = $('#profiles'); nav.replaceChildren();
  for (const n of [1, 2, 3, 4, 5]) {
    const L = S.drafts[n].lighting, m = EFFECTS.find(x => x.mode === L.mode);
    const sw = profileSwatch(n, L);
    nav.append(el('button', {
      'aria-current': String(n === S.editing), class: isDirty(n) ? 'dirty' : '',
      title: isDirty(n) ? 'Unsaved changes' : '', onclick: () => chooseProfile(n),
      }, el('span', { class: 'swatch', style: `background:${sw}` }),
    (S.drafts[n].name && !/^DEFAULT/i.test(S.drafts[n].name) ? S.drafts[n].name : `Profile ${n}`),
    el('span', { class: 'sub' }, (m ? m.name : L.mode) + (n === S.dev.active ? ', active' : ''))));
  }
}

function renderBoardLabels() {
  const d = draft(), fac = saved(254);
  const lc = d.lighting.lcd.color;
  const scaled = lc.color.map(v => Math.round(v * (0.3 + 0.7 * lc.brightness / 255)));
  S.lcdColor = scaled;
  drawPanel();
  const layer = 'keys';
  const macroNames = [...new Set(S.layout.map(k => d[layer][k.pos]).filter(v => v[0] === 3)
    .map(v => S.dev.macros[v[1]]?.name).filter(Boolean))];
  for (const k of S.layout) {
    const def = d[layer][k.pos], changed = JSON.stringify(def) !== JSON.stringify(fac[layer][k.pos]);
    const show = S.view === 'keys' && changed;
    k.el.classList.toggle('has-remap', show);
    k.el.querySelector('.remap').textContent = show ? shortLabel(def, macroNames) : '';
    k.el.title = `${FULLNAME[k.id] || LEGEND[k.id] || k.id}: ${describe(def)}`;
    k.el.classList.toggle('sel', S.sel === k.pos && S.view === 'keys');
  }
  $('#boardhint').textContent = S.view === 'lighting' && draft().lighting.mode === 'trigger' ? 'Type anywhere on this page to see the reactive effect.' : '';
}

function renderActions() {
  const dirty = isDirty(S.editing);
  $('#save').disabled = !dirty || S.busy;
  $('#revert').disabled = !dirty || S.busy;
  $('#save').textContent = `Save profile ${S.editing}`;
}

function renderView() {
  closeMenu();
  for (const v of ['lighting', 'keys', 'display']) {
    $('#view-' + v).hidden = v !== S.view;
    $('#tab-' + v).setAttribute('aria-selected', String(v === S.view));
  }
  // the macros sit on the Keys page, under the key settings
  $('#view-macros').hidden = S.view !== 'keys';
  ({ lighting: renderLighting, keys: renderKeys, display: renderDisplay })[S.view]();
  if (S.view === 'keys') renderMacros();
}

// change helper: mutate the draft, then re-render the chrome (not the open panel)
function change(fn, rerenderPanel = false) {
  fn(draft());
  renderProfiles(); renderActions(); renderBoardLabels();
  if (rerenderPanel) renderView();
  if (S.view === 'lighting') startPreview();
}

// ---------------------------------------------------------------- lighting view
function renderLighting() {
  const L = draft().lighting, box = $('#view-lighting');
  box.replaceChildren();
  const row = el('div', { class: 'effects', role: 'group', 'aria-label': 'Effect' });
  for (const fx of EFFECTS) {
    row.append(el('button', { 'aria-pressed': String(L.mode === fx.mode), onclick: () => change(d => { d.lighting.mode = fx.mode; }, true) },
      el('span', { class: 'icon', style: `background:${effectIcon(fx.mode)}` }), el('span', { class: 'name' }, fx.name)));
  }
  box.append(row);
  if (L.mode === 'off') { box.append(el('p', { class: 'quiet' }, 'The backlight stays dark while this profile is active.')); return; }
  if (L.mode === 'custom') { renderCustom(box); return; }
  const e = L[L.mode], fx = EFFECTS.find(x => x.mode === L.mode), grid = el('div', { class: 'grid' });

  // colours
  if (L.mode === 'static') grid.append(field('Colour', colorInput(e.color, c => change(() => { e.color = c; }))));
  if (L.mode === 'breathing') grid.append(field('Colours', el('div', { class: 'swatches' }, e.colors.map(c => colorInput(c.color, v => change(() => { c.color = v; }))))
    , 'Breathes in the first colour, then the second.'));
  if (['pulse', 'wave', 'trigger', 'spiral'].includes(L.mode)) grid.append(field('Colours', el('div', { class: 'swatches' },
    e.colors.slice(0, 7).map(c => colorInput(c.color, v => change(() => { c.color = v; })))),
    L.mode === 'trigger' ? 'Each key press takes the next colour.' : 'The effect runs through these seven colours in order.'));
  if (L.mode === 'pulse') grid.append(field('Between pulses', colorInput(e.background.color, v => change(() => { e.background.color = v; }))));

  // brightness
  const briGet = () => L.mode === 'static' ? e.brightness : L.mode === 'stars' ? e.brightness : e.colors[0].brightness;
  const briSet = v => { if (L.mode === 'static' || L.mode === 'stars') e.brightness = v; else { e.colors.forEach(c => { c.brightness = v; }); if (e.background) e.background.brightness = v; } };
  grid.append(rangeField('Brightness', 0, 255, briGet(), v => `${Math.round(v / 2.55)}%`, v => change(() => briSet(v))));

  // speed: stored as a cycle time; the slider runs slow -> fast like EVGA's
  if (fx.speed) {
    const key = fx.key || 'cycle', [lo, hi] = fx.speed;
    grid.append(rangeField('Speed', lo, hi, lo + hi - e[key], v => `${((lo + hi - v) / 1000).toFixed(2)} s ${L.mode === 'trigger' ? 'colour change' : 'per cycle'}`,
      v => change(() => { e[key] = lo + hi - v; })));
  }

  if (L.mode === 'spiral') grid.append(field('Direction', seg([[0, 'Clockwise'], [1, 'Counterclockwise']], e.direction > 1 ? 0 : e.direction, v => change(() => { e.direction = v; }, true))));
  if (L.mode === 'wave') grid.append(field('Direction', seg([[0, 'Up'], [1, 'Down'], [3, 'Left'], [2, 'Right']], e.direction > 3 ? 0 : e.direction, v => change(() => { e.direction = v; }, true))));
  if (L.mode === 'trigger') grid.append(field('Lights up', seg([[0, 'Each key typed'], [1, 'One key'], [2, '3 by 3 around the key']], e.submode > 2 ? 0 : e.submode, v => change(() => { e.submode = v; }, true)),
    el('label', { class: 'check' }, el('input', { type: 'checkbox', checked: !!S.demoTyping, onchange: ev => { S.demoTyping = ev.target.checked; } }), 'Preview with pretend typing')));
  if ('random' in e && L.mode !== 'trigger') grid.append(field('Order', el('label', { class: 'check' },
    el('input', { type: 'checkbox', checked: !!e.random, onchange: ev => change(() => { e.random = ev.target.checked ? 1 : 0; }) }), 'Random colours instead of the list')));

  // zone
  grid.append(field('Where', zoneChips(e), 'Keys outside these areas stay dark.'));
  // start and stop
  grid.append(field('Starts', seg([[0, 'With the profile'], [1, 'On a key press']], e.start, v => change(() => { e.start = v; }, true))));
  grid.append(field('Stops', seg([[0, 'Never'], [1, 'On key press'], [2, 'On key release']], e.stop, v => change(() => { e.stop = v; }, true))));
  grid.append(field('', el('button', { class: 'linkish', onclick: () => change(d => { d.lighting[L.mode] = clone(saved(254).lighting[L.mode]); }, true) }, `Reset ${fx.name.toLowerCase()} to factory settings`)));
  box.append(grid);
}

function renderCustom(box) {
  const cols = customColors();
  box.append(el('div', { class: 'paintbar' },
    el('span', { class: 'lab' }, 'Colour'),
    el('span', { class: 'swatch-in' },
      el('input', { type: 'color', value: S.paint, 'aria-label': 'Colour to paint with',
        oninput: e => { S.paint = e.target.value; } })),
    el('button', { class: 'ghost', onclick: () => {
      const c = unhex(S.paint);
      for (const k of S.layout) if (k.led) cols[k.pos] = [...c];
      saveCustom(); startPreview(); renderView();
    } }, 'Fill every key'),
    el('button', { class: 'ghost', onclick: () => {
      for (const k of S.layout) cols[k.pos] = [0, 0, 0];
      saveCustom(); startPreview(); renderView();
    } }, 'Clear'),
    el('button', { class: 'ghost', onclick: () => {
      for (const k of S.layout) if (k.led) cols[k.pos] = [Math.random() * 256 | 0, Math.random() * 256 | 0, Math.random() * 256 | 0];
      saveCustom(); startPreview(); renderView();
    } }, 'Scatter')));
  box.append(el('p', { class: 'quiet', style: 'max-width:62ch' },
    'Click any key on the keyboard above to paint it. The colours appear on the real keyboard straight away. '
    + 'The spacebar, the mode button and the media keys have no light behind them, so they stay dark.'),
    el('p', { class: 'quiet', style: 'max-width:62ch' },
      'The keyboard cannot store per-key colours by itself, so these only show while this editor is open. '
      + 'Saving this profile stores the backlight as off, so the keyboard is dark rather than showing a stale effect when the editor is closed.'));
}

function effectIcon(mode) {
  return {
    off: '#0c0e10', static: '#d8421e', breathing: 'linear-gradient(90deg,#140806,#d8421e,#140806)',
    pulse: 'repeating-linear-gradient(90deg,#1d6bff 0 10px,#0c0e10 10px 16px)', spiral: 'conic-gradient(from 90deg,#f33,#fd3,#3f6,#3cf,#94f,#f33)',
    wave: 'linear-gradient(90deg,#f33,#fd3,#3f6,#3cf,#94f)', stars: 'radial-gradient(circle at 20% 40%,#fff 0 2px,transparent 3px),radial-gradient(circle at 60% 70%,#9cf 0 2px,transparent 3px),radial-gradient(circle at 82% 30%,#fc9 0 2px,transparent 3px),#0c0e10',
    trigger: 'radial-gradient(circle at 50% 50%,#3cf 0 5px,#123 12px,#0c0e10 22px)',
    custom: 'linear-gradient(135deg,#f33 0 25%,#3cf 25% 50%,#fd3 50% 75%,#8f5 75%)',
  }[mode];
}

function field(label, ...kids) {
  const note = typeof kids[kids.length - 1] === 'string' ? kids.pop() : null;
  return el('div', { class: 'field' }, label ? el('span', { class: 'lab' }, label) : null, ...kids, note ? el('span', { class: 'note' }, note) : null);
}
function colorInput(c, on) {
  return el('span', { class: 'swatch-in' }, el('input', { type: 'color', value: hex(c), 'aria-label': 'Colour', oninput: e => on(unhex(e.target.value)) }));
}
function rangeField(label, min, max, val, fmt, on) {
  const out = el('span', { class: 'range-val' }, fmt(val));
  const inp = el('input', { type: 'range', min, max, value: val, 'aria-label': label, oninput: e => { const v = +e.target.value; out.textContent = fmt(v); on(v); } });
  return el('div', { class: 'field' }, el('span', { class: 'lab' }, label, ' ', out), inp);
}
function seg(options, value, on) {
  return el('div', { class: 'seg', role: 'group' }, options.map(([v, n]) => el('button', { 'aria-pressed': String(v === value), onclick: () => on(v) }, n)));
}
function zoneChips(e) {
  return el('div', { class: 'chips row' }, ZONES.map(([bit, name]) => el('button', {
    'aria-pressed': String(bit === 0x80 ? !!(e.zone & 0x80) : !(e.zone & 0x80) && !!(e.zone & bit)),
    onclick: () => change(() => {
      if (bit === 0x80) e.zone = 0x80;
      else { let z = e.zone & 0x80 ? 0 : e.zone; z ^= bit; e.zone = z || 0x80; }
    }, true),
  }, name)));
}

// ---------------------------------------------------------------- menus
// Every drop-down in the editor is one of these: a panel fixed to the screen
// under the button that opened it, or above it when there is no room below.
// A click outside it, Escape, or leaving the window closes it, and so does a
// second click on the button that opened it. Closed any of those ways, without
// a choice made, its `onDismiss` runs: menus opened from the macro's chain use
// it to let go of the step or insert point they were opened on.
function openMenu(anchor, build, { width, align = 'left', onDismiss } = {}) {
  // This click is the second one on the button whose menu it just closed. Some
  // buttons are redrawn by their own click (a step, an insert point), so the
  // same button is also known by where it sits.
  const shut = S.menuShut, r0 = shut?.rect, r1 = anchor.getBoundingClientRect();
  const same = shut && (shut.anchor === anchor || (Math.abs(r0.left - r1.left) < 2 && Math.abs(r0.top - r1.top) < 2 && Math.abs(r0.width - r1.width) < 2));
  if (same && performance.now() - shut.at < 800) { S.menuShut = null; shut.onDismiss?.(); return; }
  S.menuShut = null;
  // Opened from the keyboard, the button's second press has no mousedown before
  // it. A choice in the open menu that opens the next one on the same button
  // (Insert here, then Key) is not a second press.
  const fromMenu = S.menu && window.event && S.menu.m.contains(window.event.target);
  if (S.menu && S.menu.anchor === anchor && !fromMenu) { const d = S.menu.onDismiss; closeMenu(); d?.(); return; }
  closeMenu();
  const m = el('div', { class: 'menu float', role: 'menu' });
  if (width) m.style.width = width + 'px';
  const render = () => { m.replaceChildren(); build(m, render); };
  render();
  document.body.append(m);
  const r = anchor.getBoundingClientRect(), h = m.offsetHeight, w = m.offsetWidth;
  // below the button when it fits there, or when there is a fair amount of room
  // there anyway (the menu then scrolls); above it only when below is cramped
  const below = innerHeight - r.bottom - 14, above = r.top - 70;
  const down = h <= below || below >= Math.min(h, 260) || below >= above;
  const room = down ? below : above;
  if (h > room) m.style.maxHeight = room + 'px';
  const top = down ? r.bottom + 6 : r.top - 6 - Math.min(h, room);
  let left = align === 'right' ? r.right - w : r.left;
  left = Math.max(8, Math.min(left, innerWidth - w - 8));
  Object.assign(m.style, { top: top + 'px', left: left + 'px' });
  const away = e => {
    if (m.contains(e.target)) return;
    // pressed on its own button: close now, and let that button's click, which
    // follows, know not to open it again
    if (anchor.contains(e.target)) { S.menuShut = { anchor, rect: anchor.getBoundingClientRect(), at: performance.now(), onDismiss }; closeMenu(); return; }
    closeMenu();
    // Let go only once this press's click has happened, so a click on another
    // step or insert point lands on it before the chain is redrawn; a press
    // that never becomes a click (a drag) lets go a moment later.
    if (onDismiss) {
      const run = () => { clearTimeout(t); removeEventListener('click', run, true); onDismiss(); };
      const t = setTimeout(run, 1000);
      addEventListener('click', run, true);
    }
  };
  const esc = e => { if (e.key === 'Escape') { e.stopPropagation(); closeMenu(); onDismiss?.(); } };
  const leave = () => { closeMenu(); onDismiss?.(); };
  setTimeout(() => { addEventListener('mousedown', away, true); addEventListener('keydown', esc, true); addEventListener('blur', leave); });
  S.menu = { m, anchor, onDismiss, off: () => { removeEventListener('mousedown', away, true); removeEventListener('keydown', esc, true); removeEventListener('blur', leave); } };
  m.querySelector('input')?.focus();
  return m;
}
function closeMenu() {
  if (!S.menu) return;
  S.menu.off(); S.menu.m.remove(); S.menu = null;
}
const mItem = (label, on, { tick = false, sub = '', tag = '', cls = '', disabled = false } = {}) =>
  el('button', { class: 'mi ' + cls, role: 'menuitem', disabled, onclick: on },
    el('span', { class: 'tick' }, tick ? '✓' : ''),
    tag ? el('span', { class: 'mtag' }, tag) : null,
    el('span', { class: 'ml' }, label), sub ? el('span', { class: 'msub' }, sub) : null);
const mHead = t => el('div', { class: 'mh' }, t);
const CHEV = () => { const s = document.createElementNS('http://www.w3.org/2000/svg', 'svg'); s.setAttribute('viewBox', '0 0 10 6'); s.setAttribute('class', 'chev'); s.innerHTML = '<path d="M1 1l4 4 4-4" fill="none" stroke="currentColor" stroke-width="1.5"/>'; return s; };

// A searchable list: type to filter, click to choose. `groups` is
// [[heading, [[label, value, tag?], ...]], ...].
function pickList(m, groups, current, on, placeholder) {
  const q = el('input', { class: 'msearch', type: 'search', placeholder, 'aria-label': placeholder });
  const list = el('div', { class: 'mlist' });
  const draw = () => {
    list.replaceChildren();
    const t = q.value.trim().toLowerCase();
    let n = 0;
    for (const [head, items] of groups) {
      const hit = items.filter(([label, , tag]) => !t || label.toLowerCase().includes(t) || (tag || '').toLowerCase().includes(t));
      if (!hit.length) continue;
      if (head) list.append(mHead(head));
      for (const [label, v, tag] of hit.slice(0, t ? 60 : 400)) {
        n++;
        list.append(mItem(label, () => { closeMenu(); on(v); }, { tick: JSON.stringify(v) === JSON.stringify(current), tag: t ? tag : '' }));
      }
    }
    if (!n) list.append(el('div', { class: 'mempty' }, 'Nothing matches.'));
  };
  q.addEventListener('input', draw);
  q.addEventListener('keydown', e => { if (e.key === 'Enter') list.querySelector('.mi')?.click(); });
  draw();
  m.append(q, list);
}

// ---------------------------------------------------------------- keys view
function selectKey(pos) {
  if (S.view === 'lighting' && draft().lighting.mode === 'custom') {
    if (!S.byPos[pos].led) {
      status(`${FULLNAME[S.byPos[pos].id] || S.byPos[pos].id} has no light behind it.`);
      return;
    }
    customColors()[pos] = unhex(S.paint);
    saveCustom(); startPreview();
    const k = S.byPos[pos];
    k.el.classList.add('press'); setTimeout(() => k.el.classList.remove('press'), 90);
    return;
  }
  if (S.view !== 'keys') { S.view = 'keys'; }
  else if (S.sel === pos) { deselectKey(); return; }     // pressing it again lets it go
  S.sel = pos;
  const k = S.byPos[pos];
  k.el.classList.add('press'); setTimeout(() => k.el.classList.remove('press'), 90);
  renderView(); renderBoardLabels();
}

// Every action a key can be given, grouped for the action menu. A value is the
// four-byte key definition the keyboard stores.
function actionGroups() {
  const keys = Object.entries(HID).filter(([h]) => +h).map(([h, n]) => [n, [0, 0, +h, 0], 'Key']);
  const mods = MODS.map(([m, n]) => [n, [0, m, 0, 0], 'Key']);
  const media = Object.entries(CONSUMER).map(([c, n]) => [n, [2, c & 255, c >> 8, 0], 'Media']);
  const macros = Object.entries(S.dev.macros).map(([i, m]) => [m.name || `Macro ${i}`, [3, +i, 0, 0], 'Macro']);
  const mouse = [['Left click', [7, 0, 0, 0]], ['Right click', [8, 0, 0, 0]], ['Middle click', [9, 0, 0, 0]], ['Scroll up', [6, 0, 0, 0]], ['Scroll down', [6, 1, 0, 0]]].map(x => [...x, 'Mouse']);
  const profiles = [1, 2, 3, 4, 5].map(n => [`Switch to profile ${n}`, [16 + n, 0, 0, 0], 'Profile'])
    .concat([['Next profile', [22, 0, 0, 0], 'Profile'], ['Previous profile', [23, 0, 0, 0], 'Profile']]);
  const board = [['Fn', [4, 0, 0, 0], 'Keyboard'], ['Game mode toggle', [5, 0, 0, 0], 'Keyboard']];
  return { macros, keys: keys.concat(mods), media, mouse, profiles, board };
}
function kindOf(def) {
  const f = def[0];
  if (f === 0) return def[1] || def[2] ? 'Key' : '';
  return { 2: 'Media', 3: 'Macro', 4: 'Keyboard', 5: 'Keyboard', 6: 'Mouse', 7: 'Mouse', 8: 'Mouse', 9: 'Mouse', 255: '' }[f]
    || (f >= 17 && f <= 23 ? 'Profile' : '');
}
const NOTHING = [0, 0, 0, 0];
const isNothing = def => (def[0] === 0 && !def[1] && !def[2]) || def[0] === 255;

// The action menu: macros first, then the other kinds, each opening its list,
// and a search across all of them.
function actionMenu(anchor, def, on) {
  let page = null;
  const G = actionGroups();
  const pages = { keys: ['A key', G.keys], media: ['A media key', G.media], mouse: ['A mouse button', G.mouse],
    profiles: ['Switch profile', G.profiles], board: ['Keyboard function', G.board] };
  openMenu(anchor, (m, again) => {
    if (page) {
      m.append(mItem('‹ Back', () => { page = null; again(); }, { cls: 'back' }));
      pickList(m, [[pages[page][0], pages[page][1]]], def, on, `Search ${pages[page][0].toLowerCase().replace(/^an? /, '')}s`);
      return;
    }
    const q = el('input', { class: 'msearch', type: 'search', placeholder: 'Search keys, media, macros', 'aria-label': 'Search keys, media, macros' });
    const body = el('div', { class: 'mlist' });
    const draw = () => {
      body.replaceChildren();
      const t = q.value.trim();
      if (t) {
        const box = el('div');
        pickList(box, [['', [...G.macros, ...G.keys, ...G.media, ...G.mouse, ...G.profiles, ...G.board]]], def, on, '');
        const inp = box.querySelector('input'); inp.value = t; inp.dispatchEvent(new Event('input'));
        body.append(box.querySelector('.mlist'));
        return;
      }
      if (G.macros.length) {
        body.append(mHead('Macros'));
        for (const [label, v] of G.macros) body.append(mItem(label, () => { closeMenu(); on(v); }, { tick: def[0] === 3 && def[1] === v[1] }));
        body.append(el('hr'));
      }
      body.append(mHead('Other'));
      for (const [key, [label]] of Object.entries(pages)) body.append(mItem(label + ' ›', () => { page = key; again(); }, { tick: kindOf(def) === pages[key][1][0]?.[2] && def[0] !== 3 }));
      body.append(mItem('Nothing', () => { closeMenu(); on(NOTHING); }, { tick: isNothing(def) }));
    };
    q.addEventListener('input', draw);
    q.addEventListener('keydown', e => { if (e.key === 'Enter') body.querySelector('.mi')?.click(); });
    draw();
    m.append(q, body);
  }, { width: 300 });
}

// How a macro plays is stored on each key it is assigned to. The editor treats
// it as one setting of the macro: this reads it back from those keys.
function macroKeys(n) {
  const out = [];
  for (const p of [1, 2, 3, 4, 5]) for (const layer of ['keys', 'fnkeys'])
    S.drafts[p][layer].forEach((d, pos) => { if (d[0] === 3 && d[1] === n) out.push({ p, layer, pos, def: d }); });
  return out;
}
function macroPlays(n) {
  const ks = macroKeys(n);
  if (!ks.length) return null;
  const modes = new Set(ks.map(k => `${k.def[2]}:${k.def[2] === 11 ? k.def[3] : 0}`));
  if (modes.size > 1) return 'varies';
  return { mode: ks[0].def[2], count: ks[0].def[3] || 2 };
}

function keyLabel(k) {
  const L = LEGEND[k.id];
  const main = typeof L === 'string' ? L : L?.main;
  return FULLNAME[k.id] || (main && main.length > 1 ? main[0] + main.slice(1).toLowerCase() : main) || k.id;
}

function deselectKey() {
  if (S.sel == null) return;
  S.sel = null; closeMenu();
  renderView(); renderBoardLabels();
}

function renderKeys() {
  const box = $('#view-keys'); box.replaceChildren();
  if (S.sel == null) { box.append(el('p', { class: 'keys-empty' }, 'Choose a key on the keyboard to see and change what it does.')); return; }
  const k = S.byPos[S.sel], fac = saved(254).keys[k.pos];
  box.append(el('div', { class: 'keypanel' },
    el('div', { class: 'kid' }, el('div', { class: 'kname' }, keyLabel(k)),
      el('div', { class: 'kfac' }, `Factory: ${isNothing(fac) ? 'does nothing' : describe(fac)}`)),
    layerEditor('When pressed', 'keys', k),
    layerEditor('When Fn is held', 'fnkeys', k),
    el('div', { class: 'kfoot' }, el('button', { class: 'ghost', onclick: () => change(d => {
      d.keys[k.pos] = clone(saved(254).keys[k.pos]); d.fnkeys[k.pos] = clone(saved(254).fnkeys[k.pos]);
    }, true) }, `Reset ${keyLabel(k)} to factory`))));
}

function layerEditor(title, layer, k) {
  const def = draft()[layer][k.pos];
  const set = v => {
    // a macro keeps playing the way it already plays on its other keys
    if (v[0] === 3) { const pl = macroPlays(v[1]); v = [3, v[1], pl && pl !== 'varies' ? pl.mode : 0, pl && pl !== 'varies' && pl.mode === 11 ? pl.count : 0]; }
    change(d => { d[layer][k.pos] = v; }, true);
  };
  const kind = kindOf(def);
  const btn = el('button', { class: 'actbtn', 'aria-haspopup': 'menu', onclick: e => actionMenu(e.currentTarget, def, set) },
    kind ? el('span', { class: 'kind' }, kind) : null,
    el('span', { class: 'al' + (isNothing(def) ? ' quiet' : '') }, def[0] === 3 ? (S.dev.macros[def[1]]?.name || `Macro ${def[1]}`) : isNothing(def) ? 'Nothing' : describe(def)),
    CHEV());
  const cap = el('button', { class: 'tb', onclick: ev => listen(ev.currentTarget, layer, k.pos) }, 'Press a key');
  const out = el('div', { class: 'layer2' }, el('div', { class: 'lh' }, title), el('div', { class: 'actrow' }, btn, cap));
  if (def[0] === 3) {
    const m = S.dev.macros[def[1]];
    const held = m ? m.steps.filter(s => s.down).map(stepName) : [];
    out.append(el('div', { class: 'actsub' },
      held.slice(0, 6).flatMap((n, i) => [i ? el('span', { class: 'plus' }, '+') : null, el('span', { class: 'ev sm' }, n)]),
      el('a', { class: 'tolink', href: '#', onclick: e => { e.preventDefault(); goMacro(def[1]); } }, 'Edit in Macro Editor ›')));
  }
  return out;
}

// "Press a key to assign": capture the next key combination typed on the page.
function listen(btn, layer, pos) {
  if (S.listening) return;
  S.listening = true;
  btn.classList.add('listening'); btn.textContent = 'Press a key or combination…';
  let mods = 0;
  const done = def => {
    removeEventListener('keydown', kd, true); removeEventListener('keyup', ku, true);
    S.listening = false;
    if (def) change(d => { d[layer][pos] = def; }, true); else renderView();
  };
  const kd = e => {
    e.preventDefault(); e.stopPropagation();
    if (CODE_TO_MOD[e.code]) { mods |= CODE_TO_MOD[e.code]; return; }
    const h = CODE_TO_HID[e.code];
    if (h) done([0, mods, h, 0]);
  };
  const ku = e => { // a lone modifier press and release assigns that modifier
    e.preventDefault();
    if (CODE_TO_MOD[e.code] && mods === CODE_TO_MOD[e.code]) done([0, mods, 0, 0]);
    else if (CODE_TO_MOD[e.code]) mods &= ~CODE_TO_MOD[e.code];
  };
  addEventListener('keydown', kd, true); addEventListener('keyup', ku, true);
}

// ---------------------------------------------------------------- display view
function renderDisplay() {
  const box = $('#view-display'); box.replaceChildren();
  const L = draft().lighting, lc = L.lcd.color;
  const grid = el('div', { class: 'grid settings' });
  grid.append(field('Profile name',
    el('input', { class: 'namefield', type: 'text', maxlength: 60,
      value: draft().name ?? saved(S.editing).name,
      'aria-label': 'Profile name',
      oninput: e => change(d => { d.name = e.target.value; }) }),
    'Stored on the keyboard with this profile.'));
  // the screen's colour and its brightness sit together, as one column
  grid.append(el('div', { class: 'field stack' },
    field('Screen backlight', el('div', { class: 'row' }, colorInput(lc.color, v => change(() => { lc.color = v; }, true)))),
    rangeField('Backlight brightness', 0, 255, lc.brightness, v => `${Math.round(v / 2.55)}%`, v => change(() => { lc.brightness = v; }))));
  grid.append(field('Built-in screens', el('div', {}, LCD_APPS.map(([bit, name]) => el('label', { class: 'check' },
    el('input', { type: 'checkbox', checked: !!(L.lcd.apps & bit), onchange: e => change(() => { L.lcd.apps = e.target.checked ? L.lcd.apps | bit : L.lcd.apps & ~bit; }) }), name))),
    'Screens the keyboard can show by itself. Your z10-lcd stats service draws over them while it runs.'));
  const blocked = ~L.game_keys & 0x7f;
  grid.append(field('Blocked in game mode', el('div', {}, GAME_KEYS.map(([bit, name]) => el('label', { class: 'check' },
    el('input', { type: 'checkbox', checked: !!(blocked & bit), onchange: e => change(() => { const b = e.target.checked ? blocked | bit : blocked & ~bit; L.game_keys = (~b & 0x7f) | 0x80; }, true) }), name))),
    'These keys do nothing while game mode is on.'));
  box.append(grid);
}

// ---------------------------------------------------------------- macros
// A macro workspace on SteelSeries Engine's model: the library on the left,
// and the chosen macro as a chain of events you record or build by hand.
// A step is {code|media, down, delay}: a key or media key going down or up,
// then how long to wait before the next step.
const MACRO_MEDIA = { 0xB5: 'Next track', 0xB6: 'Previous track', 0xB7: 'Stop', 0xCD: 'Play/pause',
  0xE2: 'Mute', 0xE9: 'Volume up', 0xEA: 'Volume down', 0x183: 'Media player', 0x18A: 'Mail',
  0x192: 'Calculator', 0x194: 'My computer', 0x221: 'Browser search', 0x223: 'Browser home',
  0x224: 'Browser back', 0x225: 'Browser forward', 0x227: 'Browser refresh' };
const MACRO_ROOM = 967;                    // bytes of steps the keyboard keeps per macro
const PLAYS = [[0, 'Play once'], [11, 'Play a number of times'], [12, 'Repeat while held'], [8, 'Repeat until pressed again']];

function macroKeyName(code) { return MACRO_KEYS[code] || `Code ${code}`; }
function stepName(st) {
  return 'media' in st ? (MACRO_MEDIA[st.media] || `Media ${st.media}`) : macroKeyName(st.code);
}
function describeMacro(steps) {
  if (!steps.length) return 'No steps yet';
  const held = [...new Set(steps.filter(s => s.down).map(stepName))];
  return held.join(' + ') + (steps.some(s => s.delay) ? ', with timing' : '');
}
const stepBytes = st => ('media' in st ? 3 : 1) + 3;
const macroBytes = steps => steps.reduce((n, st) => n + stepBytes(st), 0);
const stepKey = st => ('media' in st ? 'm' : 'k') + (st.media ?? st.code);

function freeSlots() {
  const out = [];
  for (let i = 1; i <= 100; i++) if (!S.dev.macros[i] && !S.mdrafts[i]) out.push(i);
  return out;
}
function mdraft(n = S.macro) {
  if (n == null) return null;
  if (!S.mdrafts[n]) {
    const m = S.dev.macros[n];
    if (!m) return null;
    S.mdrafts[n] = { name: m.name, steps: clone(m.steps), isNew: false, timing: 'keep', same: 20, plays: null };
  }
  return S.mdrafts[n];
}
// the steps as they will be saved, with the timing choice applied
function effective(d) {
  const st = clone(d.steps);
  if (d.timing === 'none') st.forEach(s => { s.delay = 0; });
  if (d.timing === 'same') st.forEach((s, i) => { s.delay = i < st.length - 1 ? d.same : 0; });
  return st;
}
function macroDirty(n) {
  const d = S.mdrafts[n];
  if (!d) return false;
  if (d.isNew) return !pristine(n);
  const m = S.dev.macros[n];
  return d.name !== m.name || JSON.stringify(effective(d)) !== JSON.stringify(m.steps) || !!d.plays;
}
// where a macro is assigned, one entry per key and layer, with its profiles
function macroPlaces(n) {
  const by = {};
  for (const k of macroKeys(n)) {
    const id = `${k.layer}:${k.pos}`;
    (by[id] ||= { layer: k.layer, pos: k.pos, profiles: [] }).profiles.push(k.p);
  }
  return Object.values(by);
}
const profilesText = ps => ps.length === 5 ? 'All profiles' : ps.length === 1 ? `Profile ${ps[0]}`
  : `Profiles ${ps.slice(0, -1).join(', ')} and ${ps[ps.length - 1]}`;
const placeKey = pl => (pl.layer === 'fnkeys' ? 'Fn + ' : '') + keyLabel(S.byPos[pl.pos]);

function goMacro(n) {
  closeMenu();
  // the Macro Editor is its own window
  if (window.__window === 'main') { window.__openMacroEditor(n); return; }
  S.view = 'keys'; S.macro = n; S.selStep = null; S.ins = null; S.scrollToMacro = true; S.macrosOpen = true;
  renderView();
}
function goKey(pl) {
  S.sel = pl.pos;
  S.view = 'keys';
  if (pl.profiles[0] !== S.editing) chooseProfile(pl.profiles[0]);
  renderView(); renderBoardLabels(); stopPreview();
  $('#board').scrollIntoView({ behavior: 'smooth', block: 'nearest' });
}

// The Macro Editor folds down to its head row when it is not in use. It opens
// when asked, and by itself when a key's macro is opened, a new one is started
// or a recording runs. Whether it was left open is remembered in this browser.
function macrosOpen() {
  if (S.rec || S.macrosOpen) return true;
  if (S.macrosOpen === false) return false;
  try { return localStorage.getItem('z10:macros-open') === '1'; } catch (e) { return false; }
}
function setMacrosOpen(on) {
  S.macrosOpen = on;
  try { localStorage.setItem('z10:macros-open', on ? '1' : '0'); } catch (e) { /* not fatal */ }
  // only the fold changes, so it can animate and nothing else on the page moves
  const box = $('#view-macros');
  box.classList.toggle('collapsed', !on);
  box.querySelector('.sec-head')?.setAttribute('aria-expanded', String(on));
  const fold = box.querySelector('.fold'); if (fold) fold.inert = !on;   // folded away, nothing in it takes the keyboard
}

function renderMacros() {
  const box = $('#view-macros'); box.replaceChildren();
  const macros = S.dev.macros || {};
  // With nothing chosen, the editor holds a blank new macro, ready to name and
  // record. It stays out of the list, and does not count as unsaved, until
  // something is put in it.
  if (S.macro == null || (!macros[S.macro] && !S.mdrafts[S.macro])) S.macro = starterMacro();
  const saved = Object.keys(macros).map(Number).sort((a, b) => a - b);
  const drafts = Object.keys(S.mdrafts).map(Number)
    .filter(n => S.mdrafts[n].isNew && !(S.mdrafts[n].starter && pristine(n))).sort((a, b) => a - b);

  const row = n => {
    const m = macros[n], d = S.mdrafts[n];
    const steps = d ? effective(d) : m.steps;
    const timed = steps.some(s => s.delay);
    return el('button', { 'aria-current': String(S.macro === n), class: macroDirty(n) ? 'dirty' : '',
      // choosing the open macro again puts it away, back to the blank one
      onclick: () => { S.macro = S.macro === n ? null : n; S.selStep = null; S.ins = null; S.editName = false; renderMacros(); },
      // right-clicking a macro opens it and offers Rename, Duplicate and Delete where the pointer is
      oncontextmenu: e => {
        e.preventDefault();
        if (S.rec) return;
        if (S.macro !== n) { S.macro = n; S.selStep = null; S.ins = null; S.editName = false; renderMacros(); }
        macroMenu(pointAt(e.clientX, e.clientY));
      } },
      el('span', { class: 'ico' }, timed ? ICO.clock() : ICO.keys()),
      el('span', { class: 'name' }, (d ? d.name : m.name) || `Macro ${n}`),
      el('span', { class: 'sub' }, d?.isNew ? 'Not saved yet' : describeMacro(steps)));
  };
  const lib = el('div', { class: 'lib' },
    el('div', { class: 'lib-head' }, el('b', {}, 'All macros'), el('span', {}, `${saved.length} of 100 slots`),
      el('button', { class: 'tb add', disabled: !freeSlots().length || !!S.rec, onclick: newMacro }, '+ New')),
    el('div', { class: 'macro-list' }, [...saved, ...drafts].map(row)));
  const ws = S.macro != null && mdraft() ? workspace() : el('div', { class: 'ws empty-ws' },
    el('p', { class: 'quiet' }, saved.length ? 'Choose a macro.' : 'No macros yet. Press + New to make one.'));
  const open = macrosOpen();
  const unsaved = [...saved, ...drafts].some(macroDirty);
  box.classList.toggle('collapsed', !open);
  box.append(el('button', { class: 'sec-head', 'aria-expanded': String(open), 'aria-controls': 'macro-body',
      onclick: () => setMacrosOpen(box.classList.contains('collapsed')) },
      el('h2', { id: 'macros-title' }, 'Macro Editor'),
      unsaved ? el('span', { class: 'unsaved' }, 'Unsaved changes') : null,
      el('span', { class: 'count' }, `${saved.length} saved`),
      CHEV()),
    el('div', { class: 'fold', id: 'macro-body', inert: !open }, el('div', { class: 'fold-in' }, el('div', { class: 'mwrap' }, lib, ws))));
  // an input that just opened for editing takes the cursor straight away
  const edit = box.querySelector('.wt.edit input, .title-in');
  if (edit && document.activeElement !== edit) { edit.focus(); edit.select(); }
  const cur = box.querySelector('.macro-list [aria-current="true"]');
  if (cur && S.scrollToMacro) { cur.scrollIntoView({ block: 'nearest' }); S.scrollToMacro = false; }
}

const ICO = {
  keys: () => svg('<rect x="1.5" y="4" width="13" height="8.5" rx="1.6"/><path d="M4 7h1M7 7h1M10 7h2M4.5 9.8h7"/>'),
  clock: () => svg('<circle cx="8" cy="8.5" r="5.8"/><path d="M8 5.3v3.2l2.2 1.4M6.5 1.5h3"/>'),
  gear: () => svg('<circle cx="8" cy="8" r="2.3"/><path d="M8 1.5v2M8 12.5v2M1.5 8h2M12.5 8h2M3.4 3.4l1.4 1.4M11.2 11.2l1.4 1.4M3.4 12.6l1.4-1.4M11.2 4.8l1.4-1.4"/>'),
  pen: () => svg('<path d="M10.5 2.5l3 3L6 13H3v-3z"/>'),
};
function svg(inner) {
  const s = document.createElementNS('http://www.w3.org/2000/svg', 'svg');
  s.setAttribute('viewBox', '0 0 16 16'); s.setAttribute('fill', 'none'); s.setAttribute('stroke', 'currentColor'); s.setAttribute('stroke-width', '1.4');
  s.innerHTML = inner; return s;
}

// A new macro is called New Macro 1, 2 and so on, the lowest number no other
// macro is using.
function newMacroName() {
  const used = new Set([...Object.values(S.dev.macros).map(m => m.name), ...Object.values(S.mdrafts).map(d => d.name)]);
  let k = 1;
  while (used.has(`New Macro ${k}`)) k++;
  return `New Macro ${k}`;
}
// the blank macro the editor opens on, kept in one free slot until it is used
function starterMacro() {
  const have = Object.keys(S.mdrafts).map(Number).find(n => S.mdrafts[n].starter);
  if (have) return have;
  const n = freeSlots()[0];
  if (!n) return null;
  const name = newMacroName();
  S.mdrafts[n] = { name, blankName: name, steps: [], isNew: true, starter: true, timing: 'keep', same: 20, plays: null };
  return n;
}
// a new macro nobody has put anything in yet
function pristine(n) {
  const d = S.mdrafts[n];
  return !!d && d.isNew && d.name === d.blankName && !d.steps.length && !d.plays;
}

function newMacro() {
  // the blank macro already open simply becomes the new one
  const cur = S.mdrafts[S.macro];
  if (cur?.starter) { delete cur.starter; S.selStep = null; S.ins = null; S.scrollToMacro = true; S.macrosOpen = true; renderMacros(); return; }
  const n = freeSlots()[0];
  if (!n) return;
  const name = newMacroName();
  S.mdrafts[n] = { name, blankName: name, steps: [], isNew: true, timing: 'keep', same: 20, plays: null };
  S.macro = n; S.selStep = null; S.ins = null; S.scrollToMacro = true; S.macrosOpen = true;
  renderMacros();
}

function workspace() {
  const n = S.macro, d = mdraft(), m = S.dev.macros[n], rec = S.rec;
  const locked = m && !m.editable;
  const eff = effective(d);
  const live = rec ? rec.steps : [];
  const ins = S.ins == null ? d.steps.length : S.ins;
  const upd = () => renderMacros();

  // head: the name, whether it is saved, and the less common actions
  const title = S.editName
    ? el('input', { class: 'title-in', value: d.name, maxlength: 50, 'aria-label': 'Macro name',
        onkeydown: e => { if (e.key === 'Enter') commitName(e.target.value); if (e.key === 'Escape') { S.editName = false; upd(); } },
        onblur: e => commitName(e.target.value) })
    : el('span', { class: 'title', ondblclick: () => { S.editName = true; upd(); } }, d.name);
  const state = rec ? el('span', { class: 'state rec' }, el('i', { class: 'blink' }), 'Recording')
    : el('span', { class: 'state' + (macroDirty(n) ? ' warn' : '') }, d.isNew ? 'Not saved yet' : macroDirty(n) ? 'Changed, not saved yet' : 'Saved on the keyboard');
  const more = el('button', { class: 'more', 'aria-label': 'More', disabled: !!rec, onclick: e => moreMenu(e.currentTarget) }, '⋯');
  const head = el('div', { class: 'ws-head' }, title,
    el('button', { class: 'rename', 'aria-label': 'Rename', disabled: locked || !!rec, onclick: () => { S.editName = true; upd(); } }, ICO.pen()),
    state, more);

  // the chain
  const chainEl = el('div', { class: 'chain' });
  const joint = i => {
    const hot = S.ins === i && i < d.steps.length;
    return el('span', { class: 'cn' + (hot ? ' hot' : ''), title: 'Insert here',
      onclick: e => { if (locked || rec) return; S.ins = i; S.selStep = null; upd(); insertMenu(document.querySelector('.cn.hot') || e.currentTarget); } },
      hot ? el('span', { class: 'plus' }, '+') : null);
  };
  const chip = (st, i, liveChip) => el('span', {
      class: 'ev' + ('media' in st ? ' media' : '') + (S.selStep === i && !liveChip ? ' sel' : '') + (liveChip ? ' fresh' : ''),
      draggable: !liveChip && !locked && !rec ? 'true' : 'false', 'data-i': i,
      onclick: e => { if (liveChip || locked || rec) return; S.selStep = i; S.ins = i + 1; upd(); stepMenu(document.querySelector(`.ev[data-i="${i}"]`) || e.currentTarget, i); },
      ondragstart: e => { e.dataTransfer.setData('text/plain', String(i)); e.dataTransfer.effectAllowed = 'move'; },
      ondragover: e => { if (!liveChip) { e.preventDefault(); e.currentTarget.classList.add('dropto'); } },
      ondragleave: e => e.currentTarget.classList.remove('dropto'),
      ondrop: e => { e.preventDefault(); const from = +e.dataTransfer.getData('text/plain'); moveStep(from, i); },
    },
    'media' in st ? el('span', { class: 'ic' }, '♫') : null, stepName(st),
    el('span', { class: 'dir ' + (st.down ? 'dn' : 'up') }, st.down ? '↓' : '↑'));
  const waitChip = (st, i, ms) => S.editWait === i
    ? el('span', { class: 'wt edit' }, ICO.clock(), el('input', { value: ms, 'aria-label': 'Wait in milliseconds', inputmode: 'numeric',
        onkeydown: e => { if (e.key === 'Enter') setWait(i, e.target.value); if (e.key === 'Escape') { S.editWait = null; upd(); } },
        onblur: e => setWait(i, e.target.value) }), ' ms')
    : el('span', { class: 'wt', onclick: () => { if (locked || rec) return; bakeTiming(d); S.editWait = i; upd(); } }, `${ms.toLocaleString()} ms `, ICO.clock());

  if (!eff.length && !live.length) {
    chainEl.classList.add('empty');
    chainEl.append(el('div', { class: 'empty' }, el('div', { class: 'big' }, el('i')), rec
      ? [el('b', {}, 'Recording'),
         el('p', {}, 'Type the keys on your Z10. Press Stop to finish.')]
      : [el('b', {}, 'No keys yet'),
         el('p', {}, 'Press Record and type the keys on your Z10, or add keys, media keys and waits with the buttons below.')]));
  } else {
    eff.forEach((st, i) => {
      if (i) chainEl.append(joint(i));
      if (rec && i === ins) liveChain(chainEl, live, chip, waitChip);
      chainEl.append(chip(st, i));
      if (st.delay) { chainEl.append(el('span', { class: 'cn plain' })); chainEl.append(waitChip(st, i, st.delay)); }
    });
    if (rec && ins >= eff.length) liveChain(chainEl, live, chip, waitChip);
    if (rec) chainEl.append(el('span', { class: 'caret' }));
    else if (!locked) chainEl.append(el('span', { class: 'ins' + (S.ins == null || S.ins >= eff.length ? '' : ' off'), title: 'New steps are added here',
      onclick: () => { S.ins = null; upd(); } }));
  }

  // the toolbar under the chain
  const bytes = macroBytes(eff) + (rec ? rec.used : 0);
  const left = MACRO_ROOM - bytes;
  const count = eff.length + eff.filter(s => s.delay).length;
  const meter = left < 60
    ? el('span', { class: 'meter warn' }, left < 4 ? 'Full' : `Nearly full: room for about ${Math.floor(left / 4)} more key presses`)
    : el('span', { class: 'meter' }, count ? `${count} step${count === 1 ? '' : 's'}` : '');
  const off = locked || !!rec;
  const tools = el('div', { class: 'tools' },
    el('button', { class: 'tb icon', 'aria-label': 'Timing', disabled: off, onclick: e => timingMenu(e.currentTarget) }, ICO.gear()),
    el('button', { class: 'tb', disabled: off || !d.steps.length, onclick: () => { d.steps = []; S.ins = null; S.selStep = null; upd(); } }, 'Clear'),
    el('span', { class: 'sep' }),
    el('button', { class: 'tb', disabled: off || left < 8, onclick: e => keyPicker(e.currentTarget, 'key', v => insertTap(v)) }, '+ Key'),
    el('button', { class: 'tb', disabled: off || left < 12, onclick: e => keyPicker(e.currentTarget, 'media', v => insertTap(v)) }, '+ Media key'),
    el('button', { class: 'tb', disabled: off || !d.steps.length, onclick: () => insertWait() }, '+ Wait'),
    meter,
    rec ? el('span', { class: 'clock' }, clockText(rec.elapsed)) : null,
    rec ? el('button', { class: 'recbtn on', onclick: () => stopRecording(true) }, el('span', { class: 'dot' }), 'Stop')
        : el('button', { class: 'recbtn', disabled: locked || left < 4, onclick: startRecording }, el('span', { class: 'dot' }), 'Record'));
  const canvas = el('div', { class: 'canvas' + (rec ? ' rec' : '') }, chainEl, tools);

  // how it plays on the keys it is on; where it is on them is the Keys page's business
  const places = macroPlaces(n);
  const plays = d.plays || macroPlays(n);
  const playsLabel = !places.length ? 'Play once' : plays === 'varies' ? 'Varies by key'
    : (PLAYS.find(([v]) => v === plays.mode)?.[1] || 'Play once') + (plays.mode === 11 ? ` (${plays.count} times)` : '');
  const playsEl = el('div', { class: 'playsrow' },
    el('span', { class: 'lab' }, 'Plays'),
    el('button', { class: 'plays', 'aria-haspopup': 'menu', disabled: !places.length || off,
      title: places.length ? '' : 'Put this macro on a key first, then choose how it plays there.',
      onclick: e => playsMenu(e.currentTarget) }, playsLabel, CHEV()),
    plays && plays !== 'varies' && plays.mode === 11 ? el('label', { class: 'count' }, el('input', { type: 'number', min: 2, max: 255, value: plays.count,
      'aria-label': 'Times', onchange: e => { d.plays = { mode: 11, count: Math.max(2, Math.min(255, +e.target.value || 2)) }; upd(); } }), 'times') : null);

  const dirty = macroDirty(n) && !rec;
  const foot = el('div', { class: 'ws-foot' }, playsEl,
    locked ? el('span', { class: 'readonly' }, 'This macro holds something the editor cannot rebuild exactly, so it is shown read only.') : null,
    window.__window === 'macros' ? STATUS : null,
    el('button', { class: 'ghost', disabled: !dirty || S.busy, onclick: revertMacro }, 'Revert'),
    el('button', { class: 'primary', disabled: !dirty || S.busy || (!d.steps.length), onclick: saveMacro }, 'Save macro'));
  return el('div', { class: 'ws' }, head, canvas, foot);
}

// The chain is drawn again at every look at the recording, several times a
// second, so a step pops in once, when it arrives: each one's arrival is kept,
// and a redrawn step carries its pop on from where it had got to, or has none.
function liveChain(parent, live, chip, waitChip) {
  const seen = S.rec ? (S.rec.seen ??= {}) : {}, now = performance.now();
  const pop = (node, id) => {
    const age = now - (seen[id] ??= now);
    if (age >= 400) node.classList.remove('fresh');
    else node.style.animationDelay = `-${Math.round(age)}ms`;
    return node;
  };
  live.forEach((st, j) => {
    parent.append(el('span', { class: 'cn plain' }));
    parent.append(pop(chip(st, 1000 + j, true), 's' + j));
    if (st.delay) { parent.append(el('span', { class: 'cn plain' })); parent.append(pop(el('span', { class: 'wt fresh' }, `${st.delay.toLocaleString()} ms `, ICO.clock()), 'w' + j)); }
  });
}
const clockText = s => `${Math.floor(s / 60)}:${(s % 60).toFixed(1).padStart(4, '0')}`;

// the timing choice becomes the steps' own waits the moment one is edited by hand
function bakeTiming(d) {
  if (d.timing === 'keep') return;
  d.steps = effective(d); d.timing = 'keep';
}
function commitName(v) {
  if (!S.editName) return;
  const d = mdraft(); d.name = (v || '').trim().slice(0, 50) || d.name;
  S.editName = false; renderMacros();
}
function setWait(i, v) {
  if (S.editWait !== i) return;           // already committed
  const d = mdraft(); bakeTiming(d);
  const ms = Math.max(0, Math.min(65535, Math.round(+String(v).replace(/[^0-9]/g, '') || 0)));
  if (d.steps[i]) d.steps[i].delay = ms;
  S.editWait = null; renderMacros();
}
function moveStep(from, to) {
  const d = mdraft(); bakeTiming(d);
  if (from === to || from == null) return renderMacros();
  const [st] = d.steps.splice(from, 1);
  d.steps.splice(from < to ? to - 1 : to, 0, st);
  S.selStep = null; S.ins = null; renderMacros();
}
function insertAt(steps) {
  const d = mdraft(); bakeTiming(d);
  const i = S.ins == null ? d.steps.length : S.ins;
  d.steps.splice(i, 0, ...steps);
  S.ins = S.ins == null ? null : i + steps.length;
  S.selStep = null; renderMacros();
}
// a key added by hand goes down and comes back up, so it can never be left held
function insertTap(v) {
  insertAt([{ ...v, down: true, delay: 0 }, { ...v, down: false, delay: 0 }]);
}
function insertWait() {
  const d = mdraft(); bakeTiming(d);
  const i = (S.ins == null ? d.steps.length : S.ins) - 1;
  if (i < 0) return;
  if (!d.steps[i].delay) d.steps[i].delay = 100;
  S.editWait = i; renderMacros();
}

// what dismissing a menu opened from the chain does: nothing stays chosen
const letGo = () => { S.ins = null; S.selStep = null; renderMacros(); };
function keyPicker(anchor, kind, on, current, onDismiss) {
  const groups = kind === 'media'
    ? [['Media keys', Object.entries(MACRO_MEDIA).map(([c, n]) => [n, { media: +c }])]]
    : [['Modifiers', [112, 113, 114, 115, 116, 117, 118, 119].map(c => [MACRO_KEYS[c], { code: c }])],
       ['Keys', Object.entries(MACRO_KEYS).filter(([c]) => +c < 112).map(([c, n]) => [n, { code: +c }])]];
  openMenu(anchor, m => pickList(m, groups, current, on, kind === 'media' ? 'Search media keys' : 'Search keys'), { width: 260, onDismiss });
}
function insertMenu(anchor) {
  openMenu(anchor, m => {
    m.append(mHead('Insert here'),
      mItem('Key', () => keyPicker(anchor, 'key', v => insertTap(v), undefined, letGo)),
      mItem('Media key', () => keyPicker(anchor, 'media', v => insertTap(v), undefined, letGo)),
      mItem('Wait', () => { closeMenu(); insertWait(); }));
  }, { width: 220, onDismiss: letGo });
}
function stepMenu(anchor, i) {
  const d = mdraft();
  openMenu(anchor, (m, again) => {
    const st = d.steps[i]; if (!st) return;
    m.append(mHead(stepName(st)),
      mItem('Press ↓', () => { st.down = true; closeMenu(); renderMacros(); }, { tick: st.down }),
      mItem('Release ↑', () => { st.down = false; closeMenu(); renderMacros(); }, { tick: !st.down }),
      el('hr'),
      mItem('Change key ›', () => keyPicker(anchor, 'media' in st ? 'media' : 'key', v => {
        delete st.code; delete st.media; Object.assign(st, v); renderMacros();
      }, 'media' in st ? { media: st.media } : { code: st.code }, letGo)),
      mItem('Remove', () => { d.steps.splice(i, 1); S.selStep = null; S.ins = null; closeMenu(); renderMacros(); }, { cls: 'danger' }));
  }, { width: 220, onDismiss: letGo });
}
function timingMenu(anchor) {
  const d = mdraft();
  openMenu(anchor, (m, again) => {
    m.append(mHead('Timing'),
      mItem('Keep the waits as they are', () => { d.timing = 'keep'; closeMenu(); renderMacros(); }, { tick: d.timing === 'keep' }),
      mItem('No waits', () => { d.timing = 'none'; closeMenu(); renderMacros(); }, { tick: d.timing === 'none' }),
      el('div', { class: 'mi row-in' + (d.timing === 'same' ? ' on' : '') },
        el('span', { class: 'tick' }, d.timing === 'same' ? '✓' : ''),
        el('button', { class: 'plain', onclick: () => { d.timing = 'same'; closeMenu(); renderMacros(); } }, 'Same wait everywhere'),
        el('input', { type: 'number', min: 0, max: 65535, value: d.same, 'aria-label': 'Milliseconds',
          onchange: e => { d.same = Math.max(0, Math.min(65535, +e.target.value || 0)); d.timing = 'same'; renderMacros(); again(); } }), ' ms'),
      el('hr'),
      mItem('Record waits while recording', () => { S.recordWaits = !S.recordWaits; try { localStorage.setItem('z10:recordWaits', S.recordWaits ? '1' : '0'); } catch (e) {} again(); }, { tick: S.recordWaits }));
  }, { width: 290 });
}
function playsMenu(anchor) {
  const d = mdraft(), cur = d.plays || macroPlays(S.macro);
  openMenu(anchor, m => {
    m.append(mHead('Each time its key is pressed'),
      ...PLAYS.map(([v, label]) => mItem(label, () => {
        closeMenu(); d.plays = { mode: v, count: cur && cur !== 'varies' && cur.count > 1 ? cur.count : 3 }; renderMacros();
      }, { tick: cur && cur !== 'varies' && cur.mode === v })));
  }, { width: 260 });
}
// A place on the screen standing in for a button, for a menu opened where the
// pointer is rather than under a button.
const pointAt = (x, y) => ({ getBoundingClientRect: () => new DOMRect(x, y, 0, 0), contains: () => false });

// The open macro's own menu: from its More button, or from right-clicking it in the list.
function moreMenu(anchor) { macroMenu(anchor, { width: 240, align: 'right' }); }
function macroMenu(anchor, place = { width: 220 }) {
  const n = S.macro, d = mdraft(), saved = !!S.dev.macros[n], m0 = S.dev.macros[n];
  openMenu(anchor, m => {
    m.append(
      mItem('Rename', () => { closeMenu(); S.editName = true; renderMacros(); }, { disabled: !!(m0 && !m0.editable) }),
      mItem('Duplicate', () => {
        closeMenu();
        const k = freeSlots()[0]; if (!k) { status('All 100 macro slots are full.', 'bad'); return; }
        S.mdrafts[k] = { name: `Copy of ${d.name}`.slice(0, 50), steps: effective(d), isNew: true, timing: 'keep', same: 20, plays: null };
        S.macro = k; S.selStep = null; S.ins = null; S.scrollToMacro = true; renderMacros();
      }, { disabled: !freeSlots().length }),
      mItem(saved ? 'Delete macro…' : 'Discard this new macro', () => {
        closeMenu();
        if (!saved) { delete S.mdrafts[n]; S.macro = null; renderMacros(); return; }
        const places = macroPlaces(n);
        confirmDialog({
          title: `Delete ${d.name}?`,
          body: [places.length
              ? `${places.map(pl => `${placeKey(pl)} (${profilesText(pl.profiles).toLowerCase()})`).join(', ')} will do nothing until you give ${places.length === 1 ? 'it' : 'them'} something else.`
              : 'No key runs this macro.',
            'This removes it from the keyboard and cannot be undone.'],
          confirm: 'Delete macro', danger: true, onConfirm: deleteMacro,
        });
      }, { cls: 'danger' }));
  }, place);
}

// A modal confirmation. Escape or a click outside cancels; Cancel has the focus,
// so pressing Enter by reflex never deletes anything.
function confirmDialog({ title, body, confirm, danger, onConfirm }) {
  const close = () => { removeEventListener('keydown', key, true); back.remove(); };
  const key = e => { if (e.key === 'Escape') { e.preventDefault(); close(); } };
  const cancel = el('button', { class: 'ghost', onclick: close }, 'Cancel');
  const box = el('div', { class: 'dialog', role: 'alertdialog', 'aria-modal': 'true', 'aria-labelledby': 'dlg-t' },
    el('h3', { id: 'dlg-t' }, title), ...[].concat(body).map(t => el('p', {}, t)),
    el('div', { class: 'row' }, cancel,
      el('button', { class: danger ? 'danger' : 'primary', onclick: () => { close(); onConfirm(); } }, confirm)));
  const back = el('div', { class: 'scrim', onmousedown: e => { if (e.target === back) close(); } }, box);
  document.body.append(back);
  addEventListener('keydown', key, true);
  cancel.focus();
}

// ---- recording -------------------------------------------------------------
async function startRecording() {
  const d = mdraft(); bakeTiming(d);
  closeMenu(); S.selStep = null; S.editWait = null;
  try {
    const r = await api('/api/record', { action: 'start', room: MACRO_ROOM - macroBytes(d.steps), waits: S.recordWaits });
    S.rec = { ...r, steps: [], used: 0, elapsed: 0 };
    status('Recording. The Z10’s keys go only to the recording until you press Stop.');
  } catch (e) {
    status(`Recording did not start: ${e.message}`, 'bad'); return;
  }
  renderMacros();
  const tick = async () => {
    if (!S.rec) return;
    try {
      const r = await api('/api/record');
      if (!S.rec) return;
      Object.assign(S.rec, r);
      if (!r.recording) return stopRecording(true, r.reason);
      renderMacros();
    } catch (e) { return stopRecording(true); }
    S.recTimer = setTimeout(tick, 150);
  };
  S.recTimer = setTimeout(tick, 150);
}
async function stopRecording(keep, reason) {
  if (!S.rec) return;
  clearTimeout(S.recTimer);
  S.rec = null;
  let r = { steps: [] };
  try { r = await api('/api/record', { action: keep ? 'stop' : 'cancel' }); } catch (e) { status(`Recording stopped: ${e.message}`, 'bad'); }
  const got = r.steps || [];
  if (got.length) insertAt(got); else renderMacros();
  const why = reason && reason !== 'stopped' ? ` It stopped because ${reason}.` : '';
  status(got.length ? `Recorded ${got.length} step${got.length === 1 ? '' : 's'}.${why}` + (r.skipped ? ` ${r.skipped} key${r.skipped === 1 ? '' : 's'} a macro cannot hold were left out.` : '')
    : `Nothing was recorded.${why}`);
}

// ---- save, revert, delete --------------------------------------------------
function revertMacro() {
  const n = S.macro;
  if (S.mdrafts[n]?.isNew) { S.mdrafts[n] = { ...S.mdrafts[n], steps: [], timing: 'keep', plays: null }; }
  else delete S.mdrafts[n];
  S.selStep = null; S.ins = null; S.editWait = null;
  renderMacros();
}

async function saveMacro() {
  const n = S.macro, d = mdraft();
  let steps = effective(d);
  // anything pressed and never let go is let go at the end, so the macro can
  // never leave a key held down
  const held = [];
  for (const st of steps) { const k = stepKey(st); if (st.down) held.push(st); else { const j = held.findLastIndex(h => stepKey(h) === k); if (j >= 0) held.splice(j, 1); } }
  const added = held.reverse().map(h => ('media' in h ? { media: h.media, down: false, delay: 0 } : { code: h.code, down: false, delay: 0 }));
  steps = steps.concat(added);
  if (macroBytes(steps) > MACRO_ROOM) { status('This macro is too long for the keyboard. Remove some steps and save again.', 'bad'); return; }
  S.busy = true; renderMacros(); status(`Saving ${d.name}…`);
  const m = S.dev.macros[n];
  try {
    S.dev.macros = await api(`/api/macro/${n}`, { name: d.name || `Macro ${n}`, steps,
      run: m?.method ?? 9, repeat: m?.repeat ?? 0, unit: m?.unit ?? 1 });
    // how it plays lives on its keys: rewrite those, profile by profile, on top
    // of what each profile has saved, and carry the change into its draft
    if (d.plays) {
      const byP = {};
      for (const k of macroKeys(n)) (byP[k.p] ||= []).push(k);
      for (const [p, ks] of Object.entries(byP)) {
        const body = { keys: clone(saved(+p).keys), fnkeys: clone(saved(+p).fnkeys) };
        for (const k of ks) {
          const v = [3, n, d.plays.mode, d.plays.mode === 11 ? d.plays.count : 0];
          body[k.layer][k.pos] = v; S.drafts[p][k.layer][k.pos] = clone(v);
        }
        S.dev.profiles[p] = await api(`/api/profile/${p}`, body);
      }
    }
    delete S.mdrafts[n];
    window.__tell('macros', S.dev.macros);
    if (d.plays) window.__tell('profiles', S.dev.profiles);
    status(`Saved ${d.name} to the keyboard.` + (added.length ? ` ${added.map(stepName).join(' and ')} ${added.length === 1 ? 'was' : 'were'} let go at the end so nothing stays held down.` : ''), 'good');
  } catch (e) {
    status(`${d.name} was not saved: ${e.message}`, 'bad');
  }
  S.busy = false; renderMacros(); renderProfiles(); renderActions();
}

async function deleteMacro() {
  const n = S.macro, name = mdraft()?.name || `Macro ${n}`;
  S.busy = true; renderMacros(); status(`Deleting ${name}…`);
  try {
    S.dev.macros = await api(`/api/macro/${n}`, { delete: true });
    window.__tell('macros', S.dev.macros);
    delete S.mdrafts[n];
    S.macro = null;
    status(`Deleted ${name}. Any key set to it now does nothing.`, 'good');
  } catch (e) {
    status(`${name} was not deleted: ${e.message}`, 'bad');
  }
  S.busy = false; renderMacros();
}

// ---------------------------------------------------------------- actions
async function chooseProfile(n) {
  S.editing = n; S.stars = {}; S.flashes = {};
  renderAll();
  if (S.view === 'lighting') startPreview();
  if (n === S.dev.active) return;
  try {
    const r = await api('/api/active', { profile: n });
    S.dev.active = r.active;
    status(`Profile ${n} is now active on the keyboard.`, 'good');
  } catch (e) {
    status(`The keyboard did not switch profiles: ${e.message}`, 'bad');
  }
  renderProfiles();
}

async function save() {
  const n = S.editing;
  S.busy = true; renderActions(); status(`Saving profile ${n}…`);
  try {
    const body = clone(draft());
    if (body.lighting.mode === 'custom') body.lighting.mode = 'off';
    const p = await api(`/api/profile/${n}`, body);
    S.dev.profiles[n] = p;
    window.__tell('profiles', S.dev.profiles);
    S.drafts[n] = clone(pick(p));
    if (draft().lighting.mode === 'off' && customColors().some(c => Math.max(...c))) {
      S.drafts[n].lighting.mode = 'custom';   // keep showing what was painted
    }
    status(`Saved profile ${n} to the keyboard and read it back to check.`, 'good');
  } catch (e) {
    status(`Profile ${n} was not saved: ${e.message}`, 'bad');
  }
  S.busy = false; renderAll();
}

function revert() {
  S.drafts[S.editing] = clone(pick(saved(S.editing)));
  status(`Discarded the unsaved changes to profile ${S.editing}.`);
  renderAll();
}

// ---------------------------------------------------------------- wiring
$('#board').addEventListener('click', e => {
  if (S.view === 'keys' && !e.target.closest('.key')) deselectKey();
});
$('#save').addEventListener('click', save);
$('#revert').addEventListener('click', revert);
for (const b of document.querySelectorAll('.navtabs button')) b.addEventListener('click', () => {
  if (!S.dev) return;                     // still reading the keyboard
  if (S.rec) { status('Stop recording first.'); return; }
  S.view = b.dataset.view;
  renderView(); renderBoardLabels();
  if (S.view === 'lighting') startPreview(); else stopPreview();
});
// A button let go of after a click gives up the focus. Typing on the Z10 is how
// this page is used, and it would otherwise light the browser's keyboard focus
// ring on whatever was last clicked. Moving around with Tab still shows it.
addEventListener('pointerup', e => {
  const b = e.target.closest?.('button');
  if (b && !b.closest('.dialog, .menu')) setTimeout(() => { if (document.activeElement === b) b.blur(); });
}, true);
addEventListener('keydown', e => {
  if (S.listening || !S.dev || e.target.closest?.('input,select')) return;
  // Delete or Backspace removes the chosen step of the open macro
  if (S.view === 'keys' && S.selStep != null && (e.key === 'Delete' || e.key === 'Backspace') && !S.rec && !S.menu) {
    e.preventDefault(); const d = mdraft(); bakeTiming(d); d.steps.splice(S.selStep, 1); S.selStep = null; S.ins = null; renderMacros();
    return;
  }
  const pos = posForEvent(e);
  if (pos == null) return;
  const k = S.byPos[pos]; if (!k) return;
  k.el.classList.add('press'); setTimeout(() => k.el.classList.remove('press'), 90);
  if (draft().lighting.mode === 'trigger') flash(pos);
});
addEventListener('beforeunload', e => {
  window.__beacon('/api/preview', { stop: true, tab: TAB });
  if (S.rec) window.__beacon('/api/record', { action: 'cancel' });
  if (S.dev && ([1, 2, 3, 4, 5].some(isDirty) || Object.keys(S.mdrafts).some(n => macroDirty(+n)))) e.preventDefault();
});

load();
requestAnimationFrame(frame);
