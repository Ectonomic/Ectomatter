'use strict';
/* The editor's calls, answered by Ectodeck's own core instead of a web server.
   Inside the app, each one becomes a call into the Rust side; opened in an
   ordinary browser during development, they go to the same paths over HTTP, so
   the page still runs against the original editor's server. */

(() => {
  const T = window.__TAURI__;

  // which window this page is: the main window, or the Macro Editor
  window.__window = location.hash.startsWith('#macros') ? 'macros' : 'main';

  window.__api = async (path, body) => {
    if (!T) {
      const r = await fetch(path, body ? { method: 'POST', headers: { 'Content-Type': 'application/json' }, body: JSON.stringify(body) } : {});
      const j = await r.json().catch(() => ({ error: `The editor server answered ${r.status}.` }));
      if (!r.ok || j.error) throw new Error(j.error || `The editor server answered ${r.status}.`);
      return j;
    }
    const call = (cmd, args) => T.core.invoke(cmd, args).catch(e => { throw new Error(String(e)); });
    let m;
    if (path === '/api/state') return call('state');
    if (path === '/api/lcd') return call('lcd');
    if (path === '/api/firmware') return call('firmware', { version: body.version });
    if (path === '/api/sliders') return call('sliders');
    if (path === '/api/dimmer') return call('set_dimmer', { level: body.level ?? null });
    if (path === '/api/audio') return call('set_audio', { target: body.target, level: body.level });
    if (path === '/api/modes') return call('set_modes', { quietVolume: !!body.quietVolume, keepDimmer: !!body.keepDimmer });
    if (path === '/api/tray') return call('set_tray_devices', { devices: body.devices });
    if (path === '/api/preview') return call('preview', { body });
    if (path === '/api/record') return call('record', body ? { action: body.action, room: body.room, waits: body.waits } : { action: 'status' });
    if (path === '/api/active') return call('activate', { profile: body.profile });
    if ((m = path.match(/^\/api\/profile\/(\d+)$/))) return call('save', { n: +m[1], body });
    if ((m = path.match(/^\/api\/macro\/(\d+)$/))) return call('write_macro', { index: +m[1], body });
    throw new Error(`Ectodeck has no call for ${path}.`);
  };

  // A call made as the page goes away, which must not wait for an answer.
  window.__beacon = (path, body) => {
    if (T) { window.__api(path, body).catch(() => {}); return; }
    try { navigator.sendBeacon(path, new Blob([JSON.stringify(body)], { type: 'application/json' })); } catch (e) { /* the core times out anyway */ }
  };

  // Open the Macro Editor window, on macro `n` if given.
  window.__openMacroEditor = n => T ? T.core.invoke('open_macro_editor', { index: n ?? null }) : (location.hash = n ? `#macros/${n}` : '#macros');

  // Build the Macro Editor window out of sight, so it opens ready.
  window.__prepareMacroEditor = () => { if (T) T.core.invoke('prepare_macro_editor').catch(() => {}); };

  // The two windows keep each other current: a macro saved in the Macro
  // Editor, or a profile saved in the main window, is sent to the other.
  window.__tell = (what, data) => { if (T) T.event.emit(what, data).catch(() => {}); };
  window.__hear = (what, fn) => { if (T) T.event.listen(what, e => fn(e.payload)); };
})();
