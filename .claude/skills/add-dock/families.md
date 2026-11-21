# What is known about each family

The state of the research of November 2026, as a starting point: every fact here must still be
checked against the sources during the research step, and new projects searched for. Confidence:
**[H]** official documentation or several independent sources agree, **[M]** projects agree with
no official documentation, **[L]** one source only or sources disagree (an open fact).

## Priorities

Estimated share (mean of the share of Amazon EU reviews and of Amazon US monthly sales, November
2026; rough figures). Elgato is about 78 % of it, Mirabox-made clones about 18 %.

| Step | Models | Share | Cumulative |
|---|---|---|---|
| 1 | Elgato MK.2, XL, Neo, Pedal (keys only) | 48 % | 48 % |
| 2 | Elgato Mini (once its BMP orientation is settled) | 15 % | 63 % |
| 3 | Encoders and strips: Elgato +, + XL; Ulanzi D200X, D200H | 15 % | 79 % |
| 4 | Mirabox 293V3, 293S, N3 | 10.5 % | 89 % |
| 5 | Mirabox M18, N4 | 7.4 % | 97 % |
| 6 | Loupedeck / Razer, Logitech MX Keypad | ~1 % | ~97.5 % |

## Ulanzi (`ulanzi`)

Ulanzi's own hardware: a Rockchip SoC running Linux, not Mirabox's. USB `2207:0019` for the
D200, D200H and D200X. Already driven for the D200 (`driver/d200/`, to move to `driver/ulanzi/`).

- Framing: report ID `0x00` + 1024 bytes; `7C 7C`, 2-byte command (big-endian), 4-byte length
  (little-endian). Pages go as a ZIP with a JSON manifest and PNGs; a padding entry first keeps
  continuation reports from starting with `0x00` or `0x7C`. Device info reply `0x0303` (JSON with
  `SerialNumber`, `DeviceType`, `Dversion`, `HardwareVersion`).
- D200: 13 keys of 196 px, wide window 452×196 (index 13). [H, our own driver]
- D200X: 14 screens (13 is the wide window), page buttons on inputs 15 and 16, dials on 17–19.
  A dial input has payload byte 2 = `0x02`; byte 3 is 0 release, 1 press, 2 left, 3 right. [M]
  Open: the `DeviceType` string that tells it from a D200. Sources:
  `edubox/opendeck-ulanzi-d200x`, `bitfocus/companion-surface-ulanzi-stream-controller`,
  `Tyaaa-aa/Ulanzi-Deck-Linux`, Ulanzi's plugin SDK (`UlanziTechnology/UlanziDeckPlugin-SDK`,
  dial canvas 126×140 per dial).
- D200H: the D200 plus a USB hub for one source, a different protocol for another. [L, open]
  Sources: `glmagalhaes/rs-ulanzi-d200`, `aleyvag/ulanzi-d200h-linux`.
- Dial D100H / Dial Mini: `fff1:0082`, Bluetooth LE, 20-byte messages encrypted with a fixed TEA
  key; no display. [M] A separate transport. Sources: `kriomant/ulanzi-d100h`,
  `brendanwelsh/ulanzi-d100h-homebrew`.
- Possibly useful D200 commands: `0x000F` / `0x0010` lock and unlock the screen; `0x0003`, whose
  reply works as a barrier around uploads; an unsolicited `0x010B` after the firmware's UI crashes,
  answered by sending everything again. [L]
- The D200 has an open root ADB shell: never use it.

## Elgato (`elgato`)

Official HID documentation: `https://docs.elgato.com/streamdeck/hid/intro`. VID `0x0FD9`
(Galleon: Corsair `0x1B1C`). Projects: `Julusian/node-elgato-stream-deck` (MIT, most complete),
`abcminiuser/python-elgato-streamdeck` (MIT), `OpenActionAPI/rust-elgato-streamdeck` (MPL-2.0;
known wrong sizes for the + XL), `bitfocus/companion-surface-elgato-stream-deck` (built on the
node library, not independent of it). Elgato's app has, since 7.1, a switch to disable a device
for third-party apps.

