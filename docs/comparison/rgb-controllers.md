# RGB lighting controllers compared

What Candeo, OpenRGB, SignalRGB, Artemis, KeyGlow, Hypercolor and Windows
Dynamic Lighting each implement, under which conditions, and the source each
claim rests on ([#369]). It exists so that choices in Candeo — composition
([#360]), saved setups ([#367]), an OpenRGB output ([#368]), an agent interface
([#359], [#364], [#365]), LampArray ([#330]) — start from verified features and
limits rather than from feature counts.

It is a technical reference: what the code and the official documentation say,
not a judgement of the products.

## How to read it

Each feature gets one status per product:

| Status | Meaning |
|---|---|
| **implemented** | the code at the pinned version, or the official documentation, has it |
| **partial** | conditional or incomplete; the row says on what: a plugin, a paid tier, a platform, one kind of device |
| **planned** | announced by the project; the issue or roadmap is cited |
| **not found** | looked for and not found; the row or §9 says where. Not proof of absence, least of all for closed source |
| **not tested** | documented, but nothing we could read confirms it |

- **One version per product**, pinned in §1 with the date it was checked. A
  later version may differ: §11 says how to refresh a column.
- **The link says what was read:** *code* (the source at the pinned commit),
  *docs* (official documentation), *notes* (release notes or changelog),
  *readme* (the project's README or roadmap), *data* (a list the project
  publishes), *forum* (a staff reply on the official forum), *issue*. Where code
  and documentation disagree, the row says so and follows the code.
- **Nothing here was run.** No product was installed or tried on hardware for
  this comparison, Candeo included: its column comes from this repository.
  *Implemented* means the source says so, not that we watched it light.
- **Detected is not verified.** A device a program lists may never have been
  tried with it. §3.2 says, for each product, what it tells about the hardware
  it was verified on.

## Terms

The terms of [#360] and [`presets.md`](../design/presets.md) §1, applied to
every product whatever it calls them.

| Term | Means here | Not to confuse with |
|---|---|---|
| **Effect** | a program computing colours for a device's lights, frame after frame, on the computer | a *hardware effect*: a mode the device's firmware runs by itself (*Spectrum Cycle*), which the computer only selects |
| **Preset** | named settings for one effect, picked again later | a whole profile, which some products call a preset (KeyGlow) |
| **Region** | a part of one device running its own effect | an OpenRGB *zone*, a property of the hardware; in Candeo, a *zone* is one light of a zones device |
| **Compositing layers** | several effects over the same lights, combined by order, opacity or blend mode | regions, which share the lights out between effects without overlap |
| **Saved device configuration** | a named state of one device (effect, settings, brightness), saved and recalled | the device's current state kept between launches without a name, counted *partial* here |
| **Whole-rig setup** | one named state for several devices at once, recalled in one gesture | a spatial scene: a setup gives each device its own effect |
| **Spatial scene** | devices placed on one shared canvas, one animation sampled at each light's position across all of them | effects synchronised in time only |
| **Input** | data an effect reads while it runs: sound, key presses, sensors, the screen, a game's state | an automation |
| **Automation** | a rule that changes which effect runs when something happens (a time, idle, an application, a signal), then gives the previous one back | an input |
| **Precedence and restoration** | which of several active rules or applications wins, and what comes back when it ends | |
| **Control path** | how frames reach the hardware: native HID, a maker's SDK, the OpenRGB SDK, Windows' LampArray API | |
| **Detected / verified** | listed by the program / confirmed by the project to light as expected on a real unit | |
| **Outbound telemetry** | data the product sends to its makers or third parties by itself | local logs and diagnostics, which stay on the computer until the user sends them |
| **Agent interface** | a local protocol through which an AI agent reads state and acts, here the Model Context Protocol (MCP) | a general REST API, listed under integrations (§6.3) |

## 1. Products and versions

| Product | Version described | Released | Pinned source | Licence | Platforms | Price | Checked on |
|---|---|---|---|---|---|---|---|
| **Candeo** | 0.22.1, and `main` at `e273c65` (presets, merged after 0.22.1) | 2026-11-05 | this repository | GPL-3.0-only | Windows 10 2004 or later (the Store package's minimum); Linux `.deb` and `.rpm`, x86_64, no device verified on Linux | free | 2026-11-05 |
| **OpenRGB** | 1.1 (SDK protocol 6, plugin API 5) | 2026-10-16 | tag `release_1.1` (`8e16de27`); `master` was at `4f2d8920` (2026-11-04) | GPL-2.0-or-later declared; see §9 | Windows, Linux, macOS (Intel and Apple Silicon) | free | 2026-11-05 |
| **SignalRGB** | 2.5.84 | 2026-10-29 | closed source: docs, changelog, privacy policy, the public plugin repository (branch `beta` at `9511979f`) | proprietary ([Terms][s-terms]) | Windows 10 1903 or later, Windows 11; Linux beta packages (contradicts the requirements page, §9) | Free, Pro subscription, Light Points purchases | 2026-11-05 |
| **Artemis** | 1.2026.1027.1 | 2026-10-27 | `master` at `8d2927a0`; Artemis.Plugins at `08595d11`; RGB.NET v3.1.0 | PolyForm Noncommercial 1.0.0 | Windows; Linux builds; macOS builds, "untested" per its FAQ | free | 2026-11-05 |
| **KeyGlow** | 1.0.1, HiddenOnLies/KeyGlow (homonyms in §9) | 2026-10-30 | `e4594013` | MIT | Windows | free | 2026-11-05 |
| **Hypercolor** | 0.9.0 | 2026-11-03 | `02f53272`; `main` was at `1bb9a58e` (2026-11-05) | Apache-2.0 | Linux (amd64, arm64), Windows x64, macOS arm64 15.2 or later | free | 2026-11-05 |
| **Windows Dynamic Lighting** | Windows 11; facts documented from 23H2 (build 22631); current release 26H2 (build 26300) | 2023-09-26 for 22H2, gradually; 23H2 on 2023-10-31 | Microsoft Learn, support pages, Windows Insider posts, HID Usage Tables 1.7 | proprietary, part of Windows | Windows 11; the LampArray API exists since Windows 10 1809, for foreground apps only | part of Windows | 2026-11-05 |

Versions from: Candeo's [changelog][c-changelog] and [PR #378][c-pr378];
OpenRGB's [releases page][o-releases] and [release notes][o-release];
SignalRGB's [changelog][s-changelog]; Artemis's update server (the version its
updater offers) and its [repository][a-repo]; KeyGlow's
[release][k-release]; Hypercolor's [release][h-release]; Windows'
[release information][w-releaseinfo], [KB5031455][w-kb5031455] and the
[Insider post for build 23475][w-23475].

## 2. Summary

Each cell is detailed, with its source, in the section named on the left.

| Feature | Candeo | OpenRGB | SignalRGB | Artemis | KeyGlow | Hypercolor | Dynamic Lighting |
|---|---|---|---|---|---|---|---|
| Control path (§3.1) | native HID | native HID, SMBus, network | own plugins: HID, SMBus, network | makers' SDKs, OpenRGB, network | OpenRGB SDK | native HID, SMBus, network, OpenRGB | HID LampArray only |
| Kernel driver or admin rights (§3.1) | none | for SMBus | yes: own driver and service | for some plugins | none of its own | for SMBus on Windows | none |
| Software effects (§4.1) | implemented | partial (plugin) | implemented, part paid | implemented (plugins) | implemented | implemented | implemented (6) |
| Effects written as code (§4.1) | implemented (TypeScript) | partial (C++, GLSL) | implemented (HTML, JS) | implemented (C#) | not found | implemented (HTML, TS, GLSL) | partial (apps) |
| Presets (§4.2) | implemented, unreleased | partial (plugin) | implemented | partial | not found | implemented | not found |
| Regions (§4.3) | implemented | implemented | not found | implemented | implemented | implemented | partial (API) |
| Compositing layers (§4.4) | planned | partial (plugin) | partial | implemented | implemented | implemented | partial (API) |
| Saved device configurations (§4.5) | partial | implemented | partial | not found | implemented | partial | partial |
| Whole-rig setups (§4.6) | planned | implemented | partial | implemented | not found | implemented | partial |
| Spatial scenes (§4.7) | not found, not planned | partial (plugin) | implemented | implemented | not found | implemented | not found |
| Sound (§5.1) | implemented, Windows | partial (plugin) | implemented | partial (plugin, Windows) | implemented, Windows | implemented | not found |
| Key presses (§5.2) | implemented, Windows | not found | partial (Pro) | partial (plugin) | implemented, Windows | implemented, opt-in | not found |
| System sensors (§5.3) | implemented | partial (plugin) | partial | partial (plugins) | planned | implemented | not found |
| Screen (§5.4) | not found | partial (plugin) | implemented | partial (plugin) | not found | implemented | not found |
| Game data (§5.5) | implemented (2 games) | not found | partial (paid) | partial (plugins) | not found | not found | not found |
| Schedules (§6.1) | implemented | partial (plugin) | not found | partial | planned | not found | not found |
| Idle, lock, sleep (§6.1) | partial (idle, Windows) | partial (sleep) | not found | partial (Windows) | planned | implemented | not found |
| Application triggers (§6.1) | planned | not found | partial (macros) | implemented, Windows | implemented | not found | partial |
| Precedence and restoration (§6.2) | implemented | not found | not found | partial | implemented | partial | implemented |
| Local API for other software (§6.3) | implemented (values only) | implemented, no auth | partial (Pro) | implemented, no auth | partial (internal) | implemented | partial (apps) |
| Home Assistant (§6.3) | partial (signals) | implemented (HA's) | partial (third-party) | not found | not found | not tested | not found |
| Agent / MCP (§6.4) | planned | not found (third-party only) | not tested | not found | not found | implemented | not found |
| Account (§7) | none | none | ambiguous; Pro needs one | optional | none | none | none specific |
| Outbound telemetry (§7) | none, per policy | not found | yes, per policy | not found | not found | not found | not documented |
| Automatic requests (§7) | update check, opt-out; none in the Store build | none found | sign-in, sync, CDN, updates | hourly updates, Workshop | 3 daily fetches, opt-out | daily update check, opt-out | — |

## 3. Devices

### 3.1 Control path and drivers

| Product | How frames reach the hardware | Prerequisites | Sources |
|---|---|---|---|
| **Candeo** | Native HID only (hidapi; `hidraw` on Linux), from a JSON device definition or a built-in family. No maker's SDK, no kernel driver, no administrator rights. LampArray is set aside until a certified device can verify it ([#330]); an OpenRGB SDK output is a proof of concept to come ([#368]). | Linux: the udev rule the packages install. | [docs][c-sdk], [docs][c-other], [docs][c-linux] |
| **OpenRGB** | Compiled C++ controllers over hidapi and libusb. SMBus/I2C for RAM and motherboards through PawnIO on Windows (WinRing0 and InpOut32 are retired, flagged by anti-cheat and antivirus tools), `i2c-dev` on Linux, macUSPCIO on macOS; NVAPI and ADL for graphics cards. Network devices in core: E1.31, DDP, Philips Hue, Nanoleaf, LIFX, Govee and others. A generic HID LampArray controller. | Windows SMBus: PawnIO installed by hand, then administrator rights or the optional service, which the MSI calls experimental. Linux: udev rules (`--generate-udev-rules` since 1.0) and i2c modules. macOS: the Input Monitoring permission. | [docs][o-smbus], [docs][o-usb], [docs][o-udev], [code][o-lamparray] |
| **SignalRGB** | Device plugins are JavaScript files typed `hid`, `rawusb`, `hybrid`, `serial`, `smbus` or `network`; SMBus plugins write registers from JavaScript. Wi-Fi devices (Hue, Nanoleaf, Govee) through their local APIs. Drives Dynamic Lighting devices when set as Windows' background controller. OpenRGB and makers' SDK bridges: not found. | The installer, run as administrator, installs SignalRGB's own driver and the `SignalRgbService` service. Onboard SMBus devices need *Enable Onboard Devices* and one run as administrator. The driver is not named in the docs; staff say it replaced WinRing0 in 2023 ([forum][s-forum-driver]). | [docs][s-devtype], [docs][s-modules], [docs][s-dl], [docs][s-install], [code][s-smbus-plugin] |
| **Artemis** | RGB.NET device providers, each an Artemis plugin. Most go through the maker's SDK, so the maker's software must be installed: Corsair iCUE, Razer Chroma (Synapse), Logitech (G HUB or LGS), ASUS Aura (Armoury Crate), MSI Mystic Light, SteelSeries GameSense. Wooting and Cooler Master through their SDK libraries alone. Also an OpenRGB SDK client, DMX over E1.31, WS281X, Novation over MIDI. Dynamic Lighting through a separate plugin and a companion WinUI app reached over gRPC. | The maker's software for each SDK provider. Artemis restarts elevated only when an enabled plugin requires it (MSI). The Dynamic Lighting plugin installs a self-signed root certificate in the machine's store. | [code][a-devices], [code][a-rgbnet], [readme][a-dl], [code][a-pluginmgmt] |
| **KeyGlow** | An OpenRGB SDK client (`openrgb-python`, 127.0.0.1:6742) in Direct mode, one colour per LED at about 30 frames a second. It starts an installed OpenRGB with its server when it finds one locally. No native lighting driver: a read-only HID probe only. | OpenRGB installed; other RGB software closed. | [code][k-openrgb], [code][k-system], [code][k-hidprobe] |
| **Hypercolor** | Native Rust drivers (ASUS, Corsair, Dygma, Lian Li, Nollie, PrismRGB, Push 2, QMK, Razer) over hidraw, hidapi, USB, serial and MIDI; SMBus for ASUS Aura only. Network drivers: WLED (DDP or E1.31), Hue Entertainment, Nanoleaf, Govee over the LAN (Govee's cloud API optional). An opt-in OpenRGB SDK bridge, which can launch a managed headless OpenRGB with the overlapping detectors disabled. LampArray: not found. | Linux: udev rules, then a systemd user service, no root. Windows SMBus: the optional PawnIO driver, through an elevated installer step. | [code][h-drivers], [code][h-smbus], [code][h-wled], [code][h-hue], [code][h-openrgb], [code][h-openrgbhost], [code][h-udev] |
| **Dynamic Lighting** | HID LampArray devices only (usage page `0x59`), in firmware or through a Virtual HID Framework driver; no maker's protocol. Apps drive lamps through `Windows.Devices.Lights`: a foreground app takes control, background ("ambient") apps in the order the user sets. | Device firmware implementing LampArray. A background app needs package identity and the `com.microsoft.windows.lighting` app extension. | [docs][w-hw], [docs][w-guide], [docs][w-support] |

### 3.2 Coverage, detection and verification

| Product | How a device is added | Per-key and zones | What it says about verification | Unknown devices | Sources |
|---|---|---|---|---|---|
| **Candeo** | A JSON definition per model (identity, lights, reports), loaded without a rebuild, checked against a schema; built-in families for what a file cannot express. | Both: a definition's lights are `keys` or `zones`, and an effect declares the kinds and capabilities it needs (`kinds`, `requires`). | Three degrees. **Built in**: 4 definitions surveyed on the device, each protocol fact with its firmware and date (DeathStalker V2 Pro; m18 R1 keyboard; m18 R1 zones; 17 R5 zones). **Unverified**: 77 definitions written from published sources, never tried (Razer 19, Alienware and Dell 17, ASUS 16, SteelSeries 15, Logitech 6, HyperX 4), shown as *Not verified*, with *Try* and *Report*. **Yours**: the user's files, unreviewed. | Lighting devices no definition describes are listed as not supported; *Report* opens a GitHub issue form filled in with what Candeo read. | [docs][c-sdk], [code][c-devices], [code][c-unverified], [data][c-sitedevices], [docs][c-protocol], [code][c-api] |
| **OpenRGB** | Compiled controllers matched on USB VID/PID or PCI subsystem IDs: 2,164 detector registrations at 1.1, 902 of them for graphics cards; the site's pipeline list has about 1,820 unique names. | Zones typed single, linear or matrix; matrix maps for per-key keyboards; segments; zones, segments and matrix maps editable by the user since 1.0. | The device list rates *Save*, *Direct* and *Effects* support per device (full, partial, problematic, not supported). No general *detected but untested* flag: untested hardware is compiled out or commented case by case. | Generic LampArray, QMK, E1.31, DDP and serial controllers; a *Get Hardware IDs* dialog for requests. | [code][o-rgbc], [code][o-supportedsh], [docs][o-devices], [data][o-csv] |
| **SignalRGB** | JavaScript plugins matched on VID/PID; a user's plugins override the built-in ones and are marked custom. | Per LED, named and placed on a grid; ARGB channels as components with a configurable LED count. | The device page lists about 1,671 entries with *New* and *Beta* badges; *Beta* is not defined there, and no "verified on hardware" marking was found. | Custom strips and generic ARGB fans; device requests from the Devices page; automatic SMBus dumps (2.5.26, 2.5.28). A Conflict Mitigator closes conflicting software (2.5.53). | [docs][s-plugin-load], [docs][s-devfuncs], [docs][s-devices], [docs][s-controllers], [docs][s-conflicts], [notes][s-changelog] |
| **Artemis** | Device providers and RGB.NET XML layouts, installed from the Workshop at first run: release builds contain no plugins. | Per key or per zone, depending on the provider. | Its README warns that the brand plugins are not thoroughly tested for lack of hardware. The Razer, Logitech and SteelSeries providers only detect devices on their built-in lists. | Generic layouts for TKL, full-size and XL keyboards (ANSI and ISO) and for small headsets. | [readme][a-readme], [code][a-ci], [code][a-wizard], [code][a-defaultlayout] |
| **KeyGlow** | Whatever OpenRGB exposes as a keyboard: the first one, or the one named in its settings. Its layout comes from an exact model file (USB id, then name), else the best of nine standard layouts fitted to OpenRGB's matrix, else the raw matrix. | Per key: OpenRGB's zones merged into one matrix. Keyboards only. | Verified on one keyboard, the HyperX Alloy Origins 65 with OpenRGB 1.1 and Windows 11; its layout says `verified: true`; the workshop's two other layouts, for the ASUS ROG Azoth, say `verified: false`. | The standard-layout fit above. | [code][k-layout], [docs][k-compat], [data][k-workshop] |
| **Hypercolor** | One TOML file per vendor generates the compatibility page: 420 devices tracked, 184 marked `supported`. Protocol support itself is Rust code; a WebAssembly plugin system is planned. | Per LED, through the spatial sampler (§4.7). | `supported` means it works in Hypercolor today; there is no separate verified-on-hardware flag, and a few notes say hardware confirmation is awaited. | The OpenRGB bridge; `hypercolor devices coverage`; an *unclaimed hardware* card that opens an issue. | [docs][h-driversreadme], [docs][h-compat], [readme][h-roadmap], [code][h-unclaimed] |
| **Dynamic Lighting** | A device appears as soon as it exposes a LampArray collection. | Per lamp: a 3D position, purposes (control, accent, branding, status, illumination, presentation), colour levels, an optional key. | Microsoft's pages list device kinds and partners; no certified-device list or LampArray certification requirement was found. The 2023 Insider posts said many launch devices were not verified by Microsoft. | None besides a firmware update or a Virtual HID Framework driver. | [docs][w-hw], [docs][w-lampinfo], [notes][w-23475] |

## 4. Lighting model

### 4.1 Effects and authoring

| Product | Effects shipped | Writing one | Preview and reload | Sources |
|---|---|---|---|---|
| **Candeo** | **implemented**: 26 TypeScript effects shipped as files, and the device's firmware effects where its definition declares them. | **implemented**: TypeScript against `@candeo/effects-api`, run by QuickJS in Rust, with the window closed too; an in-app Monaco editor with type checking. An effect declares its parameters, its inputs and the device kinds it supports. | The simulator plays the frames the same QuickJS engine computes; *Apply* sends them to the device. An effect edited in another editor is picked up with *Refresh*. | [code][c-shipped], [code][c-api], [docs][c-studio], [docs][c-runtime], [docs][c-effectspage] |
| **OpenRGB** | **partial**: core only selects hardware modes (Direct, Static and the firmware's modes). The Effects plugin, a separate download, adds 58 effects running in Direct mode. | **partial**: C++ compiled into the Effects plugin; GLSL fragment shaders in its *Shaders* effect, with an editor and ShaderToy-like inputs; GIF files. Anything else is an SDK client. | A live preview in the plugin. | [docs][o-modes], [readme][o-readme], [readme][o-effects-readme], [docs][o-shaders] |
| **SignalRGB** | **implemented**: library effects from the in-app store, some free, some Pro or bought, some from the community. The "250+" on the pricing page is not counted here. | **implemented**: *Lightscripts*, one HTML file drawing in JavaScript on a canvas (320 × 200 by default), parameters declared in `<meta>` tags. | Live preview and an effect console. A new effect file needs a restart; device plugins reload on save. | [docs][s-discover], [docs][s-meta], [docs][s-lightscripts] |
| **Artemis** | **implemented** through official plugins installed from the Workshop: layer brushes (gradients, noise, particles, ambilight, key presses, remote control) and layer effects (strobe, filters, audio). | **implemented**: C# (.NET) plugins for device providers, brushes, effects, data-model modules and nodes; visual node scripts for conditions and data bindings; a keyframe timeline. Community GLSL and JavaScript brushes exist. | The profile editor renders live. | [code][a-brushes], [code][a-effects], [code][a-plugins], [code][a-nodes] |
| **KeyGlow** | **implemented**: 22 effects, 8 styles of shortcut lights, 56 bundled profiles. | **not found**: profiles are JSON data, never code, by choice; a new effect type is Python in KeyGlow's own source. The app has a layer editor and a visual layout editor. | A live on-screen preview; hovering a profile shows it on the keyboard for a moment. | [code][k-effects], [docs][k-format] |
| **Hypercolor** | **implemented**: its catalogue says 57 (46 from the SDK, 11 native renderers), its README 58. | **implemented**: HTML pages rendered by an embedded Servo (Canvas, WebGL, GLSL); a TypeScript SDK on npm with a project generator. A compatibility layer for SignalRGB Lightscripts is **not tested**, and its own docs disagree on whether they run unchanged. | A file watcher reloads effects; the canvas is shown live in the web interface and the terminal interface. | [docs][h-catalog], [code][h-builtin], [code][h-lightscript], [docs][h-rawhtml], [code][h-watcher] |
| **Dynamic Lighting** | **implemented**: Solid colour, Breathing, Rainbow, Wave, Wheel and Gradient in Settings, with two colours, brightness, speed and directions; matching the Windows accent colour. | **partial**: through apps only, with WinRT effect classes (solid, blink, colour ramp, bitmap, custom) and playlists, in C# or C++/WinRT. Nothing in Windows itself. | — | [docs][w-unattend], [docs][w-support], [docs][w-effects], [notes][w-23486] |

### 4.2 Presets

| Product | Status | What, and on what condition | Sources |
|---|---|---|---|
| **Candeo** | implemented, unreleased | On `main`, after 0.22.1: named values and bindings for one effect, offered on every device and region. Picking one changes the settings, never which effect runs, and writes nothing to the hardware. | [docs][c-presets], [code][c-pr378] |
| **OpenRGB** | partial (plugin) | The Effects plugin's *Patterns* save and load named parameter sets per effect. Core has no software effects to keep presets for. | [code][o-patterns] |
| **SignalRGB** | implemented | Named presets per effect, optionally tied to a preferred layout, shared as a link or a file; listed and applied through the API (Pro). The pricing page lists presets under Free. | [docs][s-customize], [docs][s-pricing] |
| **Artemis** | partial | A brush's author can ship named presets (`ILayerBrushPreset`); only the Particle brush does. Named settings saved by the user for one brush: not found in core, the interface or a code search. Whole profiles export as a zip. | [code][a-preset] |
| **KeyGlow** | not found | Its *presets* are 56 whole profiles. The nearest is a profile of one layer. | [code][k-profile] |
| **Hypercolor** | implemented | Read-only presets bundled with an effect, presets saved by the user, REST endpoints to list and apply them; playlists can sequence them. | [code][h-library], [code][h-effectsapi] |
| **Dynamic Lighting** | not found | One current configuration per device or for all devices; makers can only set first-boot defaults. | [docs][w-support], [docs][w-unattend] |

### 4.3 Regions

| Product | Status | What, and on what condition | Sources |
|---|---|---|---|
| **Candeo** | implemented | A definition declares regions, each light in exactly one; *Split into regions* in *Devices*. Each region runs its own effect, settings and brightness follow; the device's thread composes one frame and keeps the device's pace. Declared today by the 17 R5 and m18 R1 zones definitions. Regions drawn by the user: later. | [docs][c-regions] |
| **OpenRGB** | implemented | Per-zone modes (SDK protocol 6) and user-defined segments in core; in the Effects plugin, each effect is assigned zones or segments. | [code][o-proto], [code][o-zone] |
| **SignalRGB** | not found | One canvas runs one effect across all devices. A forced colour per device and key painting are the workarounds. | [docs][s-dashboard] |
| **Artemis** | implemented | Every layer holds an explicit selection of LEDs: part of one device, one device, or several. Adaption hints remap a selection when a profile meets other hardware. | [code][a-layer], [code][a-hints] |
| **KeyGlow** | implemented | A layer's `keys` selector: key names, or portable groups (`@wasd`, `@arrows`, `@fn_row`…) narrowed to the keys the keyboard has. One keyboard only. | [code][k-layout] |
| **Hypercolor** | implemented | Logical devices split a controller into ranges of LEDs; zones claim segments, each with its own canvas and layer stack. | [code][h-logical], [docs][h-zones] |
| **Dynamic Lighting** | partial (API) | The API sets lamps one by one, by key or by purpose. Settings applies an effect to a whole device. | [docs][w-lamparray], [docs][w-support] |

### 4.4 Compositing layers

| Product | Status | What, and on what condition | Sources |
|---|---|---|---|
| **Candeo** | planned | The second increment of [#360]: layers naming an effect or a preset, with masks and blending, on one device or region. Not designed yet, and two design notes still say layers are not planned (§9). | [issue][#360], [docs][c-regions] |
| **OpenRGB** | partial (plugin) | The Effects plugin's *Layers* effect stacks layer groups with ten blend modes (multiply, screen, overlay, dodge, burn, mask, lighten, darken, exclusive, difference); a *Mask* effect works with it. | [code][o-layers], [code][o-blend] |
| **SignalRGB** | partial | No layer stack offered to the user was found in the docs or the changelog. Game-integration effects compose a background, event triggers and HUD elements internally (Pro or Light Points). | [docs][s-gameint] |
| **Artemis** | implemented | Layers and nested folders, blend modes, opacity, shapes, transforms, and display conditions per layer or folder (always, once, on a condition, on an event). | [code][a-general], [docs][a-conditions] |
| **KeyGlow** | implemented | Layers from bottom to top, each with an opacity and one of normal, add, multiply or screen; reactive and music effects return transparency. | [code][k-profile] |
| **Hypercolor** | implemented | An ordered layer stack per zone, eleven blend modes, a GPU compositor with a CPU fallback. | [code][h-layer], [docs][h-zones] |
| **Dynamic Lighting** | partial (API) | `ZIndex` settles precedence when two effects touch the same lamp; no blending is documented, nor which value wins. Settings has no layers. | [docs][w-zindex], [docs][w-playlist] |

### 4.5 Saved device configurations

| Product | Status | What, and on what condition | Sources |
|---|---|---|---|
| **Candeo** | partial | The applied effect, its settings and what its brightness follows are kept per device model and resumed when the device opens. No named configurations per device. Writing to the device's memory: not found in the definition schema or the design notes. | [docs][c-inputs] (§4, step 1), [code][c-schema] |
| **OpenRGB** | implemented | Profiles (JSON since 1.0; `.orp` files migrated) hold the devices and plugin state the profile editor selects. *Save To Device* writes a mode to the device's flash where the controller allows it, also over the SDK (`SAVEMODE`); the device list's *Save* column says where. | [code][o-profiles], [code][o-proto], [docs][o-devices] |
| **SignalRGB** | partial | Per-device brightness, plugin settings, colour tuning (Pro), plugin rollback. Saving lighting to the device's memory depends on each plugin; no general feature was found. | [docs][s-brightness], [notes][s-changelog] |
| **Artemis** | not found | A device's position, rotation, scale, colour calibration and layout persist, but named configurations of one device were not found (surface and profile models, wiki). A profile whose layers cover one device is the nearest. | [code][a-device] |
| **KeyGlow** | implemented | A profile (`keyglow/1` JSON) holds a name, brightness, layers and shortcut hints, and no device identity. On another keyboard it is adapted key by key, by name then position, and what was dropped is reported. Saving to keyboard memory needs a mode OpenRGB flags savable: **not tested**, impossible on the one keyboard tested. | [code][k-retarget], [code][k-onboard], [docs][k-format] |
| **Hypercolor** | partial | Per-device settings, aliases and attachment profiles (what is plugged into each port) persist; named lighting states are scenes, which cover the rig (§4.6). Onboard memory: not checked. | [code][h-devicesettings], [code][h-attachments] |
| **Dynamic Lighting** | partial | Per-device settings override the global ones. They are keyed to the device and its USB port: the same device on another port starts afresh. One configuration per device, no names. | [docs][w-support] |

### 4.6 Whole-rig setups

| Product | Status | What, and on what condition | Sources |
|---|---|---|---|
| **Candeo** | planned | [#367]: a named bundle of per-device and per-region assignments, the changes previewed before restoring, an outcome per device on a partial failure. Not a spatial scene. | [issue][#367] |
| **OpenRGB** | implemented | One profile covers the selected devices and the plugins' state (the Effects plugin's effects included since 1.x). Recalled from the window, the command line (`--profile`), the SDK, and on open, exit, suspend, resume, service start and stop. | [code][o-profiles], [code][o-cli] |
| **SignalRGB** | partial | *Layouts* keep device positions and brightness and switch at any time; a preset can name a preferred layout, recalling an effect and a layout together. One object bundling everything: not found. | [docs][s-layouts], [docs][s-customize] |
| **Artemis** | implemented | Profiles cover the whole surface, grouped in categories, with activation conditions, hotkeys and fades; several can be active at once, drawn in the sidebar's order. | [code][a-profileconfig], [docs][a-sidebar] |
| **KeyGlow** | not found | One keyboard only; mice, RAM and fans are on its roadmap. | [readme][k-readme] |
| **Hypercolor** | implemented | A scene holds zones, layers, brightness and transitions; scenes are created, snapshotted from what runs, and activated with cross-fades. 0.9.0 adds `.hypercolor-backup` export, import and apply, as one transaction over its own state. | [code][h-scenes], [docs][h-zones], [notes][h-release] |
| **Dynamic Lighting** | partial | The settings for all devices act as one rig-wide configuration. Several saved ones: not found. | [docs][w-support] |

### 4.7 Spatial scenes

| Product | Status | What, and on what condition | Sources |
|---|---|---|---|
| **Candeo** | not found; not planned | A definition's geometry is the device's own, with no place on a desk or in a room; [#360] lists the spatial scene as not planned. | [issue][#360], [docs][c-regions] |
| **OpenRGB** | partial (plugins) | The Visual Map plugin places whole devices or single LEDs on a grid and registers virtual controllers, which the Effects plugin then animates as one surface. Otherwise effects run device by device. | [code][o-visualmap], [docs][o-plugins] |
| **SignalRGB** | implemented | The layout editor places, scales and rotates devices on one canvas; the effect is sampled at each device's position. Devices may sit partly off the canvas since 2.5.72. | [docs][s-layouts] |
| **Artemis** | implemented | The surface editor places devices on a 2D canvas in millimetres (at most 4096 × 4096 mm at full render scale); layers render in surface space, so one effect flows across devices. | [docs][a-surface] |
| **KeyGlow** | not found | Spatial within one keyboard only. | [code][k-effects] |
| **Hypercolor** | implemented | Effects draw on a 640 × 480 canvas by default; devices are placed on a 2D layout as strips, matrices or rings; LED positions are computed once and the canvas sampled there (nearest, bilinear, area average, Gaussian). | [code][h-spatial], [code][h-sampling], [code][h-config] |
| **Dynamic Lighting** | not found | Wave, Wheel and Gradient are described as moving on each device. Playlists can start together, a synchronisation in time, not in space; no shared placement in Settings or the API. | [docs][w-unattend], [docs][w-playlist] |

## 5. Inputs

### 5.1 Sound

| Product | Status | What, and on what condition | Sources |
|---|---|---|---|
| **Candeo** | implemented, Windows | WASAPI loopback of what plays, never the microphone; captured only while an effect, a setting or the meter needs it, turned into a few numbers per frame. Linux: not supported yet. | [code][c-audio], [docs][c-privacy] |
| **OpenRGB** | partial (plugin) | The Effects plugin: WASAPI loopback or capture devices on Windows, OpenAL capture on Linux and macOS. | [code][o-audio] |
| **SignalRGB** | implemented | Level, density, width and 200 frequency bins of the system's sound for effects. A microphone as a source is not documented; some visualisers are Pro. | [docs][s-audio] |
| **Artemis** | partial (plugin, Windows) | The Audio plugin: WASAPI loopback, a spectrum and the volume. | [code][a-audio] |
| **KeyGlow** | implemented, Windows | WASAPI loopback into 16 bands, only while a music layer is used. | [code][k-audio] |
| **Hypercolor** | implemented | Spectrum, beats, mel bands, chromagram. Native on Linux; Windows and macOS need a loopback device. | [code][h-audio], [readme][h-readme] |
| **Dynamic Lighting** | not found | Not in Windows itself; through apps only. | [docs][w-guide] |

### 5.2 Key presses

| Product | Status | What, and on what condition | Sources |
|---|---|---|---|
| **Candeo** | implemented, Windows | Raw Input, only while an effect declares `keys`; scancodes and instants, never characters, never logged. The capture is Windows only. | [code][c-presses], [docs][c-keys] |
| **OpenRGB** | not found | Only firmware reactive modes; no key capture in core or the official plugins. | [code][o-effects] |
| **SignalRGB** | partial (Pro) | *Keytap* effects, relying on standard key names in the device plugin. | [docs][s-pro] |
| **Artemis** | partial (plugin) | The Input plugin: keys, modifiers, mouse, time since the last input. Raw Input on Windows; evdev on Linux, the user in the `input` group; macOS: not found. | [code][a-input] |
| **KeyGlow** | implemented, Windows | A low-level keyboard hook while a reactive layer is used; modifiers polled for shortcut lights. | [code][k-keyhook], [code][k-engine] |
| **Hypercolor** | implemented, opt-in | Off by default, behind consent; evdev, Raw Input or a macOS event tap. | [readme][h-readme] |
| **Dynamic Lighting** | not found | Not natively; the API maps lamps to virtual keys for apps. | [docs][w-lamparray] |

### 5.3 System sensors

| Product | Status | What, and on what condition | Sources |
|---|---|---|---|
| **Candeo** | implemented | CPU, memory and GPU load, GPU and CPU temperature, CPU power; read without a driver or administrator rights, on Windows and Linux; a source the computer cannot provide is `null`. | [docs][c-inputs] (§2.4), [code][c-system] |
| **OpenRGB** | partial (plugin) | The Hardware Sync plugin: LibreHardwareMonitor on Windows, libsensors on Linux. | [code][o-hwsync] |
| **SignalRGB** | partial | Monitoring, fan curves and LCD faces; effects driven by sensors were not found in the Lightscripts docs. | [docs][s-monitoring] |
| **Artemis** | partial (plugins) | A Performance module on Windows, and plugins reading HWiNFO, Open or Libre Hardware Monitor, AIDA64, each needing that tool. | [code][a-hwmon] |
| **KeyGlow** | planned | CPU indicators are on its roadmap. | [readme][k-readme] |
| **Hypercolor** | implemented | CPU, GPU and memory; temperatures on Windows through WMI and LibreHardwareMonitor. | [code][h-sensors] |
| **Dynamic Lighting** | not found | — | [docs][w-guide] |

### 5.4 Screen

| Product | Status | What, and on what condition | Sources |
|---|---|---|---|
| **Candeo** | not found | "Later, if asked": the costliest input and the most sensitive. | [docs][c-inputs] (§2.5) |
| **OpenRGB** | partial (plugin) | The Effects plugin's *Ambient*: Windows capture, Qt, PipeWire on Wayland. | [code][o-screen] |
| **SignalRGB** | implemented | *Screen Ambience*, free and processed locally; a monitor or a capture card. | [docs][s-screen] |
| **Artemis** | partial (plugin) | The Ambilight brush, Windows and Linux per its manifest. | [code][a-brushes] |
| **KeyGlow** | not found | The image effect reads a still file. | [code][k-effects] |
| **Hypercolor** | implemented | Desktop Duplication on Windows, on by default; the Wayland portal and ScreenCaptureKit, opt-in. | [readme][h-readme] |
| **Dynamic Lighting** | not found | Microsoft's AutoRGB sample app derives a colour from the screen. | [code][w-autorgb] |

### 5.5 Game data

| Product | Status | What, and on what condition | Sources |
|---|---|---|---|
| **Candeo** | implemented | Counter-Strike 2 and Dota 2 through Valve's game state integration: the game posts its state to Candeo on the computer, turned into signals; connected from a *Games* tab. | [code][c-gsi-code], [docs][c-gsi] |
| **OpenRGB** | not found | Neither core nor the official plugins; other programs drive it over the SDK. | [readme][o-readme] |
| **SignalRGB** | partial (paid) | 62 listed integrations, with Pro or a Light Points purchase per game; read from screen pixels, or posted over HTTP by game developers. | [docs][s-gameint], [docs][s-integrations], [docs][s-devint] |
| **Artemis** | partial (plugins) | Official game plugins (League of Legends, Counter-Strike 2, Elite Dangerous, TrackMania, Euro Truck and American Truck Simulator), community ones, and wrappers re-routing Razer Chroma, Logitech and LightFX game output. | [code][a-games], [code][a-wrappers] |
| **KeyGlow** | not found | It detects the game in front (43 known) to pick a profile, nothing more. | [code][k-games] |
| **Hypercolor** | not found | `GameDetected` exists only as an unused trigger type (§6.1). | [code][h-automation] |
| **Dynamic Lighting** | not found | Not as an input; games can drive LampArray devices themselves (GDK lighting API). | [docs][w-gdk] |

## 6. Automations and integrations

### 6.1 Triggers

| Product | Schedules | Idle, lock, sleep | Applications | Sources |
|---|---|---|---|---|
| **Candeo** | **implemented**: cron rules on the local clock, each with a duration. | **partial**: idle on Windows; Linux waits ([#183]); lock and sleep not found. | **planned**: an `app` trigger is listed as later; no issue. | [code][c-resolver], [docs][c-inputs] (§3) |
| **OpenRGB** | **partial (plugin)**: the Scheduler plugin's cron, whose only action in the code is loading a profile (its readme also names turning off). | **partial**: profiles on suspend and resume, in core. Idle and lock: not found. | **not found** | [code][o-scheduler], [code][o-suspend], [code][o-profiles] |
| **SignalRGB** | **not found**: playlists switch effects on a timer. | **not found** | **partial**: macros, *Application Started* then *Activate Effect*; two macros free, unlimited with Pro. | [docs][s-macros], [docs][s-pricing] |
| **Artemis** | **partial**: time is a condition input; no scheduler. | **partial**: idle time, power and session events as conditions, on Windows. | **implemented**: process and focused-window conditions, on Windows. | [code][a-profileservice], [docs][a-conditions] |
| **KeyGlow** | **planned** | **planned** | **implemented**: the executable in front picks a profile, checked every second, on Windows. | [readme][k-readme], [code][k-games] |
| **Hypercolor** | **not found**: a rule engine with time-of-day, sunrise, application and audio triggers exists in `hypercolor-core` but is not wired into the daemon, whose MCP prompt says Hypercolor does not schedule scenes itself. The README claims automation rules. | **implemented**: dim or off on idle, screen lock, suspend, lid close; on wake, the previous output or a scene. | **not found**: `AppLaunched` is unwired. | [code][h-automation], [code][h-prompts], [code][h-session], [readme][h-readme] |
| **Dynamic Lighting** | **not found** | **not found** | **partial**: a foreground app using the API takes control (§6.2). | [docs][w-support] |

### 6.2 Precedence and restoration

| Product | Status | What, and on what condition | Sources |
|---|---|---|---|
| **Candeo** | implemented | One effect per device at a time. The highest active rule in the list wins; an occurrence hidden by a higher rule goes by unseen. Applying an effect by hand or *Resume* ends every run under way on the device. When a rule ends, the applied effect comes back; a firmware effect is read back first, and *Off* replaces one that cannot be. *Pause automations* suspends them all. | [docs][c-inputs] (§3.1–3.6) |
| **OpenRGB** | not found | What wins between the scheduler, the HTTP hook, suspend profiles and a manual change is not documented. | [code][o-profiles] |
| **SignalRGB** | not found | No documented precedence or restoration. | [docs][s-macros] |
| **Artemis** | partial | Active profiles stack in the sidebar's order and stop drawing when deactivated, revealing what lies below. An `ActivationBehaviour` setting (disable the others) is stored but no code enforcing it was found: **not tested**. | [code][a-profileservice], [code][a-profileconfig] |
| **KeyGlow** | implemented | Hover preview over game profile over base profile, with shortcut and Caps Lock lights drawn on top; the base profile returns when the game loses focus; on quit the keyboard keeps the last frame. | [code][k-engine], [code][k-games] |
| **Hypercolor** | partial | The session policy restores the previous output, or a scene, on wake. How scene priorities and session overrides combine: not verified. | [code][h-session], [code][h-sessiontypes] |
| **Dynamic Lighting** | implemented | A foreground app wins, unless the user turns that off; background apps in the user's order; control returns when the foreground app is minimised or closed; a device not selected goes back to its firmware. | [docs][w-support], [docs][w-guide] |

### 6.3 Local APIs and integrations

| Product | API for other software | Home Assistant, MQTT | Command line | Sources |
|---|---|---|---|---|
| **Candeo** | **implemented**: *signals*, `POST /signals` on 127.0.0.1:7317, off by default, a bearer token, requests from web pages refused, other network interfaces only when ticked. It carries flat named values, never a device, an effect or a colour: rules and bindings decide what they light. | **partial**: Home Assistant sends signals (a documented example); MQTT not found. | not found | [docs][c-signals], [code][c-http], [docs][c-privacy] |
| **OpenRGB** | **implemented**: the SDK server, binary TCP protocol 6 on 127.0.0.1:6742, off until started. No authentication or TLS: any connected client can drive LEDs, load profiles or write a mode to flash; clients on loopback may also ask for settings and logs. The HTTP Hook plugin loads a profile over HTTP, on 0.0.0.0:8080 by default, with no authentication found. | **implemented**: Home Assistant's own integration since 2025.11 (whole devices and a profile select). MQTT: third-party only. | implemented | [code][o-proto], [code][o-server], [docs][o-sdkdoc], [code][o-httphook], [docs][o-ha] |
| **SignalRGB** | **partial (Pro)**: a REST API at 127.0.0.1:16038 (effects, presets, brightness, layouts), which may change without notice; `signalrgb://` links; HTTP events to effects on port 16034. A Mobile Remote (Pro, 2.5.80) goes through `remote.signalrgb.com`; whether control stays on the local network is not documented. | **partial**: a third-party integration only. | partial: the launcher takes `--url` | [docs][s-api], [docs][s-urls], [docs][s-devint], [notes][s-changelog], [code][s-ha3p] |
| **Artemis** | **implemented**: a web server on port 9696, loopback unless *Remote access* is on, no authentication. The Web API plugin lists profiles, reads the data model and takes JSON data; a Remote Control brush sets LEDs over HTTP. | **partial**: an official MQTT plugin that only subscribes, feeding the data model. Home Assistant: not found. | partial: startup options only | [code][a-webserver], [code][a-webapi], [docs][a-restdoc], [code][a-mqtt] |
| **KeyGlow** | **partial**: the app's own JSON API on 127.0.0.1:47821, serving its interface, without authentication but checking for a local Host header; not documented as public. | **not found** | implemented | [code][k-server], [code][k-cli] |
| **Hypercolor** | **implemented**: REST and WebSocket on 127.0.0.1:9420, with an OpenAPI description. Keyless on loopback, with Origin and Host checks; keys and an allowlist off the computer, and the daemon refuses to listen beyond loopback without a key. | **not tested**: a separate Home Assistant integration by the same author, not inspected. MQTT appears in design documents only. | implemented, and a terminal interface | [code][h-config], [code][h-security], [docs][h-security-doc], [code][h-hass] |
| **Dynamic Lighting** | **partial**: WinRT APIs for apps on the same computer; no network API. | **not found** | not found | [docs][w-lights] |

### 6.4 Agent and MCP interfaces

| Product | Status | What, and on what condition | Sources |
|---|---|---|---|
| **Candeo** | planned | [#359]. The proposal keeps generating and previewing apart from any hardware write, opt-in, with no network service turned on implicitly. Headless TypeScript validation ([#364]) and an end-to-end authoring proof of concept ([#365]) come first. | [docs][c-mcp] |
| **OpenRGB** | not found (official) | Two small third-party MCP servers over its SDK exist, not reviewed here. Its contributing guide generally refuses AI-generated submissions. | [docs][o-contrib], [code][o-mcp3p-a], [code][o-mcp3p-b] |
| **SignalRGB** | not tested | The 2.5.80 changelog says an MCP server is served at `localhost:16037/mcp` and can be registered with Claude Code and Claude Desktop. No documentation page, tool list or tier was found. | [notes][s-changelog] |
| **Artemis** | not found | Its organisation's repositories, the Workshop, the web. | [code][a-repo] |
| **KeyGlow** | not found | Its whole repository. | [code][k-repo] |
| **Hypercolor** | implemented | Streamable HTTP at `/mcp` on the daemon, off by default; no stdio. 21 tools: 11 read-only (status, devices, effects, layout, sensors, diagnosis, `capture_canvas`…), 10 that change state, 6 of them marked destructive (`set_effect`, `activate_scene`, `install_effect`…); 5 resources and 4 prompts. No confirmation on the server's side: tools carry MCP annotations and the client decides; each change is written to an audit log, without its arguments. `install_effect` takes a whole HTML page up to 1 MiB, checks it and hot-swaps a running one: authoring without a compile step. The same security layer as the REST API. | [code][h-mcp], [code][h-tools], [code][h-authoring], [code][h-mcpconfig], [code][h-audit], [docs][h-mcpsetup], [docs][h-toolsref] |
| **Dynamic Lighting** | not found | Windows' own MCP connectors and Settings agent document nothing about lighting; whether they reach the Dynamic Lighting page was not tested. | [docs][w-mcp] |

## 7. Network, telemetry, account

| Product | Account | Outbound telemetry | Automatic requests | Local diagnostics | Sources |
|---|---|---|---|---|---|
| **Candeo** | none | none, per its privacy policy | an update check to `api.github.com` once per launch, on by default outside the Store build, which never makes it; a setting turns it off | a daily log, seven kept; *Copy the diagnostic*, for the user to send; serial numbers never written, a fingerprint instead | [docs][c-privacy], [code][c-storage] |
| **OpenRGB** | none | not found: core builds without Qt's network module, and no telemetry, crash-reporting or update code was found | none found beyond the devices the user configures and discovery on the local network | logs in its configuration folder, ten kept | [code][o-pro], [code][o-resources] |
| **SignalRGB** | ambiguous: the requirements page says the app needs a sign-in, other pages describe a guest mode; Pro needs an account | yes, per its privacy policy: hardware and system details, IP address and approximate location, device identifiers, crash and performance data, usage analytics; advertising for non-subscribers. An in-app opt-out: not found | sign-in, cloud sync of effects, preferences and playlists, components from a CDN (2.5.84), an update check at every launch | logs, verbose mode, crash dumps, kept on the computer until sent | [docs][s-privacy], [docs][s-requirements], [docs][s-update], [docs][s-logs], [notes][s-changelog] |
| **Artemis** | optional: only to upload to the Workshop | not found: no analytics package in the build; a 2021 wiki draft says telemetry is not included | an hourly update check (version, channel, platform) with automatic install, both on by default; Workshop requests, including a layout search sending the device's type, vendor and model; plugin prerequisites may download installers | daily logs, fourteen kept; a debugger window | [code][a-update], [code][a-workshop], [code][a-packages], [docs][a-telemetry], [docs][a-workshopdoc] |
| **KeyGlow** | none; a GitHub account to share a profile, through the browser | not found; every outbound call in the code was read | three fetches from GitHub a day at most (releases, gallery index, layouts index), each with its own switch, sending only a User-Agent | none: no log file | [code][k-update], [code][k-gallery], [code][k-layouts] |
| **Hypercolor** | none | not found: no analytics or crash-reporting crate in the lockfile | a daily release check on GitHub, on by default, with a switch; Hue cloud discovery when no bridge answers locally; Govee's cloud API with the user's key | an audit log of changes; `diagnose` | [code][h-updates], [code][h-huescanner], [code][h-govee], [code][h-audit] |
| **Dynamic Lighting** | none of its own | not documented for Dynamic Lighting; Windows' diagnostic data applies in general, and which events Dynamic Lighting emits is not known | — | Feedback Hub; no export | [docs][w-support] |

## 8. First run and diagnostics

| Product | Steps before the first light | Diagnostics | Sources |
|---|---|---|---|
| **Candeo** | Install (Store, its signed package, winget, or `.deb`/`.rpm`); *Control* on the device in *Devices*; pick an effect and *Apply*. No driver, no administrator rights; the udev rule comes with the Linux packages. | The log; *Copy the diagnostic*; *Report* for a device; definitions that fail to load listed with the reason. | [readme][c-readme], [docs][c-install], [docs][c-sdk] |
| **OpenRGB** | Install. For RAM or motherboard SMBus on Windows: PawnIO, then administrator rights or the service, and remove WinRing0 or InpOut32 left by older versions. udev rules on Linux; Input Monitoring on macOS. For software effects: the Effects plugin. | A log console and levels; an SMBus page with register dumps, marked risky; a hardware-IDs dialog; fake debug devices; a Wireshark dissector for the SDK. | [docs][o-smbus], [docs][o-udev], [code][o-cli] |
| **SignalRGB** | Run the installer as administrator, approve the driver and the service; sign in or continue as a guest; close the makers' software (or let the Conflict Mitigator do it); turn Dynamic Lighting off or put SignalRGB first; set up controller channels and a layout. | Logs and crash dumps; a device information page; plugin and effect consoles; plugin rollback. | [docs][s-firstrun], [docs][s-install], [docs][s-logs] |
| **Artemis** | A first-run wizard installs the essential plugins, the chosen device providers and layouts from the Workshop (so a first run needs the Workshop reachable) and arranges the surface. The makers' software for SDK providers. The Windows build is unsigned. | A debugger window (render, data model, performance, logs); startup options for logging and software rendering. | [code][a-wizard], [code][a-debugger], [docs][a-faq] |
| **KeyGlow** | Install OpenRGB and start its SDK server (KeyGlow can start it), close other RGB software, run the unsigned `KeyGlow.exe` past SmartScreen. | An in-app guide when no keyboard is found, the devices OpenRGB sees, conflicting programs detected by name, a report of keys dropped when a profile adapts; `keyglow probe`. | [code][k-conflicts], [code][k-system] |
| **Hypercolor** | Install the package; udev rules on Linux; the elevated step for PawnIO on Windows if SMBus is wanted (the Windows installer is unsigned). The first run seeds a default scene. | `hypercolor diagnose` and its MCP twin; conflicting-software detection (SignalRGB, iCUE, L-Connect…); `hypercolor devices coverage`. | [code][h-firstrun], [code][h-diagnostics], [code][h-conflicts], [docs][h-firstlaunch] |
| **Dynamic Lighting** | Settings › Personalization › Dynamic Lighting, on Windows 11, with current device firmware; *Use Dynamic Lighting on my devices* on. | Feedback Hub (Devices and Drivers › Lighting); a placeholder when no device is found (since November 2024). | [docs][w-support], [notes][w-kb5046740] |

## 9. Notes per product

What the tables could not say in a cell: ambiguities in the sources, and what
could not be verified.

**Candeo**

- The column describes `main` at `e273c65`: presets were merged after 0.22.1
  and are not released yet.
- Three design notes have status lines older than the code:
  [`key-input.md`](../design/key-input.md) and
  [`game-state-integration.md`](../design/game-state-integration.md) say
  *proposed*, and [`inputs-and-automations.md`](../design/inputs-and-automations.md)
  says sound is proposed. Key presses, games and sound ship ([code][c-presses],
  [code][c-gsi-code], [code][c-audio]); the tables follow the code.
- Composition: [#360] plans layers as its second increment, while
  [`regions.md`](../design/regions.md) §10 and
  [`inputs-and-automations.md`](../design/inputs-and-automations.md) §5 still
  say layers and masks are not planned. To settle in #360's design.
- Linux: the build, the packages and their udev rule are checked in CI; no
  device has been tried on Linux ([#230]). Sound, key presses and idle are
  Windows only.
- Running the maker's software alongside (Synapse, Alienware Command Center) is
  not documented.

**OpenRGB**

- Licence: `LICENSE` holds the GPLv2 text and the AppStream metadata declares
  GPL-2.0-or-later, but 18 source files are GPL-2.0-only, the LampArray
  controller among them ([code][o-meta]). The project's own statements differ.
- Two hosts: GitLab for the code and issues, Codeberg for release notes and
  downloads.
- On 2026-11-05 the device list for 1.1 on openrgb.org held rows with empty
  names; counts here come from the code and the pipeline list.
- The plugins were read at their `release_1.0` tags (Visual Map, Scheduler,
  HTTP Hook, Hardware Sync, E1.31 receiver); their compatibility with 1.1 was
  not checked. Visual Map's newest GitLab release is 1.0rc2 although the tag
  exists.
- The wiki's FAQ could not be read (GitLab refused further requests), so its
  advice about the makers' software running alongside is not reflected.

**SignalRGB**

- Closed source: everything comes from the documentation, the changelog, the
  privacy policy, the terms and the public plugin repository. That repository's
  default branch was last updated on 2023-08-30 and its `beta` branch on
  2026-01-26: it likely lags what ships.
- The requirements page says Windows only and a sign-in required; the Linux
  page offers beta packages, and other pages a guest mode.
- Prices load in the browser and were not read; the *Beta* badge on the device
  page is not defined.

**Artemis**

- Releases come from Artemis's update server, not from GitHub, whose one
  release (`prerelease`) dates from 2023.
- Many wiki pages date from 2021 to 2023 and contradict the current code
  (bundled plugins, a scripting provider); the tables follow the code.
- Whether the Razer and SteelSeries providers need the maker's app running, or
  only installed, was not confirmed.
- macOS: builds exist and the FAQ says they are untested; a maintainer wrote
  in 2023 that devices realistically work only through OpenRGB there
  ([issue][a-mac]).

**KeyGlow**

- The name is shared. This page describes HiddenOnLies/KeyGlow, the only one
  found that drives keyboards across brands. The others:
  [beylerburak/keyglow][k-homonym-msi] (a GNOME app for MSI laptops with a
  SteelSeries KLC controller, GPL-3.0, 0.1.0 of 2026-10-13),
  [amansingh909/keyglow][k-homonym-razer] (layered lighting for Razer keyboards
  on Linux through OpenRazer, MIT, 0.3.0 of 2026-10-26),
  [kach4n/keyglow][k-homonym-hyperx] (one HyperX keyboard, MIT, 1.1.0 of
  2026-10-15), and an unrelated `keyglow` package on PyPI, a keyboard-usage
  heatmap. A docstring in HiddenOnLies/KeyGlow suggests
  `pip install keyglow[openrgb]`, which would install that unrelated package.
- A young project: its repository was created on 2026-10-21.

**Hypercolor**

- Young and fast-moving: created on 2026-03-03, before 1.0, about one release a
  week.
- Effect counts differ between the README (58), the catalogue (57) and the
  folders.
- The README's automation rules against an engine the daemon does not use
  (§6.1).
- Not verified: whether `capture_canvas`, keyless on loopback, returns screen
  content while a screen effect runs; whether effects running in Servo can
  reach the network; how a read-only API key fares with read-only MCP tools,
  since access tiers follow the HTTP method and every MCP call is a POST.

**Windows Dynamic Lighting**

- Background control's first build: the developer guide says 23466, the
  Insider posts introduce it in 23475.
- `LampArray.IsAvailable` still carries an experimental attribute in some
  projections, and the `LampArray` class remarks still say lamp state applies
  only while the app has focus, which predates background control.
- Raw HID: the UWP HID API refuses to open the LampArray collection
  ([docs][w-hidapi]); the collection is not among those Windows opens for its
  own use ([docs][w-tlc]). Whether a desktop app can write it over raw HID while
  Windows drives it, and what happens when both write, is not documented.
- The speed control in Settings appears only in a screenshot of the support
  page.

## 10. What this suggests for Candeo

Observations drawn from the tables, each with the issue it concerns.

- **Composition ([#360]).** Layers over the same lights are implemented in
  Artemis, Hypercolor and KeyGlow, and in OpenRGB's Effects plugin. Each pairs a
  layer with a selection of lights; KeyGlow's named key groups and Artemis's
  adaption hints keep that selection meaningful on another keyboard. Candeo's
  notes disagree on whether layers are planned (§9).
- **Saved setups ([#367]).** OpenRGB, Artemis and Hypercolor save the whole rig.
  An OpenRGB profile chooses which devices it holds; Hypercolor snapshots what
  runs. Hypercolor's backup is applied as one transaction over its own state;
  no source claims an atomic write across devices, which #367 already refuses
  to promise.
- **Spatial scenes.** SignalRGB, Artemis and Hypercolor are built around a
  shared canvas, and OpenRGB adds one with a plugin. Candeo's definitions carry
  each device's own geometry only, and #360 keeps the spatial scene out.
- **OpenRGB output ([#368]).** Protocol 6 gives stable controller ids, per-LED
  Direct mode (`SETCUSTOMMODE`, then `UPDATELEDS`), zones, segments and matrix
  maps. The server authenticates nobody: any local process can also load
  profiles or write a mode to flash (`SAVEMODE`), and Home Assistant's
  documentation asks users to listen on every interface. Its device data
  carries `serial` and `location`, which Candeo's privacy rule keeps out of
  logs. KeyGlow, Artemis and Hypercolor already drive OpenRGB as clients;
  Hypercolor runs its own headless OpenRGB with the overlapping detectors
  turned off, so that one device does not get two writers.
- **Agent interface ([#359], [#364], [#365]).** Hypercolor ships the one
  documented MCP server here: opt-in, Streamable HTTP only, tools annotated
  read-only or destructive, confirmation left to the client, an audit log, and
  authoring by installing a whole HTML effect. Candeo's proposal differs on
  two points: an explicit authorisation before any hardware write, and
  TypeScript compiled and checked without a window ([#364]), where Hypercolor
  takes HTML as it is. SignalRGB announces an MCP endpoint without documenting
  it.
- **LampArray ([#330]).** OpenRGB has a generic LampArray controller and turns
  it off where its Razer driver knows the device, the situation met on the
  DeathStalker V2 Pro. SignalRGB and Artemis drive Dynamic Lighting devices
  through Windows' API as background controllers: that needs package identity,
  which Candeo's Store package has and the MSI and NSIS builds do not, and
  gives way to foreground apps. Windows documents no arbitration for an app
  writing LampArray over raw HID, the path #330 plans.
- **Inputs and triggers.** Others have what Candeo's column lacks: screen
  capture (SignalRGB, Hypercolor; OpenRGB and Artemis by plugin), application
  triggers (Artemis, KeyGlow, SignalRGB's macros), lock and sleep (Hypercolor;
  OpenRGB's suspend profiles), and on Linux, sound (Hypercolor, OpenRGB's
  plugin) and key presses (Hypercolor, Artemis). Candeo's idle trigger on Linux
  is [#183].
- **Verification.** OpenRGB rates each device's Save, Direct and Effects
  support, KeyGlow marks its layouts verified, and Candeo separates surveyed
  definitions from unverified ones. In SignalRGB, Artemis and Hypercolor no
  marking separating devices verified on hardware from devices listed was
  found.
- **Privacy.** Outbound telemetry is stated in SignalRGB's privacy policy, and
  stated absent in Candeo's; none was found in the code read for OpenRGB,
  Artemis, KeyGlow and Hypercolor, and Windows documents none specific to
  Dynamic Lighting.
  An automatic update check is on by default in Candeo outside the Store build,
  in Artemis (hourly, with automatic install), KeyGlow and Hypercolor.

## 11. Refreshing this comparison

One product at a time, in a docs-only pull request:

1. **Find the newest version.** OpenRGB: its [releases page][o-releases] and
   `https://gitlab.com/api/v4/projects/CalcProgrammer1%2FOpenRGB/releases`.
   SignalRGB: its [changelog][s-changelog]. Artemis: the version its updater
   offers, and `gh api repos/Artemis-RGB/Artemis/commits/master`. KeyGlow and
   Hypercolor: `gh api repos/<owner>/<repo>/releases/latest`. Windows: the
   [release information][w-releaseinfo], then the KB articles and Insider posts
   published since the last check that mention Dynamic Lighting. Candeo: its
   latest release and `main`.
2. **Read what changed since the pinned version**: the release notes in
   between and, for open source, the diff between the pinned commit and the new
   one, limited to the paths the rows cite:
   `gh api repos/<owner>/<repo>/compare/<old>...<new>`, or for GitLab
   `repository/compare?from=<old>&to=<new>`.
3. **Re-check each row whose cited files or pages changed**, then re-pin its
   links to the new commit. Rows whose sources did not change keep their
   status; re-pin them only after opening the link.
4. **Move a status only on evidence.** A status read in code does not change on
   documentation alone; a row whose evidence weakens says why in §9. For
   Candeo, *planned* becomes *implemented* when the release that ships it is
   out, named with its version.
5. **Update the product's line in §1**: version, release date, pinned commit,
   *checked on*. Add new ambiguities to §9, remove those resolved, and revisit
   §2 and §10 where a status changed.

<!-- Issues -->

[#183]: https://github.com/o2csi/candeo/issues/183
[#230]: https://github.com/o2csi/candeo/issues/230
[#330]: https://github.com/o2csi/candeo/issues/330
[#359]: https://github.com/o2csi/candeo/issues/359
[#360]: https://github.com/o2csi/candeo/issues/360
[#364]: https://github.com/o2csi/candeo/issues/364
[#365]: https://github.com/o2csi/candeo/issues/365
[#367]: https://github.com/o2csi/candeo/issues/367
[#368]: https://github.com/o2csi/candeo/issues/368
[#369]: https://github.com/o2csi/candeo/issues/369

<!-- Candeo, this repository at e273c65 -->

[c-readme]: ../../README.md
[c-privacy]: ../../PRIVACY.md
[c-changelog]: ../../CHANGELOG.md
[c-pr378]: https://github.com/o2csi/candeo/pull/378
[c-sdk]: ../design/device-sdk.md
[c-other]: ../design/other-keyboards.md
[c-regions]: ../design/regions.md
[c-presets]: ../design/presets.md
[c-inputs]: ../design/inputs-and-automations.md
[c-keys]: ../design/key-input.md
[c-gsi]: ../design/game-state-integration.md
[c-mcp]: ../design/mcp-architecture.md
[c-studio]: ../design/studio.md
[c-runtime]: ../design/effects-runtime.md
[c-linux]: ../linux.md
[c-signals]: ../api/signals.md
[c-protocol]: ../protocol/
[c-devices]: ../../crates/candeo-device/devices/
[c-unverified]: ../../crates/candeo-device/devices/unverified/
[c-schema]: ../../crates/candeo-device/devices/device-definition.schema.json
[c-sitedevices]: ../../site/devices.json
[c-shipped]: ../../apps/desktop/src-tauri/src/shipped.rs
[c-api]: ../../packages/effects-api/src/index.ts
[c-resolver]: ../../apps/desktop/src-tauri/src/automations/resolver.rs
[c-audio]: ../../apps/desktop/src-tauri/src/audio/mod.rs
[c-presses]: ../../apps/desktop/src-tauri/src/runtime/presses.rs
[c-system]: ../../apps/desktop/src-tauri/src/system/
[c-gsi-code]: ../../apps/desktop/src-tauri/src/signals/gsi.rs
[c-http]: ../../apps/desktop/src-tauri/src/signals/http.rs
[c-storage]: ../../apps/desktop/src-tauri/src/storage.rs
[c-install]: https://o2csi.github.io/candeo/install.html
[c-effectspage]: https://o2csi.github.io/candeo/effects.html

<!-- OpenRGB 1.1 (tag release_1.1, 8e16de27), Effects plugin release_1.1, other plugins release_1.0 -->

[o-releases]: https://openrgb.org/releases.html
[o-release]: https://codeberg.org/OpenRGB/OpenRGB/releases/tag/release_1.1
[o-readme]: https://gitlab.com/CalcProgrammer1/OpenRGB/-/blob/release_1.1/README.md
[o-smbus]: https://gitlab.com/CalcProgrammer1/OpenRGB/-/blob/release_1.1/Documentation/SMBusAccess.md
[o-usb]: https://gitlab.com/CalcProgrammer1/OpenRGB/-/blob/release_1.1/Documentation/USBAccess.md
[o-udev]: https://gitlab.com/CalcProgrammer1/OpenRGB/-/blob/release_1.1/Documentation/UdevRules.md
[o-sdkdoc]: https://gitlab.com/CalcProgrammer1/OpenRGB/-/blob/release_1.1/Documentation/OpenRGBSDK.md
[o-modes]: https://gitlab.com/CalcProgrammer1/OpenRGB/-/blob/release_1.1/Documentation/Common-Modes.md
[o-proto]: https://gitlab.com/CalcProgrammer1/OpenRGB/-/blob/release_1.1/NetworkProtocol.h
[o-server]: https://gitlab.com/CalcProgrammer1/OpenRGB/-/blob/release_1.1/NetworkServer.cpp
[o-profiles]: https://gitlab.com/CalcProgrammer1/OpenRGB/-/blob/release_1.1/ProfileManager.cpp
[o-cli]: https://gitlab.com/CalcProgrammer1/OpenRGB/-/blob/release_1.1/cli.cpp
[o-suspend]: https://gitlab.com/CalcProgrammer1/OpenRGB/-/tree/release_1.1/SuspendResume
[o-rgbc]: https://gitlab.com/CalcProgrammer1/OpenRGB/-/blob/release_1.1/RGBController/RGBControllerInterface.h
[o-lamparray]: https://gitlab.com/CalcProgrammer1/OpenRGB/-/tree/release_1.1/Controllers/HIDLampArrayController
[o-resources]: https://gitlab.com/CalcProgrammer1/OpenRGB/-/blob/release_1.1/ResourceManager.cpp
[o-pro]: https://gitlab.com/CalcProgrammer1/OpenRGB/-/blob/release_1.1/OpenRGB.pro
[o-contrib]: https://gitlab.com/CalcProgrammer1/OpenRGB/-/blob/release_1.1/CONTRIBUTING.md
[o-meta]: https://gitlab.com/CalcProgrammer1/OpenRGB/-/blob/release_1.1/qt/org.openrgb.OpenRGB.metainfo.xml
[o-supportedsh]: https://gitlab.com/CalcProgrammer1/OpenRGB/-/blob/release_1.1/scripts/build-supported-devices-md.sh
[o-devices]: https://openrgb.org/devices_1.1.html
[o-csv]: https://openrgb.org/data/supported_devices_pipeline.csv
[o-plugins]: https://openrgb.org/plugins.html
[o-effects]: https://gitlab.com/OpenRGBDevelopers/OpenRGBEffectsPlugin/-/tree/release_1.1
[o-effects-readme]: https://gitlab.com/OpenRGBDevelopers/OpenRGBEffectsPlugin/-/blob/release_1.1/README.md
[o-shaders]: https://gitlab.com/OpenRGBDevelopers/OpenRGBEffectsPlugin/-/blob/release_1.1/Effects/Shaders/README.md
[o-layers]: https://gitlab.com/OpenRGBDevelopers/OpenRGBEffectsPlugin/-/tree/release_1.1/Effects/Layers
[o-blend]: https://gitlab.com/OpenRGBDevelopers/OpenRGBEffectsPlugin/-/blob/release_1.1/ColorUtils.h
[o-patterns]: https://gitlab.com/OpenRGBDevelopers/OpenRGBEffectsPlugin/-/blob/release_1.1/OpenRGBEffectPage.cpp
[o-zone]: https://gitlab.com/OpenRGBDevelopers/OpenRGBEffectsPlugin/-/blob/release_1.1/ControllerZone.h
[o-audio]: https://gitlab.com/OpenRGBDevelopers/OpenRGBEffectsPlugin/-/blob/release_1.1/Audio/AudioManager.cpp
[o-screen]: https://gitlab.com/OpenRGBDevelopers/OpenRGBEffectsPlugin/-/tree/release_1.1/ScreenCapturer
[o-visualmap]: https://gitlab.com/OpenRGBDevelopers/OpenRGBVisualMapPlugin/-/tree/release_1.0
[o-hwsync]: https://gitlab.com/OpenRGBDevelopers/OpenRGBHardwareSyncPlugin/-/tree/release_1.0
[o-scheduler]: https://gitlab.com/OpenRGBDevelopers/OpenRGBSchedulerPlugin/-/tree/release_1.0
[o-httphook]: https://gitlab.com/OpenRGBDevelopers/OpenRGBHttpHookPlugin/-/tree/release_1.0
[o-ha]: https://www.home-assistant.io/integrations/openrgb/
[o-mcp3p-a]: https://github.com/PurimDEVY/openrgb-mcp
[o-mcp3p-b]: https://github.com/theankitbhardwaj/openrgb-mcp-server

<!-- SignalRGB 2.5.84 -->

[s-changelog]: https://docs.signalrgb.com/changelogs/
[s-terms]: https://signalrgb.com/terms/
[s-privacy]: https://signalrgb.com/privacy/
[s-pricing]: https://signalrgb.com/pricing/
[s-pro]: https://docs.signalrgb.com/guides/account-billing/about-pro-features/
[s-requirements]: https://docs.signalrgb.com/quick-start/about-system-requirements/
[s-install]: https://docs.signalrgb.com/quick-start/how-to-install/
[s-firstrun]: https://docs.signalrgb.com/quick-start/first-time-setup/
[s-update]: https://docs.signalrgb.com/guides/installation-setup/how-to-update/
[s-logs]: https://docs.signalrgb.com/troubleshooting/advanced-troubleshooting/how-to-read-logs/
[s-devtype]: https://docs.signalrgb.com/developer/plugins/determining-device-type/
[s-modules]: https://docs.signalrgb.com/developer/plugins/modules-and-features/
[s-plugin-load]: https://docs.signalrgb.com/developer/plugins/how-is-a-plugin-loaded-/
[s-devfuncs]: https://docs.signalrgb.com/developer/plugins/device-functions/
[s-smbus-plugin]: https://gitlab.com/signalrgb/signal-plugins/-/blob/9511979fa21a/Plugins/Asus/Motherboard%20Controllers/AsusSMBusMotherboardController.js
[s-dl]: https://docs.signalrgb.com/troubleshooting/software-conflicts/dynamic-lighting/
[s-forum-driver]: https://forum.signalrgb.com/t/257
[s-devices]: https://signalrgb.com/devices/
[s-controllers]: https://docs.signalrgb.com/guides/device-configuration/configure-rgb-controllers/
[s-conflicts]: https://docs.signalrgb.com/troubleshooting/software-conflicts/conflicting-programs/
[s-discover]: https://docs.signalrgb.com/quick-start/discover/
[s-meta]: https://docs.signalrgb.com/developer/lightscripts/meta-tags-/
[s-lightscripts]: https://docs.signalrgb.com/developer/lightscripts/
[s-devint]: https://docs.signalrgb.com/developer/lightscripts/creating-dev-integrations/
[s-customize]: https://docs.signalrgb.com/guides/effects-customization/how-to-customize-effects/
[s-dashboard]: https://docs.signalrgb.com/quick-start/understanding-the-dashboard/
[s-gameint]: https://docs.signalrgb.com/guides/advanced-features/how-to-configure-game-integrations/
[s-integrations]: https://signalrgb.com/integrations/
[s-brightness]: https://docs.signalrgb.com/guides/device-configuration/about-brightness/
[s-layouts]: https://docs.signalrgb.com/guides/device-configuration/about-layouts/
[s-audio]: https://docs.signalrgb.com/developer/lightscripts/audio-visualizer/
[s-screen]: https://docs.signalrgb.com/guides/advanced-features/about-screen-capture/
[s-monitoring]: https://docs.signalrgb.com/guides/advanced-features/system-monitoring/
[s-macros]: https://docs.signalrgb.com/guides/advanced-features/macros/
[s-api]: https://docs.signalrgb.com/developer/signalrgb-api/introduction/
[s-urls]: https://docs.signalrgb.com/developer/application-urls/
[s-ha3p]: https://github.com/hyperb1iss/signalrgb-homeassistant

<!-- Artemis 1.2026.1027.1 (master 8d2927a0), Artemis.Plugins 08595d11, wiki c18e6172, RGB.NET v3.1.0 -->

[a-repo]: https://github.com/Artemis-RGB/Artemis/tree/8d2927a05436f6a79ef5ad3ea741bb5ac43ff7eb
[a-readme]: https://github.com/Artemis-RGB/Artemis/blob/8d2927a05436f6a79ef5ad3ea741bb5ac43ff7eb/README.md
[a-ci]: https://github.com/Artemis-RGB/Artemis/blob/8d2927a05436f6a79ef5ad3ea741bb5ac43ff7eb/.github/workflows/master.yml
[a-pluginmgmt]: https://github.com/Artemis-RGB/Artemis/blob/8d2927a05436f6a79ef5ad3ea741bb5ac43ff7eb/src/Artemis.Core/Services/PluginManagementService.cs
[a-wizard]: https://github.com/Artemis-RGB/Artemis/tree/8d2927a05436f6a79ef5ad3ea741bb5ac43ff7eb/src/Artemis.UI/Screens/StartupWizard
[a-debugger]: https://github.com/Artemis-RGB/Artemis/tree/8d2927a05436f6a79ef5ad3ea741bb5ac43ff7eb/src/Artemis.UI/Screens/Debugger
[a-defaultlayout]: https://github.com/Artemis-RGB/Artemis/blob/8d2927a05436f6a79ef5ad3ea741bb5ac43ff7eb/src/Artemis.Core/Providers/DefaultLayoutProvider.cs
[a-plugins]: https://github.com/Artemis-RGB/Artemis/tree/8d2927a05436f6a79ef5ad3ea741bb5ac43ff7eb/src/Artemis.Core/Plugins
[a-nodes]: https://github.com/Artemis-RGB/Artemis/tree/8d2927a05436f6a79ef5ad3ea741bb5ac43ff7eb/src/Artemis.Core/VisualScripting
[a-preset]: https://github.com/Artemis-RGB/Artemis/blob/8d2927a05436f6a79ef5ad3ea741bb5ac43ff7eb/src/Artemis.Core/Plugins/LayerBrushes/ILayerBrushPreset.cs
[a-layer]: https://github.com/Artemis-RGB/Artemis/blob/8d2927a05436f6a79ef5ad3ea741bb5ac43ff7eb/src/Artemis.Core/Models/Profile/Layer.cs
[a-hints]: https://github.com/Artemis-RGB/Artemis/tree/8d2927a05436f6a79ef5ad3ea741bb5ac43ff7eb/src/Artemis.Core/Models/Profile/AdaptionHints
[a-general]: https://github.com/Artemis-RGB/Artemis/blob/8d2927a05436f6a79ef5ad3ea741bb5ac43ff7eb/src/Artemis.Core/Models/Profile/LayerGeneralProperties.cs
[a-device]: https://github.com/Artemis-RGB/Artemis/blob/8d2927a05436f6a79ef5ad3ea741bb5ac43ff7eb/src/Artemis.Core/Models/Surface/ArtemisDevice.cs
[a-profileconfig]: https://github.com/Artemis-RGB/Artemis/blob/8d2927a05436f6a79ef5ad3ea741bb5ac43ff7eb/src/Artemis.Core/Models/ProfileConfiguration/ProfileConfiguration.cs
[a-profileservice]: https://github.com/Artemis-RGB/Artemis/blob/8d2927a05436f6a79ef5ad3ea741bb5ac43ff7eb/src/Artemis.Core/Services/Storage/ProfileService.cs
[a-webserver]: https://github.com/Artemis-RGB/Artemis/blob/8d2927a05436f6a79ef5ad3ea741bb5ac43ff7eb/src/Artemis.Core/Services/WebServer/WebServerService.cs
[a-update]: https://github.com/Artemis-RGB/Artemis/blob/8d2927a05436f6a79ef5ad3ea741bb5ac43ff7eb/src/Artemis.UI/Services/Updating/UpdateService.cs
[a-workshop]: https://github.com/Artemis-RGB/Artemis/blob/8d2927a05436f6a79ef5ad3ea741bb5ac43ff7eb/src/Artemis.WebClient.Workshop/WorkshopConstants.cs
[a-packages]: https://github.com/Artemis-RGB/Artemis/blob/8d2927a05436f6a79ef5ad3ea741bb5ac43ff7eb/src/Directory.Packages.props
[a-devices]: https://github.com/Artemis-RGB/Artemis.Plugins/tree/08595d11a20f7dbd91e8fa5ce016307c627606c0/src/Devices
[a-brushes]: https://github.com/Artemis-RGB/Artemis.Plugins/tree/08595d11a20f7dbd91e8fa5ce016307c627606c0/src/LayerBrushes
[a-effects]: https://github.com/Artemis-RGB/Artemis.Plugins/tree/08595d11a20f7dbd91e8fa5ce016307c627606c0/src/LayerEffects
[a-audio]: https://github.com/Artemis-RGB/Artemis.Plugins/tree/08595d11a20f7dbd91e8fa5ce016307c627606c0/src/Collections/Artemis.Plugins.Audio
[a-input]: https://github.com/Artemis-RGB/Artemis.Plugins/tree/08595d11a20f7dbd91e8fa5ce016307c627606c0/src/Collections/Artemis.Plugins.Input
[a-webapi]: https://github.com/Artemis-RGB/Artemis.Plugins/tree/08595d11a20f7dbd91e8fa5ce016307c627606c0/src/Collections/Artemis.Plugins.WebAPI
[a-rgbnet]: https://github.com/DarthAffe/RGB.NET/tree/v3.1.0
[a-dl]: https://github.com/Artemis-RGB/Artemis.Plugins.Devices.DynamicLighting/blob/9b9832a829e47a6c55806ff1848a7032502a05f4/README.md
[a-hwmon]: https://github.com/Artemis-RGB/Artemis.Plugins.HardwareMonitors
[a-games]: https://github.com/Artemis-RGB/Artemis.Plugins.Games
[a-wrappers]: https://github.com/Artemis-RGB/Artemis.Plugins.Wrappers/tree/ba49fa3409d4ed3ac5263352a86aede0a208a4f4
[a-mqtt]: https://github.com/Artemis-RGB/Artemis.Plugins.Mqtt/tree/5372826dc9d28118d8fc13b33d5c9760a6bbfd41
[a-surface]: https://github.com/Artemis-RGB/Artemis.Wiki/blob/c18e6172ba0935736da5f64969af473b3b6a5fea/guides/user/surface.md
[a-sidebar]: https://github.com/Artemis-RGB/Artemis.Wiki/blob/c18e6172ba0935736da5f64969af473b3b6a5fea/guides/user/sidebar.md
[a-conditions]: https://github.com/Artemis-RGB/Artemis.Wiki/blob/c18e6172ba0935736da5f64969af473b3b6a5fea/guides/user/profiles/conditions.md
[a-restdoc]: https://github.com/Artemis-RGB/Artemis.Wiki/blob/c18e6172ba0935736da5f64969af473b3b6a5fea/guides/developer/local-rest-api.md
[a-telemetry]: https://github.com/Artemis-RGB/Artemis.Wiki/blob/c18e6172ba0935736da5f64969af473b3b6a5fea/telemetry.md
[a-workshopdoc]: https://github.com/Artemis-RGB/Artemis.Wiki/blob/c18e6172ba0935736da5f64969af473b3b6a5fea/guides/user/workshop.md
[a-faq]: https://github.com/Artemis-RGB/Artemis.Wiki/blob/c18e6172ba0935736da5f64969af473b3b6a5fea/faq.md
[a-mac]: https://github.com/Artemis-RGB/Artemis/issues/825

<!-- KeyGlow 1.0.1 (HiddenOnLies/KeyGlow at e4594013) -->

[k-release]: https://github.com/HiddenOnLies/KeyGlow/releases/tag/v1.0.1
[k-repo]: https://github.com/HiddenOnLies/KeyGlow/tree/e4594013c16eecb5ee5e8c014ede209c91b41073
[k-workshop]: https://github.com/HiddenOnLies/KeyGlow-layouts/blob/9cd1e398743c802818f6fd11337838b80cbbedd4/layouts.json
[k-readme]: https://github.com/HiddenOnLies/KeyGlow/blob/e4594013c16eecb5ee5e8c014ede209c91b41073/README.md
[k-compat]: https://github.com/HiddenOnLies/KeyGlow/blob/e4594013c16eecb5ee5e8c014ede209c91b41073/docs/compatibility.md
[k-format]: https://github.com/HiddenOnLies/KeyGlow/blob/e4594013c16eecb5ee5e8c014ede209c91b41073/docs/profile-format.md
[k-openrgb]: https://github.com/HiddenOnLies/KeyGlow/blob/e4594013c16eecb5ee5e8c014ede209c91b41073/keyglow/backends/openrgb.py
[k-hidprobe]: https://github.com/HiddenOnLies/KeyGlow/blob/e4594013c16eecb5ee5e8c014ede209c91b41073/keyglow/backends/hidprobe.py
[k-system]: https://github.com/HiddenOnLies/KeyGlow/blob/e4594013c16eecb5ee5e8c014ede209c91b41073/keyglow/app/system.py
[k-layout]: https://github.com/HiddenOnLies/KeyGlow/blob/e4594013c16eecb5ee5e8c014ede209c91b41073/keyglow/layout.py
[k-effects]: https://github.com/HiddenOnLies/KeyGlow/blob/e4594013c16eecb5ee5e8c014ede209c91b41073/keyglow/effects.py
[k-profile]: https://github.com/HiddenOnLies/KeyGlow/blob/e4594013c16eecb5ee5e8c014ede209c91b41073/keyglow/profile.py
[k-retarget]: https://github.com/HiddenOnLies/KeyGlow/blob/e4594013c16eecb5ee5e8c014ede209c91b41073/keyglow/retarget.py
[k-onboard]: https://github.com/HiddenOnLies/KeyGlow/blob/e4594013c16eecb5ee5e8c014ede209c91b41073/keyglow/onboard.py
[k-audio]: https://github.com/HiddenOnLies/KeyGlow/blob/e4594013c16eecb5ee5e8c014ede209c91b41073/keyglow/app/audio.py
[k-keyhook]: https://github.com/HiddenOnLies/KeyGlow/blob/e4594013c16eecb5ee5e8c014ede209c91b41073/keyglow/app/keyhook.py
[k-games]: https://github.com/HiddenOnLies/KeyGlow/blob/e4594013c16eecb5ee5e8c014ede209c91b41073/keyglow/app/games.py
[k-engine]: https://github.com/HiddenOnLies/KeyGlow/blob/e4594013c16eecb5ee5e8c014ede209c91b41073/keyglow/app/engine.py
[k-cli]: https://github.com/HiddenOnLies/KeyGlow/blob/e4594013c16eecb5ee5e8c014ede209c91b41073/keyglow/cli.py
[k-server]: https://github.com/HiddenOnLies/KeyGlow/blob/e4594013c16eecb5ee5e8c014ede209c91b41073/keyglow/app/server.py
[k-update]: https://github.com/HiddenOnLies/KeyGlow/blob/e4594013c16eecb5ee5e8c014ede209c91b41073/keyglow/app/update_check.py
[k-gallery]: https://github.com/HiddenOnLies/KeyGlow/blob/e4594013c16eecb5ee5e8c014ede209c91b41073/keyglow/app/gallery_sync.py
[k-layouts]: https://github.com/HiddenOnLies/KeyGlow/blob/e4594013c16eecb5ee5e8c014ede209c91b41073/keyglow/app/layout_sync.py
[k-conflicts]: https://github.com/HiddenOnLies/KeyGlow/blob/e4594013c16eecb5ee5e8c014ede209c91b41073/keyglow/app/conflicts.py
[k-homonym-msi]: https://github.com/beylerburak/keyglow
[k-homonym-razer]: https://github.com/amansingh909/keyglow
[k-homonym-hyperx]: https://github.com/kach4n/keyglow

<!-- Hypercolor 0.9.0 (02f53272) -->

[h-release]: https://github.com/hyperb1iss/hypercolor/releases/tag/v0.9.0
[h-readme]: https://github.com/hyperb1iss/hypercolor/blob/02f53272731f4c9fbcb06daf342732059bd4a4e7/README.md
[h-roadmap]: https://github.com/hyperb1iss/hypercolor/blob/02f53272731f4c9fbcb06daf342732059bd4a4e7/ROADMAP.md
[h-drivers]: https://github.com/hyperb1iss/hypercolor/tree/02f53272731f4c9fbcb06daf342732059bd4a4e7/crates/hypercolor-hal/src/drivers
[h-smbus]: https://github.com/hyperb1iss/hypercolor/blob/02f53272731f4c9fbcb06daf342732059bd4a4e7/crates/hypercolor-hal/src/smbus_registry.rs
[h-wled]: https://github.com/hyperb1iss/hypercolor/blob/02f53272731f4c9fbcb06daf342732059bd4a4e7/crates/hypercolor-driver-wled/src/lib.rs
[h-hue]: https://github.com/hyperb1iss/hypercolor/blob/02f53272731f4c9fbcb06daf342732059bd4a4e7/crates/hypercolor-driver-hue/src/lib.rs
[h-huescanner]: https://github.com/hyperb1iss/hypercolor/blob/02f53272731f4c9fbcb06daf342732059bd4a4e7/crates/hypercolor-driver-hue/src/scanner.rs
[h-govee]: https://github.com/hyperb1iss/hypercolor/blob/02f53272731f4c9fbcb06daf342732059bd4a4e7/crates/hypercolor-driver-govee/src/lib.rs
[h-openrgb]: https://github.com/hyperb1iss/hypercolor/blob/02f53272731f4c9fbcb06daf342732059bd4a4e7/crates/hypercolor-driver-openrgb/src/lib.rs
[h-openrgbhost]: https://github.com/hyperb1iss/hypercolor/tree/02f53272731f4c9fbcb06daf342732059bd4a4e7/crates/hypercolor-openrgb-host
[h-udev]: https://github.com/hyperb1iss/hypercolor/blob/02f53272731f4c9fbcb06daf342732059bd4a4e7/udev/99-hypercolor.rules
[h-driversreadme]: https://github.com/hyperb1iss/hypercolor/blob/02f53272731f4c9fbcb06daf342732059bd4a4e7/data/drivers/README.md
[h-compat]: https://github.com/hyperb1iss/hypercolor/blob/02f53272731f4c9fbcb06daf342732059bd4a4e7/docs/content/hardware/compatibility.md
[h-unclaimed]: https://github.com/hyperb1iss/hypercolor/blob/02f53272731f4c9fbcb06daf342732059bd4a4e7/crates/hypercolor-ui/src/components/unclaimed_hardware.rs
[h-catalog]: https://github.com/hyperb1iss/hypercolor/blob/02f53272731f4c9fbcb06daf342732059bd4a4e7/docs/content/effects/catalog.md
[h-builtin]: https://github.com/hyperb1iss/hypercolor/blob/02f53272731f4c9fbcb06daf342732059bd4a4e7/crates/hypercolor-core/src/effect/builtin/mod.rs
[h-lightscript]: https://github.com/hyperb1iss/hypercolor/blob/02f53272731f4c9fbcb06daf342732059bd4a4e7/crates/hypercolor-core/src/effect/lightscript.rs
[h-rawhtml]: https://github.com/hyperb1iss/hypercolor/blob/02f53272731f4c9fbcb06daf342732059bd4a4e7/docs/content/effects/raw-html.md
[h-watcher]: https://github.com/hyperb1iss/hypercolor/blob/02f53272731f4c9fbcb06daf342732059bd4a4e7/crates/hypercolor-core/src/effect/watcher.rs
[h-library]: https://github.com/hyperb1iss/hypercolor/blob/02f53272731f4c9fbcb06daf342732059bd4a4e7/crates/hypercolor-types/src/library.rs
[h-effectsapi]: https://github.com/hyperb1iss/hypercolor/blob/02f53272731f4c9fbcb06daf342732059bd4a4e7/crates/hypercolor-daemon/src/api/effects.rs
[h-logical]: https://github.com/hyperb1iss/hypercolor/blob/02f53272731f4c9fbcb06daf342732059bd4a4e7/crates/hypercolor-daemon/src/logical_devices.rs
[h-zones]: https://github.com/hyperb1iss/hypercolor/blob/02f53272731f4c9fbcb06daf342732059bd4a4e7/docs/content/studio/zones.md
[h-layer]: https://github.com/hyperb1iss/hypercolor/blob/02f53272731f4c9fbcb06daf342732059bd4a4e7/crates/hypercolor-types/src/layer.rs
[h-devicesettings]: https://github.com/hyperb1iss/hypercolor/blob/02f53272731f4c9fbcb06daf342732059bd4a4e7/crates/hypercolor-daemon/src/device_settings.rs
[h-attachments]: https://github.com/hyperb1iss/hypercolor/blob/02f53272731f4c9fbcb06daf342732059bd4a4e7/crates/hypercolor-daemon/src/attachment_profiles.rs
[h-scenes]: https://github.com/hyperb1iss/hypercolor/blob/02f53272731f4c9fbcb06daf342732059bd4a4e7/crates/hypercolor-daemon/src/api/scenes.rs
[h-spatial]: https://github.com/hyperb1iss/hypercolor/blob/02f53272731f4c9fbcb06daf342732059bd4a4e7/crates/hypercolor-core/src/spatial/mod.rs
[h-sampling]: https://github.com/hyperb1iss/hypercolor/blob/02f53272731f4c9fbcb06daf342732059bd4a4e7/crates/hypercolor-types/src/spatial.rs
[h-config]: https://github.com/hyperb1iss/hypercolor/blob/02f53272731f4c9fbcb06daf342732059bd4a4e7/crates/hypercolor-types/src/config/mod.rs
[h-audio]: https://github.com/hyperb1iss/hypercolor/tree/02f53272731f4c9fbcb06daf342732059bd4a4e7/crates/hypercolor-core/src/input/audio
[h-sensors]: https://github.com/hyperb1iss/hypercolor/blob/02f53272731f4c9fbcb06daf342732059bd4a4e7/crates/hypercolor-windows-telemetry/src/lib.rs
[h-session]: https://github.com/hyperb1iss/hypercolor/blob/02f53272731f4c9fbcb06daf342732059bd4a4e7/crates/hypercolor-daemon/src/session.rs
[h-sessiontypes]: https://github.com/hyperb1iss/hypercolor/blob/02f53272731f4c9fbcb06daf342732059bd4a4e7/crates/hypercolor-types/src/session.rs
[h-automation]: https://github.com/hyperb1iss/hypercolor/blob/02f53272731f4c9fbcb06daf342732059bd4a4e7/crates/hypercolor-core/src/scene/automation.rs
[h-prompts]: https://github.com/hyperb1iss/hypercolor/blob/02f53272731f4c9fbcb06daf342732059bd4a4e7/crates/hypercolor-daemon/src/mcp/prompts.rs
[h-security]: https://github.com/hyperb1iss/hypercolor/blob/02f53272731f4c9fbcb06daf342732059bd4a4e7/crates/hypercolor-daemon/src/api/security.rs
[h-security-doc]: https://github.com/hyperb1iss/hypercolor/blob/02f53272731f4c9fbcb06daf342732059bd4a4e7/docs/content/api/auth-and-security.md
[h-mcp]: https://github.com/hyperb1iss/hypercolor/blob/02f53272731f4c9fbcb06daf342732059bd4a4e7/crates/hypercolor-daemon/src/mcp/mod.rs
[h-mcpconfig]: https://github.com/hyperb1iss/hypercolor/blob/02f53272731f4c9fbcb06daf342732059bd4a4e7/crates/hypercolor-types/src/config/mcp.rs
[h-tools]: https://github.com/hyperb1iss/hypercolor/blob/02f53272731f4c9fbcb06daf342732059bd4a4e7/crates/hypercolor-daemon/src/mcp/tools/mod.rs
[h-authoring]: https://github.com/hyperb1iss/hypercolor/blob/02f53272731f4c9fbcb06daf342732059bd4a4e7/crates/hypercolor-daemon/src/mcp/tools/authoring.rs
[h-audit]: https://github.com/hyperb1iss/hypercolor/blob/02f53272731f4c9fbcb06daf342732059bd4a4e7/crates/hypercolor-daemon/src/audit_log.rs
[h-updates]: https://github.com/hyperb1iss/hypercolor/blob/02f53272731f4c9fbcb06daf342732059bd4a4e7/crates/hypercolor-daemon/src/release_notifications.rs
[h-firstrun]: https://github.com/hyperb1iss/hypercolor/blob/02f53272731f4c9fbcb06daf342732059bd4a4e7/crates/hypercolor-daemon/src/first_run.rs
[h-diagnostics]: https://github.com/hyperb1iss/hypercolor/blob/02f53272731f4c9fbcb06daf342732059bd4a4e7/crates/hypercolor-daemon/src/domain/diagnostics.rs
[h-conflicts]: https://github.com/hyperb1iss/hypercolor/blob/02f53272731f4c9fbcb06daf342732059bd4a4e7/crates/hypercolor-core/src/device/conflicts/catalog.toml
[h-firstlaunch]: https://github.com/hyperb1iss/hypercolor/blob/02f53272731f4c9fbcb06daf342732059bd4a4e7/docs/content/guide/first-launch.md
[h-mcpsetup]: https://hyperb1iss.github.io/hypercolor/agents/mcp-setup/
[h-toolsref]: https://hyperb1iss.github.io/hypercolor/agents/tools-reference/
[h-hass]: https://github.com/hyperb1iss/hypercolor-hass

<!-- Windows Dynamic Lighting -->

[w-guide]: https://learn.microsoft.com/en-us/windows/apps/develop/devices-sensors/lighting-dynamic-lamparray
[w-lights]: https://learn.microsoft.com/en-us/uwp/api/windows.devices.lights
[w-lamparray]: https://learn.microsoft.com/en-us/uwp/api/windows.devices.lights.lamparray
[w-lampinfo]: https://learn.microsoft.com/en-us/uwp/api/windows.devices.lights.lampinfo
[w-effects]: https://learn.microsoft.com/en-us/uwp/api/windows.devices.lights.effects
[w-playlist]: https://learn.microsoft.com/en-us/uwp/api/windows.devices.lights.effects.lamparrayeffectplaylist
[w-zindex]: https://learn.microsoft.com/en-us/uwp/api/windows.devices.lights.effects.ilamparrayeffect.zindex
[w-hidapi]: https://learn.microsoft.com/en-us/uwp/api/windows.devices.humaninterfacedevice
[w-tlc]: https://learn.microsoft.com/en-us/windows-hardware/drivers/hid/top-level-collections-opened-by-windows-for-system-use
[w-hw]: https://learn.microsoft.com/en-us/windows-hardware/design/component-guidelines/dynamic-lighting-devices
[w-unattend]: https://learn.microsoft.com/en-us/windows-hardware/customize/desktop/unattend/microsoft-windows-devices-lights-winrt
[w-gdk]: https://learn.microsoft.com/en-us/gaming/gdk/docs/features/common/lighting/lighting-basics
[w-mcp]: https://learn.microsoft.com/en-us/windows/ai/mcp/overview
[w-releaseinfo]: https://learn.microsoft.com/en-us/windows/release-health/windows11-release-information
[w-support]: https://support.microsoft.com/en-us/windows/hardware/input-devices/control-dynamic-lighting-devices-in-windows
[w-kb5031455]: https://support.microsoft.com/en-gb/topic/october-31-2023-kb5031455-os-builds-22621-2506-and-22631-2506-preview-6513c5ec-c5a2-4aaf-97f5-44c13d29e0d4
[w-kb5046740]: https://support.microsoft.com/en-au/topic/november-21-2024-kb5046740-os-build-26100-2454-preview-2040f716-b719-482a-8aff-f7f02c79b147
[w-23475]: https://blogs.windows.com/windows-insider/2023/06/07/announcing-windows-11-insider-preview-build-23475/
[w-23486]: https://blogs.windows.com/windows-insider/2023/06/22/announcing-windows-11-insider-preview-build-23486/
[w-autorgb]: https://github.com/microsoft/Dynamic-Lighting-AutoRGB
