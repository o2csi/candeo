# Linux

Candeo is developed on Windows, written so as not to be locked into it. Nobody
has a Linux machine in the loop, so CI plays that role: the `linux` job builds
the full workspace on `ubuntu-latest`, runs the tests, packages as `.deb` and
`.rpm`, and **reads the produced packages** to check that the udev rule is in
them.

Hence the distinction this page keeps: what is **compiled** and what remains
**assumed**.

## Compiled and verified

- **Hardware access.** The `hidraw` backend of `hidapi` builds on Linux, and
  nothing in the three crates depends on Windows to compile.
- **Packaging.** `.deb` and `.rpm` are produced, and the udev rule is present
  in both at `/usr/lib/udev/rules.d/60-candeo.rules`, verified by reading the
  packages, not by rereading the configuration. ⚠️ **This check does not run
  on pull requests**, only on `main` and on demand: see
  [Verifying locally](#verifying-locally).
- **Protocol.** `candeo-protocol` has no system dependencies: it builds bytes,
  and its tests run everywhere.
- **Paths.** No path is hard-coded: Tauri's API applies the system's convention.
  Shipped effects go in `app_data_dir()` (`~/.local/share/…`), the user's in
  `Documents/candeo/effects`, settings in `app_config_dir()`.

## Assumed, for lack of Linux hardware

There is no USB behind a GitHub runner. Three points are waiting for a keyboard
plugged into a Linux machine: that feature report writes go through over hidraw,
that `interface_number` tells the composite device's interfaces apart as it does
on Windows, and that the folders resolved by Tauri really do land in
`~/.local/share` and `~/.config`.

Key presses and sound are read on Windows only for now.

## Building

Stable Rust, Node 22+, pnpm 10+, and on Debian or Ubuntu the Tauri 2 system
dependencies, plus `libudev-dev`, which `hidapi` resolves through `pkg-config`:

```bash
sudo apt-get install -y \
  libwebkit2gtk-4.1-dev libgtk-3-dev libsoup-3.0-dev \
  libayatana-appindicator3-dev librsvg2-dev libxdo-dev libssl-dev \
  libudev-dev build-essential pkg-config file
```

This is the list the CI `linux` job installs: if it goes stale, CI will say so.
The job adds `rpm`, which is only used to read back the produced package.

## Verifying locally

The CI `linux` job takes a quarter of an hour, about ten minutes of which go into
recompressing a debug `.rpm`. So it does **not** run on pull requests: only on
`main`, and on demand via **Actions → CI → Run workflow**, picking the branch.

In the meantime, WSL does the same work without waiting for a runner:

```bash
cargo check --workspace --all-targets
cargo test --workspace
```

And if the change touches packaging, HID I/O or system dependencies, the full
check, the slow step, to run only in that case (with `rpm` installed too):

```bash
pnpm install --frozen-lockfile
pnpm --filter @candeo/desktop exec tauri build --debug --bundles deb,rpm

rule='usr/lib/udev/rules.d/60-candeo.rules'
dpkg-deb -c "$(ls target/debug/bundle/deb/*.deb | head -1)" | grep -F "$rule"
rpm -qpl "$(ls target/debug/bundle/rpm/*.rpm | head -1)" | grep -F "/$rule"
```

The two `grep` calls are the proof: the rule really is **in the packages**, not
merely declared in `tauri.conf.json`.

## udev rule

`/dev/hidraw*` is created as `0600 root:root`. The rule is in
[`packaging/linux/`](../packaging/linux/60-candeo.rules); the `deb` and `rpm`
packages install it. From source, copy it yourself:

```bash
sudo cp packaging/linux/60-candeo.rules /etc/udev/rules.d/
sudo udevadm control --reload-rules && sudo udevadm trigger
```

Why `uaccess` rather than a group, why the 60 prefix, and why the `*-native`
backends of `hidapi` were set aside:
[`design/effects-runtime.md`](design/effects-runtime.md) §6.
