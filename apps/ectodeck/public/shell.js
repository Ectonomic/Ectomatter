'use strict';
/* Ectodeck's own frame round a device's editor: the rail down the left, with
   Home, a glyph for each device and Settings, which opens into a blade with
   their names when you click the item you are already on; the home, where each device is its glyph and its name; and
   Settings. The editor itself is the device's page. The Macro Editor window
   shows only its editor, so it has none of this. */

(() => {
  const body = document.body;
  body.classList.add('win-' + window.__window);
  // The Macro Editor shows only its editor; while the keyboard is read, it
  // shows the same notice the main window does, where its editor will be.
  if (window.__window === 'macros') {
    const wait = document.createElement('div');
    wait.className = 'mboot';
    wait.append(document.getElementById('boot'));
    document.querySelector('main.page').prepend(wait);
    return;
  }
  if (window.__window !== 'main') return;

  // the rail's own marks, from the same outline set as the rest of the frame
  const mark = (d, cls = '') => `<svg class="${cls}" viewBox="0 0 256 256" fill="currentColor" aria-hidden="true">${d}</svg>`;
  const HOUSE = mark("<path d=\"M217.9,110.1l-80-80a14,14,0,0,0-19.8,0l-80,80A13.92,13.92,0,0,0,34,120v96a6,6,0,0,0,6,6h64a6,6,0,0,0,6-6V158h36v58a6,6,0,0,0,6,6h64a6,6,0,0,0,6-6V120A13.92,13.92,0,0,0,217.9,110.1ZM210,210H158V152a6,6,0,0,0-6-6H104a6,6,0,0,0-6,6v58H46V120a2,2,0,0,1,.58-1.42l80-80a2,2,0,0,1,2.84,0l80,80A2,2,0,0,1,210,120Z\"/>"), GEAR = mark("<path d=\"M128,82a46,46,0,1,0,46,46A46.06,46.06,0,0,0,128,82Zm0,80a34,34,0,1,1,34-34A34,34,0,0,1,128,162Zm108-54.4a6,6,0,0,0-2.92-4L202.64,86.22l-.42-.71L202.1,51.2A6,6,0,0,0,200,46.64a110.12,110.12,0,0,0-36.07-20.31,6,6,0,0,0-4.84.45L128.46,43.86h-1L96.91,26.76a6,6,0,0,0-4.86-.44A109.92,109.92,0,0,0,56,46.68a6,6,0,0,0-2.12,4.55l-.16,34.34c-.14.23-.28.47-.41.71L22.91,103.57A6,6,0,0,0,20,107.62a104.81,104.81,0,0,0,0,40.78,6,6,0,0,0,2.92,4l30.42,17.33.42.71.12,34.31A6,6,0,0,0,56,209.36a110.12,110.12,0,0,0,36.07,20.31,6,6,0,0,0,4.84-.45l30.61-17.08h1l30.56,17.1A6.09,6.09,0,0,0,162,230a5.83,5.83,0,0,0,1.93-.32,109.92,109.92,0,0,0,36-20.36,6,6,0,0,0,2.12-4.55l.16-34.34c.14-.23.28-.47.41-.71l30.42-17.29a6,6,0,0,0,2.92-4.05A104.81,104.81,0,0,0,236,107.6Zm-11.25,35.79L195.32,160.1a6.07,6.07,0,0,0-2.28,2.3c-.59,1-1.21,2.11-1.86,3.14a6,6,0,0,0-.91,3.16l-.16,33.21a98.15,98.15,0,0,1-27.52,15.53L133,200.88a6,6,0,0,0-2.93-.77h-.14c-1.24,0-2.5,0-3.74,0a6,6,0,0,0-3.07.76L93.45,217.43a98,98,0,0,1-27.56-15.49l-.12-33.17a6,6,0,0,0-.91-3.16c-.64-1-1.27-2.08-1.86-3.14a6,6,0,0,0-2.27-2.3L31.3,143.4a93,93,0,0,1,0-30.79L60.68,95.9A6.07,6.07,0,0,0,63,93.6c.59-1,1.21-2.11,1.86-3.14a6,6,0,0,0,.91-3.16l.16-33.21A98.15,98.15,0,0,1,93.41,38.56L123,55.12a5.81,5.81,0,0,0,3.07.76c1.24,0,2.5,0,3.74,0a6,6,0,0,0,3.07-.76l29.65-16.56a98,98,0,0,1,27.56,15.49l.12,33.17a6,6,0,0,0,.91,3.16c.64,1,1.27,2.08,1.86,3.14a6,6,0,0,0,2.27,2.3L224.7,112.6A93,93,0,0,1,224.73,143.39Z\"/>");

  // the devices Ectodeck has a core for; each one names its kind's glyph
  const devices = [{ id: 'z10', name: 'EVGA Z10 RGB', glyph: 'keyboard' }];

  const rail = document.getElementById('rail');
  const item = (id, name, glyph) => `<button type="button" class="b" data-go="${id}" aria-label="${name}">${glyph}<span class="tx"><b>${name}</b></span><span class="tip" aria-hidden="true">${name}</span></button>`;
  rail.innerHTML = item('home', 'Home', HOUSE) + '<span class="gap"></span>'
    + devices.map(d => item(d.id, d.name, DESK_GLYPHS[d.glyph])).join('')
    + '<span class="grow"></span>' + item('settings', 'Settings', GEAR);

  document.getElementById('tiles').innerHTML = devices.map(d =>
    `<button type="button" class="tile" data-go="${d.id}"><span class="sq">${DESK_GLYPHS[d.glyph]}</span><b>${d.name}</b><small>Not connected</small></button>`).join('');

  // Nothing on screen opens or closes the rail: clicking the item you are
  // already on does, as the activity bar does in VS Code. Whether it was left
  // open is remembered; it opens as it was without animating, and animates
  // from then on.
  const setOpen = open => {
    rail.classList.toggle('open', open);
  };
  try { setOpen(localStorage.getItem('ectodeck:rail-open') === '1'); } catch (e) { setOpen(false); }
  requestAnimationFrame(() => requestAnimationFrame(() => rail.classList.add('ready')));
  const toggle = () => {
    const open = !rail.classList.contains('open');
    setOpen(open);
    try { localStorage.setItem('ectodeck:rail-open', open ? '1' : '0'); } catch (e) { /* not fatal */ }
  };

  // One place is shown at a time: the home, Settings, or a device's editor.
  // Ectodeck opens on the home.
  let settingsTimer = 0;
  const go = where => {
    body.dataset.place = where;
    clearTimeout(settingsTimer);
    if (where === 'settings') renderSettings();
    for (const b of rail.querySelectorAll('.b')) b.classList.toggle('on', b.dataset.go === where);
    scrollTo(0, 0);
    // a device's keyboard is drawn from pictures sized to it, so draw them now it shows
    if (where !== 'home' && where !== 'settings') { window.drawCaps?.(); window.drawHalos?.(); }
  };
  addEventListener('click', e => {
    const t = e.target.closest('[data-go]');
    if (!t) return;
    if (t.closest('#rail') && t.dataset.go === body.dataset.place) toggle();
    else go(t.dataset.go);
  });
  go('home');
  // picking a device from the tray brings Ectodeck up on that device
  window.__hear?.('device', id => go(id));
  // a change that reloads the page comes back to where it was made
  try { const back = sessionStorage.getItem('ectodeck:return'); if (back) { sessionStorage.removeItem('ectodeck:return'); go(back); } } catch (e) { /* none */ }


  // An unplugged device dims, in the rail and on the home, and its tile says so.
  // The editor already knows: it marks its connection line when it cannot reach
  // the keyboard.
  const conn = document.getElementById('conn');

  // The tray lists the devices, so one is a click away without opening the
  // window first. It is told again whenever a device's connection changes,
  // since a device that is not plugged in is greyed there.
  const tellTray = () => window.__api('/api/tray', {
    devices: devices.map(d => ({ id: d.id, name: d.name, connected: !conn.classList.contains('off') })),
  }).catch(() => {});

  const sync = () => {
    const off = conn.classList.contains('off'), free = conn.classList.contains('free');
    for (const el of document.querySelectorAll('[data-go="z10"]')) el.classList.toggle('off', off || free);
    const note = document.querySelector('.tile[data-go="z10"] small');
    if (note) note.textContent = free ? 'Not controlled by Ectodeck' : 'Not connected';
    tellTray();
  };
  new MutationObserver(sync).observe(conn, { attributes: true, attributeFilter: ['class'] });
  sync();

  // ---------------------------------------------------------------- Settings
  // Ectodeck's own settings. Devices first: for each one, whether Ectodeck may
  // open it at all, whether it controls it, and what else has
  // it open. Kept current while the page shows, since a device can be plugged
  // in, or another program can take it, at any time.
  const UDEV = 'SUBSYSTEM=="hidraw", ATTRS{idVendor}=="3842", ATTRS{idProduct}=="2611", TAG+="uaccess"';
  const FIX = `echo '${UDEV}' | sudo tee /etc/udev/rules.d/70-ectodeck.rules && sudo udevadm control --reload && sudo udevadm trigger`;
  const h = (tag, attrs = {}, ...kids) => {
    const n = document.createElement(tag);
    for (const [k, v] of Object.entries(attrs)) if (k.startsWith('on')) n.addEventListener(k.slice(2), v); else n.setAttribute(k, v);
    n.append(...kids.flat().filter(k => k != null && k !== false));
    return n;
  };
  const srow = (label, ...kids) => {
    const note = typeof kids[kids.length - 1] === 'string' ? kids.pop() : null;
    return h('div', { class: 'lrow' }, h('span', { class: 'lrow-l' }, label),
      h('div', { class: 'lrow-c' }, ...kids, note ? h('span', { class: 'note' }, note) : null));
  };
  const onOff = (value, set) => h('div', { class: 'seg', role: 'group' },
    [[true, 'On'], [false, 'Off']].map(([v, t]) => h('button', { type: 'button', 'aria-pressed': String(value === v), onclick: () => value !== v && set(v) }, t)));

  // A change to what Ectodeck may use reloads the page, so the device's editor
  // starts again under the new setting; the page comes back to Settings.
  const change = (path, value) => window.__api('/api/settings', { path, value }).then(() => {
    try { sessionStorage.setItem('ectodeck:return', 'settings'); } catch (e) { /* stays on the home */ }
    location.reload();
  }, e => {
    const box = document.getElementById('settings-body');
    box.querySelector('.fail')?.remove();
    box.prepend(h('p', { class: 'fail' }, `The setting was not saved: ${e.message || e}`));
  });

  let drawn = '';
  function drawSettings(status, conf) {
    const key = JSON.stringify([status, conf]);
    if (key === drawn) return;   // redrawn only when something changed, so a menu or a selection survives the check
    drawn = key;
    const box = document.getElementById('set-devices');
    const cards = devices.map(d => {
      const st = status.find(s => s.id === d.id) || {}, dc = conf.devices?.[d.id] || {};
      const control = dc.control !== false;
      const line = !st.present ? 'Not connected' : !st.reachable ? 'Ectodeck can’t open this keyboard' : !control ? 'Control is off' : 'Connected';
      const rows = [srow('Control', onOff(control, v => change(`devices.${d.id}.control`, v)),
        'When off, Ectodeck leaves the keyboard to other programs.')];
      // whether Ectodeck may open the keyboard; always shown, so the state is never a guess
      if (st.reachable === null || st.reachable === undefined) rows.push(srow('Permission', h('span', { class: 'state' }, 'Not needed on this system')));
      else if (!st.present) rows.push(srow('Permission', h('span', { class: 'state' }, 'Checked when the keyboard is connected')));
      else if (st.reachable) rows.push(srow('Permission', h('span', { class: 'state ok' }, 'Granted')));
      else rows.push(srow('Permission', h('span', { class: 'state bad' }, 'Not granted'),
        h('span', { class: 'note' }, 'Linux lets only root open the keyboard. This command gives access to whoever is logged in:'),
        h('code', { class: 'cmd' }, FIX),
        h('button', { type: 'button', class: 'ghost', onclick: e => { navigator.clipboard?.writeText(FIX); e.currentTarget.textContent = 'Copied'; } }, 'Copy command')));
      if (st.holders?.length) {
        rows.push(srow('Also open in', h('span', { class: 'holders' }, st.holders.map(p => p.name).join(', ')),
          'Two programs sending lighting at once make the keys flicker. Close the other program, or turn Live lighting off here.'));
      }
      return h('section', { class: 'lpanel devcard' + (st.present && st.reachable && control ? '' : ' dim') },
        h('div', { class: 'devhead' }, h('span', { class: 'sq' }), h('div', {}, h('b', {}, d.name), h('small', {}, line))),
        h('div', { class: 'lcol' }, ...rows));
    });
    box.replaceChildren(h('h2', {}, 'Devices'), ...cards);
    cards.forEach((c, i) => { c.querySelector('.sq').innerHTML = DESK_GLYPHS[devices[i].glyph]; });
  }

  // ---------------------------------------------------------------- Weather
  // Ectodeck's, not any one device's: one place and one set of units, and every
  // device that shows weather reads the same report (crate ectodeck-weather).
  // Built once and then only its readings are updated, so the search field and
  // its list survive the Settings page checking again every two seconds.
  const WMO = { 0: 'Clear', 1: 'Mostly clear', 2: 'Partly cloudy', 3: 'Overcast', 45: 'Fog', 48: 'Freezing fog',
    51: 'Light drizzle', 53: 'Drizzle', 55: 'Heavy drizzle', 56: 'Freezing drizzle', 57: 'Freezing drizzle',
    61: 'Light rain', 63: 'Rain', 65: 'Heavy rain', 66: 'Freezing rain', 67: 'Freezing rain',
    71: 'Light snow', 73: 'Snow', 75: 'Heavy snow', 77: 'Snow grains', 80: 'Light showers', 81: 'Showers', 82: 'Heavy showers',
    85: 'Snow showers', 86: 'Heavy snow showers', 95: 'Thunderstorm', 96: 'Thunderstorm with hail', 99: 'Thunderstorm with hail' };
  const ALERTS = { 1: 'Tropical storm', 2: 'Hurricane', 3: 'Tornado' };
  let wx = null;
  const setW = (path, value) => window.__api('/api/settings', { path, value });

  function buildWeather() {
    const q = h('input', { type: 'text', autocomplete: 'off', spellcheck: 'false', 'aria-label': 'Place', placeholder: 'Search for a city or town' });
    const list = h('div', { class: 'menu float wlist', role: 'listbox', hidden: '' });
    const where = h('div', { class: 'where' }, q, list);
    const placeLine = h('span', { class: 'note' });
    const temp = h('b'), cond = h('span'), when = h('small'), facts = h('div', { class: 'facts' });
    const now = h('div', { class: 'now' }, temp, cond, when);
    const warnState = h('span', { class: 'state' }), warnNote = h('span', { class: 'note' });
    const unitSeg = (path, opts) => h('div', { class: 'seg', role: 'group' }, opts.map(([v, t]) =>
      h('button', { type: 'button', 'data-v': v, onclick: () => setW(path, v).then(renderSettings) }, t)));
    const tSeg = unitSeg('weather.units.temperature', [['c', 'Celsius'], ['f', 'Fahrenheit']]);
    const wSeg = unitSeg('weather.units.wind', [['kmh', 'km/h'], ['mph', 'mph'], ['ms', 'm/s']]);
    const unitNote = h('span', { class: 'note' });
    const nowRow = srow('Now', h('div', { class: 'wbox' }, now, facts));

    // the place: a search as you type, in this computer's language
    let timer = 0, found = [];
    const close = () => { list.hidden = true; list.classList.remove('shown'); };
    q.addEventListener('input', () => {
      clearTimeout(timer);
      const text = q.value.trim();
      if (!text) { close(); return; }
      timer = setTimeout(async () => {
        try { found = await window.__api('/api/weather/search', { query: text }); } catch (e) { found = []; }
        if (q.value.trim() !== text) return;   // typed on since
        list.replaceChildren(...(found.length ? found.map(p => h('button', { type: 'button', class: 'mi', role: 'option', onclick: () => pick(p) },
          h('span', {}, p.name, p.latin && p.latin !== p.name ? h('span', { class: 'alt' }, ` (${p.latin})`) : null),
          h('span', { class: 'msub' }, [p.region, p.country].filter(Boolean).join(', '))))
          : [h('div', { class: 'mh' }, 'No place by that name')]));
        list.hidden = false; list.classList.add('shown');
      }, 300);
    });
    q.addEventListener('keydown', e => { if (e.key === 'Escape') { close(); q.value = wx.placeName; } });
    addEventListener('pointerdown', e => { if (!where.contains(e.target)) close(); });
    const pick = p => { close(); q.value = p.name; setW('weather.place', p).then(renderSettings); };

    const section = h('section', { class: 'lpanel devcard wcard' }, h('div', { class: 'lcol' },
      srow('Place', where, placeLine),
      nowRow,
      srow('Warnings', warnState, warnNote),
      srow('Temperature', tSeg),
      srow('Wind', wSeg, unitNote),
      srow('', h('span', { class: 'credit' }, 'Weather and air from Open-Meteo (open-meteo.com, CC BY 4.0), every 15 minutes. '
        + 'Places from Open-Meteo, and from Photon over OpenStreetMap (ODbL) for names Open-Meteo cannot match. '
        + 'Warnings every 3 minutes from the National Weather Service, Environment and Climate Change Canada, or MeteoAlarm (meteoalarm.org, CC BY 4.0).'))));
    wx = { q, placeLine, temp, cond, when, facts, nowRow, warnState, warnNote, tSeg, wSeg, unitNote, placeName: '' };
    return h('div', {}, h('h2', {}, 'Weather'), section);
  }

  const two = n => String(Math.floor(n)).padStart(2, '0');
  const clock = secs => `${two(secs / 3600)}:${two((secs % 3600) / 60)}`;
  const ago = t => { const m = Math.round((Date.now() / 1000 - t) / 60); return m < 1 ? 'Updated just now' : m === 1 ? 'Updated a minute ago' : m < 60 ? `Updated ${m} minutes ago` : `Updated ${Math.round(m / 60)} h ago`; };

  function drawWeather(w, conf) {
    if (!wx || !w) return;
    const p = w.place, r = w.report, u = w.units || { temperature: 'c', wind: 'kmh' };
    wx.placeName = p?.name || '';
    if (document.activeElement !== wx.q && wx.q.value !== wx.placeName) wx.q.value = wx.placeName;
    wx.placeLine.textContent = p ? [p.name, p.region, p.country].filter(Boolean).join(', ') : 'Every device that shows weather uses this place.';
    wx.nowRow.hidden = !p;
    if (p && !r) { wx.temp.textContent = ''; wx.cond.textContent = 'Getting the weather…'; wx.when.textContent = ''; wx.facts.replaceChildren(); }
    if (r) {
      const t = u.temperature === 'f' ? r.temperature * 9 / 5 + 32 : r.temperature;
      const wind = k => u.wind === 'mph' ? `${Math.round(k / 1.609)} mph` : u.wind === 'ms' ? `${Math.round(k / 3.6)} m/s` : `${Math.round(k)} km/h`;
      wx.temp.textContent = `${Math.round(t)}°${u.temperature.toUpperCase()}`;
      wx.cond.textContent = WMO[r.code] || `Weather code ${r.code}`;
      wx.when.textContent = r.fetched ? ago(r.fetched) : '';
      const fact = (k, v) => h('span', {}, `${k} `, h('b', {}, v));
      wx.facts.replaceChildren(fact('Cloud', `${Math.round(r.cloud * 100)}%`), fact('Wind', wind(r.wind_speed)),
        fact('Sunrise', clock(r.sunrise)), fact('Sunset', clock(r.sunset)));
    }
    // the warning service for the place, or the weather standing in for one
    const official = !!w.warnings;
    wx.warnState.textContent = !p ? 'Chosen with the place' : official ? w.warnings : 'Read from the weather';
    wx.warnState.className = 'state' + (!p ? ' off' : official ? ' ok' : ' inf');
    const inForce = r?.alert ? `${ALERTS[r.alert]} ${official ? 'warning in force' : 'conditions now'}. ` : '';
    wx.warnNote.textContent = !p ? '' : inForce + (official ? 'Official warnings for this place, checked every 3 minutes.'
      : 'No warning service covers this country yet, so storm-force wind with rain stands in for an official warning. A tornado cannot be told from the weather alone.');
    wx.warnNote.classList.toggle('alert', !!r?.alert);
    for (const [seg, v] of [[wx.tSeg, u.temperature], [wx.wSeg, u.wind]]) for (const b of seg.children) b.setAttribute('aria-pressed', String(b.dataset.v === v));
    const chosen = conf?.weather?.units || {};
    wx.unitNote.textContent = chosen.temperature || chosen.wind ? '' : `Set from this computer's region (${w.locale}). Change either and Ectodeck keeps your choice.`;
    wx.unitNote.hidden = !wx.unitNote.textContent;
  }

  // ---------------------------------------------------------------- HDR
  // An interface that lights brighter than the desktop's white, on screens
  // with room above it (a screen in HDR mode, or one whose brightness is
  // turned down, since KWin keeps the backlight's headroom). Ectodeck's own
  // setting, not a device's. Peak nits defaults to the brightest screen's peak.
  let hx = null;
  function buildHdr() {
    const mode = h('div', { class: 'seg', role: 'group' }, [['auto', 'On'], ['off', 'Off']].map(([v, t]) =>
      h('button', { type: 'button', 'data-v': v, onclick: () => setW('hdr.lighting', v).then(renderSettings) }, t)));
    const range = h('input', { type: 'range', min: '100', max: '1000', step: '10', 'aria-label': 'Peak nits' });
    const out = h('span', { class: 'range-val' });
    range.addEventListener('input', () => { out.textContent = `${range.value} nits`; });
    // the top of the slider means "the screen's own peak", saved as no limit
    range.addEventListener('change', () => setW('hdr.glow_limit', Number(range.value) >= Number(range.max) ? null : Number(range.value)).then(renderSettings));
    const section = h('section', { class: 'lpanel devcard wcard' }, h('div', { class: 'lcol' },
      srow('HDR interface', mode, 'Lit keys glow brighter than the desktop’s white on screens with room for it.'),
      srow('Peak nits', h('div', { class: 'lslider' }, range, out))));
    hx = { mode, range, out };
    return h('div', {}, h('h2', {}, 'HDR'), section);
  }

  function drawHdr(screens, conf) {
    if (!hx) return;
    const c = conf?.hdr || {}, on = c.lighting !== 'off';
    const top = Math.max(0, ...(screens || []).map(s => s.peak || 0)) || 1000;
    const peak = typeof c.glow_limit === 'number' ? Math.min(c.glow_limit, top) : top;
    for (const b of hx.mode.children) b.setAttribute('aria-pressed', String(b.dataset.v === (on ? 'auto' : 'off')));
    hx.range.max = String(Math.round(top));
    if (document.activeElement !== hx.range) { hx.range.value = String(Math.round(peak)); hx.out.textContent = `${Math.round(peak)} nits`; }
    hx.mode.closest('.lcol').classList.toggle('off', !on);
  }

  function renderSettings() {
    const bodyBox = document.getElementById('settings-body');
    if (!document.getElementById('set-devices')) bodyBox.replaceChildren(h('div', { id: 'set-devices' }), buildWeather(), buildHdr());
    Promise.all([window.__api('/api/devices'), window.__api('/api/settings'), window.__api('/api/weather').catch(() => null), window.__api('/api/screens').catch(() => [])])
      .then(([status, conf, w, scr]) => { drawSettings(status, conf); drawWeather(w, conf); drawHdr(scr, conf); }, () => drawSettings([], {}))
      .finally(() => { clearTimeout(settingsTimer); if (body.dataset.place === 'settings') settingsTimer = setTimeout(renderSettings, 2000); });
  }
})();
