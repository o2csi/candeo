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

## What the token protects

Whoever holds the token can set values, and nothing else: not choose a device, an
effect or a colour, not read the settings. A rule decides what a value lights, and
rules are only written in Candeo. Keep the token out of shared scripts all the
same — an environment variable, a secret store — and make a new one if it leaks.
