'use strict';
/* The window's own controls, since Ectodeck draws its title bar itself:
   minimize, maximize, close (which keeps Ectodeck in the tray), resizing
   from the edges, and squaring the corners off when maximized. */

(() => {
  const T = window.__TAURI__;
  if (!T) return;
  const win = T.window.getCurrentWindow();
  const root = document.documentElement;
  root.classList.add('framed');
  const sync = async () => root.classList.toggle('maximized', await win.isMaximized());
  sync();
  win.onResized(sync);
  // Right-clicking gets no browser menu (Back, Forward, Stop, Reload,
  // Inspect): Ectodeck is an app, not a page. Text fields keep theirs, for
  // cut, copy and paste.
  addEventListener('contextmenu', e => {
    if (!e.target.closest('input, textarea, [contenteditable]')) e.preventDefault();
  });
  addEventListener('DOMContentLoaded', () => {
    const on = (id, f) => document.getElementById(id)?.addEventListener('click', f);
    on('w-min', () => win.minimize());
    on('w-max', () => win.toggleMaximize());
    on('w-close', () => win.close());
    on('open-macros', () => window.__openMacroEditor());
    for (const [c, edge] of [['n', 'North'], ['s', 'South'], ['w', 'West'], ['e', 'East'],
                             ['nw', 'NorthWest'], ['ne', 'NorthEast'], ['sw', 'SouthWest'], ['se', 'SouthEast']]) {
      const d = document.createElement('div');
      d.className = 'edge ' + c;
      d.addEventListener('mousedown', e => { if (e.button === 0) win.startResizeDragging(edge); });
      document.body.append(d);
    }
  });
})();
