# OpenRGB as an optional output — study (#368)

> **Status: study, 2026-11-06. Nothing here is implemented.** It prepares the
> proof of concept of [#368]: drive, through OpenRGB, a device Candeo has no
> definition for, while native HID stays the default. It answers the two
> questions asked before any code: **how does Hypercolor do it, and may Candeo
> embed OpenRGB?** It is an engineering reading of licences and store policies,
> not legal advice: §1's conclusions should be confirmed before anything of
> OpenRGB is distributed.

**In short.**

- **Licence.** OpenRGB's `LICENSE` is the GPLv2 text and its metadata says
  *GPL-2.0-or-later*, but 18 source files are *GPL-2.0-only*, the LampArray
  controller among them. The program as built can therefore only be passed on
  under GPLv2, which is not compatible with Candeo's GPL-3.0-only: **its code
  cannot be linked into Candeo.** Talking to an unmodified OpenRGB over its
  documented socket keeps the two programs separate. **Connecting to a server
  the user installed carries no obligation. Shipping OpenRGB's executable is
  allowed as an aggregate**, but Candeo would then owe the GPLv2 text and the
  exact corresponding source of OpenRGB and of every library its build bundles,
  and Store users terms no stricter than the GPL's.
- **Hypercolor does not embed OpenRGB.** It finds an OpenRGB the user
  installed and, on request, starts it headless on `127.0.0.1` with a
  configuration folder of its own, in which it turns off the OpenRGB detectors
  for hardware its native drivers own. It stops only the server it started, and
  its SDK client, Apache-2.0, is written from OpenRGB's public documentation
  and never sends `SAVEMODE`. Shipping OpenRGB is deferred in its own specs
  until a legal review.
- **Recommendation: option A** (§5): connect only to an OpenRGB 1.0 or later
  that the user runs, opt-in, over loopback, with a fixed list of packets that
  excludes every persistent or administrative one, and native first on every
  device Candeo knows. The main risk: **two programs writing one device**.
  Candeo can refuse to route a device it drives natively through OpenRGB, but
  it cannot stop the user's OpenRGB from writing it, and it cannot see whether
  a mode OpenRGB selects writes to the device's memory.

## Sources and versions

Nothing was installed, built or run: what follows comes from the sources and
documents below, read on 2026-11-06.

| What | Version read | Date |
|---|---|---|
| OpenRGB | tag `release_1.1`, commit `8e16de27` ([code][o-tree]); licence headers counted on `master` at `4f2d8920` through the GitHub mirror's code search ([search][o-search]) | released 2026-10-16; `master` of 2026-11-05 |
| Hypercolor | 0.9.0, commit `02f53272` ([code][h-tree]); the OpenRGB paths are identical on `main` at `1bb9a58e` | 2026-11-03 |
| PawnIO | driver repository at `9d529658` ([code][p-readme]); installer 2.2.0 ([notes][p-setup]) | 2026-08-14; 2026-03-15 |
| Microsoft Store Policies | 7.20 ([docs][m-policies]) | 2026-09-14 |
| App Developer Agreement | 8.11 ([docs][m-ada]) | published 2026-03-17, effective 2026-04-17 |
| MSIX preparation guide | ([docs][m-prepare]) | updated 2025-06-10 |
| FSF licence list and GPL FAQ | ([docs][f-list], [docs][f-faq]) | 2026-04-22; 2026-01-13 |

## 1. Licensing

### 1.1 OpenRGB's licence

| Evidence | What it says | Source |
|---|---|---|
| `LICENSE` | the GPL version 2 text, nothing else | [code][o-license] |
| AppStream metadata | `<project_license>GPL-2.0-or-later</project_license>` | [code][o-meta] |
| SPDX lines in the sources | about 1,640 files `GPL-2.0-or-later`; **18 files `GPL-2.0-only`**: the HID LampArray controller, the Lian Li Uni Hub SL, the Mountain 60 keyboard, the Gigabyte RGB Fusion 2 USB tables and the manual-devices settings page; the 79 others found are mbedtls 3.2.1's, `Apache-2.0` | [search][o-search]; the LampArray header checked at `release_1.1` too ([code][o-lamparray]) |
| The SDK's files | `NetworkClient.cpp`, `NetworkServer.cpp`, `NetworkProtocol.h`, `RGBController.cpp`, `cli.cpp`: `GPL-2.0-or-later` | [code][o-netclient], [code][o-proto] |
| Debian's packaging | `Files: *`, GPL-2.0 with the "or (at your option) any later version" text; it strips the prebuilt binaries whose source is not in the tree | [data][o-debian] |

So the project states *or later*, and most files are; but the program as
built contains files that are *only*. A work combining all of it can be
distributed under GPLv2 alone. The FSF: GPLv2 "is, by itself, not compatible
with GPLv3", while code under "version 2 or later" is ([docs][f-list],
[FAQ][f-v2v3]).

### 1.2 The SDK protocol

The SDK is a binary TCP protocol, versioned and negotiated on connection,
documented in the repository ([docs][o-sdkdoc]). Protocol 6 ships in 1.0
(2026-09-11) and 1.1 ([code][o-proto-10], [code][o-proto]); the document's own
version table still marks 6 as unreleased. It gives each controller an id that
survives changes to the list while the server runs, where 5 and earlier
address controllers by index. The document carries no licence of its own.