| Model | PID | Gen | Keys | Key px | Image | Extras |
|---|---|---|---|---|---|---|
| Original 2017 | `0x0060` | G1 old | 5×3 | 72 | BMP, 180°, columns mirrored, two halves numbered from 1 | not in the official docs [M] |
| Original 2019, MK.2, MK.2 Scissor | `0x006D`, `0x0080`, `0x00A5` | G2 | 5×3 | 72 | JPEG, 180° | [H] |
| 15-Key Module | `0x00B9` (scissor `0x00E4` [M]) | G2 | 5×3 | 72 | JPEG, 180° | [H] |
| Mini, Mini MK.2, Mini Discord | `0x0063`, `0x0090`, `0x00B3` | G1 mini | 3×2 | 80 | BMP | orientation open [L] |
| 6-Key Module | `0x00B8` | G1 mini | 3×2 | 80 | BMP | as the Mini |
| XL, XL v2 | `0x006C`, `0x008F` | G2 | 8×4 | 96 | JPEG, 180° | [H] |
| 32-Key Module | `0x00BA` | G2 | 8×4 | 96 | JPEG, 180° | [H] |
| Neo | `0x009A` | G2 | 4×2 | 96 | JPEG, 180° | info screen 248×58; touch points are keys 8 and 9, LEDs only [H] |
| + | `0x0084` | G2 | 4×2 | 120 | JPEG, no rotation | 4 dials, strip 800×100 [H] |
| + XL | `0x00C6` | G2 | 9×4 | 112 | JPEG, 90° counter-clockwise | 6 dials, strip 1200×100; strip coordinates stay logical [H] |
| Pedal | `0x0086` | G2 input only | 3 | — | — | [M] |
| Studio | `0x00AA` | G2, USB and Ethernet | 16×2 | open [L] | JPEG | 2 dials with 24-LED rings, NFC |
| Network Dock | TCP only | — | — | — | — | one source [L] |
| Galleon 100 SD | `1B1C:2B18` | G2 variant | 3×4 | 160 | JPEG | 720×384 screen, 2 dials; keep-alive every 500 ms [M] |

- G2 framing: real report IDs, no extra `0x00`. Output ID `0x02`, 1024 bytes; input ID `0x01`,
  read 512; feature reports up to 32 bytes. Key image command `0x07`: key index, last-chunk flag,
  chunk length and chunk index (little-endian) in an 8-byte header, up to 1016 bytes of JPEG per
  report. `0x08` whole panel, `0x0B` whole strip or info screen, `0x0C` a strip rectangle.
- G2 input: `0x00` keys (one byte per key), `0x02` strip gestures (tap, long press, swipe with
  coordinates; no touch down/up), `0x03` dials (press states, or signed ticks per dial).
- G2 feature reports: brightness 0–100, reset to logo, fill panel or key with a colour, firmware
  sleep timeout, firmware versions, serial, unit info (rows, columns, key size, LCD size).
- G1 mini: 16-byte header, BMP, key index + 1, 1024-byte reports; 17-byte feature reports.
- JPEG: baseline, 4:4:4 subsampling (4:2:0 smears colours at these sizes), quality 90–95. Send
  "show logo" when closing.
- The + does not restore brightness after the computer wakes: set it again.
- Never: `0x0D` (stores a background, may write flash).
- Plugin SDK (`elgatosf/streamdeck`, MIT) device types: 0 Stream Deck, 1 Mini, 2 XL, 3 Mobile,
  4 Corsair G-Keys, 5 Pedal, 6 Corsair Voyager, 7 +, 8 SCUF, 9 Neo, 10 Studio, 11 Virtual,
  12 Galleon 100 SD, 13 + XL. Controllers `Keypad`, `Encoder`, `Neo`.

## Mirabox / HotSpot (`mirabox`)

One OEM (HotSpot, Shenzhen) sold as Ajazz, Soomfon, Mars Gaming, Redragon, VSDinside, Fifine,
TreasLin, Streonor, Womier, TMICE, Risemode, GK, MonstarDeck and more. HID product string
"HOTSPOTEKUSB HID DEMO". No official protocol documentation (Mirabox refuses it); its device SDK
(`MiraboxSpace/StreamDock-Device-SDK`, MIT source, closed transport binaries) gives per-model
facts. Projects: `4ndv/mirajazz` (MPL-2.0) and the OpenDeck plugins built on it (one source with
it), `bitfocus/companion-surface-mirabox-stream-dock` (MIT), `mishamyrt/ajazz-sdk`,
StreamController's device library, `kriomant/mirabox`, per-model OpenDeck plugins
(`opendeck-akp05`, `opendeck-m18`, `opendeck-stream-dock-xl`, `opendeck-ss550`,
`opendeck-mirabox293v3`, `opendeck-mirabox-n1`, `opendeck-ajazz-n1`, `opendeck-magtran-m3`;
check which are built on `mirajazz`).

