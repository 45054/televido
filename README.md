<!-- SPDX-FileCopyrightText: David Cabot <d-k-bo@mailbox.org> -->
<!-- SPDX-FileCopyrightText: 2026 Televido contributors -->
<!-- SPDX-FileContributor: Claude (Anthropic) -->
<!-- SPDX-License-Identifier: GPL-3.0-or-later -->

[Deutsche Version](README.de.md)

# Televido

> [!NOTE]
> ## This is a fork
>
> This repository continues [d-k-bo/televido](https://github.com/d-k-bo/televido), which its original author has discontinued (see his notice below). The fork adds [FritzTV](#fritztv-fritzbox-cable-dvb-c), the DVB-C channels of a FRITZ!Box Cable, plays all videos with mpv and contains further changes. Its Arch Linux package is called `televido-fritz-git`.
>
> All changes in this fork since October 2026 were written by Claude, an AI assistant by Anthropic, according to the requirements of the fork's maintainer. Their copyright notices name “Televido contributors”, and the files name Claude as contributor (`SPDX-FileContributor`). The copyright of the original code remains with David Cabot. Everything stays licensed under the GPL-3.0-or-later.

> [!WARNING]
> ## This project is no longer maintained
>
> ### Why?
>
> As a hobbyist programmer, I have invested a significant amount of my personal time and energy into many projects. Due to increasing personal and professional commitments, I have significant less time to dedicate to software development. I am therefore focusing on the projects that I actively use myself and enjoy working on the most. Recent developments in "AI" have also contributed to a general decline in my interest in software development.
>
> ### What does this mean?
>
> - This project will not be actively maintained for the foreseeable future. Accordingly, it should be considered as deprecated.
> - The project remains available under the terms of the corresponding free software license.
> - You are welcome to redistribute, fork and continue its development under the terms of the said ‍‍‍license.


**Televido** (“Television” in Esperanto) lets you livestream, search, play and download media from German-language public television services. It is powered by [MediathekViewWeb](https://mediathekviewweb.de/)'s API and the [Zapp backend](https://github.com/mediathekview/zapp-backend) API which are both part of the [MediathekView](https://mediathekview.de/) project.

The presented content is provided directly by the respective television services, this program only facilitates finding and accessing the shows.

Televido plays videos with [mpv](https://mpv.io/), which must be installed. Additionally, Televido supports external programs that are installed on the user's system for video playback and download.  Currently supported players: [GNOME Videos (Totem)](https://flathub.org/apps/org.gnome.Totem), [Celluloid](https://flathub.org/apps/io.github.celluloid_player.Celluloid), [Clapper](https://flathub.org/apps/com.github.rafostar.Clapper), [Daikhan](https://flathub.org/apps/io.gitlab.daikhan.stable). Currently supported downloaders: [Parabolic](https://flathub.org/apps/org.nickvision.tubeconverter).

## FritzTV (FRITZ!Box Cable DVB-C)

Televido can play the DVB-C channels of a FRITZ!Box Cable (e.g. 6490, 6591, 6660, 6690) in your home network. Enable it in the preferences under *FritzTV*; a new *FritzTV* tab then lists the TV and radio channels of your FRITZ!Box. By default, every TV channel is listed once in the best available quality (`tv.m3u`); alternatively, the HD and SD channels can be listed separately. Radio channels can be hidden in the preferences.

- **Requirements:** the FRITZ!Box must have completed a channel scan (DVB-C settings in the FRITZ!Box web interface). Encrypted channels can't be played.
- **Address:** `fritz.box` is used by default. If that doesn't resolve in your network, enter the IP address of your FRITZ!Box (e.g. `192.168.178.1`).
- **mpv arguments:** the arguments passed to mpv can be edited in the preferences: general arguments for all videos (*Playback*) and additional ones for FRITZ!Box channels (*FritzTV*). The FritzTV defaults are tuned for the FRITZ!Box (`--rtsp-transport=udp` is required, the FRITZ!Box doesn't support RTSP over TCP; longer stream analysis and an initial buffer avoid stutter). They are not used for live channels and the Mediathek, which therefore start without delay.
- **One stream per device:** the FRITZ!Box only delivers one channel per device, so starting a channel stops the previous one.
- **Programme guide:** the cards show the current and the next show of the TV channels (expand a card for its description), like the livestreams. The data comes from the cable signal itself (the DVB Event Information Table, no external service involved). The FRITZ!Box only passes on the programme data of the frequency it is tuned to, so while no channel is playing, Televido tunes each frequency for a few seconds in turn and remembers the result, starting with the frequencies whose channels don't show a programme yet. Many cable networks announce the programme of the other frequencies as well, which keeps the data current without visiting every frequency again. The first run takes a minute or two. Playing a channel interrupts reading at once, and it continues a few seconds after the playback ended. It can be turned off in the preferences (*Show programme guide*).
- **Filter:** the field above the channels filters them by name while typing, like on the *Live* page. Typing while a channel is selected goes to the filter as well, <kbd>Ctrl</kbd>+<kbd>F</kbd> focuses it and <kbd>Esc</kbd> clears it.
- **Channel logos** are downloaded from AVM and cached: from `https://tv.avm.de/tvapp/logos/` like in the FRITZ!Box web interface, otherwise from `https://download.avm.de/tv/logos/`.
- **Stuttering:** the streams arrive via UDP. If the picture stutters, the kernel's maximum socket receive buffer may be too small for the default `buffer_size` (4 MiB):
  ```
  echo 'net.core.rmem_max=4194304' | sudo tee /etc/sysctl.d/90-televido-rtp.conf && sudo sysctl --system
  ```
  Firewalls must allow incoming UDP traffic from the FRITZ!Box.

FritzTV starts mpv as a regular process and is therefore intended for native installations (see [Building on Arch Linux](#building-on-arch-linux)). The development Flatpak ([`build-aux/de.k_bo.Televido.Devel.json`](build-aux/de.k_bo.Televido.Devel.json)) bundles mpv as well.

## Building on Arch Linux

[`build-aux/arch/PKGBUILD`](build-aux/arch/PKGBUILD) builds a `televido-fritz-git` package from this repository (it replaces an installed `televido` or `televido-git`), including FritzTV and its mpv dependency:

```
cd build-aux/arch
makepkg -si
```

By default, the `main` branch is fetched from GitHub. To build another branch or the local checkout (committed changes only):

```
TELEVIDO_BRANCH=my-branch makepkg -si
TELEVIDO_SOURCE="file://$(git rev-parse --show-toplevel)" TELEVIDO_BRANCH="$(git branch --show-current)" makepkg -si
```

When switching `TELEVIDO_SOURCE`, delete the cached clone `build-aux/arch/televido-fritz/` first. Note that makepkg updates the `pkgver` line in the PKGBUILD on every build.

## Channel logos

The ARD, ORF and SRF logos were taken from [Wikimedia Commons](https://commons.wikimedia.org) and are in the public domain.

The other channel logos were extracted from the source code of [zapp](https://github.com/mediathekview/zapp) and converted to SVG using [`vd2svg`](https://github.com/seanghay/vector-drawable-svg).

## FAQ

### How can I use a different video player / use a player with custom options?

Televido supports any video player that is [DBus activatable](https://specifications.freedesktop.org/desktop-entry-spec/latest/ar01s08.html) and supports opening https:// URIs via the `org.freedesktop.Application.Open` DBus method.

To use a custom player, create a flatpak permission override to allow it to access the player. E.g.

```
flatpak override --user de.k_bo.Televido --talk-name=org.example.VideoPlayer
```

and set the video player in the preferences.

If you want to use program that doesn't support DBus activation, you can create a wrapper script. See [d-k-bo/dbus-activatable-wrapper](https://github.com/d-k-bo/dbus-activatable-wrapper).

### Could you add support for TV channels from other countries?

Since this project is basically a client for [MediathekView](https://mediathekviewweb.de/), it's limited to the channels supported by them. The upstream project is focused on German content and is developed in German, so I doubt that there are any plans for non-German content.
ORF (Austrian TV) & SRF (Swiss TV) are supported though.

## License

Copyright (C) 2023 David Cabot

This program is free software; you can redistribute it and/or modify it under the terms of the GNU General Public License as published by the Free Software Foundation; either version 3 of the License, or (at your option) any later version.

This program is distributed in the hope that it will be useful, but WITHOUT ANY WARRANTY; without even the implied warranty of MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE. See the GNU General Public License for more details.
