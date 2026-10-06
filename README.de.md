<!-- SPDX-FileCopyrightText: David Cabot <d-k-bo@mailbox.org> -->
<!-- SPDX-License-Identifier: GPL-3.0-or-later -->

[English Version](README.md)

# Televido

> [!WARNING]
> ## Dieses Projekt ist eingestellt
>
> ### Warum?
>
> Als Hobby-Programmierer habe ich eine beträchtliche Menge an persönlicher Zeit und Energie in viele Projekte investiert. Aufgrund zunehmender privater und beruflicher Verpflichtungen habe ich jedoch deutlich weniger Zeit, die ich in die Softwareentwicklung stecken kann. Daher möchte ich mich lieber auf die Projekte konzentrieren, die ich selbst aktiv nutze und bei denen mir die Arbeit am meisten Spaß macht. Darüber hinaus verspüre ich insgesamt durch den aktuellen Einfluss von „KI“ einen leichten Interessensverlust an Softwareentwicklung.
>
> ### Was bedeutet das?
>
> - Dieses Projekt wird auf absehbare Zeit nicht mehr gepflegt und sollte dementsprechend als veraltet (deprecated) betrachtet werden.
> - Das Projekt bleibt weiterhin gemäß den Bedingungen der entsprechenden Frei-Software-Lizenz verfügbar.
> - Gerne kann es unter den Bedingungen der genannten Lizenz weiterverbreitet, geforkt und weiterentwickelt werden.


**Televido** („Fernsehen“ auf Esperanto) ermöglicht das Empfangen von Livestreams sowie das Suchen, Abspielen und Herunterladen von Inhalten aus Mediatheken öffentlich-rechtlicher Sender aus dem DACH-Raum. Es basiert auf den APIs von [MediathekViewWeb](https://mediathekviewweb.de/) und des [Zapp-Backends](https://github.com/mediathekview/zapp-backend), die beide Teil des [MediathekView](https://mediathekview.de/)-Projekts sind.

Die präsentierten Inhalte werden direkt von den jeweiligen Sendern angeboten, dieses Programm erleichtert dabei nur das Auffinden und Abrufen der Sendungen.

Televido bietet einen integrierten Videoplayer zum Abspielen der Inhalte. Darüber hinaus unterstützt Televido für Videowiedergabe und -download externe Programme, die auf dem System installiert sind. Derzeit unterstütze Player: [GNOME Videos (Totem)](https://flathub.org/apps/org.gnome.Totem), [Celluloid](https://flathub.org/apps/io.github.celluloid_player.Celluloid), [Clapper](https://flathub.org/apps/com.github.rafostar.Clapper), [Daikhan](https://flathub.org/apps/io.gitlab.daikhan.stable). Derzeit unterstütze Videodownloader: [Parabolic](https://flathub.org/apps/org.nickvision.tubeconverter).

## FritzTV (FRITZ!Box Cable DVB-C)

Televido kann die DVB-C-Sender einer FRITZ!Box Cable (z. B. 6490, 6591, 6660, 6690) im Heimnetz abspielen. Die Funktion wird in den Einstellungen unter *FritzTV* aktiviert; anschließend listet der neue Reiter *FritzTV* die HD-, SD- und Radiosender der FRITZ!Box auf.

- **Voraussetzungen:** [mpv](https://mpv.io/) muss installiert sein (FritzTV nutzt nicht den integrierten Player) und die FRITZ!Box muss einen Sendersuchlauf abgeschlossen haben (DVB-C-Einstellungen in der Benutzeroberfläche der FRITZ!Box). Verschlüsselte Sender können nicht abgespielt werden.
- **Adresse:** Standardmäßig wird `fritz.box` verwendet. Falls das im eigenen Netz nicht aufgelöst wird, die IP-Adresse der FRITZ!Box eintragen (z. B. `192.168.178.1`).
- **mpv-Argumente:** Die an mpv übergebenen Argumente lassen sich in den Einstellungen bearbeiten. Die Voreinstellungen sind auf die FRITZ!Box abgestimmt (`--rtsp-transport=udp` ist notwendig, die FRITZ!Box unterstützt kein RTSP über TCP).
- **Ein Stream pro Gerät:** Die FRITZ!Box liefert nur einen Sender pro Gerät, deshalb beendet der Start eines Senders den vorherigen.
- **Senderlogos** werden von AVM (`https://download.avm.de/tv/logos/`) heruntergeladen und zwischengespeichert.
- **Ruckeln:** Die Streams kommen per UDP. Ruckelt das Bild, ist eventuell der maximale Empfangspuffer des Kernels zu klein für die voreingestellte `buffer_size` (4 MiB):
  ```
  echo 'net.core.rmem_max=4194304' | sudo tee /etc/sysctl.d/90-televido-rtp.conf && sudo sysctl --system
  ```
  Firewalls müssen eingehenden UDP-Verkehr von der FRITZ!Box zulassen.

FritzTV startet mpv als normalen Prozess und ist daher für native Installationen gedacht (siehe [Bauen unter Arch Linux](#bauen-unter-arch-linux)). In der Flatpak-Sandbox steht mpv nicht zur Verfügung.

## Bauen unter Arch Linux

[`build-aux/arch/PKGBUILD`](build-aux/arch/PKGBUILD) baut aus diesem Repository ein Paket `televido-git`, inklusive FritzTV und der Abhängigkeit mpv:

```
cd build-aux/arch
makepkg -si
```

Standardmäßig wird der Branch `main` von GitHub geholt. Um einen anderen Branch oder den lokalen Checkout (nur committete Änderungen) zu bauen:

```
TELEVIDO_BRANCH=mein-branch makepkg -si
TELEVIDO_SOURCE="file://$(git rev-parse --show-toplevel)" TELEVIDO_BRANCH="$(git branch --show-current)" makepkg -si
```

Beim Wechsel von `TELEVIDO_SOURCE` vorher den zwischengespeicherten Klon `build-aux/arch/televido/` löschen. makepkg aktualisiert bei jedem Bau die Zeile `pkgver` im PKGBUILD.

## Senderlogos

Die ARD-, ORF- and SRF-Logos wurden [Wikimedia Commons](https://commons.wikimedia.org) entnommen und sind gemeinfrei.

Die anderen Logos wurden aus dem Quellcode von [zapp](https://github.com/mediathekview/zapp) extrahiert und mit [`vd2svg`](https://github.com/seanghay/vector-drawable-svg) ins SVG-Format konvertiert.

## FAQ

Siehe [README.md#faq](README.md#faq) (Englisch)

## Lizenz

Copyright (C) 2023 David Cabot

Dieses Programm ist freie Software. Sie können es unter den Bedingungen der GNU General Public License, wie von der Free Software Foundation veröffentlicht, weitergeben und/oder modifizieren, entweder gemäß Version 3 der Lizenz oder (nach Ihrer Option) jeder späteren Version.

Die Veröffentlichung dieses Programms erfolgt in der Hoffnung, daß es Ihnen von Nutzen sein wird, aber OHNE IRGENDEINE GARANTIE, sogar ohne die implizite Garantie der MARKTREIFE oder der VERWENDBARKEIT FÜR EINEN BESTIMMTEN ZWECK. Details finden Sie in der GNU General Public License.