- Framing: report ID `0x00` + 512 or 1024 bytes out; input always 512. Vendor collection on usage
  page `0xFFA0`, interface 0 (interface 1 is a keyboard). Commands start with `CRT\0\0` and an
  ASCII name: `DIS` wake, `LIG` brightness, `BAT` announces an image (length, key) followed by the
  raw JPEG in report-sized chunks and `STP` to commit, `CLE` clear one or all, `HAN` sleep,
  `CONNECT` keep-alive, `MOD` mode, `LBLIG` / `SETLB` LEDs, `BGPIC` / `BGCLE` RAM background on
  newer models.
- Protocol versions (as `mirajazz` names them): pv0/pv1 512-byte output, presses only (synthesise
  releases); pv2 1024 bytes, presses only, clears need a commit; pv3 1024 bytes, presses and
  releases.
- Input: `ACK` … `OK`, then a code and a state; each layout has its own code table. Code `0xFF`
  only acknowledges a write.
- Keep-alive every 5–10 s: some models reset after 25–35 s of silence. JPEGs under about 9 KB.
- Identity: VID+PID pair plus usage page, then the firmware string (feature report `0x01`; on
  Windows through an input report). pv1 serials are useless (always the same). Many VIDs are
  squatted.
- Never: `LOG` (boot logo to ROM, can brick), `QUCMD` (persistent configuration), unknown commands.
- Known bugs in `mirajazz`: first press after connecting swallowed, keys latched when a release is
  lost, acknowledgements not filtered. No n-key rollover on the XL and N4 Pro.

| Layout | Variants (VID:PID) | Controls | Images | pv |
|---|---|---|---|---|
| 293 | Stream Dock 293 `5500:1001` | 15 keys | 100×100, 180° | 512 |
| 293S | HSV293S / Soomfon Studio `5548:6670`; AKP153 `5548:6674`; AKP153E/R `0300:1010` / `1020` (rev 2 `0300:3010` / `3011`, pv3); HSV293SV3 `6603:1014`; Soomfon XF-CN001 `1500:3003`; Streonor S15 `1500:3005`; Mars MSD-ONE `0b00:1000` (v3 `0b00:1005`); GK150K `0c00:1000`, Risemode Vision 01 `0a00:1001`, TMICE `0500:1001`, Womier D15 `0600:1000`, MonstarDeck TS115 `0400:1000` | 15 keys + 3 side screens; column-major wire order from the right | 85×85 or 96×96, rotated 90°; rev 2 95 vs 96 [L] | pv1 / pv3 |
| 293V3 | 293V3 / V3EN / V25 `6603:1005` / `1006` / `1010`; Redragon SS-550 `0200:1000` (v3 `0200:3000`); Fifine D6 | 15 keys | 112×112 (100 on SS-550), 180° | pv3 |
| N3 | Mirabox N3 `6602:1000` / `1002`, `6603:1002` / `1003`, N3V2 `eeef:2929`; AKP03/E/R `0300:1001`–`1003` (rev 2 `0300:3002` / `3003`); Soomfon SE `1500:3001`; Mars MSD-TWO `0b00:1001`; TreasLin N3 `5548:1001`; Redragon SS-551 `0200:2000` | 6 keys, 3 buttons, 3 knobs | 60×60 (pv2), 64×64 rotated (pv3) | pv2 / pv3 |
| N4 | N4 `6602:1001`, N4E `6603:1007`, N4 Pro `5548:1008` / `1021`; VSDinside N4 `5548:1004`, N4 Pro `5548:1023`; AKP05 family `0300:3004` / `3006` / `3013` / `3014`; Mars MSD-Pro `0b00:1003`; Soomfon CN003 `1500:3002`; Streonor S10 Pro `1500:3007`; Redragon SS-552 `0200:3001` | 10 keys, strip in 4 segments, 4 knobs | keys 112×112, 180°; segments 176×112 vs 176×124 [L] | pv3 |
| M18 | `6603:1009` / `1012`, VSDinside `5548:1000` | 15 keys, 3 buttons, 24 RGB LEDs | 64×64 vs 60×60 [L] | pv3 |
| XL | `5548:1028` / `1031` | 32 keys, 2 rockers | 80×80, 180° | pv3 |
| N1 | `6603:1000`, Ajazz `0300:3007`, VSD/TreasLin `5548:1002` | portrait 15 keys, 3 small screens, knob | four different sizes in the sources [L]; mode 2 or 3 [L] | — |
| Others | M3 / MagTran `5548:1020` / `1032` / `1038`; Mini `5548:1036` / `1037`; H1 Pro `5548:1030` / `1033`; keyboards K1 Pro (report ID `0x04`) and AKP815 `5548:6672` | | thin sources | |

