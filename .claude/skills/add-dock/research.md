# Researching a dock

Nothing is implemented until the research is done. The device will never be plugged in during
development, so the research is the only thing that makes the driver right.

## What to find out

For the family:

- Transport: USB HID (interface, usage page, report IDs, report sizes in and out), USB serial,
  Bluetooth, network. Whether a handshake, a mode switch or a keep-alive is needed, and what
  happens without it.
- Every command the host sends: bytes, fields, byte order, limits, acknowledgements, ordering,
  chunking of images, commit or flush steps.
- Every input report: layout, codes, press and release semantics, encoders, touch coordinates
  and gestures, acknowledgements mixed into the input stream.
- Identity: serial, firmware version, model string; how to read them; how to tell apart models
  that share a USB id.
- Image path: format, quality and subsampling, size limits, rotation, mirroring, which surface
  and region each control draws into, whole-page versus per-key updates.
- Timing: polling, measured or documented throughput, minimum delays, timeouts.
- Commands that **change persistent memory** (firmware, boot logo, stored configuration, flash)
  and any reported way to brick, wedge or reset the device. These go into the safety section.
- Quirks per operating system: exclusive access on macOS, shared handles on Windows (and the
  vendor's app holding the device), udev rules and other drivers grabbing it on Linux
  (ModemManager for serial devices).
- How the vendor's plugin SDK sees the device (device type, controllers, layouts).

For each model: USB ids of every variant (rebrand), layout (grid, spans, controls and their
kinds), pixel size and format per control, wire order of images and input codes, extras (LEDs,
haptics, strips), firmware differences.

## Where to look

Start from [families.md](families.md), then search for more. Cast wide, then read code:

- Official documentation and SDKs (Elgato's HID docs; vendors' plugin SDKs on GitHub, which
  carry device types and layouts even when the USB protocol is closed).
- Rust: `elgato-streamdeck`, `mirajazz`, `ajazz-sdk`, the OpenDeck device plugins listed in
  `OpenActionAPI/plugins`, and any newer crate on crates.io or GitHub.
- OpenDeck itself (`nekename/OpenDeck`), StreamController and its device library, Bitfocus
  Companion and its `companion-surface-*` repositories, `node-elgato-stream-deck`,
  `python-elgato-streamdeck`, Loupedeck libraries, Logitech HID++ projects.
- GitHub search by USB id (`"0x1234"`, `1234:5678`, lowercase and uppercase), product names,
  brand names; issues and pull requests of the projects above asking for the device (they often
  contain the facts and the bugs).
- Write-ups and reverse-engineering notes (blogs, Hackaday, forums).

Use subagents in parallel, one per group of sources, each asked for a structured fact table with
the source of every fact. Clone into a temporary folder outside the repository (never inside it).

## The rule of sources

- A fact is **accepted** when it comes from official documentation and the projects that support
  the device do not contradict it, or when **at least two independent projects agree** on it.
- Two is the minimum. Analyse every project found: five agreeing is better than two.
- **Independent** means not derived from each other: an OpenDeck plugin built on `mirajazz` and
  `mirajazz` itself are one source; a fork is the same source as its parent; a port that says it
  was ported from another project is that project. Check imports, credits and history.
- Measurements on real hardware recorded by a project (with the hardware named) weigh more than
  values copied between projects.
- When sources **disagree**, the fact is **open** and the model waits, unless official
  documentation settles it or a clear majority of independent sources agrees and the minority is
  explained (a known bug, an older firmware).
- A model with any open fact the driver needs is not added. Models of the same family whose facts
  are settled go ahead.

## Licences and copying

Reading code under any licence to understand a protocol is fine. The repository never receives
third-party code, firmware, assets, captures or traffic dumps. Facts (numbers, message layouts,
names) are described in our own words and implemented from scratch. Note each project's licence
in the fact sheet.

## The fact sheet

Write `docs/devices/<family>.md` (create the folder if needed), in English, in our own words, with
no local paths, serial numbers or captures. Update it when adding a model to an existing family.

```markdown
# <Family>

Short description: who makes it, which brands sell it, transport.

## Sources

| Project | Language | Licence | Independent of | What it covers |
|---|---|---|---|---|

## Protocol

Transport, framing, commands, input, identity, images, timing: tables of fields and values,
written by us.

## Commands never sent

Each command that can change persistent memory, and why.

## Models

| Model id | Variants (brand, name, VID:PID) | Controls | Images | Confirmed on hardware |
|---|---|---|---|---|

## Facts per model

For each model, each fact with its sources: "112×112, rotated 180° — official / A, B, C agree".

## Open questions

Facts the sources disagree on, which models wait for them, and what would settle them.
```

## Reporting to the user

A short summary in their language: models that pass, models that wait and why, number of
independent sources per model, risks found, anything the project would need that is new. Then go
on to build, unless nothing passes or a new dependency, permission or transport needs their say.
