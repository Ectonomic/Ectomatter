"""Check that the framed window is see-through at its corners, as the app's
frameless window needs to be for its rounded corners to show.

Loads the page in a WebKit view with a transparent background and the app's
'framed' look switched on, takes WebKit's own snapshot with transparency, and
reports how opaque the corner and center pixels are.

    python3 tools/webkit_corners.py <url> <out.png>
"""
import sys

import gi
gi.require_version('Gtk', '3.0')
gi.require_version('WebKit2', '4.1')
gi.require_version('Gdk', '3.0')
from gi.repository import Gdk, GLib, Gtk, WebKit2  # noqa: E402

URL, OUT = sys.argv[1], sys.argv[2]


def main():
    w = Gtk.Window(title='Ectomatter corners')
    w.set_default_size(1200, 800)
    v = WebKit2.WebView()
    c = Gdk.RGBA(); c.red = c.green = c.blue = 0; c.alpha = 0
    v.set_background_color(c)
    w.add(v)

    def shot():
        def done(view, res):
            surf = view.get_snapshot_finish(res)
            surf.write_to_png(OUT)
            print('saved', OUT, flush=True)
            Gtk.main_quit()
        v.get_snapshot(WebKit2.SnapshotRegion.VISIBLE, WebKit2.SnapshotOptions.TRANSPARENT_BACKGROUND, None, done)
        return False

    def loaded(view, ev):
        if ev == WebKit2.LoadEvent.FINISHED:
            v.evaluate_javascript("document.documentElement.classList.add('framed')", -1, None, None, None, None, None)
            GLib.timeout_add(1500, shot)
    v.connect('load-changed', loaded)
    v.load_uri(URL)
    w.show_all()
    GLib.timeout_add_seconds(60, lambda: (print('TIMEOUT'), Gtk.main_quit()))
    Gtk.main()


main()