- A client written from that documentation is Candeo's own code, under
  Candeo's licence. Should it borrow from OpenRGB's client, those files are
  *or later*, usable under GPLv3 with their notices kept.
- **The existing Rust crates cannot be used.** `openrgb` 0.1.2 and `openrgb2`
  0.4.1 both declare `GPL-2.0`, a version without *or later*
  ([data][r-openrgb], [data][r-openrgb2]).
- Hypercolor, Apache-2.0, went further: its client is written from the public
  documentation and fake-server tests only, and its provenance note forbids
  reading OpenRGB's implementation ([code][h-cleanroom]).

### 1.3 What each way of using it implies

| Way | What Candeo distributes | Consequence | Obligations |
|---|---|---|---|
| **Link or compile OpenRGB code into Candeo**, or Candeo code into OpenRGB as a plugin, which OpenRGB loads in its own process | a combined program | not possible with the 18 *only* files; possible in principle with *or later* files alone, under GPLv3 | their notices kept |
| **Ship the unmodified executable beside Candeo** and talk to it over the socket | two separate programs | an aggregate: GPLv2 §2 does not extend to "mere aggregation", and the FSF counts sockets among the mechanisms "normally used between two separate programs" ([FAQ][f-aggregate]); the SDK is a public interface many independent clients use | GPLv2 §1 and §3 for OpenRGB: its notices and licence text, and its **complete corresponding source**, alongside or through a written offer valid three years, scripts controlling the build included; the same for each bundled library under its own licence (§1.4); §6: no further restrictions, which in the Store means giving users terms of Candeo's own rather than Microsoft's standard ones (§3.3) |
| **Connect to a server the user installed** | nothing of OpenRGB | none for OpenRGB; Candeo's client is Candeo's code | none; naming OpenRGB in the documentation is courtesy |

Candeo's own licence allows the aggregate too (GPLv3 §5, last paragraph). The
hard part of shipping is "corresponding source": the source from which users
can rebuild *the same binary* ([FAQ][f-extended]). Shipping upstream's build
means pinning its tag, its build scripts, its Qt build and the sources of the
prebuilt libraries in its tree; building OpenRGB in Candeo's CI would be the
honest way to know them.

### 1.4 What OpenRGB's Windows build bundles

From `OpenRGB.pro` and the build script at `release_1.1` ([code][o-pro],
[code][o-buildwin], [code][o-deps]); licences read from each file's header or
licence, upstream where the tree has none.

| Component | How it is in the build | Licence | Note |
|---|---|---|---|
| Qt 6.8.3 (Core, Gui, Widgets, Core5Compat, OpenGL) | DLLs copied by `windeployqt` | LGPL-3.0 ([docs][q-lic]) | its source, and the DLLs replaceable |
| hidapi (hotplug fork) | prebuilt DLL | GPL-3.0, BSD or the original HIDAPI licence, at the user's choice ([code][hidapi-lic]) | source not in the tree |
| libusb 1.0.27 | prebuilt DLL | LGPL-2.1 ([code][libusb-lic]) | source not in the tree |
| mbedtls 3.2.1 | prebuilt libraries, its headers in the tree | Apache-2.0 ([code][o-mbedtls]) | its sources not in the tree; the FSF: not compatible with GPLv2 ([docs][f-apache]) |
| PawnIOLib and its modules | prebuilt DLL and `.bin` modules | LGPL-2.1-or-later ([code][o-pawniolib]); modules LGPL-2.1 ([code][p-modules]) | the PawnIO driver itself is not bundled (§3.1) |
| hueplusplus 1.2.0 | compiled in | LGPL-3.0 ([code][o-hueplusplus]) | the FSF: LGPLv3 "is not compatible with GPLv2 by itself" ([docs][f-list]) |
| libe131 | compiled in | Apache-2.0 ([code][o-e131]) | the FSF: not compatible with GPLv2 ([docs][f-apache]) |
| cpp-httplib, nlohmann/json, CRCpp, mdns, AMD ADL headers | compiled in | MIT, MIT, BSD-3-Clause, public domain, MIT | — |

mbedtls, hueplusplus and libe131 are OpenRGB's to settle: Apache-2.0 and LGPLv3
code in a program with GPL-2.0-only parts. A redistributor carries the question
with the binary.

## 2. How Hypercolor does it

Hypercolor (Apache-2.0, [code][h-license]) splits its OpenRGB support into an
SDK client, a bridge driver, a host crate that finds and launches OpenRGB, and
the desktop app's supervisor ([docs][h-spec68], [docs][h-spec81]).

