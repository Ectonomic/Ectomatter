"""Capture a page as WebKitGTK draws it, the engine Tauri uses on Linux.

Opens the page in its own WebKit window, answers the editor's preview and
recording calls locally (so the keyboard is never touched), waits for it to
settle, runs an optional script, and saves WebKit's own snapshot of it.

    python3 tools/webkit_shot.py <url> <out.png> [width height] [script]
"""
import sys

import gi
gi.require_version('Gtk', '3.0')
gi.require_version('WebKit2', '4.1')
from gi.repository import GLib, Gtk, WebKit2  # noqa: E402

URL, OUT = sys.argv[1], sys.argv[2]
W, H = (int(sys.argv[3]), int(sys.argv[4])) if len(sys.argv) > 4 else (1440, 940)
SCRIPT = sys.argv[5] if len(sys.argv) > 5 else ''

STUB = r"""
const real = window.fetch;
window.fetch = (u, o) => /\/api\/(preview|record)/.test(String(u))
  ? Promise.resolve(new Response('{"mode":null,"recording":false}', { headers: { 'content-type': 'application/json' } }))
  : real(u, o);
"""


def main():
    w = Gtk.Window(title='Ectomatter capture')
    w.set_default_size(W, H)
    ucm = WebKit2.UserContentManager()
    ucm.add_script(WebKit2.UserScript(STUB, WebKit2.UserContentInjectedFrames.TOP_FRAME,
                                      WebKit2.UserScriptInjectionTime.START, None, None))
    v = WebKit2.WebView.new_with_user_content_manager(ucm)
    w.add(v)

    def shot():
        def done(view, res):
            surf = view.get_snapshot_finish(res)
            surf.write_to_png(OUT)
            print('saved', OUT, flush=True)
            Gtk.main_quit()
        v.get_snapshot(WebKit2.SnapshotRegion.VISIBLE, WebKit2.SnapshotOptions.NONE, None, done)
        return False

    def ready():
        v.evaluate_javascript("!!document.querySelector('#boot')", -1, None, None, None, check, None)
        return False

    def check(view, res, _):
        try:
            booting = view.evaluate_javascript_finish(res).to_boolean()
        except Exception:
            booting = True
        if booting:
            GLib.timeout_add(500, ready)
            return
        if SCRIPT:
            v.evaluate_javascript(SCRIPT, -1, None, None, None, None, None)
        GLib.timeout_add(1500, shot)

    v.connect('load-changed', lambda view, ev: ev == WebKit2.LoadEvent.FINISHED and GLib.timeout_add(1000, ready))
    v.load_uri(URL)
    w.show_all()
    GLib.timeout_add_seconds(90, lambda: (print('TIMEOUT', flush=True), Gtk.main_quit()))
    Gtk.main()


main()
