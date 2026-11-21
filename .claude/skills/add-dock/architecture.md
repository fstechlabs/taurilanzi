# Architecture for many docks

How Taurilanzi supports many dock models at once, and where every piece of a new model goes.
The first part describes the multi-device base; the second, what each model and family adds.

## Is the base in place?

The base exists when `src-tauri/src/driver/model.rs` (the descriptor types) and
`src-tauri/src/driver/catalog.rs` (the list of models) exist and the engine runs one session per
connected device. If they do not, the app still assumes one D200: build the base first (below),
as its own change, and check on the D200 that nothing changed for it. Only then add models.

What the app assumed before the base (to find and replace):

- `driver/d200/hardware.rs` constants (`COLUMNS`, `KEY_COUNT`, `WIDE_KEY`, `KEY_SIZE`,
  `WIDE_KEY_SIZE`) imported across the engine, the plugin protocols, the screensaver, MCP and
  the config defaults; the same numbers (13, 5, 196, 452, 1220×708, 256, `"3_2"`) written as
  literals in the UI (`DockGrid.vue`, `DockKey.vue`, `KeyInspector.vue`,
  `ImageEditorDialog.vue`, `image-crop.ts`, `SceneCard.vue`), MCP (`tools/reading.rs`,
  `tools/rendering.rs`, `guide.md`, `instructions.md`) and the tests.
- `Page.keys: BTreeMap<u8, Key>` in D200 order; profiles with no device; one `DeviceSettings`
  shared by every computer through the cloud.
- `engine/runtime.rs`: one `D200` handle, one screen, one navigation, one pause; state keyed by
  `(page, u8)`; `check_index` accepting 0–13.
- `driver/d200/connection.rs`: `Event::Key {index, pressed}` only; the first matching device
  only. A D200X dial turn (input 17–19, state 2 or 3) is read as a key release.
- Plugin host: the Elgato side announces one device `TAURILANZI-D200`, type 0, 5×3, and drops
  dial events and `setFeedback`; the Ulanzi side sends `device: "D200"`, `controller: "Keypad"`;
  actions are filtered with `supports_device("D200")`.
- Renderer: `render(design, size)` already works at any size, but text sizes and the safe margin
  are pixels of a 196 px key, and the output is PNG only.

## Rust layout

Follow the project's module rules: a module with submodules lives in its folder, `mod.rs` only
declares and re-exports, constants and logic live in files named for what they hold.

```
src-tauri/src/driver/
  mod.rs              declarations and re-exports only
  device.rs           DeviceInfo: identity read back from a connected device
  model.rs            descriptor types (DeviceModel, Control, ControlKind, Surface, …)
  catalog.rs          every model of every family; matching a USB device to a model
  driver.rs           what every family implements, the requests and the events
  watcher.rs          hot-plug: enumerates, matches the catalog, opens one session per device
  encoding.rs         RGBA to PNG / JPEG / BMP / RGB565, with rotation and mirroring
  virtual_dock.rs     virtual devices of any model, for development and e2e builds
  hid.rs              shared hidapi access (one HidApi for all families)
  serial.rs           shared serial-port access, only once a family needs it
  <family>/
    mod.rs            declarations and re-exports only
    protocol.rs       pure encoding of commands and decoding of input; no I/O
    connection.rs     the worker: open, identify, keep-alive, flush, read
    emulator.rs       the device's side, for tests and virtual docks
    models/
      mod.rs          declares the family's model files and lists them
      <model>.rs      one hardware model: its DeviceModel and its variants
```

Families today: `ulanzi` (the current `driver/d200/` moves here when the base is built; its
`dock_check.rs` stays with it), `elgato`, `mirabox`, `loupedeck`, `logitech`. Name new ones after
the protocol's owner, not a reseller.

A **model file** holds data only: the descriptor and its variants (rebrands with their own USB id
and name). Behaviour that differs between models of a family is expressed as fields of the
descriptor (report size, protocol version, keep-alive, input code table) read by the family's
code, never as `if model == …` scattered through it.

## The descriptor

One static `DeviceModel` per hardware model, never changed at runtime. The UI (through the
generated bindings), the MCP schema, the config rules, the plugin host and the driver all read it.

- `id`: stable string, persisted forever in configurations: `<family>.<model>`
  (`ulanzi.d200`, `ulanzi.d200x`, `elgato.mk2`, `elgato.plus`, `mirabox.n3`…). Never renamed.
- `name`, `family`.
- `variants`: each `{ vendor, name, usb: { vid, pid, interface or usage page, product string? } }`.
- `surfaces`: framebuffers `{ id, pixel size, format (png | jpeg | bmp | rgb565…), rotation,
  mirror, how it is updated (per control | whole page archive | whole frame) }`.
- `controls`: each with
  - a stable id within the model: `key/<n>`, `dial/<n>`, `strip/<n>`, `touch/<n>`, `info/<n>`,
    `button/<n>`;
  - a grid cell `{ row, column, row_span, column_span }` (the D200 wide key spans two columns)
    and its bounds on the face;
  - a kind: key (with or without a display, with or without an LED), encoder (press, LED ring,
    its part of a strip), touch strip (tap, long press, swipe), touch point, information screen
    (display only), plain button;
  - the region of a surface it draws into, if it has a display.
