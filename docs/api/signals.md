# Sending signals to Candeo

A signal is a named value another program sends — `build` is `failed`, `doorbell`
is `ring` — and a rule in the Automations tab decides what it lights. The sender
never names a device, an effect or a colour: it states a fact, and the rule
decides the rest. Why it is shaped this way is
[`inputs-and-automations.md`](../design/inputs-and-automations.md) §2.3.

## Turning it on

In **Settings › Signals**:

1. Turn on the API. Candeo listens on this computer only, at
   `http://127.0.0.1:7317/signals`.
2. Copy the token. Every request carries it; **New token** refuses every sender
   still holding the old one.
3. For another machine — Home Assistant on the local network — tick the
   interface it reaches this computer through. Windows asks once whether to let
   Candeo through its firewall: allow it on private networks only.

The port is 7317 unless changed there.

## Sending

```text
POST /signals?ttl=<seconds>
Authorization: Bearer <token>
Content-Type: application/json

{ "build": "failed", "volume": 0.4, "muted": true }
```

- **Values are flat**: strings, numbers, booleans. An empty string erases its
  name.
- **A value lasts 60 seconds** unless `ttl` says otherwise; `ttl=0` keeps it until
  it is erased — for a state pushed once, such as a build that stays broken.
- **A request is taken whole or not at all**: one refused name, and nothing in it
  is set.

The answer:

```json
{ "accepted": ["build", "volume", "muted"], "erased": [], "expiresIn": 60 }
```

`expiresIn` is in seconds, `null` until erased. `GET /signals`, with the same
token, lists what is held.

| Status | When |
|---|---|
| `200` | taken |
| `400` | not JSON, a name or a value out of bounds, a `ttl` that is not whole seconds — the body says which |
| `401` | no token, or the wrong one |
| `403` | the request comes from a web page (it carries `Origin`) |
| `404`, `405` | another path than `/signals`, another method than `GET` or `POST` |
| `413` | a body over 32 KB |

**Bounds**: 64 signals held at once, names of 1 to 64 letters, digits or
`_ - . :`, string values of 256 characters at most.

## Examples

**PowerShell**, at the end of a build script:

```powershell
$headers = @{ Authorization = "Bearer $env:CANDEO_TOKEN" }
Invoke-RestMethod -Method Post -Uri "http://127.0.0.1:7317/signals?ttl=0" `
  -Headers $headers -ContentType "application/json" -Body '{"build":"failed"}'
```

**curl**, which Windows 10 and later ship, and every Linux:

```bash
curl -H "Authorization: Bearer $CANDEO_TOKEN" -H "Content-Type: application/json" \
  -d '{"build":"ok"}' http://127.0.0.1:7317/signals
```

Erasing a value sent until erased:

```bash
curl -H "Authorization: Bearer $CANDEO_TOKEN" -H "Content-Type: application/json" \
  -d '{"build":""}' http://127.0.0.1:7317/signals
```

**Home Assistant**, from another machine, once its interface is ticked. In
`configuration.yaml`, with the address of the computer running Candeo:

```yaml
rest_command:
  candeo_signal:
    url: "http://192.0.2.23:7317/signals"
    method: post
    headers:
      Authorization: !secret candeo_bearer
    content_type: "application/json"
    payload: '{"{{ name }}": "{{ value }}"}'
```

and in `secrets.yaml`, `candeo_bearer: "Bearer <token>"`. An automation then calls
it:

```yaml
action:
  - action: rest_command.candeo_signal
    data:
      name: doorbell
      value: ring
```

## Lighting something with it

In **Automations**, a new rule: *when the signal* `doorbell` *equals* `ring`, on a
device, show an effect —

- **while it holds**, for a state: the device shows the effect from the moment
  the value arrives until it changes, expires or is erased;
- **for a few seconds**, for an event: a flash at each receipt, so a doorbell rung
  again flashes again.

**Send a test signal**, in Settings, tries a rule before any sender exists; the
list above it shows what arrived, and when each value expires.

## Driving a setting with it

A rule decides *which* effect runs. A signal can also drive **one setting of the
effect already running**: in the gallery, a setting's **Signal** switch makes it
read a signal instead of its value — *Fixed gradient*, its colour from `status`;
*Ripples*, its speed from `volume`. A rule's settings bind the same way.

What the sender sends is converted to the setting's kind:

| Setting | Accepted |
|---|---|
| colour | `#rrggbb` or `#rgb`, the `#` optional |
| number | a number or its text, kept within the setting's range |
| on/off | `true`, `false`, `1`, `0` |
| choice | one of the choices, by its value |

Anything else — and a signal absent or expired — leaves the setting's own value,
which is what the effect shows meanwhile. So a sender computing a colour sends
`{"status": "#ff0000"}` and the colour follows; a word like `failed` is for a
rule.

## Reading signals in an effect you write

An effect declaring `inputs: ['signals']` receives every value held, by name, in
`ctx.signals` — for an effect drawing many at once. It then works only with
senders using those exact names; binding a setting, above, works with any effect.

*Status row*, shipped with Candeo, is one: a key of the top row per signal, by
name. Another, to copy into your effects folder as `Meters.ts` — each number
received drawn as a bar, one row per signal:

```ts
import { defineEffect, mix, BLACK } from '@candeo/effects-api'

const LOW = { r: 32, g: 200, b: 64 }
const HIGH = { r: 235, g: 24, b: 24 }

export default defineEffect({
  description: 'Each number received as a bar, one row per signal',
  inputs: ['signals'],
  params: {
    unlit: { kind: 'number', label: 'Unlit part (%)', min: 0, max: 100, default: 8 },
  },
  render({ layout, signals, frame, params }) {
    // Numbers only, by name: `cpu`, `ram`, `volume`… each on its own row, top down.
    const levels = Object.keys(signals)
      .sort()
      .flatMap((name) => {
        const value = signals[name]
        return typeof value === 'number' ? [value <= 1 ? value : value / 100] : []
      })
    const rows = [...new Set(layout.keys.map((key) => key.row))].sort((a, b) => a - b)

    levels.forEach((level, i) => {
      const row = rows[i]
      if (row === undefined) return
      const keys = layout.keys.filter((key) => key.row === row).sort((a, b) => a.col - b.col)
      keys.forEach((key, k) => {
        const at = (k + 1) / keys.length
        const colour = mix(LOW, HIGH, at)
        frame.set(key, at <= level ? colour : mix(BLACK, colour, params.unlit / 100))
      })
    })
  },
})
```

Fed by a watcher sending, every few seconds:

```bash
curl -H "Authorization: Bearer $CANDEO_TOKEN" -H "Content-Type: application/json"   -d '{"cpu":0.42,"ram":61}' http://127.0.0.1:7317/signals
```

## What the token protects

Whoever holds the token can set values, and nothing else: not choose a device or
an effect, not read the settings. What a value changes — a rule, or a setting
bound to it — is only decided in Candeo. Keep the token out of shared scripts all the
same — an environment variable, a secret store — and make a new one if it leaks.
