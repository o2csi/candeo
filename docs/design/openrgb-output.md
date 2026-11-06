# OpenRGB as an optional output (#368)

> **Status: decided, 2026-11-06. Not implemented.** Candeo drives, through
> OpenRGB, a device it has no definition for, while native HID stays the
> default ([#368]). §1 to §5 are the study that prepared the decision: how
> Hypercolor does it, and whether Candeo may embed OpenRGB. **The decision is
> option B′ of §5: do as Hypercolor does.** Candeo never ships OpenRGB; when
> the user has installed it and turns the option on, Candeo starts that
> OpenRGB headless, with a configuration of its own in which the detectors of
> every device Candeo drives natively are off. §6 is the design, §7 what the
> Store build allows, §8 the proof of concept, §9 what is not verified. §1 is
> an engineering reading of licences and store policies, not legal advice.

**In short.**

- **Licence.** OpenRGB's `LICENSE` is the GPLv2 text and its metadata says
  *GPL-2.0-or-later*, but 18 source files are *GPL-2.0-only*, the LampArray
  controller among them. The program as built can therefore only be passed on
  under GPLv2, which is not compatible with Candeo's GPL-3.0-only: **its code
  cannot be linked into Candeo.** Talking to an unmodified OpenRGB over its
  documented socket keeps the two programs separate. **Starting or connecting
  to an OpenRGB the user installed carries no obligation**: Candeo distributes
  nothing of it.
- **Hypercolor does not embed OpenRGB.** It finds an OpenRGB the user
  installed and, on request, starts it headless on `127.0.0.1` with a
  configuration folder of its own, in which it turns off the OpenRGB detectors
  for hardware its native drivers own. It stops only the server it started, and
  its SDK client, Apache-2.0, is written from OpenRGB's public documentation
  and never sends `SAVEMODE`.
- **Decision: B′, as Hypercolor does** (§5). Off by default. On, Candeo finds
  the installed OpenRGB 1.0 or later, writes a private configuration with the
  detectors of its native devices off, starts it with its SDK server on
  `127.0.0.1` and a port Candeo picks, and stops it with itself. An OpenRGB
  server already running is reused instead; Candeo cannot turn off that one's
  detectors, and says so once. The study had recommended A (connect only); A
  leaves out of Candeo's reach the one defence that acts before a device is
  opened: the detectors.
- **The client** is Candeo's own, written from OpenRGB's two public documents.
  It speaks protocol 4 to 6 and sends six packets: the handshake, the
  controller list, the mode and the colours. Never `SAVEMODE`, profiles,
  settings, rescans nor local status (§6.6). **Native first**: a controller
  Candeo has a definition for is never driven through OpenRGB (§6.8). An
  OpenRGB device is known by a fingerprint; its serial and location stay in
  memory (§6.9).
- **The Store build may do it** (§7): a full-trust packaged app may start an
  executable outside its package, as Candeo already starts the file manager.
  Never elevated, no driver of Candeo's, OpenRGB never offered for download.
- **The remaining risk is still two writers on one device**: an OpenRGB
  server Candeo reuses, or one the user starts beside Candeo's, may write a
  device Candeo drives natively. Candeo then refuses its own route and says so;
  it cannot stop the other program.

## Sources and versions

Nothing was installed, built or run: what follows comes from the sources and
documents below, read on 2026-11-06.

| What | Version read | Date |
|---|---|---|
| OpenRGB | tag `release_1.1`, commit `8e16de27` ([code][o-tree]); licence headers counted on `master` at `4f2d8920` through the GitHub mirror's code search ([search][o-search]) | released 2026-10-16; `master` of 2026-11-05 |
| OpenRGB's documents for the client | `OpenRGBSDK.md` and `RGBControllerAPI.md` at `release_1.1` ([docs][o-sdkdoc], [docs][o-apidoc]); for §6.2 and §6.5, the command-line help in `cli.cpp` at `release_1.0` and `release_1.1`, and for §6.4 `DetectionManager.cpp` and `LogManager.cpp` at `release_1.1`, none of them used for the client | 1.1 of 2026-10-16; 1.0 of 2026-09-11 |
| winget's OpenRGB package | `OpenRGB.OpenRGB` 1.1.0 ([data][w-winget]) | 2026-10-16 |
| Hypercolor | 0.9.0, commit `02f53272` ([code][h-tree]); the OpenRGB paths are identical on `main` at `1bb9a58e` | 2026-11-03 |
| PawnIO | driver repository at `9d529658` ([code][p-readme]); installer 2.2.0 ([notes][p-setup]) | 2026-08-14; 2026-03-15 |
| Microsoft Store Policies | 7.20 ([docs][m-policies]) | 2026-09-14 |
| App Developer Agreement | 8.11 ([docs][m-ada]) | published 2026-03-17, effective 2026-04-17 |
| MSIX preparation guide | ([docs][m-prepare]) | updated 2025-06-10 |
| How packaged desktop apps run; `UpdateProcThreadAttribute`; nested jobs | ([docs][m-behind], [docs][m-procattr], [docs][m-nested]) | 2026-09-30; updated 2025-07-01; 2025-07-14 |
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
  names with `--config`. For an OpenRGB installed outside the package, which
  is what was decided, §7 answers.
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
| Require protocol 6, OpenRGB 1.0 or later: its default bind is loopback and its controller ids are stable. Decided (§6.2, §6.6): required of what Candeo starts; a server the user runs is accepted from protocol 4 | yes | yes |
| A fixed list of packets in the encoder, the rest impossible to build, tested: handshake (40, 50), controller count and data (0, 1), mode selection (1100, 1101, 1103), colours (1050, 1051); received, `ACK` (10) and `DEVICE_LIST_UPDATED` (100). Never `SAVEMODE`, profiles, settings, rescans, zone or segment edits, device configuration. Decided narrower (§6.6): 1100, 1103 and 1051 left out | yes | yes |
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
   VID:PID that the conflict guard compares with Candeo's definitions (§6.8).
3. **settings.json keys an OpenRGB device by fingerprint**: of the serial when
   there is one, otherwise of vendor, name and zone shape, marked as the weaker
   kind. Hypercolor keeps the raw serial in its fingerprint and its
   configuration (§2); Candeo would not.
4. **The log, the copied diagnostic and *Report*** name an OpenRGB device by
   its fingerprint, OpenRGB's name for it, its type, its zones and LED counts,
   OpenRGB's version and the negotiated protocol. Never its serial, nor its
   location, whatever that holds: a path or an address.
5. **PRIVACY.md gains a paragraph** before it ships: off by default; on, Candeo
   starts or connects to OpenRGB on this computer, reads the devices it lists
   and sends colours, nothing else.

Seen while reading for this study: *Control* writes the unit's serial into
`settings.json` — the one the device's protocol reports, the USB descriptor's
otherwise ([code][c-record], [code][c-control]) — while PRIVACY.md says a
keyboard's serial "is never written down". Since fixed: `settings.json` keeps the
serial's fingerprint, and a serial an earlier version wrote becomes its
fingerprint at the next save. An OpenRGB record follows the same rule.

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
| **Store** | compatible: loopback client, no driver, no elevation | HID only; the licence terms and the source offer to settle; a package about 22 MB larger, the size of 1.1's Windows zip ([notes][o-release]) | allowed from full trust (§7) | compatible, as today |
| **Coverage** | everything the user's OpenRGB drives, SMBus included when the user set PawnIO up | HID and USB in the Store; more elsewhere | everything the user's OpenRGB drives | keyboards and laptop zones a definition can express; no RAM, motherboards, GPUs or network lights |

### Recommendation, and the decision

The study recommended **A beside C**: A reaches at no legal cost what C never
will (RAM, fans, strips, motherboards, network lights), C stays the path for
keyboards and laptops, B is not worth a GPLv2 source offer for a Store
package that would drive HID only, and B′ was to be reconsidered after A's
proof of concept.

**Decided on 2026-11-06: B′, beside C.** What tipped it:

- **Under A, nothing acts before OpenRGB opens a device.** Candeo must not
  rewrite the user's settings, so the user's OpenRGB detects, opens and
  initialises every device it knows, Candeo's native ones included; the
  conflict guard can only refuse Candeo's own route afterwards. Under B′,
  Candeo's OpenRGB never runs the detectors of the devices Candeo drives
  natively: they are off in its configuration before it starts (§6.4).
- **A's friction is the server**: start it, keep it running. B′ is one switch
  once OpenRGB is installed.
- **Nothing is shipped**: B′'s legal row is A's. Starting a program the user
  installed distributes nothing of it (§1.3, last row).

What it costs, accepted: while the option is on, Candeo opens an
unauthenticated port on loopback (§4.1), which any local process can reach;
a detector map to refresh with each OpenRGB release (§6.4); process
supervision (§6.5). C stays first: a device Candeo has a definition for never
goes through OpenRGB (§6.8).

It departs from [#368] on one point: step 6 excluded a background server
start. The server starts only once the user has turned the option on, with
Candeo, and it never outlives Candeo (§6.5).

## 6. Design

### 6.1 The switch

- *Settings* gets an **OpenRGB** switch, off by default. Off, Candeo looks for
  no executable, opens no port, connects to nothing, starts nothing and writes
  nothing: install and startup are unchanged, as [#368] requires.
- On, §6.2 to §6.5 run at once, and again each time Candeo starts while it
  stays on. Turned off, or Candeo quitting: the OpenRGB devices are released
  (§6.7), the connection closed, and the OpenRGB Candeo started is stopped
  (§6.5).
- One at a time: Candeo runs as a single instance (`single_instance.rs`), and
  so does the OpenRGB it starts.

### 6.2 Finding the installed OpenRGB

Looked for in this order; the first found is used.

| Platform | Where | Why there |
|---|---|---|
| Windows | `%ProgramFiles%\OpenRGB\OpenRGB.exe` | the MSI's default folder. winget's `OpenRGB.OpenRGB` 1.1.0 installs that MSI, machine-wide, with `DefaultInstallLocation: %ProgramFiles%\OpenRGB` ([data][w-winget]) |
| Windows | `OpenRGB.exe` in a `PATH` folder | a portable zip someone put on the path |
| Linux | `openrgb` in a `PATH` folder | the distribution packages and upstream's `.deb` and `.rpm` |
| Linux | the Flatpak `org.openrgb.OpenRGB` ([readme][o-readme]): `/var/lib/flatpak/app/org.openrgb.OpenRGB/current`, or `~/.local/share/flatpak/app/org.openrgb.OpenRGB/current` for a user installation, with `flatpak` on the `PATH` | started through `flatpak run` (§6.5) |

- **Not looked for**: an AppImage or a portable zip off the `PATH`, which have
  no standard place, and an MSI installed with another `INSTALLDIR`. A path
  the user picks comes before the table; the proof of concept takes one, the
  interface for it is left for later.
- **The version, before starting.** `OpenRGB --version` prints a `Version:`
  line and exits before any detection, in 1.0 and 1.1 ([code][o-cli-10],
  [code][o-cli]). Candeo reads that line, allowing three seconds and 4 KiB of
  output, and **starts only 1.0 or later**: the first release read here whose
  help documents `--server-host` with a loopback default, and whose default
  bind is loopback (§3.4). Candeo does not find out by trying: 1.1 reports an
  option it does not know only after detection, once devices are opened
  ([code][o-cli]). A pipeline build reads `0.9+ (1.0rc3)`: the version in
  parentheses counts ([docs][h-host]). The 1.0 release candidates are refused.
- **Not found, or older than 1.0**: one sentence and the action (§6.10).

### 6.3 A server already running

Before starting anything, Candeo tries `127.0.0.1:6742`, the default port
([docs][o-sdkdoc], *Protocol Basics*): connecting within half a second, the
protocol version answered within a second.

- **It answers: Candeo reuses it**, and never stops it. Candeo cannot turn off
  that server's detectors. That would mean rewriting the user's OpenRGB
  settings, which Candeo refuses to do (§5), and over the SDK it would take the
  local status Candeo never asks for (§6.6). It says so once (§6.10). Its
  controllers go through the same guard (§6.8). Protocol 4 or later is
  accepted, OpenRGB 0.9 onwards: the server is the user's and runs already;
  Candeo only connects to it.
- **Nothing answers, or something that is not OpenRGB**: Candeo starts its own
  (§6.5), on another port.
- **An OpenRGB runs without its server** (a process named `OpenRGB.exe` or
  `openrgb`, the user's window): Candeo does not start a second one, which
  would detect the same devices, and says what to do (§6.10). It reads process
  names, nothing else.
- A server the user runs on another port is not found; that case is the
  user's OpenRGB beside Candeo's (§6.8).

### 6.4 The private configuration

- **The folder**: `openrgb` in Candeo's local data folder
  (`app_local_data_dir()`: `%LOCALAPPDATA%\com.o2csi.candeo\openrgb`,
  `~/.local/share/com.o2csi.candeo/openrgb`), given to OpenRGB with `--config`.
  The user's own configuration (`%APPDATA%\OpenRGB`, `~/.config/OpenRGB`) is
  never read nor written, so none of the user's profiles is loaded.
- **`OpenRGB.json`**, written before each start, through a temporary file and
  a rename. Candeo sets two things and keeps every other key as it finds it:

| Key | Value | Why |
|---|---|---|
| `Detectors.detectors.<name>` | `false` for each detector to turn off; **removed** for the other names the map knows | OpenRGB skips a detector turned off before running its function, so before the device is opened ([code][o-detection]). A removed name gets OpenRGB's registered default back at its next start, where `true` would turn on a detector OpenRGB ships off |
| `LogManager.log_file` | `false` | OpenRGB logs device paths on hot plug at *info* ([code][o-detection]), and its log files would land in Candeo's folder, under `logs` ([code][o-logmanager]) |

- OpenRGB adds the detector names it lacks and drops those it no longer
  registers, then saves, before it detects ([code][o-detection]): a name
  this OpenRGB does not know is harmless, and OpenRGB removes it itself.
- An `OpenRGB.json` Candeo cannot read is replaced by a fresh one: the file is
  Candeo's own, in Candeo's folder, and the log says so.

**Which detectors are turned off.**

1. **Candeo's native ids**: the VID:PID of every definition Candeo has for
   this computer, built in, unverified or yours, plugged in or not, controlled
   or not. Interface and usage are ignored: a detector claiming that VID:PID
   on any interface is off.
2. **The map**: from each OpenRGB detector name to the USB ids it claims,
   generated for one OpenRGB release and embedded with `include_str!`, as
   Hypercolor's ([code][h-host], [data][h-detectors]). Its source is the
   `60-openrgb.rules` that release installs ([docs][o-udevdoc]), read as data:
   each entry names the detector it comes from, which is what Hypercolor's
   generator relies on. The file records the release and the rules file's
   SHA-256. Hypercolor's holds about 1,100 names for 1.0.
3. **Turned off**: every detector whose ids meet Candeo's, and **every generic
   HID detector**, registered for any VID and PID and turned on or off by the
   same setting ([code][o-detection]): one cannot be told to leave a single
   device alone. The rules file cannot list those; the generator finds them in
   the release's detector registrations, read as data, each file's SPDX line
   checked first like the device generators (§5, C). The cost: a device only a
   generic detector reaches, such as a LampArray device Candeo has no
   definition for, is not reached through OpenRGB either (§9); [#330] reopens
   it.
4. **An OpenRGB newer than the map** runs its new detectors; the guard (§6.8)
   remains. The log says the installed release is newer than the map's.
5. **A definition added while OpenRGB runs** (*Yours*): its detectors are off
   from the next start; the guard covers the time between.

### 6.5 Starting and stopping it

The arguments, and nothing else:

```
OpenRGB --server --server-host 127.0.0.1 --server-port <port> --noautoconnect --config <folder> --loglevel 2
```

| Argument | Why |
|---|---|
| `--server` | starts the SDK server. The help shows the window with `--gui`, or with no argument at all ([code][o-cli]) |
| `--server-host 127.0.0.1` | the default since 1.0, passed anyway, as Hypercolor does ([code][h-process]): no setting of OpenRGB's can then widen it |
| `--server-port <port>` | a free port Candeo picks by binding `127.0.0.1:0`. Never 6742, where other clients look first and where Candeo looks for the user's server (§6.3). If it was taken in the meantime, the handshake never answers: Candeo stops the child and tries once more with another |
| `--noautoconnect` | without it, OpenRGB tries at startup to connect to a local server ([docs][o-man]), which may be the user's |
| `--config <folder>` | §6.4 |
| `--loglevel 2` | warnings and above, below the *info* level where paths are logged ([code][o-logmanager]) |

- **Flatpak**: `flatpak run --filesystem=<folder> org.openrgb.OpenRGB` and the
  same arguments. The folder must exist first, `--filesystem` ignoring one that
  does not, and hold no `:` ([code][h-process]).
- **Its output goes nowhere**: what OpenRGB prints may carry device paths, which
  Candeo's log must not. On Windows it starts with `CREATE_NO_WINDOW`, so no
  console appears.
- **Tied to Candeo's lifetime.** Windows: a job object with
  `JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE`, the child assigned to it as soon as it
  is created, Candeo holding the only handle: however Candeo ends, Windows
  closes the handle and kills the child. Inside the Store package Candeo's
  process is itself in a job, and jobs nest since Windows 8 ([docs][m-nested]).
  Linux: `PR_SET_PDEATHSIG` with `SIGTERM` set in the child before `exec`, and
  a process group of its own. That signal follows the *thread* that started the
  child, so a supervisor thread lives as long as the output.
- **Ready** when the handshake answers on the port: polled every 250 ms for at
  most 15 seconds. Devices then arrive as OpenRGB detects them, each list change
  announced by `DEVICE_LIST_UPDATED` ([docs][o-sdkdoc]).
- **Stopping**: on Windows the job is terminated; on Linux `SIGTERM`, then
  `SIGKILL` after three seconds. Only the OpenRGB Candeo started is ever
  stopped.
- **If it exits on its own**, it is not restarted: the switch says so and gives
  the action (§6.10). No loop.

### 6.6 The client

Written from OpenRGB's public documentation only: the SDK document and the
RGBController API document at `release_1.1` ([docs][o-sdkdoc],
[docs][o-apidoc]). No OpenRGB source and no GPL-2.0 crate (§1.2), the rule
Hypercolor's client follows ([code][h-cleanroom]). It is Candeo's code, under
Candeo's licence.

**Framing** ([docs][o-sdkdoc], *NetPacketHeader structure*): sixteen bytes,
`ORGB`, `pkt_dev_id`, `pkt_id`, `pkt_size`, then the payload. Neither document
states the byte order: Candeo uses little endian, the order of every platform
OpenRGB is released for (§9). A colour is `0x00BBGGRR` ([docs][o-apidoc]),
so red, green, blue and a zero byte on the wire. A string's length counts its
terminating zero ([docs][o-sdkdoc]).

**Negotiation** ([docs][o-sdkdoc], *NET_PACKET_ID_REQUEST_PROTOCOL_VERSION*):
Candeo sends 6, the server answers its own highest, the lower of the two is
used. No answer means a version 0 server; below 4, Candeo stops there with one
sentence (§6.10). Then `SET_CLIENT_NAME` carries `Candeo`, the program's name,
never the computer's.

| | 4 (0.9) | 5 (1.0rc1, rc2) | 6 (1.0, 1.1) |
|---|---|---|---|
| A controller in `pkt_dev_id` | its index | its index | its id, kept while the server runs |
| `REQUEST_CONTROLLER_COUNT` answers | the count | the count | the count and the ids |
| `mode_value`, `led_value` | present; `mode_value` sent back in `UPDATEMODE` | same | gone |
| Zones | segments | segments, zone flags | matrix maps in segments, zone modes |
| Controller | — | flags, LED display names | display name, configuration |
| On `DEVICE_LIST_UPDATED` | every controller read again | same | the ids read again; gone ones dropped, new ones read |

Candeo starts only 1.0 or later (§6.2): versions 4 and 5 come only from a
server the user runs (§6.3).

**The packets.** Each is in [docs][o-sdkdoc] under its own name.

| Direction | Packet | Id | When |
|---|---|---|---|
| sent | `REQUEST_PROTOCOL_VERSION` | 40 | once, on connecting |
| sent | `SET_CLIENT_NAME` | 50 | once |
| sent | `REQUEST_CONTROLLER_COUNT` | 0 | after connecting, and on each `DEVICE_LIST_UPDATED` |
| sent | `REQUEST_CONTROLLER_DATA` | 1 | for each controller to read, with the negotiated version |
| sent | `RGBCONTROLLER_UPDATEMODE` | 1101 | on *Control*, to the direct mode; on *Release*, back to the mode found (§6.7) |
| sent | `RGBCONTROLLER_UPDATELEDS` | 1050 | each frame, every LED of the controller |
| received | the answers to 40, 0 and 1 | | |
| received | `DEVICE_LIST_UPDATED` | 100 | the list is read again |
| received | `ACK` | 10 | protocol 6; a failure is logged with its status code |
| received | anything else | | read to its length and dropped |

**Everything else cannot be encoded**, and a test tries every documented id:
`SAVEMODE` (1102); `SETCUSTOMMODE` (1100), which lets OpenRGB pick a mode by
name, Direct, then Custom, then Static ([docs][o-apidoc]), possibly one Candeo
would refuse (§6.7); `UPDATEZONEMODE`, `UPDATEZONELEDS` and
`UPDATESINGLELED` (1103, 1051, 1052), not needed; zone resizing, segments,
zone and device configuration and hiding (1000 to 1005); device-specific
configuration (1130, 1131); rescanning (140); profiles (150 to 161); plugins
(200, 201); settings (250 to 254); the log (300 to 303); the device listings
(120 to 124); and `SET_CLIENT_FLAGS` (52), the only way to ask for local
status, which the server grants a loopback client that asks ([docs][o-sdkdoc]).
It is narrower than §4.2's list: 1100, 1103 and 1051 are gone.

The client also refuses any address that is not loopback.

**Reading.** A thread reads each connection, so packets nobody asked for never
fill the socket. A payload above 8 MiB drops the connection. Every length and
count is checked against the bytes left and every product for overflow, a
matrix map's `8 + 4 × width × height` against the length announced
([docs][o-sdkdoc], *Matrix Map Data*). Strings are decoded as UTF-8, invalid
bytes replaced. `data_size` is "the size of all data in packet": Candeo writes
the whole payload's length, itself included, and when reading only checks
that it fits (§9).

### 6.7 Control and Release

- **Control.** Candeo reads the controller again and picks its **direct mode**:
  a mode with per-LED colours (flag bit 5 and colour mode 1, [docs][o-apidoc],
  *Mode Flags*, *Color Mode Values*) **that does not save itself** (bit 9,
  "automatically saves"), the one named `Direct` first. It sends
  `UPDATEMODE` with that mode, at its maximum brightness when it has one
  (bit 4), then reads the controller back to check the active mode: the
  hardware rule of [`AGENTS.md`](../../AGENTS.md), as far as the SDK reaches.
- **No such mode**: the device is listed and not driven (§6.10).
- **A mode flagged "automatically saves" is never selected**, neither on
  *Control* nor on *Release*: it is the one sign the documentation gives of a
  mode written to the device's memory. A controller that persists a mode
  without the flag remains invisible (§4.2).
- **Each frame**: `UPDATELEDS` with every LED's colour, in OpenRGB's LED
  order. Zones keep their LED range; a matrix map gives positions. Only the
  latest frame is kept for each controller.
- **Release**: `UPDATEMODE` back to the mode active on *Control*, unless that
  mode saves itself; then the lights keep the last frame. Never `SAVEMODE`.
- **A zone OpenRGB reports with no LED** (an addressable header whose length
  the user sets) has nothing to drive. `RESIZEZONE` stays out of the list: a
  later decision, which Hypercolor takes only for lengths the user entered.

### 6.8 Native first

After each read of the list, before anything is driven, each controller is
sorted:

| Controller | Result |
|---|---|
| `location` is `HID: ` and a path (§4.3) whose VID:PID is one of Candeo's (§6.4): on Windows, `VID_xxxx&PID_xxxx` in the path; on Linux, `/dev/hidrawN` read through `/sys/class/hidraw/hidrawN/device/uevent` (`HID_ID=bus:vid:pid`) | **native**: not listed as an OpenRGB device, never driven through OpenRGB; the log says it was set aside |
| its serial's fingerprint is that of a unit Candeo has open natively | native, likewise |
| `HID: ` with a path that gives no VID:PID | not driven: Candeo cannot tell it is not one of its own |
| another kind of location: SMBus, network, serial port | an OpenRGB device |
| two controllers with one fingerprint | listed as duplicates, neither driven |

What the guard cannot do, since nothing in Windows or Linux reserves a HID
device for one program (hidapi opens it for shared reading and writing,
[code][hidapi-open]):

- **A reused server** has its own detectors on: it may write a native device
  while Candeo writes it too. Candeo says so once (§6.10).
- **The user's own OpenRGB started beside Candeo's** looks for a server on
  6742, not on Candeo's port, so it detects on its own: two OpenRGBs on the
  same devices, the native ones among them. Candeo does not watch for it.
- **A device OpenRGB reaches through libusb** has no HID location; only the
  serial rule can catch it (§9).
- A definition added in *Yours* for a device driven through OpenRGB: Candeo
  releases it from OpenRGB first, then drives it natively.

### 6.9 Identity

- `serial` and `location` are parsed into a type that prints neither: its
  `Debug` writes a placeholder, it has no `Display` and no `Serialize`. They
  live in memory for the connection only, for two uses: the fingerprint, and
  the guard's VID:PID. Never in `settings.json`, the log, the copied
  diagnostic, *Report* or a `Failure`.
- **The fingerprint** is `journal::fingerprint`'s function ([code][c-fingerprint]),
  of the serial when there is one, otherwise of the vendor, the name and each
  zone's name and LED count. The second kind is the weaker: two identical
  units share it, and §6.8 lists them as duplicates.
- `settings.json` keys an OpenRGB device by its fingerprint and kind. Protocol
  6 ids last as long as the server; after a reconnection or a new server,
  devices are bound again by fingerprint before the next frame.
- The log and the diagnostic name an OpenRGB device by its fingerprint,
  OpenRGB's name for it, its type, its zones and LED counts, OpenRGB's version
  and the negotiated protocol.
- [`PRIVACY.md`](../../PRIVACY.md) gains its paragraph (§4.3) before the
  option ships.

### 6.10 What the user sees

Beside the switch, states and actions only ([`AGENTS.md`](../../AGENTS.md),
*Interface text*); the English below is a draft for the catalogs:

| State | Text | When |
|---|---|---|
| looking | *Looking for OpenRGB* | §6.2 to §6.5 running |
| not found | *OpenRGB was not found. Install OpenRGB 1.0 or later, then turn this on again.* | §6.2 |
| too old | *This OpenRGB is too old. Update it to 1.0 or later.* | §6.2 |
| open without its server | *OpenRGB is open without its server. Start its SDK server, or quit it.* | §6.3 |
| connected, started | *Connected to OpenRGB 1.1* | §6.5 |
| connected, reused | *Connected to the OpenRGB already running* and, once: *Candeo cannot turn off this OpenRGB's detection: it may also write the devices Candeo drives.* | §6.3; the one warning, for what is surprising |
| stopped | *OpenRGB has stopped. Turn this off and on again to restart it.* | §6.5 |
| protocol below 4 | *This OpenRGB is too old. Update it to 1.0 or later.* | §6.6 |

In *Devices*, OpenRGB devices are listed with the others, marked **via
OpenRGB**, with *Control* and *Release* like any device and the existing
states: *controlled*, *not open*, and *unplugged* when OpenRGB no longer lists
them. Two new states: **no direct mode** (§6.7) and **duplicate** (§6.8), both
listed, with no *Control*. A controller set aside as native (§6.8) is not
listed a second time: the native entry is the only one.

New terms, to settle with the interface: *via OpenRGB* (*via OpenRGB*),
*no direct mode* (*sans mode direct*), *duplicate* (*en double*). Text goes
through the catalogs (`openrgb.*`), errors as `Failure` codes under `errors.`.

## 7. What the Store build allows

Candeo's package is full trust (`runFullTrust`, [`msix.md`](msix.md) §1). For
starting an OpenRGB installed outside it:

| Question | Answer | Source |
|---|---|---|
| May a full-trust packaged app start an executable outside its package? | Yes. Candeo's package already starts the file manager through `CreateProcessW`; the certification kit passes, reporting it in an optional test only | [`msix.md`](msix.md) §4 |
| Does that child run inside the package? | Not by default. For a process with no policy, the documented default is that it creates its children "outside of the desktop app runtime environment"; `PROC_THREAD_ATTRIBUTE_DESKTOP_APP_POLICY` changes it. Candeo sets none: OpenRGB runs as if started from the Start menu, without the package's identity | [docs][m-procattr]; to measure (§9) |
| Does the private configuration land where OpenRGB reads it? | Yes, either way: it is under `%LOCALAPPDATA%`, which was measured not redirected for Candeo's own package, and a child outside the package is not redirected at all | [`msix.md`](msix.md) §2, [docs][m-behind] |
| Can Candeo tie it to its lifetime? | Yes: a job object of Candeo's own, nested in the package's, Windows 8 and later | [docs][m-nested] |
| Elevation | Never: "apps that require elevation for any part of their functionality" are refused. Started as the user, OpenRGB reaches HID and USB devices, not SMBus ones (§3.1) | [docs][m-prepare] |
| Drivers | None of Candeo's (policy 10.2.4). winget's OpenRGB package pulls in PawnIO as a dependency: that is the user's installation, not Candeo's | [docs][m-policies], [data][w-winget] |
| Offering OpenRGB | Never for download nor installation: 10.2.3 forbids offering secondary software the publisher did not develop and that does not enhance the product, and 10.1.5 limits what an app may offer to acquire to Store products and add-ons. Candeo names OpenRGB and says it is needed | [docs][m-policies] |
| Dynamic code (10.2.2) | Candeo runs no code it downloads; it starts a program the user installed, for a function its listing describes | [docs][m-policies] |
| Disclosure (10.2.4) | Required at the start of the description for a dependency of the *primary* functionality, which this is not; the listing names OpenRGB where it presents the option | [docs][m-policies] |
| S mode | Store apps must be compatible with it, and there only Store apps run: OpenRGB cannot be installed, and the option says it is not | [docs][m-prepare] |
| Loopback | A full-trust client and its child on `127.0.0.1` declare nothing (§3.2) | |

Everything else is as in the MSI and NSIS builds.

## 8. The proof of concept

In a pull request of its own, after this one: the client and the process
management in a crate of their own, behind an `openrgb` feature of the desktop
application, off by default, wired into nothing yet. Mapped to [#368]'s steps:

1. **Connect**: framing, negotiation from 4 to 6, the packets of §6.6 and the
   refusal of every other, the reader thread, the list kept in step with
   `DEVICE_LIST_UPDATED`.
2. **Capabilities as reported**: the controller data of protocols 4, 5 and 6
   parsed into zones and LEDs, matrix maps into positions, the direct mode
   chosen or refused (§6.7), serial and location kept out (§6.9).
3. **One effect**: `UPDATELEDS` from a frame; running a real effect needs the
   wiring.
4. **Reconnects**: ids against indexes, fingerprints.
5. **Native first**: the guard of §6.8 on Windows paths and Linux sysfs.
6. **Trust and lifecycle**: discovery, the version check, the configuration
   writer, the launch decision of §6.3, the arguments of §6.5, the child tied
   to the process.
7. **Measures** and 8. **attribution**: §9, and none owed (§1.3).

Tested without OpenRGB: byte fixtures built from the documents, each citing
its section; a fake server in the test process for the round trip; the
configuration writer on a sample file; discovery on a fake file system.

## 9. Not verified

Nothing below was run: no OpenRGB was installed, started or connected to.

**What needs a real OpenRGB, and a device:**

- The client against a server: the byte order and colour order (§6.6), what
  `data_size` counts, whether a protocol 6 server sends `ACK` or other packets
  unasked, and the parsing of real controller data, protocols 4 to 6.
- The direct mode chosen by §6.7 lighting each LED, the read-back of the mode,
  and the release putting the previous mode back; `UPDATEMODE` on a protocol 6
  device whose zones have modes of their own.
- Latency and frame rate against native HID, and OpenRGB's start time
  ([#368], step 7).
- Each entry of `60-openrgb.rules` naming its detector, as Hypercolor's
  generator relies on; the generic HID detectors' names; the map for 1.1.
- A detector turned off leaving its device untouched, and whether OpenRGB
  writes to a device when it detects it, which makes the partition matter
  beyond the guard.
- Whether OpenRGB's LampArray controller, which OpenRGB turns off where its
  Razer driver knows the device
  ([`rgb-controllers.md`](../comparison/rgb-controllers.md) §10), claims the
  DeathStalker V2 Pro once the Razer detector for it is off, and whether it is
  one of the generic HID detectors §6.4 turns off.
- Plugins: whether a private configuration folder is the only place OpenRGB
  loads them from, on Linux in particular.
- Headless on Windows: no window, no notification area icon, and no dialog,
  the *Dialog* log level always showing one ([code][o-logmanager]).
- `--version`'s line on 1.0 and 1.1 as built, and the pipeline banner.
- Flatpak: whether the sandboxed OpenRGB dies with `flatpak run`, and its
  access to devices.
- A device reached through libusb rather than HID (§6.8).

**What needs the Store package:**

- Whether the child runs outside the package as the documentation says, where
  its writes land, and the job object nesting in the package's.

**Carried from the study:**

- Which licence terms Candeo's Store listing gives (§3.3).
- Whether Defender or the vulnerable driver blocklist list PawnIO, and whether
  the current blocklist names WinRing0 (§3.1).
- The licences of the NVAPI wrapper (`dependencies/NVFC`) and the colour wheel
  widget in OpenRGB's tree: no statement found in their headers.
- Which OpenRGB controllers persist a mode change without `SAVEMODE` and
  without the automatic-save flag (§4.2, §6.7): per controller, in OpenRGB's
  code.
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
[o-apidoc]: https://gitlab.com/CalcProgrammer1/OpenRGB/-/blob/release_1.1/Documentation/RGBControllerAPI.md
[o-udevdoc]: https://gitlab.com/CalcProgrammer1/OpenRGB/-/blob/release_1.1/Documentation/UdevRules.md
[o-readme]: https://gitlab.com/CalcProgrammer1/OpenRGB/-/blob/release_1.1/README.md
[o-cli]: https://gitlab.com/CalcProgrammer1/OpenRGB/-/blob/release_1.1/cli.cpp
[o-cli-10]: https://gitlab.com/CalcProgrammer1/OpenRGB/-/blob/release_1.0/cli.cpp
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
[o-man]: https://manpages.debian.org/testing/openrgb/openrgb.1.en.html

<!-- Libraries OpenRGB bundles -->

[q-lic]: https://doc.qt.io/qt-6/licensing.html
[hidapi-lic]: https://github.com/libusb/hidapi/blob/master/LICENSE.txt
[hidapi-open]: https://github.com/libusb/hidapi/blob/master/windows/hid.c
[libusb-lic]: https://github.com/libusb/libusb/blob/master/COPYING
[r-openrgb]: https://crates.io/crates/openrgb
[r-openrgb2]: https://crates.io/crates/openrgb2

<!-- Installing OpenRGB -->

[w-winget]: https://github.com/microsoft/winget-pkgs/blob/master/manifests/o/OpenRGB/OpenRGB/1.1.0/OpenRGB.OpenRGB.installer.yaml

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

<!-- Microsoft Store, MSIX and Windows -->

[m-policies]: https://learn.microsoft.com/en-us/windows/apps/publish/store-policies
[m-ada]: https://go.microsoft.com/fwlink/?linkid=528905
[m-prepare]: https://learn.microsoft.com/en-us/windows/msix/desktop/desktop-to-uwp-prepare
[m-behind]: https://learn.microsoft.com/en-us/windows/msix/desktop/desktop-to-uwp-behind-the-scenes
[m-procattr]: https://learn.microsoft.com/en-us/windows/win32/api/processthreadsapi/nf-processthreadsapi-updateprocthreadattribute
[m-nested]: https://learn.microsoft.com/en-us/windows/win32/procthread/nested-jobs

<!-- Free Software Foundation -->

[f-list]: https://www.gnu.org/licenses/license-list.html#GPLv2
[f-apache]: https://www.gnu.org/licenses/license-list.html#apache2
[f-faq]: https://www.gnu.org/licenses/gpl-faq.html
[f-v2v3]: https://www.gnu.org/licenses/gpl-faq.html#v2v3Compatibility
[f-aggregate]: https://www.gnu.org/licenses/gpl-faq.html#MereAggregation
[f-extended]: https://www.gnu.org/licenses/gpl-faq.html#DistributeExtendedBinary
