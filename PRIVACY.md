# Privacy policy

**Candeo collects nothing, sends nothing, and has no accounts.**

Published by O2CSI. This policy covers the Candeo desktop application, however
it was installed: from the Microsoft Store, from a release of this repository,
or through a package manager.

## What stays on the computer

Everything the application writes stays on the computer it runs on:

| What | Where |
|---|---|
| Settings, including which keyboards you chose to control | `settings.json`, in the application's configuration folder |
| The effects you write, duplicate or drop in | `Documents\candeo\effects`, yours to open, move or delete |
| The effects the application ships, and their compiled form | the application's data and cache folders |
| The log | the application's log folder, one file a day, the last seven kept |

None of it is sent anywhere. There is no account to create, no identifier
assigned, and no telemetry: the application does not count launches, does not
report which effects run, and does not measure anything about the computer.

**A keyboard's serial number is never written down.** The log identifies a unit
by a fingerprint derived from it, so two keyboards of the same model can be told
apart in a bug report without the serial itself appearing.

**Key presses are not read**, except by an effect that declares it reacts to
typing — and then only as key positions and times, in memory, for the effect
being drawn. Nothing is written to the log or to disk, and nothing leaves the
computer.

## The one request the application can make

Settings offers *Check for new versions*. When it is on, the application asks
`api.github.com` whether a newer version of Candeo is published, once per
launch. The request carries no identifier and no information about the computer;
the answer is the public list of this repository's releases, and the comparison
happens on the computer.

- It is a setting: **off, no request is ever made.**
- **In the Microsoft Store version it never runs**, since the Store updates the
  application.
- GitHub sees such a request the way it sees any visit to a public page,
  including the address it came from. GitHub's own
  [privacy statement](https://docs.github.com/site-policy/privacy-policies/github-privacy-statement)
  covers what it does with that.

Nothing is downloaded or installed by that check: it shows a version and a link.

## What you send, if you choose to

**Copy the diagnostic**, in Settings, puts on the clipboard what a bug report
needs: versions, the system, the state of the devices and of the engine. It is
written to be read before it is sent, and paths in it are shortened so the
account name does not appear. Nothing sends it: you decide where it goes.

## Children

The application is a tool for controlling keyboard lighting. It is not directed
at children, and it collects no data from anyone.

## Changes

This policy is versioned with the application, in this repository. A change to
it is a commit, visible in the history like any other.

## Contact

Questions about this policy, or about what the application does with anything:
open an issue at <https://github.com/o2csi/candeo/issues>.