| Question | What its code does | Sources |
|---|---|---|
| **What it ships** | No OpenRGB. "Hypercolor never bundles OpenRGB binaries"; the bridge is compiled in and off by default. It gives install hints instead: winget, the MSI, distribution packages, Flatpak, AppImage. Its Windows installer does ship the PawnIO installer 2.2.0 and modules 0.2.5, for its own SMBus drivers and its `HypercolorSmBus` service, not for OpenRGB. | [docs][h-fallback], [code][h-host], [code][h-fetchpawnio], [code][h-pawnio] |
| **Starting it** | Only on request (the app's button or `hypercolor openrgb start`), never at launch. A pure plan function decides: adopt a server that completes the SDK handshake on the endpoint; wait for its own child still starting; hold when something else answers on the port; hold with hints when no binary is found or a Linux permission check fails; otherwise spawn. | [code][h-plan], [code][h-supervisor] |
| **Arguments and port** | `--server --server-host 127.0.0.1 --server-port 6742 --noautoconnect --config <its data folder>/openrgb --loglevel 4`, wrapped in `flatpak run --filesystem=…` for Flatpak; `QT_QPA_PLATFORM=offscreen` on Linux without a display. A non-loopback endpoint is refused. The user's own `~/.config/OpenRGB` is never read or written. | [code][h-process], [code][h-supervisor], [docs][h-fallback] |
| **Stopping it** | Stops only the child it spawned: a graceful request, three seconds, then kill. A kill-on-close Job object on Windows, a process group and a parent-death signal on Linux, so the child dies with Hypercolor. An adopted server is never stopped; an exited one is never restarted. | [code][h-child], [code][h-childproc], [code][h-supervisor] |
| **Overlapping detectors** | Before spawning, it rewrites `Detectors.detectors` in the managed `OpenRGB.json`: every OpenRGB detector claiming a USB id that an enabled native driver can drive is set `false`, connected or not; detectors with no known USB id stay off for the whole brand prefix (`Razer `, `ASUS Aura`, `ENE SMBus DRAM`…). The detector-to-USB-id map (1,100 names) is generated from the udev rules an unmodified OpenRGB 1.0 binary prints. | [code][h-host], [data][h-detectors] |
| **Two writers on one device** | Two layers: the partition above, then a conflict guard after every discovery: when a device has a working native driver and an OpenRGB route, the route is turned off, "native driver owns this device". Controllers are matched by serial, then SMBus address, then USB path. Controllers known only by their index, with no serial nor location, are never driven when HID or SMBus. Native and bridge SMBus scans never run at once. | [docs][h-spec81], [docs][h-fallback] |
| **Security** | Loopback by default, a non-loopback endpoint only with `allow_insecure_remote = true`. `SAVEMODE` cannot be encoded under any setting; `RESIZEZONE` only for zone sizes the user entered. The selected mode is the one flagged per-LED, written at full brightness and read back. On release, the previous mode is restored. | [code][h-cleanroom], [docs][h-spec68], [docs][h-fallback] |
| **Identity** | A fingerprint of the endpoint and the serial, or the location, or vendor, name and shape. The fingerprint, serial included, is a key in its configuration file (`zone_sizes`). | [docs][h-fallback] |

What it does not cover: a hand-started OpenRGB is adopted as it is, its
detectors not partitioned, and only the conflict guard stands between the two
writers; its docs warn that the first program to open a device wins. Its docs
also predate OpenRGB 1.0 in places: protocol 5 as "OpenRGB 1.0", and no
`--generate-udev-rules` in releases, which 1.1's `cli.cpp` has ([code][o-cli]).

## 3. Platforms

### 3.1 Windows drivers and administrator rights

- **HID and USB devices need no driver**, in OpenRGB as in Candeo; OpenRGB's
  guide suggests administrator rights only when a device is not detected, and
  asks users to remove the WinUSB driver older versions had them install
  ([docs][o-usb]).
- **SMBus devices** (RAM, some motherboards) need PawnIO, installed by hand,
  and OpenRGB run as administrator or as its service, which its MSI calls
  experimental ([docs][o-smbus], [code][o-msi]).
- **WinRing0**, which OpenRGB used before, is retired. CVE-2020-14979: any
  user, low integrity processes included, reads and writes physical memory and
  becomes SYSTEM, rated 7.8 ([data][w-cve]). Microsoft Defender detects it as
  `VulnerableDriver:WinNT/Winring0.A` since 2025-03-24 and names old OpenRGB
  versions among the programs that carried it ([docs][w-defender]). OpenRGB's
  documentation asks users to remove it ([docs][o-smbus]).
- **PawnIO** replaces it: a driver that runs signed Pawn modules (RSA, SHA-256,
  checked by the driver, [code][p-sig]), GPL-2.0-or-later with an exception for
  programs using it through its IOCTLs ([code][p-readme]). Its device opens to
  SYSTEM and Administrators only (`D:P(A;;GA;;;SY)(A;;GA;;;BA)`, [code][p-inf]);
  2.2.0 made exposing it to other users possible, "although not recommended"
  ([notes][p-setup]). Its site says the official edition is signed, not by whom
  ([docs][p-site]). Whether Defender or Microsoft's vulnerable driver blocklist
  name PawnIO: not checked. The blocklist is on by default since the Windows 11
  2022 update ([docs][w-blocklist]).

### 3.2 What an MSIX package from the Store may hold and do

- **No driver.** "MSIX doesn't support Windows drivers" ([docs][m-prepare]).
  Policy 10.2.4: a dependency on non-Microsoft drivers or NT services "is not
  allowed" in general, case by case for WHCP-certified drivers, and must be
  told to Microsoft in the certification notes; 10.1.5 excludes drivers and NT
  services from what an app may offer to acquire ([docs][m-policies]).
- **No elevation.** An app that needs it for any part of its functionality
  "won't be accepted into the Store" ([docs][m-prepare]).
- **Starting another program** is open to a full-trust app: Candeo's package
  already starts the file manager, which the certification kit reports in an
  optional test ([`msix.md`](msix.md) §4). The package is read only, so a
  bundled OpenRGB would keep its configuration and logs in a folder Candeo
  names with `--config`. Not verified: starting an executable shipped inside
  the package, whether that child keeps the package's identity, and whether its
  writes are redirected.
- **Disclosure.** Policy 10.2.4: depending on non-integrated software for the
  primary functionality must be said at the beginning of the description. An
  optional OpenRGB output is not Candeo's primary functionality; the listing
  should still name it where it presents it.
- **Loopback.** Connecting to `127.0.0.1` needs nothing from a full-trust app;
  the loopback exemption concerns app containers. Candeo already listens there
  for signals.

So **an OpenRGB inside Candeo's Store package would drive HID and USB devices
only**: no PawnIO, no service, no administrator. Those are what a user's own
OpenRGB covers without any of that too.

### 3.3 The Store's terms and the GPL

The App Developer Agreement lets the publisher give customers its own licence;
without one, Microsoft's *Standard Application License Terms* apply. Its *FOSS
Licenses* clause makes the publisher solely responsible for open source terms,
"including any source code availability requirements" ([docs][m-ada]). Under
option B, the Store listing would have to carry terms no stricter than the GPL,
and the source offer would have to reach Store users. Which terms Candeo's
listing gives today: not checked in Partner Center.

### 3.4 Linux

- OpenRGB needs its own udev rules (`60-openrgb.rules`), which distribution
  packages install; for an AppImage or Flatpak, 1.1 prints or writes them
  (`--print-udev-rules`, `--generate-udev-rules`, [code][o-cli]). SMBus needs
  `i2c-dev` and the chipset's module ([docs][o-smbus]). Candeo's own rule
  (`60-candeo.rules`, [`linux.md`](../linux.md)) is untouched.
- Versions in distributions, 2026-11-06: Debian has 1.0 in *forky* and *sid*
  only ([data][o-debian]); Fedora *rawhide* a 1.0rc2 snapshot ([data][o-fedora]);
  upstream publishes 1.1 as `.deb`, `.rpm`, AppImage and Flatpak
  ([notes][o-release]). 1.0rc2 speaks protocol 5 and binds `0.0.0.0` by default;
  1.0 and 1.1 speak 6 and bind `127.0.0.1` ([code][o-proto-rc2],
  [code][o-proto]).
- Under option A, Candeo's packages could at most *suggest* `openrgb`. Shipping
  an OpenRGB inside Candeo's `.deb` or `.rpm` would duplicate a distribution
  package.

## 4. Security and privacy

### 4.1 What the SDK port lets any local process do

No client is authenticated. Loopback is shared by every account on the
computer, and the server grants *local client* status to any loopback
connection that asks for it ([code][o-server], [docs][o-sdkdoc]).

| Any connected client | A client with local status |
|---|---|
| set colours, select modes, resize zones, edit segments and device configuration | save, delete and upload profiles |
| **`SAVEMODE`: write a mode to the device's memory** | **rewrite OpenRGB's settings and save them**: the detectors, the server's host |
| load, list and download profiles; read OpenRGB's settings; rescan the devices | read OpenRGB's log buffer, change its level |
| | **list every HID and USB device of the computer, with serial numbers and paths** (`hid_enumerate(0, 0)`, [code][o-resources]), I2C buses, serial ports |

A web page cannot speak to it: the server reads the magic `ORGB` byte by byte
and closes the connection on the first byte of an HTTP request
([code][o-server]). Beyond the computer, only when bound beyond loopback: the
1.0rc2 default, the `Server.default_host` setting, or `--server-host`.

### 4.2 What can be mitigated

| Measure | Option A | Options B, B′ (§5) |
|---|---|---|
| Connect to loopback only, no setting to change it in the proof of concept | yes | yes |
| Require protocol 6, OpenRGB 1.0 or later: its default bind is loopback and its controller ids are stable | yes | yes |
| A fixed list of packets in the encoder, the rest impossible to build, tested: handshake (40, 50), controller count and data (0, 1), mode selection (1100, 1101, 1103), colours (1050, 1051); received, `ACK` (10) and `DEVICE_LIST_UPDATED` (100). Never `SAVEMODE`, profiles, settings, rescans, zone or segment edits, device configuration | yes | yes |
| Never ask for local status; never send the client's host name, so the server never sends its own ([code][o-server]) | yes | yes |
| Bounded parsing: lengths, counts, multiplication overflow, as Hypercolor tests | yes | yes |
| Off by default; no connection attempt, no process started while off | yes | yes |
| Pass `--server-host 127.0.0.1` on every launch | — | yes |
| A random port | — | a little: other clients look on 6742, a port scan finds it |
| A server only while the output is on, killed with Candeo (Job object, parent-death signal) | — | yes |
| `LogManager.log_file: false` in its configuration: OpenRGB logs HID paths at *info* ([code][o-detection], [code][o-logmanager]) | — | yes |

**What cannot be mitigated:** while a server runs, any process of any account
can connect to it, drive the lights, write a mode to flash, load profiles, list
the computer's devices with their serial numbers, and rewrite the detector
partition then rescan. Under A, that server is the user's; under B and B′,
Candeo opens it.

**Flash writes Candeo cannot see.** Excluding `SAVEMODE` is not enough: OpenRGB
decides per controller what a mode change sends, and Candeo's own reading of
OpenRGB found its older Logitech take-over writing with *persist = 1*
([`other-keyboards.md`](other-keyboards.md) §2). The SDK says nothing of it.
Candeo's hardware rule — verify a write by reading it back — reaches only what
OpenRGB reports back, never the device.

### 4.3 Keeping `serial` and `location` out of Candeo

Each controller OpenRGB describes carries `serial` and `location` ([docs][o-sdkdoc]):

- `serial` is often read from the device itself (the Razer controller asks the
  keyboard for it, [code][o-razer]);
- `location` is `HID: ` and the HID path: on Windows the interface path, which
  holds the device instance ID; on Linux `/dev/hidrawN`. For a network device,
  its address and port ([code][o-nanoleaf]); for SMBus, the bus and address.

Every one of those is on the list [`AGENTS.md`](../../AGENTS.md) forbids in
logs, docs and issues. What the proof of concept should hold:

1. **Parsed into a type that cannot print them**: no `Debug` or `Display`
   showing the value, so a log line or a `Failure` cannot carry them by
   accident.
2. **Used in memory only**, for two things: an identity, through
   `journal::fingerprint` ([code][c-fingerprint]); and, for a HID path, the
   VID:PID that the conflict guard compares with Candeo's definitions (§6).
3. **settings.json keys an OpenRGB device by fingerprint**: of the serial when
   there is one, otherwise of vendor, name and zone shape, marked as the weaker
   kind. Hypercolor keeps the raw serial in its fingerprint and its
   configuration (§2); Candeo would not.
4. **The log, the copied diagnostic and *Report*** name an OpenRGB device by
   its fingerprint, OpenRGB's name for it, its type, its zones and LED counts,
   OpenRGB's version and the negotiated protocol. Never its serial, nor its
   location, whatever that holds: a path or an address.
5. **PRIVACY.md gains a paragraph** before it ships: off by default; on, Candeo
   connects to OpenRGB on this computer, reads the devices it lists and sends
   colours, nothing else.

Seen while reading for this study: *Control* writes the unit's serial into
`settings.json` — the one the device's protocol reports, the USB descriptor's
otherwise ([code][c-record], [code][c-control]) — while PRIVACY.md says a
keyboard's serial "is never written down". Out of scope here; an OpenRGB record
should not follow that precedent.

## 5. Options

- **A. Connect only to an OpenRGB the user runs** — what [#368] scopes. The
  user installs OpenRGB and starts its SDK server; Candeo, opted in, connects
  to `127.0.0.1:6742`.
- **B. Ship a headless OpenRGB inside Candeo**, started by Candeo.
- **B′. Start the user's OpenRGB headless, as Hypercolor does**: nothing
  shipped, Candeo launches the installed binary with its own configuration
  folder and detector partition.
- **C. Do nothing with OpenRGB at run time**: keep adding native definitions,
  verified or *Not verified*, from OpenRGB's code read as data and from
  projects like `alienfx-tools` ([`device-sdk.md`](device-sdk.md) §9,
  [`other-keyboards.md`](other-keyboards.md)), with *Try* and *Report*.
- **D. Others**, each ruled out or set aside:
  - an OpenRGB plugin: loaded in OpenRGB's process, so combined with its
    GPL-2.0-only files (§1.3);
  - sending E1.31 to OpenRGB's E1.31 receiver plugin: frames only, no mode or
    flash writes, but every universe mapped by hand in OpenRGB, and the
    plugin's compatibility with 1.1 not checked;
  - LampArray ([#330]): a device that describes itself through a standard,
    with no third program; set aside until a certified device is at hand
    ([`other-keyboards.md`](other-keyboards.md) §4).

| | A — user's server | B — shipped | B′ — user's binary, started by Candeo | C — native definitions |
|---|---|---|---|---|
| **Legal** | nothing to ship; Candeo writes its own client (§1.2) | GPLv2 text, the corresponding source of OpenRGB and its libraries for three years, Store terms (§1.3, §1.4, §3.3) | nothing to ship | none new: the generators read *or later* files only (Razer, ASUS Aura USB, HyperX, Logitech, SteelSeries, the keyboard layout manager; §1.1's count); a new generator checks each file's SPDX line first |
| **Effort** | SDK client, conversion of zones and matrix maps to a layout, a `DeviceOut` ([code][c-deviceout]), settings, *Devices*, a conflict guard, tests on a fake server. Hypercolor's client and bridge driver: about 190 KB of Rust sources, test folders aside | A, plus process supervision, a detector-to-USB-id map per OpenRGB release, packaging for MSIX, NSIS, MSI, `.deb` and `.rpm`, the source offer, and an OpenRGB update in Candeo's releases | A, plus supervision and the detector map. Hypercolor's host crate and supervisor: about 150 KB more | a generator per family; bounded by what a definition can express ([`other-keyboards.md`](other-keyboards.md) §3) |
| **Risks** | two writers, which Candeo detects but cannot prevent; mode writes that persist unseen; a user's server bound beyond loopback | Candeo opens the unauthenticated port (§4.1) and answers for OpenRGB's flaws and updates; a Store package that drives HID only | Candeo opens the port; the partition is rewritable by any local client | a definition wrong for a model nobody verified, which *Not verified* already says |
| **User experience** | install OpenRGB, start its server, keep it running; nothing to do when it is absent | one switch | one switch once OpenRGB is installed | nothing to install; *Try* and *Report* per device |
| **Store** | compatible: loopback client, no driver, no elevation | HID only; the licence terms and the source offer to settle; a package about 22 MB larger, the size of 1.1's Windows zip ([notes][o-release]) | technically possible from full trust; not verified | compatible, as today |
| **Coverage** | everything the user's OpenRGB drives, SMBus included when the user set PawnIO up | HID and USB in the Store; more elsewhere | everything the user's OpenRGB drives | keyboards and laptop zones a definition can express; no RAM, motherboards, GPUs or network lights |

### Recommendation

**A, as [#368] scopes it, beside C.** C stays the path for keyboards and
laptops: native, verified when possible, and first. A reaches what C never
will (RAM, fans, strips, motherboards, network lights) at no legal cost and
without Candeo opening a port. B is not worth it: in the Store it would drive
only what the user's own OpenRGB drives without it, and it would make Candeo
answer for a GPLv2 source offer and an unauthenticated server. B′ is the step
to reconsider after A's proof of concept, if starting the server proves the
main friction, outside the Store first.

**Main risk: two writers on one device.** Under A Candeo cannot configure the
user's detectors without rewriting the user's OpenRGB settings, which it must
not do. It can refuse to route through OpenRGB a device it drives natively,
and say once, on that device, that another program may write it too. It
cannot stop the user's OpenRGB, its profiles or its Effects plugin from
writing that device, and nothing in Windows or Linux reserves a HID device for
one program (hidapi opens with shared read and write, [code][hidapi-open]).

## 6. What the proof of concept should hold

Mapped to [#368]'s steps, with what this study found:

1. **Connect** to `127.0.0.1:6742` only, when opted in; negotiate; refuse a
   protocol below 6 with one sentence and the action ("Update OpenRGB to 1.0 or
   later"). Not reachable: one sentence, nothing retried in a loop.
2. **Capabilities as reported**: a controller with no mode flagged per-LED is
   listed and not driven; zones without a matrix map become zones, a matrix
   map becomes keys with positions; LED names are OpenRGB's.
3. **One effect** on a per-key device and a zones device.
4. **Reconnects**: protocol 6's controller ids hold while the server runs; on
   `DEVICE_LIST_UPDATED`, compare the lists as the documentation describes. A
   server restart gives new ids: rebind by fingerprint before the next frame.
   Two controllers with one fingerprint: listed, not driven.
5. **Native first**: an OpenRGB controller whose HID path gives a VID:PID that
   a Candeo definition for this computer describes — built in, yours or
   unverified — is not driven through OpenRGB, controlled or not. On Linux,
   `/dev/hidrawN` resolves to its VID:PID through sysfs. A location that is not
   HID is not a device Candeo drives natively.
6. **Trust**: §4.2's list, enforced in the encoder and tested. On release,
   restore the mode read on take-over; never `SAVEMODE`.
7. **Measure** latency and frame rate against native HID; pace each controller
   with the latest frame kept and the others dropped.
8. **Attribution**: none required (§1.3); OpenRGB named in the docs and the
   listing as the program the user installs.

## 7. Not verified

- Whether a full-trust MSIX app's child process keeps its package identity, and
  where its writes land (§3.2): to measure on the package if B or B′ is
  pursued.
- Which licence terms Candeo's Store listing gives (§3.3).
- Whether Defender or the vulnerable driver blocklist list PawnIO, and whether
  the current blocklist names WinRing0 (§3.1).
- The licences of the NVAPI wrapper (`dependencies/NVFC`) and the colour wheel
  widget in OpenRGB's tree: no statement found in their headers.
- Which OpenRGB controllers persist a mode change without `SAVEMODE` (§4.2):
  per controller, in OpenRGB's code.
- Whether OpenRGB reaches, through libusb, a device Candeo drives over HID: its
  location would not be a HID path, and §6's guard would not see it.
- The licence counts of §1.1 come from `master` at `4f2d8920`, twenty days
  after 1.1; the LampArray header was checked at `release_1.1`.

<!-- Issues -->

[#330]: https://github.com/o2csi/candeo/issues/330
[#368]: https://github.com/o2csi/candeo/issues/368

<!-- Candeo -->

[c-fingerprint]: ../../apps/desktop/src-tauri/src/journal.rs
[c-record]: ../../apps/desktop/src-tauri/src/storage.rs
[c-control]: ../../apps/desktop/src-tauri/src/lib.rs
[c-deviceout]: ../../apps/desktop/src-tauri/src/runtime/mod.rs

<!-- OpenRGB 1.1 (tag release_1.1, 8e16de27) unless stated -->

[o-tree]: https://gitlab.com/CalcProgrammer1/OpenRGB/-/tree/release_1.1
[o-search]: https://github.com/search?q=repo%3ACalcProgrammer1%2FOpenRGB+%22SPDX-License-Identifier%3A+GPL-2.0-only%22&type=code
[o-license]: https://gitlab.com/CalcProgrammer1/OpenRGB/-/blob/release_1.1/LICENSE
[o-meta]: https://gitlab.com/CalcProgrammer1/OpenRGB/-/blob/release_1.1/qt/org.openrgb.OpenRGB.metainfo.xml
[o-lamparray]: https://gitlab.com/CalcProgrammer1/OpenRGB/-/blob/release_1.1/Controllers/HIDLampArrayController/HIDLampArrayController.cpp
[o-netclient]: https://gitlab.com/CalcProgrammer1/OpenRGB/-/blob/release_1.1/NetworkClient.cpp
[o-proto]: https://gitlab.com/CalcProgrammer1/OpenRGB/-/blob/release_1.1/NetworkProtocol.h
[o-proto-10]: https://gitlab.com/CalcProgrammer1/OpenRGB/-/blob/release_1.0/NetworkProtocol.h
[o-proto-rc2]: https://gitlab.com/CalcProgrammer1/OpenRGB/-/blob/release_candidate_1.0rc2/NetworkProtocol.h
[o-server]: https://gitlab.com/CalcProgrammer1/OpenRGB/-/blob/release_1.1/NetworkServer.cpp
[o-sdkdoc]: https://gitlab.com/CalcProgrammer1/OpenRGB/-/blob/release_1.1/Documentation/OpenRGBSDK.md
[o-cli]: https://gitlab.com/CalcProgrammer1/OpenRGB/-/blob/release_1.1/cli.cpp
[o-resources]: https://gitlab.com/CalcProgrammer1/OpenRGB/-/blob/release_1.1/ResourceManager.cpp
[o-detection]: https://gitlab.com/CalcProgrammer1/OpenRGB/-/blob/release_1.1/DetectionManager.cpp
[o-logmanager]: https://gitlab.com/CalcProgrammer1/OpenRGB/-/blob/release_1.1/LogManager.cpp
[o-pro]: https://gitlab.com/CalcProgrammer1/OpenRGB/-/blob/release_1.1/OpenRGB.pro
[o-buildwin]: https://gitlab.com/CalcProgrammer1/OpenRGB/-/blob/release_1.1/scripts/build-windows.bat
[o-msi]: https://gitlab.com/CalcProgrammer1/OpenRGB/-/blob/release_1.1/scripts/build-msi.ps1
[o-smbus]: https://gitlab.com/CalcProgrammer1/OpenRGB/-/blob/release_1.1/Documentation/SMBusAccess.md
[o-usb]: https://gitlab.com/CalcProgrammer1/OpenRGB/-/blob/release_1.1/Documentation/USBAccess.md
[o-deps]: https://gitlab.com/CalcProgrammer1/OpenRGB/-/tree/release_1.1/dependencies
[o-pawniolib]: https://gitlab.com/CalcProgrammer1/OpenRGB/-/blob/release_1.1/dependencies/PawnIO/PawnIOLib.h
[o-mbedtls]: https://gitlab.com/CalcProgrammer1/OpenRGB/-/blob/release_1.1/dependencies/mbedtls-3.2.1/include/mbedtls/ssl.h
[o-hueplusplus]: https://gitlab.com/CalcProgrammer1/OpenRGB/-/blob/release_1.1/dependencies/hueplusplus-1.2.0/LICENSE
[o-e131]: https://gitlab.com/CalcProgrammer1/OpenRGB/-/blob/release_1.1/dependencies/libe131/src/e131.c
[o-razer]: https://gitlab.com/CalcProgrammer1/OpenRGB/-/blob/release_1.1/Controllers/RazerController/RazerController/RazerController.cpp
[o-nanoleaf]: https://gitlab.com/CalcProgrammer1/OpenRGB/-/blob/release_1.1/Controllers/NanoleafController/RGBController_Nanoleaf.cpp
[o-release]: https://codeberg.org/OpenRGB/OpenRGB/releases/tag/release_1.1
[o-debian]: https://sources.debian.org/src/openrgb/1.0+ds-0.1/debian/copyright/
[o-fedora]: https://mdapi.fedoraproject.org/rawhide/pkg/openrgb

<!-- Libraries OpenRGB bundles -->

[q-lic]: https://doc.qt.io/qt-6/licensing.html
[hidapi-lic]: https://github.com/libusb/hidapi/blob/master/LICENSE.txt
[hidapi-open]: https://github.com/libusb/hidapi/blob/master/windows/hid.c
[libusb-lic]: https://github.com/libusb/libusb/blob/master/COPYING
[r-openrgb]: https://crates.io/crates/openrgb
[r-openrgb2]: https://crates.io/crates/openrgb2

<!-- Hypercolor 0.9.0 (02f53272) -->

[h-tree]: https://github.com/hyperb1iss/hypercolor/tree/02f53272731f4c9fbcb06daf342732059bd4a4e7
[h-license]: https://github.com/hyperb1iss/hypercolor/blob/02f53272731f4c9fbcb06daf342732059bd4a4e7/LICENSE
[h-spec68]: https://github.com/hyperb1iss/hypercolor/blob/02f53272731f4c9fbcb06daf342732059bd4a4e7/docs/specs/68-openrgb-fallback-bridge.md
[h-spec81]: https://github.com/hyperb1iss/hypercolor/blob/02f53272731f4c9fbcb06daf342732059bd4a4e7/docs/specs/81-openrgb-fallback-productization.md
[h-fallback]: https://github.com/hyperb1iss/hypercolor/blob/02f53272731f4c9fbcb06daf342732059bd4a4e7/docs/content/hardware/openrgb-fallback.md
[h-cleanroom]: https://github.com/hyperb1iss/hypercolor/blob/02f53272731f4c9fbcb06daf342732059bd4a4e7/crates/hypercolor-openrgb-sdk/CLEANROOM.md
[h-host]: https://github.com/hyperb1iss/hypercolor/blob/02f53272731f4c9fbcb06daf342732059bd4a4e7/crates/hypercolor-openrgb-host/README.md
[h-detectors]: https://github.com/hyperb1iss/hypercolor/blob/02f53272731f4c9fbcb06daf342732059bd4a4e7/crates/hypercolor-openrgb-host/data/detector_usb_ids.toml
[h-process]: https://github.com/hyperb1iss/hypercolor/blob/02f53272731f4c9fbcb06daf342732059bd4a4e7/crates/hypercolor-openrgb-host/src/process.rs
[h-plan]: https://github.com/hyperb1iss/hypercolor/blob/02f53272731f4c9fbcb06daf342732059bd4a4e7/crates/hypercolor-openrgb-host/src/launch_plan.rs
[h-childproc]: https://github.com/hyperb1iss/hypercolor/blob/02f53272731f4c9fbcb06daf342732059bd4a4e7/crates/hypercolor-openrgb-host/src/child_process.rs
[h-supervisor]: https://github.com/hyperb1iss/hypercolor/blob/02f53272731f4c9fbcb06daf342732059bd4a4e7/crates/hypercolor-app/src/supervisor/openrgb.rs
[h-child]: https://github.com/hyperb1iss/hypercolor/blob/02f53272731f4c9fbcb06daf342732059bd4a4e7/crates/hypercolor-app/src/supervisor/child.rs
[h-pawnio]: https://github.com/hyperb1iss/hypercolor/blob/02f53272731f4c9fbcb06daf342732059bd4a4e7/crates/hypercolor-windows-pawnio/README.md
[h-fetchpawnio]: https://github.com/hyperb1iss/hypercolor/blob/02f53272731f4c9fbcb06daf342732059bd4a4e7/scripts/fetch-pawnio-assets.ps1

<!-- PawnIO, WinRing0 -->

[p-readme]: https://github.com/namazso/PawnIO/blob/9d52965895588b0ffa3703b72eec75ba19f4ccc0/README.md
[p-inf]: https://github.com/namazso/PawnIO/blob/9d52965895588b0ffa3703b72eec75ba19f4ccc0/PawnIO/PawnIO.inf.in
[p-sig]: https://github.com/namazso/PawnIO/blob/9d52965895588b0ffa3703b72eec75ba19f4ccc0/PawnIO/src/signature.cpp
[p-setup]: https://github.com/namazso/PawnIO.Setup/releases/tag/2.2.0
[p-modules]: https://github.com/namazso/PawnIO.Modules
[p-site]: https://pawnio.eu/
[w-cve]: https://nvd.nist.gov/vuln/detail/CVE-2020-14979
[w-defender]: https://www.microsoft.com/en-us/wdsi/threats/malware-encyclopedia-description?Name=VulnerableDriver:WinNT/Winring0.A
[w-blocklist]: https://learn.microsoft.com/en-us/windows/security/application-security/application-control/app-control-for-business/design/microsoft-recommended-driver-block-rules

<!-- Microsoft Store and MSIX -->

[m-policies]: https://learn.microsoft.com/en-us/windows/apps/publish/store-policies
[m-ada]: https://go.microsoft.com/fwlink/?linkid=528905
[m-prepare]: https://learn.microsoft.com/en-us/windows/msix/desktop/desktop-to-uwp-prepare

<!-- Free Software Foundation -->

[f-list]: https://www.gnu.org/licenses/license-list.html#GPLv2
[f-apache]: https://www.gnu.org/licenses/license-list.html#apache2
[f-faq]: https://www.gnu.org/licenses/gpl-faq.html
[f-v2v3]: https://www.gnu.org/licenses/gpl-faq.html#v2v3Compatibility
[f-aggregate]: https://www.gnu.org/licenses/gpl-faq.html#MereAggregation
[f-extended]: https://www.gnu.org/licenses/gpl-faq.html#DistributeExtendedBinary
