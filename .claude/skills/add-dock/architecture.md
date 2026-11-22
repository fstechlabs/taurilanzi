# Architecture for many docks

How Taurilanzi supports many dock models at once, and where every piece of a new model goes. The
multi-device base is in place: this describes it as built, then what a new model or family adds.

## The base, as built

- `src-tauri/src/driver/model.rs` (the descriptor types), `catalog.rs` (every model), `dock.rs`
  (the contract between the engine and a family's worker), `watcher.rs` (hot-plug over USB HID)
  and `virtual_dock.rs` (docks that exist only in the app).
- The engine (`src-tauri/src/engine/docks.rs`) runs one session per connected dock and binds each
  connection to a known dock.
- Configuration schema 3: known docks, a dock for every profile, the profile each dock shows.
- The plugin host tells each Stream Deck plugin of every dock and gives each instance its dock and
  key size; Ulanzi plugins get the model's name and the key's place in its grid.
- The UI draws the dock the editor shows from its model, lists every known dock, and moves or
  copies profiles between docks. MCP sees every dock with its layout.
- Only one model exists yet: `ulanzi.d200`, in the `ulanzi` family.

What is not there yet, and comes with the first model that needs it:

- Controls other than keys with a screen (dials, touch strips, buttons, information screens), and
  their events in the engine and the plugin SDKs (`dialRotate`, `setFeedback`…).
- Image formats other than PNG, and rotation or mirroring of images (`encoding.rs`).
- Transports other than HID (a shared `serial.rs`).
- Margins of widgets relative to the key's size: texts, borders and the status badges already
  scale with the key's height, widgets keep the D200's margins (`render/widgets.rs`).

## Rust layout

Follow the project's module rules: a module with submodules lives in its folder, `mod.rs` only
declares and re-exports, constants and logic live in files named for what they hold.

```
src-tauri/src/driver/
  mod.rs              declarations and re-exports only
  device.rs           DeviceInfo: identity read back from a connected device
  model.rs            the descriptor types
  catalog.rs          every model of every family; matching a USB device to a model
  dock.rs             requests to a worker, events from it, the Dock handle
  watcher.rs          finds docks, starts a worker for each one by its family
  virtual_dock.rs     docks of any model with no device, for development and e2e builds
  <family>/
    mod.rs            declarations and re-exports only
    protocol.rs       pure encoding of commands and decoding of input; no I/O
    connection.rs     the worker: identify, keep-alive, flush, read, until the device goes
    emulator.rs       the device's side, for tests (see testing.md)
    models/
      mod.rs          declares the family's model files
      <model>.rs      one hardware model: its DeviceModel and its variants
```

The `ulanzi` family holds the D200 (`connection.rs`, `protocol.rs`, `page_archive.rs`,
`hardware.rs`, `dock_check.rs`, `models/d200.rs`). It has no emulator: the D200 is checked on the
real device. Name new families after the protocol's owner, not a reseller. A new family is a new
variant of `Family` and a new arm in `watcher.rs`, which starts its worker.

A **model file** holds data only: the descriptor and its variants (rebrands with their own USB id
and name). Behaviour that differs between models of a family is a field of the descriptor read by
the family's code, never `if model == …` scattered through it. Add a field to `DeviceModel` when
a family needs one.

## The descriptor (`driver/model.rs`)

One `DeviceModel` per hardware model, built once in the catalog and never changed. The UI gets
them through `dock_models`, the engine and the plugin host read them, MCP describes them.

- `id`: `<family>.<model>` (`ulanzi.d200`), kept in configurations: never renamed.
- `name`, `family`, `variants` (each `{ vendor, name, usb: { vendor_id, product_id, interface } }`).
- `panel`: the face in pixels; `columns`, `rows`: the grid.
- `controls`: each `{ index, kind, cell: { row, column, row_span, column_span }, bounds }`.
  `index` is where its key is kept in a page (`Page.keys` is a map from it), unique within the
  model. `kind` is `Key { image: Size }` today. The D200 numbers its keys 0–13 as its firmware
  does, the wide key 13 spanning two columns.
- `sleep_minutes`: what the firmware offers; `firmware_press_effect`; `screensaver` (the device
  keeps a picture of the panel of its own).
- `plugins`: `elgato_type` (the Stream Deck with the same keypad, or 0) and `ulanzi_model` (the
  name Ulanzi manifests filter actions by).
- `stable_serial`, `confirmed_on_hardware`.

## The driver contract (`driver/dock.rs`)

- The watcher matches a USB device to a model by an exact variant, opens it, and starts the
  family's worker with the device, a `Receiver<Request>` and a `Reporter`.
- Events, each tagged with its connection: `Attached { model, dock }` as the worker starts,
  `Connected(DeviceInfo)` once the device told who it is (identity first, see safety.md),
  `Key { index, pressed }`, `Disconnected` when the worker ends. The watcher finds the device again
  when it comes back, as a new connection.
- Requests: brightness, sleep, set some keys, replace the page, store the screensaver. Images
  arrive rendered at the control's size. A newer image for a key replaces a queued one.
- Unknown input is ignored and logged, never an error. Inputs that are not a control of the model
  are dropped in the worker.

## Docks, profiles and pages (`config/devices.rs`, `engine/docks.rs`, `engine/transfer.rs`)

Decided by the owner, and built so:

- **A profile belongs to one physical dock** (`Profile.device`). Several docks run at once and
  never share or mix profiles.
- **Known docks** (`Config.devices`, synced): `{ id, model, name, serial, settings }`. A dock that
  connects is the known one of its model with its serial, else the first of its model nobody
  claimed yet (no serial), else a new known dock with a starter profile (`starter_profile`, which
  fits any model).
- **One physical dock is one known dock on every computer.** A new dock's id follows from its
  model and serial (`dock_id`), and its starter profile's ids from the dock's (`derived_id`), so two
  computers that meet it before they sync make the same dock and the same profile. Models whose
  serial cannot be trusted (`stable_serial: false`) get random ids instead.
- When the account's list arrives (`engine/shared_docks.rs`): two known docks of the same model
  and serial are joined into one (the first dock if one of them is it, else the smaller id), with
  every profile of both. One known dock that two different units took on two computers (the first
  dock, claimed by a D200 on each) is told apart: the smaller serial keeps it, the same choice on
  every computer, and the other unit becomes a dock of its own with a copy of every synced profile
  (ids derived, so every computer makes the same copies) and the profiles only that computer had.
  Nothing a dock had is lost on the way.
- A dock keeps its last profile when the cloud deletes it, unless another computer forgot the dock
  and it is not connected here; a connected dock left without profiles gets a starter one.
- `FIRST_DOCK` is the D200 of configurations from before: the same id on every computer, claimed
  by the first D200 that connects. Schema 2 migrates to it (`devices::migrate`).
- `Config.active_profiles` (this computer only) says which profile each dock shows; without one it
  shows its first.
- **Docks that are not connected stay visible** with their profiles, and can be edited, renamed
  and forgotten (forgetting drops their profiles, past undo).
- **Moving or copying a profile to another dock** is `Change::TransferProfile`, one step of undo.
  Keys go to the control in the same cell of the other model's grid; those it has no place for are
  left out. `transfer_plan` tells before what will be left out and what will not work.
- **Keys whose action the dock cannot run stay, with an alert**: worked out on the fly
  (`key_runs_on`, today the plugin manifest's `Controllers` and `Devices`), never stored. The dock
  draws `Layer::Unsupported` on them and the snapshot lists them (`unsupported`), so moving the
  profile back clears the alert by itself.
- A key only switches to a profile of its own dock; an app is linked to one profile per dock.
- The engine keeps a session per connected dock (screen, gestures, the handle) and a focused dock,
  the one the editor shows: commands without a dock (creating a page or a profile from the UI) are
  about it. Navigation, second states, widgets and results are keyed by page, which belongs to one
  dock.
- Cloud: `SharedSettings.devices` is the list of known docks; `device` stays the first dock's
  settings for versions that knew one. Docks connected here or with profiles here survive a list
  that lacks them.

## Plugin SDKs

- **Stream Deck and Stream Dock** (`plugins/elgato`): `info.devices` lists every known dock
  (`PluginDock`); `deviceDidConnect` / `deviceDidDisconnect` follow connections
  (`PluginHost::set_docks`); each event has the instance's dock and the key's size, carried inside
  the host as `_device` and `_size` (stripped for Ulanzi plugins by `for_plugin`).
- **Ulanzi** (`plugins/ulanzi`): keys as `column_row` from the control's cell (`model.key_name`);
  `device` is the model's Ulanzi name; the manifest's `Devices` filter goes through `runs_on`.
- Actions list the models they fit (`PluginActionInfo.models`).

## UI and MCP

- UI: `lib/dock.ts` (key sizes, shapes, placement, names), `DockGrid` / `DockKey` drawn from the
  focused model's face, `DeviceList` in the sidebar (focus, rename, forget, the warning of untested
  models), `TransferProfileDialog`, the alert badge on unsupported keys.
- MCP: `get_overview` lists docks with their `layout` text and profiles; `set_device_settings`
  and `rename_dock` take a dock; `apply_changes` has `transfer_profile` and `create_profile` with a
  dock; `render_page` draws any model's face.
