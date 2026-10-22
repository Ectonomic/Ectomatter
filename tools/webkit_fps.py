"""How smoothly the editor animates in WebKitGTK, the engine Tauri uses on Linux.

Opens the editor in a WebKit window with its preview and recording requests
answered locally (so the keyboard is never touched), switches to an animated
lighting effect, and counts frames for a few seconds.

    python3 tools/webkit_fps.py [url] [mode]
"""
import sys

import gi
gi.require_version('Gtk', '3.0')
gi.require_version('WebKit2', '4.1')
from gi.repository import GLib, Gtk, WebKit2  # noqa: E402

URL = sys.argv[1] if len(sys.argv) > 1 else 'http://127.0.0.1:8910/'
MODE = sys.argv[2] if len(sys.argv) > 2 else 'Wave'

STUB = r"""
const real = window.fetch;
window.fetch = (u, o) => /\/api\/(preview|record)/.test(String(u))
  ? Promise.resolve(new Response('{"mode":null,"recording":false}', { headers: { 'content-type': 'application/json' } }))
  : real(u, o);
"""
MEASURE = r"""
(function wait() {
  if (document.querySelector('#boot') || typeof S === 'undefined' || !S.dev) return setTimeout(wait, 300);
  if ('%s' !== 'idle') document.querySelector('#tab-lighting').click();
  setTimeout(() => {
    const b = [...document.querySelectorAll('#view-lighting button')].find(b => b.textContent.trim().startsWith('%s'));
    b && b.click();
    setTimeout(() => {
      let n = 0, worst = 0, last = performance.now(); const t0 = last;
      (function f(now) {
        n++; worst = Math.max(worst, now - last); last = now;
        if (now - t0 < 5000) requestAnimationFrame(f);
        else document.title = 'FPS ' + (n / ((now - t0) / 1000)).toFixed(1) + ' WORST ' + worst.toFixed(0);
      })(t0);
    }, 1000);
  }, 800);
})();
""" % (MODE, MODE)


def main():
    w = Gtk.Window(title='Ectomatter WebKit check')
    w.set_default_size(1400, 1000)
    ucm = WebKit2.UserContentManager()
    ucm.add_script(WebKit2.UserScript(STUB, WebKit2.UserContentInjectedFrames.TOP_FRAME,
                                      WebKit2.UserScriptInjectionTime.START, None, None))
    ucm.add_script(WebKit2.UserScript(MEASURE, WebKit2.UserContentInjectedFrames.TOP_FRAME,
                                      WebKit2.UserScriptInjectionTime.END, None, None))
    v = WebKit2.WebView.new_with_user_content_manager(ucm)
    w.add(v)

    def title(*_):
        t = v.get_title() or ''
        if t.startswith('FPS'):
            print(t, flush=True)
            Gtk.main_quit()
    v.connect('notify::title', title)
    v.load_uri(URL)
    w.show_all()
    GLib.timeout_add_seconds(90, lambda: (print('TIMEOUT', flush=True), Gtk.main_quit()))
    Gtk.main()


main()
