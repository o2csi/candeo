# Store listing — English (en-US)

For the Microsoft Store submission (#126). The release workflow sends this text
and `listing-fr.md` to Partner Center when both describe the version released
(#162, `docs/releasing.md`); keep the five sections in this order. **What's new**
is rewritten at every release; the rest changes only when the application does.

Version described: **0.9.0**. Its *What's new* covers everything since 0.8.0.

## Short description (shown in search results)

Your keyboard's lighting, exactly the way you want it, without the maker's software.

## Description

Make your keyboard light up exactly the way you want. Candeo drives every key directly — fifteen ready-made effects, a live preview, rules that change the lighting on their own — without the maker's software, without a background service and without an account.

Works with the Razer DeathStalker V2 Pro (wired) and the Alienware m18 R1, its keyboard and its lighting zones. A supported device is required; the list grows at https://o2csi.github.io/candeo/devices.html.

HIGHLIGHTS
• Fifteen effects, ready to go: ripples that follow your typing, rain, a starry night, lightning, a clock that scrolls the time across the keys… Tune each one while you watch it.
• Every key, its own colour: the whole matrix, gradients, and the effects built into your device's firmware.
• Lighting that follows your day: the time on the hour, the keyboard dark at night or when you step away — and your effect comes back on its own.
• Write your own: an effect is a small TypeScript file, previewed live on a keyboard simulator before it reaches the hardware. Edit it in Candeo or in the editor you already use.
• Signals from your other tools: a build script or Home Assistant tells Candeo what happened, and your rules decide what lights up. Off until you turn it on.
• Always on, never in the way: effects keep running with the window closed, from the notification area.
• Private by design: no account, nothing collected, nothing sent — this Store version makes no network request at all, and updates come from the Store.
• Starts with Windows if you want it to. Light and dark themes. English and French.

Open source, under the GPL-3.0.

## Features (a short list, shown on the Store page)

- Fifteen ready-made effects, tuned while you watch them
- Every key its own colour, plus your device's firmware effects
- Automations: the time on the hour, dark at night or when you step away
- Write your own effects in TypeScript, with a live preview
- Keeps running with the window closed, from the notification area
- Signals from scripts and Home Assistant, through a local API
- No account, nothing collected, no network request
- English and French, light and dark themes

## What's new in this version (0.9.0)

• Signals: other programs — a build script, Home Assistant — can now tell Candeo what happened, through a local HTTP API, and your rules decide what it lights. Off until you turn it on in Settings.
• A rule can wait for a signal: while it holds, for a state, or a flash of a few seconds at each receipt, for an event.
• Settings lists the signals received and how long each one lasts, and sends a test one to try a rule.
• The reset confirmation now says that automation rules are deleted too.

## Search terms (7 at most, 30 characters each)

alienware, rgb, keyboard lighting, razer, deathstalker, typescript effects, home assistant
