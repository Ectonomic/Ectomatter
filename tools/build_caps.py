"""Draw the Z10's keycaps once, as WebKit draws them, into caps.png.

The board shows this one picture under its keys instead of drawing every cap
live (see `.board .caps` in style.css). Run it after changing how a cap is
drawn, with the app's dev server running on :5173:

    python3 tools/build_caps.py

WebKit draws the board alone at 1240 wide, zoomed to twice its size as a
high-density screen would, on a transparent ground; nothing talks to the
keyboard.
"""
import os
import sys

os.environ.setdefault('WEBKIT_DISABLE_DMABUF_RENDERER', '1')
os.environ.setdefault('WEBKIT_DISABLE_COMPOSITING_MODE', '1')

import gi  # noqa: E402
gi.require_version('Gtk', '3.0')
gi.require_version('Gdk', '3.0')
gi.require_version('WebKit2', '4.1')
from gi.repository import Gdk, GLib, Gtk, WebKit2  # noqa: E402

OUT = os.path.join(os.path.dirname(__file__), '..', 'apps', 'ectodeck', 'public', 'caps.png')
URL = sys.argv[1] if len(sys.argv) > 1 else 'http://localhost:5173/?capsheet'
# no keyboard: every call to the core fails, and the board is drawn before any is made
STUB = "window.fetch = (f => (u, o) => String(u).includes('/api/') ? Promise.reject(new Error('no keyboard')) : f(u, o))(window.fetch.bind(window));"

w = Gtk.Window(title='Ectodeck caps')
ucm = WebKit2.UserContentManager()
ucm.add_script(WebKit2.UserScript(STUB, WebKit2.UserContentInjectedFrames.TOP_FRAME,
                                  WebKit2.UserScriptInjectionTime.START, None, None))
v = WebKit2.WebView.new_with_user_content_manager(ucm)
rgba = Gdk.RGBA(); rgba.parse('rgba(0,0,0,0)'); v.set_background_color(rgba)
ZOOM = 2
v.set_zoom_level(ZOOM)
w.add(v)


def size(view, res, _):
    h = view.evaluate_javascript_finish(res).to_double()
    w.resize(1240 * ZOOM, int(round(h * ZOOM)))
    GLib.timeout_add(1500, shot)


def shot():
    def done(view, res):
        view.get_snapshot_finish(res).write_to_png(OUT)
        print('wrote', os.path.normpath(OUT), flush=True)
        Gtk.main_quit()
    v.get_snapshot(WebKit2.SnapshotRegion.VISIBLE, WebKit2.SnapshotOptions.TRANSPARENT_BACKGROUND, None, done)
    return False


def titled(view, _):
    if view.get_title() == 'SHEET READY':
        v.evaluate_javascript("document.querySelector('#board').getBoundingClientRect().height", -1, None, None, None, size, None)


v.connect('notify::title', titled)
v.load_uri(URL)
w.set_default_size(1240 * ZOOM, 480 * ZOOM)
w.show_all()
GLib.timeout_add_seconds(40, lambda: (print('TIMEOUT', flush=True), Gtk.main_quit()))
Gtk.main()