- `face`: an SVG of the device for the editor, drawn by us.
- Identities for each plugin SDK: Elgato device type and keypad size, Ulanzi model name
  (`D200`, `D200H`, `D200X`, `Dial`), Mirabox type; and each control's controller name in each
  SDK (Elgato `Keypad` / `Encoder` / `Neo`; Ulanzi `Keypad` / `Encoder`; Mirabox `Keypad` /
  `Knob` / `SecondaryScreen` / `Information`).
- Behaviour the engine must know: the firmware draws its own press effect, brightness range,
  sleep values, frame budget, keep-alive period, whether the serial number is stable.
- `confirmed_on_hardware: bool`.

## The driver contract

Each family implements one contract, so the engine never knows a protocol:

- **Start** with the model and the matched USB device. The worker opens it, **reads its identity
  first** and refuses to go on if it does not match the model ([safety.md](safety.md)).
- **Requests** from the engine: set the image of a control (already rendered at that control's
  size), set LED colours, brightness, sleep, clear, close. A newer image for a control replaces a
  queued one, as the D200 driver already does; families that upload whole pages batch them.
- **Events** to the engine, tagged with the device: connected (with `DeviceInfo`), disconnected,
  and input by control id: key down/up, dial press/release/turn (signed ticks), strip tap / long
  press / swipe with coordinates, touch.
- Unknown input is ignored and logged, never an error.
- One worker per connected device; the watcher opens every matching device, not just the first.

## Devices, profiles and pages

Decided by the owner:

- **A profile belongs to one physical device.** Two D200s connected at once each have their own
  profiles. Profiles are never shared or mixed between devices.
- **Devices that are not connected stay visible**, with their profiles, so they can be edited
  and their profiles moved, for example to a new dock that replaces a broken one.
- **Moving or copying a profile to another device is one button**, as simple as possible:
  - if the target is fully compatible (same layout and every action supported), it moves as is;
  - otherwise each key is checked against the target: a key on a control the target does not
    have is removed; a key the target has but whose action or widget it cannot run stays, with
    an alert on that key;
  - what will be removed and what will carry an alert is shown before confirming;
  - it is a `Change` in `engine/batch.rs`: all or nothing, one step of undo.
  - Example: a D200 profile moved to a D200X, edited there, and moved back; its dial keys are
    removed on the way back.

How that lands in the data:

- Config gains **known devices**: `{ id, model, name, serial, brightness, sleep, active profile }`
  for every device ever seen. Brightness and sleep move out of the shared `DeviceSettings` into
  each device.
- A device's id is model + serial when the model's serial is stable. When it is not (some
  Mirabox units report a fixed serial, some Ajazz a new one on every connection), fall back to
  the USB port path; moving profiles covers the rare case where that changes.
- Each profile has a `device` (a known device id). Pages map a control id to a key, so dials,
  strips and buttons get slots.
- Migration (a schema bump in `config/model.rs`, read in `config/store.rs`): today's profiles go
  to one known D200 entry, which the first D200 to connect claims (taking its serial).
- Runtime state (active profile, page, navigation, pause, screensaver, live widgets, gestures,
  second states, counters) is kept per connected device.
- Cloud (`src/cloud`): profiles sync with their device id and known devices sync too, so another
  computer shows them as not connected. The API is a contract with installed versions: only
  additive changes.
- Rules (in Rust, `config/rules` and the engine's `check_key`) check control ids against the
  profile's device model and say whether an action can run on a control. The compatibility check
  used when moving profiles reuses them.

## Plugin SDKs

- **Elgato and Stream Dock** (`plugins/elgato`, `plugins/mirabox`): every connected device in the
  registration info's `devices[]` with its real type and keypad size (columns × rows of keys
  only); `deviceDidConnect` / `deviceDidDisconnect` on hot-plug; the device in every context;
  coordinates from the control's cell. A clone takes the type of the Elgato model with exactly
  its keypad shape, otherwise 0. Encoder events (`dialRotate`, `dialDown`, `dialUp`, `touchTap`)
  and `setFeedback` / `setFeedbackLayout` implemented, layouts drawn on Elgato's canvas (200×100
  per dial; 232×50 for the Neo bar) and scaled to the device.
- **Ulanzi** (`plugins/ulanzi`): keys as `col_row` from the cell; the manifest's `Devices` filter
  (with `~` exclusions) against the model's Ulanzi name; dial events and the D200X dial canvas
  (126×140 per dial).
- An action is offered only on controls whose controller is in its manifest's `Controllers`.

## UI, MCP, renderer

- UI (`src-nuxt/app`): the dock is drawn from the descriptor (face, cells, spans, control kinds);
  a device switcher lists connected and known devices; the move/copy dialog; the alert badge on
  incompatible keys; the "not confirmed on hardware" warning.
- MCP (`src/mcp`): tools take the device; layout text, `render_page` geometry, `guide.md` and
  `instructions.md` are generic and read the descriptor; `get_schema` comes from the bindings.
- Renderer: text sizes and margins relative to the control's size; output encoded per surface
  (`driver/encoding.rs`). Images are always drawn whole on the computer.
- Virtual docks replace `TAURILANZI_NO_DOCK` ([testing.md](testing.md)).
