"""teley's dashboard as a desktop app: one WebKitGTK window, no browser.

`just dev` opens it (through scripts/dev-herdr.sh's dashboard.sh) on the
dev room's `/live/<room>?token=…` address: no tabs, no address bar, its own
window with its own app id (`dev.fts.teley`), so a window rule can place it.
The web app's storage (IndexedDB) is kept in DATA_DIR between runs.

Usage: python3 teley-window.py URL DATA_DIR

Runs under `nix-shell` with GTK 4 and WebKitGTK 6 (see dashboard.sh).
"""

import sys

import gi

gi.require_version("Gtk", "4.0")
gi.require_version("WebKit", "6.0")
from gi.repository import Gtk, WebKit  # noqa: E402

APP_ID = "dev.fts.teley"


def main() -> int:
    if len(sys.argv) != 3:
        print(__doc__, file=sys.stderr)
        return 2
    url, data_dir = sys.argv[1], sys.argv[2]

    def activate(app: Gtk.Application) -> None:
        session = WebKit.NetworkSession.new(f"{data_dir}/data", f"{data_dir}/cache")
        view = WebKit.WebView(network_session=session)
        window = Gtk.ApplicationWindow(application=app, title="teley — Studio")
        window.set_default_size(1400, 900)
        window.set_child(view)
        view.load_uri(url)
        window.present()

    app = Gtk.Application(application_id=APP_ID)
    app.connect("activate", activate)
    return app.run([sys.argv[0]])


if __name__ == "__main__":
    sys.exit(main())
