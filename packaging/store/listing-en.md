# Store listing — English (en-US)

For the Microsoft Store submission (#126). Text is pasted into Partner Center by
hand until #162 automates it. **What's new** is rewritten at every release; the
rest changes only when the application does.

Version described: **0.6.0**.

## Short description (shown in search results)

Lighting control, by talking straight to the device.

## Description

Candeo controls your device's lighting by talking to the hardware itself. No vendor runtime, no background service, no account: the application writes over the HID interface Windows already exposes, and the effects you write run inside it — so they keep going with the window closed.

Works with the Razer DeathStalker V2 Pro (wired); a supported device is required. Each device Candeo drives comes from a protocol read off the hardware and checked write by write. The list is at https://o2csi.github.io/candeo/devices.html.

WHAT IT DOES
• The whole matrix: solid colours, per-row gradients, and the effects the device's own firmware runs.
• Effects written in TypeScript, running inside the application: a ripple that follows your typing, a spectrum, whatever you write.
• Automations: rules that take over for a while, then give your effect back — the time on the hour, the keyboard dark at night. Written as a sentence, or as a cron expression.
• A Clock effect scrolls the time across the keyboard, and the effects you write can read the clock too.
• Effects keep running with the window closed — Candeo folds into the notification area and the lighting stays.
• Your effects are ordinary files in Documents\candeo\effects: edit them here, or in the editor you already use.
• Starts with Windows if you ask it to. Light and dark. English and French.

WHAT IT DOES NOT DO
• It does not phone home. Nothing is collected, nothing is sent: this Store version makes no network request at all, and updates come from the Store.
• It does not stay in your way. Quit it and the device keeps the lighting it had.

The protocol it speaks was established by observing the hardware, and is documented in the open with the firmware version each fact was checked against. The source is available under the GPL-3.0.

## What's new in this version (0.6.0)

• Automations, in their own tab: a rule interrupts the effect on a device for a while, then gives it back. Every hour, on weekdays, at a set time — or any cron expression under Advanced.
• Try runs a rule once, right away. Resume, on the device and in the notification area, gives your effect back at once. Pause automations holds every rule, for a meeting or a game.
• Clock, a new effect: the time scrolls across the keyboard, with the seconds if you want them.
• Effects can read the clock: write your own clock face in TypeScript.

## Search terms (7 at most, 30 characters each)

hid, rgb, keyboard lighting, razer, deathstalker, typescript effects, open source
