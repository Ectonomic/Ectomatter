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
    const box = document.getElementById('settings-body');
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

  function renderSettings() {
    Promise.all([window.__api('/api/devices'), window.__api('/api/settings')])
      .then(([status, conf]) => drawSettings(status, conf), () => drawSettings([], {}))
      .finally(() => { if (body.dataset.place === 'settings') settingsTimer = setTimeout(renderSettings, 2000); });
  }
})();
