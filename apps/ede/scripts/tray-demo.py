#!/usr/bin/env python3
"""A StatusNotifierItem with a dbusmenu for trying eDE's tray (#EDE-12).

    apps/ede/scripts/tray-demo.py [--watcher BUSNAME]

Registers with org.kde.StatusNotifierWatcher (or the bus name given, e.g. the
shell's unique name while it runs nested under another desktop) and prints
every Activate and menu click it receives. Ctrl-C ends it.
"""
import sys
import dbus
import dbus.service
from dbus.mainloop.glib import DBusGMainLoop
from gi.repository import GLib

DBusGMainLoop(set_as_default=True)
bus = dbus.SessionBus()
watcher = "org.kde.StatusNotifierWatcher"
if len(sys.argv) > 2 and sys.argv[1] == "--watcher":
    watcher = sys.argv[2]


class Menu(dbus.service.Object):
    def __init__(self):
        super().__init__(bus, "/MenuBar")
        self.rows = [
            (1, {"label": "Connect to Wi-Fi", "enabled": True}),
            (2, {"label": "Airplane mode", "toggle-type": "checkmark", "toggle-state": 0}),
            (3, {"type": "separator"}),
            (4, {"label": "Settings…"}),
        ]

    @dbus.service.method("com.canonical.dbusmenu", in_signature="iias", out_signature="u(ia{sv}av)")
    def GetLayout(self, parent, depth, names):
        children = [dbus.Struct((dbus.Int32(i), dbus.Dictionary(p, signature="sv"), dbus.Array([], signature="v")), signature="ia{sv}av") for i, p in self.rows]
        return (1, (0, {"children-display": "submenu"}, children))

    @dbus.service.method("com.canonical.dbusmenu", in_signature="isvu")
    def Event(self, id, event_id, data, timestamp):
        print(f"menu: row {id} {event_id}", flush=True)

    @dbus.service.method("com.canonical.dbusmenu", in_signature="i", out_signature="b")
    def AboutToShow(self, id):
        return False


class Item(dbus.service.Object):
    def __init__(self):
        super().__init__(bus, "/StatusNotifierItem")

    @dbus.service.method("org.kde.StatusNotifierItem", in_signature="ii")
    def Activate(self, x, y):
        print(f"activated at {x},{y}", flush=True)

    @dbus.service.method("org.freedesktop.DBus.Properties", in_signature="ss", out_signature="v")
    def Get(self, iface, name):
        return self.GetAll(iface)[name]

    @dbus.service.method("org.freedesktop.DBus.Properties", in_signature="s", out_signature="a{sv}")
    def GetAll(self, iface):
        return {
            "Category": "Hardware", "Id": "tray-demo", "Title": "Network (demo)", "Status": "Active",
            "IconName": "network-wireless-symbolic", "IconThemePath": "", "ItemIsMenu": True,
            "Menu": dbus.ObjectPath("/MenuBar"), "IconPixmap": dbus.Array([], signature="(iiay)"),
        }


name = dbus.service.BusName(f"org.kde.StatusNotifierItem-{GLib.getpid() if hasattr(GLib, 'getpid') else 1}-1", bus)
item, menu = Item(), Menu()
proxy = bus.get_object(watcher, "/StatusNotifierWatcher")
proxy.RegisterStatusNotifierItem(name.get_name(), dbus_interface="org.kde.StatusNotifierWatcher")
print(f"registered {name.get_name()} with {watcher}", flush=True)
GLib.MainLoop().run()
