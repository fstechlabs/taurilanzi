# Safety

The driver ships to users whose device nobody on the project has touched. It must never harm a
device, never talk to the wrong device, and never leave one in a bad state.

## Only commands that cannot change persistent memory

- Each family's commands are a closed Rust enum in its `protocol.rs`; the encoder cannot build
  anything else, and nothing writes raw bytes to a device from outside it.
- Allowed: what changes what is shown or volatile state: images, clearing, brightness, LEDs,
  sleep and wake, keep-alive, reading identity, a mode switch the research shows is volatile.
- Never sent, in any family, whatever a project or SDK does:
  - firmware updates;
  - boot logo or anything written to ROM or flash (Mirabox `LOG`; Elgato `0x0D`, which stores a
    background and may write flash);
  - stored configuration (Mirabox `QUCMD`);
  - commands whose purpose the research could not establish;
  - values the research marks as harmful (brightness 0 on the Logitech MX keypad resets it);
  - anything over the D200's ADB shell, or any debug interface.
- The fact sheet lists these commands under "Commands never sent".

## Talk only to the right device

- Match on the exact USB identity of a variant: VID and PID together, plus interface or usage
  page, plus the product string when it helps. Many Mirabox VIDs are squatted by several
  unrelated makers: a VID alone, or a PID alone, is never enough.
- After opening, **read the identity first** (serial, firmware, model string, device info reply)
  and check it against the model. Only then write. If it does not match, close the device, write
  nothing, and show it as not supported.
- Serial-port devices: open only ports whose USB VID/PID match a variant. Never open a port by
  name or send a handshake to an unknown port.
- Several devices can match at once: each gets its own worker; never assume one.

## Bounded and polite

- Report sizes, image byte limits, chunk counts and rates come from the descriptor; the encoder
  refuses anything larger. Images are re-encoded smaller rather than sent too big.
- Keep-alive at the period the research gives, for models that reset without it.
- On close, hand the device back clean (Elgato "show logo", Mirabox clear), as documented.
- Respect minimum delays documented between commands.

## Fail safe

- Unknown or malformed input is ignored and logged, never a panic or an error that stops the
  worker.
- A device that stops answering or errors is closed and retried later, as the D200 is today.
- Never write in a loop on error.
- Input-only devices and input parsing cannot harm a device: mistakes there show as wrong keys,
  not damage.

## Sharing the device with other apps

- macOS: hidapi opens devices exclusively; if another app holds it, opening fails. Show that
  the dock is in use by another app.
- Windows: HID handles are shared. If the vendor's app (Elgato Stream Deck, Ulanzi Studio,
  Stream Dock…) is running, both would draw on the device. Detect it and tell the user to quit it
  or disable the device in it, instead of fighting over the device.
- Linux: add the variant's USB ids to `src-tauri/packaging/linux/70-taurilanzi.rules`; for serial
  devices also tell ModemManager to ignore them.