Open: `6603:1005` is the 15-key 293V3 for two sources and the 18-key HSV293SV3 for another; the
firmware string tells them apart.

## Loupedeck (`loupedeck`)

Loupedeck Live `2ec2:0004`, Live S `2ec2:0006`, CT `2ec2:0003` / `2ec2:0007`, Razer Stream
Controller `1532:0d06` (a Live), Razer Stream Controller X `1532:0d09`. Hardware discontinued in
2025. Projects: `foxxyz/loupedeck` (MIT), `Julusian/node-loupedeck` (MIT), `RadiatorTwo/LoupixDeck`
(MIT, measured on hardware), `scottlaird/loupedeck` (Apache-2.0), `GuyOnWifi/loupedeck-rs` (GPL).

- USB CDC serial. After opening, the host sends a fake HTTP WebSocket upgrade and waits for a reply
  starting `HTTP/1.1`; then WebSocket binary frames with a zero mask. Messages: length, command,
  transaction id (1–255, never 0), data; each command is acknowledged; events carry id 0.
- Commands: button colours, serial, version, brightness 0–10, refresh, write a framebuffer
  rectangle (RGB565 little-endian; the CT wheel big-endian), vibrate (a DRV2605 effect).
- Events: press/release, rotate (signed), touch start/move/end with coordinates and an id. Keys are
  regions of the touch screen (Live: 4×3 of 90 px between two 60 px strips on 480×270).
- Open: framing of 126–255-byte messages; key pitch 80 vs 90; Stream Controller X geometry.
- Linux: the Stream Controller X also queues keys on a HID interface that must be read or keys
  stall; ModemManager must ignore these ports. Never the native per-key haptic command `0x2e`
  (crashes the firmware).

## Logitech (`logitech`)

MX Creative Keypad / MX Keypad `046d:c354`: wired, HID++ 4.5, vendor collection on usage page
`0xff43`. 480×480 panel, 9 keys of 118×118; JPEG tiles placed at a position in 4095-byte reports,
acknowledged; the first image after a restart is refused, retry. Keys arrive as the list held;
page buttons and brightness through HID++ features. Never brightness 0 (resets it). MX Dialpad
`046d:bc00`: Bluetooth only, no display; macOS asks for Input Monitoring. Projects:
`AprilNEA/OpenLogi`, `hawkish/mx-keypad` (MIT OR Apache-2.0), `Julusian/node-logitech-mx-creative-console`,
`logilinux`, `keyvo`. Open: the MX Keypad's PID (inferred); a Windows LCD bug in one project.

## Long tail

Mountain DisplayPad `3282:0009` (12 keys of 102 px, four projects); Corsair iCUE Nexus
`1b1c:1b8e` (640×48 strip, four projects, pixel order open); Cooler Master MasterHub, Yamaha
CC121MK2, Infinitton iDisplay, AVerMedia AX310 (one source each: not yet). Input-only devices
(X-keys, Contour Shuttle, TourBox, Blackmagic Speed Editor, Xencelabs Quick Keys) fit poorly in an
app of keys drawn as images. Phone apps (Stream Deck Mobile, Touch Portal) use proprietary,
encrypted protocols: not supported.
