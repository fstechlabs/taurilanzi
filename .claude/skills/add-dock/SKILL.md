---
name: add-dock
description: Add native support for a stream dock model or family (Elgato Stream Deck, Mirabox/Ajazz clones, Ulanzi, Loupedeck, Logitech…) to Taurilanzi. Starts with a deep research of the device's low-level protocol and of every existing project that supports it, then builds the driver, the model descriptor and everything the app needs around it, safely and without the device at hand. Use when asked to support a new dock, model, rebrand or family.
---

# Add a dock to Taurilanzi

Taurilanzi drives docks natively: each supported model is plug and play, driven by the app's own
Rust code. There are no device plugins of any kind (not OpenDeck's `DeviceNamespace` plugins, not
Companion's surface plugins, not a protocol of our own for third parties).

The owner only has an Ulanzi D200. **Every other model is built and shipped without ever being
plugged in.** Correctness comes from the research and safety from the method: follow every step.

Read before starting, in this order:

1. The project's `CLAUDE.md` (workflow, conventions, public-repository rules).
2. [architecture.md](architecture.md): where everything lives and the multi-device base.
3. [families.md](families.md): what is already known about each protocol family and model.
4. [research.md](research.md), [safety.md](safety.md), [testing.md](testing.md) and
   [checklist.md](checklist.md) when you reach their step.

## Steps

### 1. Prepare

- Stop any running `tauri dev`.
- Find out which model(s) were asked for, and their family in [families.md](families.md). A
  "model" here is one hardware layout: rebrands of identical hardware are variants of one model.
- Read what the multi-device base already has and what it still lacks ("The base, as built" in
  [architecture.md](architecture.md)). What a new model needs and the base lacks (dials, another
  image format, another transport) is added to the base first, checked on the D200, then the
  model.
- Check whether the family already has a folder in `src-tauri/src/driver/`. A new model of a
  known family is mostly a model file; a new family is a new folder.

### 2. Research (mandatory, before any code)

Follow [research.md](research.md). In short:

- Find **every** project that supports the device or its family, plus official documentation
  where it exists. Read their source code, not their READMEs alone. Use subagents in parallel,
  one per group of sources, and clone into a temporary folder outside the repository.
- Every fact the driver relies on needs **official documentation** (cross-checked against the
  projects) or **at least two independent projects that agree**. Two is the minimum: analyse all
  that exist; the more agree, the better. Projects built on one another count once.
- A fact the sources disagree on keeps that model out until settled.
- Write the result to `docs/devices/<family>.md` in our own words (template in research.md).
- Tell the user, briefly: which models pass, which wait and why, sources found, risks. Then
  continue. Stop and ask only if no model passes, or the family needs something new for the
  project (a new transport crate, a new permission, Bluetooth).

### 3. Build

- Follow [architecture.md](architecture.md) for where each piece goes and
  [safety.md](safety.md) for what the driver may send. The safety rules are not optional.
- Build the family's **emulator** together with the driver: it is how the driver is checked
  without the device ([testing.md](testing.md)).
- Go through [checklist.md](checklist.md): descriptor, catalog, udev rule, plugin SDK identities,
  device face, translations, MCP, profile compatibility, the "not confirmed on hardware"
  warning, docs, README.
- New models ship with `confirmed_on_hardware: false`.

### 4. Check

- Build, `bun run typecheck`, `cargo clippy --all-targets`, and the Windows clippy from
  `CLAUDE.md` when the change touches anything system-specific.
- Run the driver against its emulator and the app with a virtual dock of the new model
  ([testing.md](testing.md)).
- The D200 must behave exactly as before. Checks on the real dock (`dock_check`, `frame_rate`)
  only when the user agrees: they may have Ulanzi Studio or the app using it.

### 5. Ask the user to try it

With `tauri dev` running and a virtual dock of the new model: simple, concrete steps (the device
appears, its face is right, keys and dials can be edited, a profile moves from the D200 to it and
back, the warning shows). Never ask them to plug in a device they do not have.

### 6. Tests, at the end

Once the user is happy, as `CLAUDE.md` says: byte-level tests of the protocol, emulator tests
of the driver, rules and compatibility tests, and e2e specs with virtual docks
([testing.md](testing.md)). The e2e suite runs whole, right before a commit.

## Rules that always hold

- Never copy third-party code, firmware, captures or assets into the repository. Protocols are
  described and implemented in our own words. No USB/HID traffic dumps, no serial numbers, no
  local paths.
- Never send a command that can change a device's persistent memory (firmware, boot logo,
  stored configuration): see [safety.md](safety.md).
- Never write to a device before reading back its identity and matching it to the model.
- Never connect to the user's real D200 unless they ask; never close Ulanzi Studio.
- Never run anything that changes data against the real configuration: use
  `TAURILANZI_DATA_DIR` or the e2e tests.
- Everything configurable is configurable through MCP too, and every rule lives in Rust
  (`CLAUDE.md`).
- Commit, push or open a pull request only when asked, reviewing the whole diff first.
