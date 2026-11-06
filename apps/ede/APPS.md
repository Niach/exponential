# The apps on an eDE desktop (#EDE-20)

eDE is a desktop environment installed on an existing distribution; Danny's
machine is Linux Mint 22.2 "Zara" (Cinnamon edition, Ubuntu noble base).
This is the audit of what that install ships against what eDE keeps,
replaces with what it already has, or rewrites later as an eApps package.
Versions are what Mint's archive (`packages.linuxmint.com/dists/zara`,
components main/upstream/backport) and Ubuntu noble carried on 2026-11-06;
the ISO's own `casper/filesystem.manifest` was not pulled (3 GB image, no
manifest on the mirrors), so an app Mint seeds only on the ISO and never
updates would be missing here.

Rules: keep everything that needs a heavy engine (a browser, mail, office,
media, games); rewrite only small apps, and only as eApps packages once the
`eapps` crate exists (#EXP-1420, "remaining"); until then the shell's own
documents (#EDE-12, #EDE-14) stand in where they can. Apache-2: behaviour
may be copied from GPL apps, code never.

## Keep as they are

| App | Package (version) | Why it stays |
|---|---|---|
| Firefox | `firefox` 159.0+linuxmint1 | browser engine; eDE's portal (#EDE-15) gives it screen sharing and file dialogs, the bridge (#EDE-19) the look |
| Thunderbird | `thunderbird` 153.6.0esr+linuxmint1 | mail engine |
| LibreOffice Writer, Calc, Impress, Draw, Math | `libreoffice-*` 24.2.2 | office suite |
| Celluloid | `celluloid` 0.21+mint2 | mpv front end; video stays native |
| Rhythmbox | `rhythmbox` 3.4.7 | music library, GStreamer |
| Hypnotix | `hypnotix` 5.2 | IPTV on mpv/libxapp |
| Pix | `pix` 3.4.7+zara | photo organiser (gThumb fork), image codecs |
| Drawing | `drawing` 1.0.2 | paint app, GTK canvas |
| Transmission | `transmission-gtk` 4.0.5 | torrent engine |
| Steam, games, Flatpaks | not in the archive list | heavy, their own toolkits; the bridge themes GTK/Qt ones |
| Nemo + nemo-fileroller, -emblems, -preview, -share | `nemo` 6.4.5+xia | the file manager until after M3 (#EDE-19's note); GTK 3, themed by the bridge; file dialogs stay GTK through the portal delegate |
| Timeshift | `timeshift` 25.07.7+zara | system snapshots, root tool |
| Simple Scan | `simple-scan` 46.0 | SANE |
| Archive Manager | `file-roller` 44.1 | archive formats |
| Disks | `gnome-disk-utility` 46.0 | partitioning, SMART; eDE's Disks domain only mounts/unmounts (#EDE-13) |
| Warpinator | `warpinator` 2.0.4+zena | LAN transfer; an eNet twin is a product question, not M1 |
| Web Apps | `webapp-manager` 1.4.3 | wraps the browser |
| Bulky | `bulky` 3.9 | batch rename, Nemo action |
| Sticky Notes | `sticky` 1.31 | small, but it is Nemo/Xapp data; rewrite candidate after Nemo |
| Document Viewer | `xreader` 4.6.8 | poppler, djvu, epub |
| Image Viewer | `xviewer` 3.4.12+zara | codecs; Pix covers the organiser |
| Text Editor | `xed` 3.8.4+zara | GtkSourceView; the IDE is the real editor |
| Terminal | `gnome-terminal` 3.52 | VTE; the compositor's Ctrl+Alt+T picks the first installed terminal (#EDE-16) |
| Character Map | `gucharmap` 15.1 | the Unicode database |
| Fonts | `gnome-font-viewer` 46.0 | small, stays until an eApps viewer |
| Passwords and Keys | `seahorse` 43.0 | gnome-keyring front end; eDE's keyring prompt is sealed (#EDE-17) |
| Printers | `system-config-printer` 1.5.18 | CUPS admin; the Printers page shows the queues (#EDE-14) and opens this for the rest |
| Bluetooth Manager | `blueman` 2.3.5 | pairing wizard; the Bluetooth page covers power, connect, disconnect |
| Firewall | `gufw` 24.04 | ufw front end, root |
| Synaptic | `synaptic` 0.91.3 | apt power tool |
| Driver Manager, Software Sources, Backup Tool, Upload/USB tools, Welcome, System Reports, Language Settings | `mintdrivers` 1.8.4, `mintsources` 2.4.3, `mintbackup` 2.4.6, `mintstick` 1.6.3, `mintwelcome` 2.6.0, `mintreport` 1.5.6, `mintlocale` 1.6.4 | Mint's own tools, root helpers with policies of their own; they run under eDE (GTK 3, polkit through `exponential-sealed polkit`) |
| On-screen keyboard | `onboard` 1.4.1 | needs an input-method path eDE has not built (text-input v3 is there; the keyboard itself is M2) |
| Redshift | `redshift-gtk` 1.12 | needs the wlr-gamma-control protocol in the compositor first (M2 candidate) |

## Replaced by what eDE already has

| Mint | eDE | Where |
|---|---|---|
| Cinnamon (`cinnamon` 6.4.14+xia), cinnamon-session, cinnamon-settings-daemon | the compositor, exponential-session, the shell | `apps/ede` |
| Cinnamon menu, panel, Alt+Tab, notifications, tray, OSD | the shell's documents | #EDE-12 |
| cinnamon-screensaver 6.4.1 | the sealed lock (`exponential-sealed lock`, PAM) | #EDE-17 |
| cinnamon-control-center 6.4.1 (Displays, Network, Sound, Bluetooth, Power, Keyboard, Themes, Users, Printers, Info) | `settings.json` over exponential-services | #EDE-13, #EDE-14 |
| polkit-gnome / the Cinnamon polkit agent | `exponential-sealed polkit` | #EDE-17 |
| xdg-desktop-portal-xapp 1.1.2 | `xdg-desktop-portal-exponential` (ScreenCast, Screenshot, RemoteDesktop, GlobalShortcuts, Settings); `-gtk` keeps FileChooser and the rest | #EDE-15 |
| Mint-Y / Mint-X themes, cursors, icons | the token palette through `exponential-theme` over Adwaita-dark, Papirus or Yaru icons, Adwaita cursors | #EDE-19 |
| Update Manager (`mintupdate` 7.1.1) | the Apps & updates page: count, list, refresh, install all (apt over exponential-services, polkit through the sealed agent) | #EDE-14 |
| gnome-screenshot 41.0 | the compositor's screenshot hotkey and the portal's Screenshot | #EDE-12, #EDE-15 |
| slick-greeter 2.2.2 (the login screen) | stays; eDE is a session entry under it (`wayland-sessions/exponential.desktop`) | packaging |

## Rewrite as eApps packages, parked until the `eapps` crate (#EXP-1420)

The issue names three. None is started: an eApps package needs the
runtime (`eapps`, tag `eapps-v*`), which is still on #EXP-1420's remaining
list, and writing them as shell documents would make them Tier C surfaces
of the shell process, not apps (wrong process, wrong grants, no window).

| App | Mint today | What the eApps version is | What exists now |
|---|---|---|---|
| Calculator | `gnome-calculator` 41.1+mint1 (Mint pins the GTK 3 one) | one genUI document with a keypad, an expression Input, a history List; the arithmetic in the package's own functions (no engine) | nothing; the launcher opens Mint's |
| System Monitor | `gnome-system-monitor` 46.0 | processes, CPU/memory/network as the styleguide's Chart and Sparkline over a `processes` domain of exponential-services (not written: it needs a `kill` action with its own capability) | the About page's static facts; `exponential-services` has power and disks, not processes |
| Software | `mintinstall` 8.3.8 (Software Manager, apt + Flatpak) and `gnome-software` 46.0 | search and install over PackageKit (or apt + flatpak as the `updates` domain does), reviews and screenshots from AppStream, the same `mcp_calls`-shaped audit for an agent installing on a person's behalf | the Apps & updates page (updates only) and its Software button, which opens `mintinstall` or `gnome-software` |

Order when the crate lands: Calculator (a weekend, proves the package
format on eDE), System Monitor (needs the `processes` domain first), then
Software (needs the PackageKit face and a consent prompt through
`exponential-sealed consent` for installs).

## Not on Mint's list, worth noting

`gnome-calendar`, `gnome-power-manager`, `gnome-logs`, `baobab` and
`hexchat` are in Ubuntu's archive but not Mint's default set; nothing to
decide. PipeWire 1.0.5 and WirePlumber 0.4.17 are what the portal's
ScreenCast runs on (`xdg-desktop-portal` 1.18.3).
