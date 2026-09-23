# Store listing — English (en-US)

For the Microsoft Store submission (#126). The release workflow sends this text
and `listing-fr.md` to Partner Center when both describe the version released
(#162, `docs/releasing.md`); keep the four sections in this order. **What's new**
is rewritten at every release; the rest changes only when the application does.

Version described: **0.8.0**. Its *What's new* covers everything since 0.7.0.

## Short description (shown in search results)

Lighting control, by talking straight to the device.

## Description

Candeo controls your device's lighting by talking to the hardware itself. No vendor runtime, no background service, no account: the application writes over the HID interface Windows already exposes, and the effects you write run inside it — so they keep going with the window closed.

Works with the Razer DeathStalker V2 Pro (wired), and with the Alienware m18 R1's keyboard and lighting zones; a supported device is required. Each device Candeo drives comes from a protocol read off the hardware and checked write by write. The list is at https://o2csi.github.io/candeo/devices.html.

WHAT IT DOES
• The whole matrix: solid colours, per-row gradients, and the effects the device's own firmware runs.
• Effects written in TypeScript, running inside the application: a ripple that follows your typing, a spectrum, whatever you write.
• Automations: rules that take over for a while, then give your effect back — the time on the hour, the keyboard dark at night or while you are away. Written as a sentence, or as a cron expression.
• A Clock effect scrolls the time across the keyboard, and the effects you write can read the clock too.
• Effects keep running with the window closed — Candeo folds into the notification area and the lighting stays.
• Your effects are ordinary files in Documents\candeo\effects: edit them here, or in the editor you already use.
• Starts with Windows if you ask it to. Light and dark. English and French.

WHAT IT DOES NOT DO
• It does not phone home. Nothing is collected, nothing is sent: this Store version makes no network request at all, and updates come from the Store.
• It does not stay in your way. Quit it and the device keeps the lighting it had.

The protocol it speaks was established by observing the hardware, and is documented in the open with the firmware version each fact was checked against. The source is available under the GPL-3.0.

## What's new in this version (0.8.0)

• A second device: the Alienware m18 R1. Candeo lights its keyboard key by key, and the ring and the logo around it as three zones.
• The seven effects the m18 R1's firmware runs, under the names Alienware gives them, with a colour for those that take one. The simulator draws them too.
• A setting you change reaches the running effect at once, the device's own effects included.
• Effects that react to key presses are no longer offered for lighting zones, where no key is ever pressed.
• Error messages name the device concerned, and each one can be selected and closed.

## Search terms (7 at most, 30 characters each)

alienware, rgb, keyboard lighting, razer, deathstalker, typescript effects, open source
