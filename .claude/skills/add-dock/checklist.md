# Checklist for a new model

Every item, for every model. A new family also needs its folder (see [architecture.md](architecture.md)).

## Research and docs

- [ ] `docs/devices/<family>.md` written or updated: sources, protocol, commands never sent,
      models, facts per model with their sources, open questions ([research.md](research.md)).
- [ ] Every fact the code uses is accepted under the rule of sources; models with open facts left
      out.

## Driver

- [ ] `driver/<family>/models/<model>.rs`: the `DeviceModel` and its variants, listed in
      `models/mod.rs` and in `driver/catalog.rs`.
- [ ] Model id `<family>.<model>`, never renamed once released.
- [ ] `confirmed_on_hardware: false` (only the owner's D200, or a user's confirmation in an issue,
      makes it `true`).
- [ ] Family code reads the model's fields; no `if model == …` outside the descriptor.
- [ ] Commands as a closed enum, none that changes persistent memory ([safety.md](safety.md)).
- [ ] Identity read and checked before the first write.
- [ ] `emulator.rs` covers the model ([testing.md](testing.md)).
- [ ] Linux: variants' USB ids in `src-tauri/packaging/linux/70-taurilanzi.rules` (and the
      ModemManager rule for serial devices).
- [ ] Windows: the vendor's app holding the device is detected and explained.

## The app around it

- [ ] Device face SVG, drawn by us (never the vendor's artwork), matching the descriptor's
      bounds.
- [ ] Plugin SDKs: Elgato device type and size, controllers per control, Ulanzi model name where
      it applies; the action filters accept the model.
- [ ] Rules: control ids and what each control can run, in Rust; profile compatibility (removed
      keys, keys with an alert) covers the model's controls.
- [ ] Renderer: every surface's format, rotation and mirroring encoded.
- [ ] Translations in `src-tauri/locales/` and `src-nuxt/app/locales/`, Spanish and English, for
      any new text (model names stay as the vendor writes them).
- [ ] MCP: nothing model-specific hardcoded; `guide.md` examples still valid
      (`guide_examples.rs`); new controls readable and editable through the tools.
- [ ] The "not confirmed on hardware" warning shows for the model when it connects, with the link
      to `https://github.com/fstechlabs/taurilanzi/issues`.
- [ ] README: supported docks list updated, saying which are confirmed on hardware.
- [ ] `CLAUDE.md`: updated if the change adds a convention, an environment variable or a command.

## Checks and tests

- [ ] Build, `bun run typecheck`, `cargo clippy --all-targets`, Windows clippy when relevant.
- [ ] The app with a virtual dock of the model: shown, edited, profile moved to and from the D200,
      warning shown.
- [ ] After the user is happy: byte tests, emulator tests, rules and compatibility tests, e2e
      specs with virtual docks; then the whole e2e suite before committing.
