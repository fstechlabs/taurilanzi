# Checking a driver without the device

Only the D200 can be tried on real hardware. Everything else is checked in four layers. They
catch mistakes in implementing the protocol; mistakes in understanding it are prevented by the
rule of sources ([research.md](research.md)), not by tests.

## 1. Protocol bytes

Pure tests of the family's `protocol.rs`, written from our own description in the fact sheet:

- each command encodes to the documented bytes (header fields, byte order, padding, report ID);
- an image of a given size splits into the documented chunks, with the right indices and flags,
  and the last one marked;
- each documented input report decodes to the right event, and unknown or short reports are
  ignored;
- the encoder cannot produce a forbidden command (there is no variant for it), and refuses
  oversized images.

Never build these from captures of real traffic: none are committed.

## 2. The emulator

Each family has `emulator.rs`: the device's side of the protocol, written from the fact sheet,
plugged into the driver in place of the real transport.

- It accepts only what the documented device accepts and records misuse (a write before the
  identity was read, a command it does not know, a chunk out of order, an image too large, a
  missing keep-alive).
- It reassembles images, decodes them, undoes the documented rotation and mirroring, and keeps
  the picture each control shows.
- It answers identity requests and produces input reports for presses, dial turns and touches.

Driver tests run the real worker against it: an image drawn with a marker in one corner must land
on the right control, the right way up; a press on a control must reach the engine as that
control; disconnecting and reconnecting must recover; nothing forbidden is ever sent.

## 3. Virtual docks

`driver/virtual_dock.rs` runs any model in development and e2e builds, through its emulator, with
no USB. It replaces `TAURILANZI_NO_DOCK` (see the environment variables in `CLAUDE.md`: keep that
section up to date with whatever variable selects the virtual docks).

- The UI, the editor, MCP, plugins, moving profiles and the warning are tried on every model
  this way, by the user (step 5 of the skill) and by e2e specs.
- e2e specs that only need a dock use the D200 model; specs about a new model or several devices
  use virtual docks of those models. Screenshots of new faces go through `shot()` for review.

## 4. The D200

The real dock checks the base and the shared code:

- after the base is built, and after any change to shared driver code, the D200 must behave
  exactly as before: `dock_check` and `frame_rate` (commands in `CLAUDE.md`), run only when the
  user agrees, with Ulanzi Studio and the app closed;
- the D200 plus a virtual dock at once exercises several devices together.

## What the CI covers

`.github/workflows/tests.yml` runs clippy and the Rust tests on macOS, Ubuntu and Windows, the
UI unit tests, and the e2e suite on macOS. Byte and emulator tests are Rust tests, so they run on
all three systems: for most models they are the only check before a user plugs one in.
