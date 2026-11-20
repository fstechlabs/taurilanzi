# Building keys

How to set up the Ulanzi D200 through these tools so every change is accepted the first time and
looks like what the app's own editor makes. The exact JSON of every type is in `get_schema`; this
guide says which combinations are valid and gives working examples.

## Workflow

1. `get_overview` first: profiles, pages (with a summary of their keys), links between pages,
   integrations and their status, and the page on the dock. Ids come from here.
2. `get_page` or `get_key` before changing a key that exists, and change what you read: keys hold
   fields you would otherwise lose (plugin settings, hidden secrets, waypoints).
3. Build several keys, pages or profiles with one `apply_changes` call: all or nothing, and one Undo
   takes it all back. Name what you create (`"name": "$scenes"`) and use that name later in the
   same call, also inside keys.
4. Check the result with `render_page` (or `render_key`) and fix what does not read well. Then tell
   the user what you made and where (profile, page, key), and that Undo in the app takes it back.

- Write every name and key text in the user's language (`language` in `get_overview`).
- Never ask the user for passwords, tokens or codes in the conversation. Secrets read as a
  placeholder: send it back unchanged to keep the value. When OBS, Home Assistant or a plugin needs
  one, ask the user to type it in the app (Settings → Integrations, or the plugin's own settings).
- You cannot install plugins. Use `search_plugin_store`, then `recommend_plugin` to show one to
  the user; once installed, `list_plugins` shows its actions.
- `press_key` really runs the key (opens apps, presses shortcuts, changes OBS). Use it only when the
  user wants to try a key.
- Refused changes say which change and why (`change 2: actions[0] (open_url): url must…`). Fix that
  field and send the call again: nothing of a refused call was applied.

## The device

```
 0   1   2   3   4
 5   6   7   8   9
10  11  12  [ 13 wide ]
```

- Keys 0-12 are 196×196 px. Key 13 is the wide key, 452×196 px, over the last two slots of the
  bottom row.
- A page holds keys by index; missing indexes are empty slots.
- The firmware shrinks a key while it is held down. Never add a press effect.
- Every new page gets a back key in slot 0. Keep it, or replace it with another way out (`back` or
  `home`).
- Pages belong to a profile. Each profile has a home page, where the dock starts and where `home`
  leads. The dock shows one profile at a time. A profile linked to apps comes to the dock when one
  of those apps comes to the front.

## Anatomy of a key

```text
{
  "kind": "action",
  "appearance": { "background": { … }, "layers": [ … ] },
  "actions": [ … ],
  "double_press": [],
  "long_press": [],
  "second_state": null
}
```

- `actions` run in order on a press. `double_press` and `long_press` run instead of `actions` for
  those gestures (a key with a double press waits a moment before a single press acts).
- `second_state` is `{ "appearance": …, "actions": [ … ] }`: a second look and list of actions.
- Fields you leave out take their defaults (empty lists, no second state). A key without
  `appearance` takes the look of new keys (the user's preference), with no layers.

### Appearance

- `background`: `{ "type": "solid", "color": C }` or
  `{ "type": "linear_gradient", "from": C, "to": C, "angle": 90 }`. `angle` is 0 to 360 degrees:
  0 goes left to right, 90 top to bottom.
- Colours are `{ "r": 0-255, "g": 0-255, "b": 0-255 }`, no alpha.
- `layers` are drawn in order, the first at the back. Keep this order, as the editor does:
  **one image, then the widgets, then the text, then anything else (borders, a second text)**.
  Widget layers must be together, either first or right after a single image layer.

Layers:

- Image: `{ "type": "image", "asset": "<asset id>", "fit": "contain" | "cover", "inset": 0.22,
  "tint": C | null, "icon": "mdi:github" | null }`. `asset` must already be in the store: get it
  from `import_icon` (which answers the whole layer, ready to use), `import_image`, or a key that
  has it. `inset` is the margin around the image as a fraction of the key, 0 to 0.45: 0.22 for icons
  (with `contain`), 0 for photos (with `cover`). `tint` recolours single-colour icons; keep `null`
  for photos and multicolour icons.
- Text: `{ "type": "text", "content": "Mic", "font_family": "Inter", "font_weight": 700,
  "size": 26, "color": C, "position": "top" | "middle" | "bottom", "outline": null }`.
  `size` is 10 to 80 (in pixels of a 196 px key). `font_weight` is one of 400, 500, 600, 700, 800,
  900. `font_family` is not empty; `list_fonts` lists the families, "Inter" is always there.
  `outline` is null or `{ "color": C, "width": 2 }`. Lines break with `\n`.
  Text can show the page it is on: `{page}` (its name), `{page_number}` (from 1) and
  `{page_count}`.
- Widget: `{ "type": "widget", "widget": W }`, live content (see Widgets).
- Border: `{ "type": "border", "color": C, "width": 4, "radius": 24 }` (width and radius 0 or more).

Readable keys: one to two short words (a label under an icon fits about 8 characters at size 24),
strong contrast between text and background, `position: "bottom"` for a text under an image,
`"middle"` for a text alone. Sizes that work: 20-26 under an icon, 28-40 alone, 34-44 on the wide
key. Prefer an icon with a short label over a sentence.

### Kinds

`kind` says what the key is, as the editor sets it up. Pick one and follow its rules:

| kind | what it is | must hold |
|------|------------|-----------|
| `basic` | a look, and optionally actions | no widget layers |
| `action` | `actions[0]` is what the key is (open an app, an OBS scene, a plugin…); the rest run after it | at least one action; an `action` key without actions is read as `basic` |
| `widget` | live content | exactly one widget layer |
| `carousel` | several live contents, each press shows the next | two or more widget layers, none of them `timer`, `stopwatch` or `counter` |
| `two_states` | two looks and two lists of actions, alternating on every press | `second_state` |
| `page_indicator` | shows the page the dock is on | a text with `{page_number}`, no widget layers |

- Only `two_states` and `action` keys may have a `second_state`. A key with a second state whose
  actions include an OBS or Home Assistant command that has an on/off state **follows** it: it shows
  the second look while the thing is on (recording, light on…), not by presses. The editor makes
  these as `action` keys.
- A key with a second state has no `double_press` or `long_press`.

### Actions

| type | fields |
|------|--------|
| `open_url` | `url`: `http://` or `https://`, no spaces |
| `open` | `path`: an app, file or folder (a `.app` on macOS, an `.exe` or shortcut on Windows, a `.desktop` file on Linux) |
| `close_app` | `app`: the path to the app, as for `open` |
| `type_text` | `text` (not empty), `enter` (press Return after), `paste` (paste instead of typing: faster, keeps any character) |
| `hotkey` | `keys`: `{ "key": "KeyC", "command", "shift", "option", "control" }` |
| `media` | `key`: `play_pause`, `next`, `previous`, `volume_up`, `volume_down`, `mute`, `brightness_up`, `brightness_down` |
| `open_page` | `page`: a page of the same profile, not the key's own page |
| `back`, `home`, `previous_page`, `next_page` | none |
| `switch_profile` | `profile` |
| `delay` | `ms`: up to 60000 |
| `random` | `actions`: one is picked on each press |
| `play_sound` | `sound`, `name`, `volume` 0-100, `mode`: `overlap`, `restart`, `toggle` or `loop`, `device`: null or a name from `list_audio_outputs` |
| `stop_sounds` | none |
| `http` | `request` (see the HTTP recipe) |
| `obs` | `server`, `command` (see OBS) |
| `home_assistant` | `server`, `command` (see Home Assistant) |
| `plugin` | `action` (from `list_plugins`); `id` and `settings` are filled in when left out |

Hotkeys name the physical key as `KeyboardEvent.code`: `KeyA`…`KeyZ`, `Digit0`…`Digit9`,
`F1`…`F24`, `Enter`, `Escape`, `Space`, `Tab`, `Backspace`, `Delete`, `ArrowUp`, `ArrowDown`,
`ArrowLeft`, `ArrowRight`, `Home`, `End`, `PageUp`, `PageDown`, `Minus`, `Equal`, `BracketLeft`,
`BracketRight`, `Semicolon`, `Quote`, `Comma`, `Period`, `Slash`, `Backquote`. `key` is never a
modifier itself. Modifiers: `command` is ⌘ on macOS, the Windows key on Windows and Super on
Linux; `control` is Ctrl; `option` is ⌥ / Alt. So copy is `command` + `KeyC` on macOS and
`control` + `KeyC` on Windows and Linux. Tell the systems apart by the app ids of
`list_running_apps`: bundle ids (`com.apple.Safari`) on macOS, `.exe` names on Windows.

## Rules that refuse a change

Check these before sending:

1. A list moves the dock at most once: one of `open_page`, `back`, `home`, `previous_page`,
   `next_page` or `switch_profile` per list (`actions`, `double_press`, `long_press`,
   `second_state.actions`).
2. `open_page` only in `actions` (the simple press), to another page of the same profile.
3. A plugin action is `actions[0]` of an `action` key, alone of its kind (one plugin action per
   key), never in gestures, never with a second state. The plugin draws the key.
4. A Home Assistant `tile` or `show` command draws the key too: only as `actions[0]` of an `action`
   key without a second state.
5. `random` holds at least one action and none that moves the dock, draws the key, or is another
   `random`.
6. The kind agrees with the contents (table above); widget layers sit together, first or right
   after one image, before text and borders.
7. Ranges: text size 10-80, weights 400-900 in hundreds, image inset 0-0.45, gradient angle 0-360,
   volume 0-100, delay up to 60000 ms.
8. Every image and sound must be in the asset store first: `import_icon`, `import_image`,
   `import_sound`. Never invent an asset id.
9. OBS and Home Assistant actions and widgets point to an integration of the right kind by its id
   (`integrations` of `get_overview`). Add one with `save_integration` if there is none.
10. A key with a second state has no gestures.
11. Required texts are not blank: `type_text.text`, `hotkey.keys.key`, `open.path`, OBS scene and
    source names, Home Assistant entity ids (`light.kitchen`).

## Recipes

Placeholders such as `"<asset id>"` stand for real ids: say where each comes from and replace it.
Everything else in these examples is valid as it is.

### Icon and label that opens a website

`import_icon` with `"icon": "mdi:github"` (find icons with `search_icons`) answers the image layer;
put it first.

```json key
{ "kind": "action",
  "appearance": { "background": { "type": "solid", "color": { "r": 36, "g": 41, "b": 47 } }, "layers": [
    { "type": "image", "asset": "<asset id>", "fit": "contain", "inset": 0.22, "tint": { "r": 255, "g": 255, "b": 255 }, "icon": "mdi:github" },
    { "type": "text", "content": "GitHub", "font_family": "Inter", "font_weight": 700, "size": 24, "color": { "r": 255, "g": 255, "b": 255 }, "position": "bottom", "outline": null }
  ] },
  "actions": [{ "type": "open_url", "url": "https://github.com" }]
}
```

### Open an app

`path` is the app on this computer (ask the user if you do not know it; `app_at_path` checks it).
A photo or logo from `import_image` uses `"fit": "cover", "inset": 0` and no tint.

```json key
{ "kind": "action",
  "appearance": { "background": { "type": "solid", "color": { "r": 30, "g": 215, "b": 96 } }, "layers": [
    { "type": "image", "asset": "<asset id>", "fit": "contain", "inset": 0.22, "tint": { "r": 0, "g": 0, "b": 0 }, "icon": "mdi:spotify" },
    { "type": "text", "content": "Spotify", "font_family": "Inter", "font_weight": 700, "size": 22, "color": { "r": 0, "g": 0, "b": 0 }, "position": "bottom", "outline": null }
  ] },
  "actions": [{ "type": "open", "path": "/Applications/Spotify.app" }]
}
```

### Keyboard shortcut

```json key
{ "kind": "action",
  "appearance": { "background": { "type": "linear_gradient", "from": { "r": 37, "g": 99, "b": 235 }, "to": { "r": 30, "g": 27, "b": 75 }, "angle": 90 }, "layers": [
    { "type": "text", "content": "Captura", "font_family": "Inter", "font_weight": 700, "size": 30, "color": { "r": 255, "g": 255, "b": 255 }, "position": "middle", "outline": null }
  ] },
  "actions": [{ "type": "hotkey", "keys": { "key": "Digit4", "command": true, "shift": true, "option": false, "control": false } }]
}
```

### Type a text

```json key
{ "kind": "action",
  "appearance": { "background": { "type": "solid", "color": { "r": 24, "g": 24, "b": 27 } }, "layers": [
    { "type": "text", "content": "Firma", "font_family": "Inter", "font_weight": 700, "size": 30, "color": { "r": 250, "g": 204, "b": 21 }, "position": "middle", "outline": null }
  ] },
  "actions": [{ "type": "type_text", "text": "Un saludo,\nAna", "enter": false, "paste": true }]
}
```

### Media keys, with a gesture

Play or pause on a press, next song on a double press, previous on a long press.

```json key
{ "kind": "action",
  "appearance": { "background": { "type": "solid", "color": { "r": 0, "g": 0, "b": 0 } }, "layers": [
    { "type": "image", "asset": "<asset id>", "fit": "contain", "inset": 0.22, "tint": { "r": 255, "g": 255, "b": 255 }, "icon": "mdi:play-pause" }
  ] },
  "actions": [{ "type": "media", "key": "play_pause" }],
  "double_press": [{ "type": "media", "key": "next" }],
  "long_press": [{ "type": "media", "key": "previous" }]
}
```

### Link to another page, back and home

`open_page` draws a line between the two pages on the app's canvas. The page must be another page
of the same profile (from `get_overview`).

```json key
{ "kind": "action",
  "appearance": { "background": { "type": "solid", "color": { "r": 79, "g": 70, "b": 229 } }, "layers": [
    { "type": "image", "asset": "<asset id>", "fit": "contain", "inset": 0.22, "tint": { "r": 255, "g": 255, "b": 255 }, "icon": "mdi:folder" },
    { "type": "text", "content": "Escenas", "font_family": "Inter", "font_weight": 700, "size": 22, "color": { "r": 255, "g": 255, "b": 255 }, "position": "bottom", "outline": null }
  ] },
  "actions": [{ "type": "open_page", "page": "<target page id>" }]
}
```

Keys that go back or home are the same with `{ "type": "back" }` or `{ "type": "home" }` (the back key
of new pages has the icon `mdi:arrow-left`, inset 0.28). `switch_profile` takes a `<profile id>`.

### A folder of pages in one call

Creates a page, fills it, and links it from a key of the page you are on. `name` is the handle for
later changes of the call; `page_name` is what the user sees. The new page already has a back key
in slot 0. `<profile id>` is the profile of `<page id>`.

```json changes
{ "changes": [
  { "op": "create_page", "profile": "<profile id>", "page_name": "Música", "name": "$music" },
  { "op": "set_key", "page": "$music", "index": 13, "key": {
    "kind": "widget",
    "appearance": { "background": { "type": "solid", "color": { "r": 0, "g": 0, "b": 0 } }, "layers": [
      { "type": "widget", "widget": { "kind": "now_playing", "style": { "color": { "r": 255, "g": 255, "b": 255 }, "accent": { "r": 56, "g": 189, "b": 248 }, "font_family": "Inter" } } }
    ] },
    "actions": [{ "type": "media", "key": "play_pause" }]
  } },
  { "op": "set_key", "page": "<page id>", "index": 4, "key": {
    "kind": "action",
    "appearance": { "background": { "type": "solid", "color": { "r": 22, "g": 163, "b": 74 } }, "layers": [
      { "type": "text", "content": "Música", "font_family": "Inter", "font_weight": 700, "size": 28, "color": { "r": 255, "g": 255, "b": 255 }, "position": "middle", "outline": null }
    ] },
    "actions": [{ "type": "open_page", "page": "$music" }]
  } }
] }
```

A new profile comes with an empty home page; `home_name` names it for the same call:

```json changes
{ "changes": [
  { "op": "create_profile", "profile_name": "Directo", "name": "$live", "home_name": "$live_home" },
  { "op": "set_key", "page": "$live_home", "index": 0, "key": {
    "kind": "action",
    "appearance": { "background": { "type": "solid", "color": { "r": 220, "g": 38, "b": 38 } }, "layers": [
      { "type": "text", "content": "Volver", "font_family": "Inter", "font_weight": 700, "size": 28, "color": { "r": 255, "g": 255, "b": 255 }, "position": "middle", "outline": null }
    ] },
    "actions": [{ "type": "switch_profile", "profile": "<profile id>" }]
  } }
] }
```

Other changes: `clear_key {page, index}`, `move_key {from_page, from_index, to_page, to_index}` (it
swaps with a key already there, within one profile), `update_page {page, page_name, position}`,
`duplicate_page {page, name}`, `delete_page {page}` (not the home page), `set_home_page {page}`,
`rename_profile {profile, profile_name}`, `duplicate_profile {profile, name}`,
`delete_profile {profile}` (not the last one), `move_profile {profile, index}`, `link_app`,
`unlink_app`.

### Page indicator

```json key
{ "kind": "page_indicator",
  "appearance": { "background": { "type": "solid", "color": { "r": 0, "g": 0, "b": 0 } }, "layers": [
    { "type": "text", "content": "{page}\n{page_number}/{page_count}", "font_family": "Inter", "font_weight": 700, "size": 28, "color": { "r": 255, "g": 255, "b": 255 }, "position": "middle", "outline": null }
  ] },
  "actions": [{ "type": "next_page" }]
}
```

### Widgets

Every widget has a `style`: `{ "color": C, "accent": C, "font_family": "Inter" }` (text colour,
the colour of what stands out: seconds, bars, hands). The editor's default is white with accent
`{ "r": 56, "g": 189, "b": 248 }`. Any widget fits any key; on the wide key most show more (the
whole month, three clocks, the forecast). Every field listed is required; labels show above
the content, and an empty one shows nothing.

| kind | fields |
|------|--------|
| `digital_clock` | `seconds`, `hour12`, `second_line`: `none` / `date` / `weekday` / `weekday_date`, `zone`: null or an IANA zone (`Asia/Tokyo`), `label` (up to 24) |
| `analog_clock` | `seconds`, `date`, `zone`, `label` (up to 24) |
| `world_clocks` | `clocks`: 1-3 `{ "zone", "label" }` (label up to 16, empty is the city), `hour12`; square keys show the first two |
| `word_clock`, `calendar`, `battery`, `now_playing`, `temperatures`, `volume`, `moon` | only `style` |
| `countdown` | `target`: local `YYYY-MM-DDTHH:MM`, `label`, `done` (shown at the end; both up to 24) |
| `timer` | `seconds` 1-36000, `label`, `done`; a press starts or pauses it, holding restarts it |
| `stopwatch` | `label`; a press starts or pauses it, holding restarts it |
| `counter` | `label`, `step` -1000 to 1000; adds `step` on each press, holding resets it |
| `system_monitor` | `metrics`: 1-4 different of `cpu`, `ram`, `gpu`, `disk`; `graph` (history instead of bars); `minutes` 1-1440 |
| `network` | `graph`, `minutes` 1-1440 |
| `top_processes` | `by`: `cpu` or `memory` |
| `latency` | `host`: a name or address, empty is the router |
| `progress` | `periods`: 1-4 different of `day`, `week`, `month`, `year` |
| `phrases` | `phrases`: at least one text, `seconds` 0-86400 between them (0: one more on each press) |
| `hass_*` | see Home Assistant |

A clock on a normal key:

```json key
{ "kind": "widget",
  "appearance": { "background": { "type": "solid", "color": { "r": 0, "g": 0, "b": 0 } }, "layers": [
    { "type": "widget", "widget": { "kind": "digital_clock", "seconds": false, "hour12": false, "second_line": "weekday_date", "zone": null, "label": "", "style": { "color": { "r": 255, "g": 255, "b": 255 }, "accent": { "r": 56, "g": 189, "b": 248 }, "font_family": "Inter" } } }
  ] }
}
```

World clocks on the wide key (index 13):

```json key wide
{ "kind": "widget",
  "appearance": { "background": { "type": "linear_gradient", "from": { "r": 15, "g": 23, "b": 42 }, "to": { "r": 49, "g": 46, "b": 129 }, "angle": 0 }, "layers": [
    { "type": "widget", "widget": { "kind": "world_clocks", "clocks": [{ "zone": "Europe/Madrid", "label": "Madrid" }, { "zone": "America/New_York", "label": "Nueva York" }, { "zone": "Asia/Tokyo", "label": "Tokio" }], "hour12": false, "style": { "color": { "r": 255, "g": 255, "b": 255 }, "accent": { "r": 56, "g": 189, "b": 248 }, "font_family": "Inter" } } }
  ] }
}
```

System monitor with graphs on the wide key:

```json key wide
{ "kind": "widget",
  "appearance": { "background": { "type": "solid", "color": { "r": 0, "g": 0, "b": 0 } }, "layers": [
    { "type": "widget", "widget": { "kind": "system_monitor", "metrics": ["cpu", "ram", "gpu"], "graph": true, "minutes": 10, "style": { "color": { "r": 255, "g": 255, "b": 255 }, "accent": { "r": 34, "g": 197, "b": 94 }, "font_family": "Inter" } } }
  ] }
}
```

Now playing that also plays or pauses on a press (a widget key may have actions):

```json key
{ "kind": "widget",
  "appearance": { "background": { "type": "solid", "color": { "r": 0, "g": 0, "b": 0 } }, "layers": [
    { "type": "widget", "widget": { "kind": "now_playing", "style": { "color": { "r": 255, "g": 255, "b": 255 }, "accent": { "r": 30, "g": 215, "b": 96 }, "font_family": "Inter" } } }
  ] },
  "actions": [{ "type": "media", "key": "play_pause" }]
}
```

A countdown with an icon behind and a text in front (image, widget, text: the editor's order):

```json key
{ "kind": "widget",
  "appearance": { "background": { "type": "solid", "color": { "r": 127, "g": 29, "b": 29 } }, "layers": [
    { "type": "image", "asset": "<asset id>", "fit": "contain", "inset": 0.3, "tint": { "r": 153, "g": 27, "b": 27 }, "icon": "mdi:airplane" },
    { "type": "widget", "widget": { "kind": "countdown", "target": "2026-12-24T18:00", "label": "Vacaciones", "done": "¡Ya!", "style": { "color": { "r": 255, "g": 255, "b": 255 }, "accent": { "r": 252, "g": 165, "b": 165 }, "font_family": "Inter" } } }
  ] }
}
```

A 25-minute timer:

```json key
{ "kind": "widget",
  "appearance": { "background": { "type": "solid", "color": { "r": 0, "g": 0, "b": 0 } }, "layers": [
    { "type": "widget", "widget": { "kind": "timer", "seconds": 1500, "label": "Pomodoro", "done": "Descanso", "style": { "color": { "r": 255, "g": 255, "b": 255 }, "accent": { "r": 239, "g": 68, "b": 68 }, "font_family": "Inter" } } }
  ] }
}
```

### Carousel

Each press shows the next widget. Never a timer, stopwatch or counter in a carousel: they use the
press themselves.

```json key wide
{ "kind": "carousel",
  "appearance": { "background": { "type": "solid", "color": { "r": 0, "g": 0, "b": 0 } }, "layers": [
    { "type": "widget", "widget": { "kind": "digital_clock", "seconds": true, "hour12": false, "second_line": "weekday_date", "zone": null, "label": "", "style": { "color": { "r": 255, "g": 255, "b": 255 }, "accent": { "r": 56, "g": 189, "b": 248 }, "font_family": "Inter" } } },
    { "type": "widget", "widget": { "kind": "calendar", "style": { "color": { "r": 255, "g": 255, "b": 255 }, "accent": { "r": 56, "g": 189, "b": 248 }, "font_family": "Inter" } } },
    { "type": "widget", "widget": { "kind": "moon", "style": { "color": { "r": 255, "g": 255, "b": 255 }, "accent": { "r": 56, "g": 189, "b": 248 }, "font_family": "Inter" } } }
  ] }
}
```

### Two states: a toggle

A mute shortcut that shows which state it is in. Each press runs the list of the look it shows,
then switches to the other look. Both states need an `appearance`. Give the second look a clear
difference (red background, another icon).

```json key
{ "kind": "two_states",
  "appearance": { "background": { "type": "solid", "color": { "r": 0, "g": 0, "b": 0 } }, "layers": [
    { "type": "image", "asset": "<asset id>", "fit": "contain", "inset": 0.22, "tint": { "r": 255, "g": 255, "b": 255 }, "icon": "mdi:microphone" },
    { "type": "text", "content": "Mic", "font_family": "Inter", "font_weight": 700, "size": 22, "color": { "r": 255, "g": 255, "b": 255 }, "position": "bottom", "outline": null }
  ] },
  "actions": [{ "type": "hotkey", "keys": { "key": "KeyM", "command": true, "shift": true, "option": false, "control": false } }],
  "second_state": { "appearance": { "background": { "type": "solid", "color": { "r": 220, "g": 38, "b": 38 } }, "layers": [
    { "type": "image", "asset": "<asset id>", "fit": "contain", "inset": 0.22, "tint": { "r": 255, "g": 255, "b": 255 }, "icon": "mdi:microphone-off" },
    { "type": "text", "content": "Mudo", "font_family": "Inter", "font_weight": 700, "size": 22, "color": { "r": 255, "g": 255, "b": 255 }, "position": "bottom", "outline": null }
  ] }, "actions": [{ "type": "hotkey", "keys": { "key": "KeyM", "command": true, "shift": true, "option": false, "control": false } }] }
}
```

A shortcut cannot know the real state: muted elsewhere, the key is out of step. OBS and Home
Assistant keys can follow the real state (below).

### Play a sound

`import_sound` answers `sound` and `name`.

```json key
{ "kind": "action",
  "appearance": { "background": { "type": "solid", "color": { "r": 234, "g": 88, "b": 12 } }, "layers": [
    { "type": "text", "content": "Aplausos", "font_family": "Inter", "font_weight": 700, "size": 24, "color": { "r": 255, "g": 255, "b": 255 }, "position": "middle", "outline": null }
  ] },
  "actions": [{ "type": "play_sound", "sound": "<sound id>", "name": "aplausos.mp3", "volume": 80, "mode": "restart", "device": null }],
  "long_press": [{ "type": "stop_sounds" }]
}
```

### HTTP request

`method` is `GET`, `POST`, `PUT`, `PATCH`, `DELETE`, `HEAD` or `OPTIONS`. `body` is
`{ "type": "none" }`, `{ "type": "json", "json": "<JSON as a string>" }`,
`{ "type": "text", "text", "content_type" }` or `{ "type": "form", "fields": [{ "name", "value" }] }`.
`timeout_ms` is 1000-120000. `show_result` makes the key show for a moment whether it worked.
`insecure` accepts certificates that cannot be verified (devices of the local network).

```json key
{ "kind": "action",
  "appearance": { "background": { "type": "solid", "color": { "r": 15, "g": 118, "b": 110 } }, "layers": [
    { "type": "text", "content": "Webhook", "font_family": "Inter", "font_weight": 700, "size": 26, "color": { "r": 255, "g": 255, "b": 255 }, "position": "middle", "outline": null }
  ] },
  "actions": [{ "type": "http", "request": { "method": "POST", "url": "https://example.com/hooks/deploy",
    "headers": [{ "name": "Content-Type", "value": "application/json" }], "body": { "type": "json", "json": "{\"branch\": \"main\"}" },
    "timeout_ms": 10000, "insecure": false, "show_result": true } }]
}
```

### OBS Studio

`server` is the id of an OBS integration (`get_overview`, or `save_integration` with
`{ "kind": "obs", "name": "OBS", "host": "", "port": 0 }`, where empty and 0 are OBS's own
127.0.0.1:4455; the user types the password in the app). Get real names with `obs_names`:
`{ "kind": "scenes" }`, `audio_inputs`, `sources`, `media_inputs`, `transitions`, `profiles`,
`scene_collections`, `{ "kind": "scene_items", "scene": "…" }`, `{ "kind": "filters", "source": "…" }`.

Commands (`mode` is `toggle`, `on` or `off`):
`record`, `record_pause`, `stream`, `virtual_cam`, `replay_buffer`, `studio_mode` `{mode}`;
`save_replay`; `transition`; `set_transition {transition, duration_ms: null | 50-20000}`;
`screenshot {source, folder}` (empty: program scene, recordings folder);
`scene_collection {collection}`; `profile {profile}`; `scene {scene, preview}`;
`source_visibility {scene, source, mode}`; `chapter {name}`; `filter {source, filter, mode}`;
`media {input, control}` (`play_pause`, `play`, `pause`, `stop`, `restart`, `next`, `previous`);
`mute {input, mode}`; `volume {input, change}` with `{ "type": "step", "db": -30..30, not 0 }` or
`{ "type": "set", "percent": 0-100 }`.

The editor makes OBS keys **follow** OBS: an `action` key with a second state that is a copy of
the first look on a coloured background, with the same command. The key shows the second look while
the scene is on air, the recording runs, the input is muted… Colours the editor uses: red
`{220,38,38}` for program scene, record, stream and mute; green `{22,163,74}` for preview scene,
virtual camera, replay buffer, source visibility and filters; amber `{217,119,6}` for record pause;
blue `{37,99,235}` for studio mode. Without the second state the key works the same, unseen.

Switch to a scene, red while it is on air:

```json key
{ "kind": "action",
  "appearance": { "background": { "type": "solid", "color": { "r": 0, "g": 0, "b": 0 } }, "layers": [
    { "type": "text", "content": "Cámara", "font_family": "Inter", "font_weight": 700, "size": 26, "color": { "r": 255, "g": 255, "b": 255 }, "position": "middle", "outline": null }
  ] },
  "actions": [{ "type": "obs", "server": "<obs server id>", "command": { "kind": "scene", "scene": "Cámara", "preview": false } }],
  "second_state": { "appearance": { "background": { "type": "solid", "color": { "r": 220, "g": 38, "b": 38 } }, "layers": [
    { "type": "text", "content": "Cámara", "font_family": "Inter", "font_weight": 700, "size": 26, "color": { "r": 255, "g": 255, "b": 255 }, "position": "middle", "outline": null }
  ] }, "actions": [{ "type": "obs", "server": "<obs server id>", "command": { "kind": "scene", "scene": "Cámara", "preview": false } }] }
}
```

Start and stop recording, red while it records:

```json key
{ "kind": "action",
  "appearance": { "background": { "type": "solid", "color": { "r": 0, "g": 0, "b": 0 } }, "layers": [
    { "type": "image", "asset": "<asset id>", "fit": "contain", "inset": 0.22, "tint": { "r": 255, "g": 255, "b": 255 }, "icon": "mdi:record-rec" },
    { "type": "text", "content": "Grabar", "font_family": "Inter", "font_weight": 700, "size": 22, "color": { "r": 255, "g": 255, "b": 255 }, "position": "bottom", "outline": null }
  ] },
  "actions": [{ "type": "obs", "server": "<obs server id>", "command": { "kind": "record", "mode": "toggle" } }],
  "second_state": { "appearance": { "background": { "type": "solid", "color": { "r": 220, "g": 38, "b": 38 } }, "layers": [
    { "type": "image", "asset": "<asset id>", "fit": "contain", "inset": 0.22, "tint": { "r": 255, "g": 255, "b": 255 }, "icon": "mdi:record-rec" },
    { "type": "text", "content": "Grabando", "font_family": "Inter", "font_weight": 700, "size": 22, "color": { "r": 255, "g": 255, "b": 255 }, "position": "bottom", "outline": null }
  ] }, "actions": [{ "type": "obs", "server": "<obs server id>", "command": { "kind": "record", "mode": "toggle" } }] }
}
```

Streaming is the same with `{ "kind": "stream", "mode": "toggle" }`. Mute an input, red while muted:

```json key
{ "kind": "action",
  "appearance": { "background": { "type": "solid", "color": { "r": 0, "g": 0, "b": 0 } }, "layers": [
    { "type": "text", "content": "Mic", "font_family": "Inter", "font_weight": 700, "size": 30, "color": { "r": 255, "g": 255, "b": 255 }, "position": "middle", "outline": null }
  ] },
  "actions": [{ "type": "obs", "server": "<obs server id>", "command": { "kind": "mute", "input": "Mic/Aux", "mode": "toggle" } }],
  "second_state": { "appearance": { "background": { "type": "solid", "color": { "r": 220, "g": 38, "b": 38 } }, "layers": [
    { "type": "text", "content": "Mic", "font_family": "Inter", "font_weight": 700, "size": 30, "color": { "r": 255, "g": 255, "b": 255 }, "position": "middle", "outline": null }
  ] }, "actions": [{ "type": "obs", "server": "<obs server id>", "command": { "kind": "mute", "input": "Mic/Aux", "mode": "toggle" } }] }
}
```

### Home Assistant

`server` is the id of a Home Assistant integration (`save_integration` with
`{ "kind": "home_assistant", "name": "Casa", "url": "http://homeassistant.local:8123", "insecure": false }`;
the user types the token in the app). Find entities with `hass_entities` (by words, domain) and
the actions of a domain with `hass_services`. Entity ids look like `light.living_room`.

Commands: `switch {entity, mode: toggle | on | off}`; `run {entity}` (scenes, scripts, buttons,
automations); `level {entity, level, change}` with `level` one of `brightness`, `color_temp`,
`position`, `tilt`, `speed`, `volume`, `temperature`, `humidity`, `value` and `change`
`{ "type": "step", "by": 10 }` or `{ "type": "set", "to": 50 }`;
`choose {entity, option, choice}` (`choice`: `{ "type": "next" }`, `previous`, or
`{ "type": "set", "option": "…" }`); `media {entity, control}` (`play_pause`, `play`, `pause`,
`stop`, `next`, `previous`, `mute`, `volume_up`, `volume_down`, `shuffle`); `color {entity, color}`;
`alarm {entity, mode}`; `call {action: "domain.action", target: {entities, devices, areas, floors,
labels}, data: {…}}`; `assist {text, agent}`; `event {event, data}`; `show {entity}` and
`tile {entity, press: auto | nothing}` (these two draw the key).

A tile: the entity drawn as what it is (a lamp lit in its colour, a blind as open as it is, a
thermostat's dial), always up to date, toggled on a press. The `tile` command and a `hass_tile`
widget of the same entity, the way the editor makes it:

```json key
{ "kind": "action",
  "appearance": { "background": { "type": "solid", "color": { "r": 0, "g": 0, "b": 0 } }, "layers": [
    { "type": "widget", "widget": { "kind": "hass_tile", "server": "<hass server id>", "entity": "light.living_room", "label": "Salón", "drawing": "auto", "extra": null, "min": null, "max": null, "hours": 24, "icon": null, "icons": {}, "style": { "color": { "r": 255, "g": 255, "b": 255 }, "accent": { "r": 56, "g": 189, "b": 248 }, "font_family": "Inter" } } }
  ] },
  "actions": [{ "type": "home_assistant", "server": "<hass server id>", "command": { "kind": "tile", "entity": "light.living_room", "press": "auto" } }]
}
```

`hass_tile` fields: `drawing` is `auto` or one of `icon`, `value`, `gauge`, `graph`, `bulb`,
`plug`, `switch`, `fan`, `curtain`, `blind`, `shutter`, `shade`, `garage`, `awning`, `gate`,
`door`, `window`, `valve`, `lock`, `thermometer`, `droplet`, `battery`; `extra` another entity to
show too (what a plug draws), or null; `min`/`max` for gauges, or null; `hours` 1-168 for graphs;
`label` up to 40 (empty is the entity's name); `icon` an image asset over Home Assistant's
(`import_icon` with an `mdi:` icon); `icons` by state, `{}` is fine. A tile that only shows uses
`"press": "nothing"`.

A light toggle with an icon of your own, amber while the light is on (it follows Home Assistant):

```json key
{ "kind": "action",
  "appearance": { "background": { "type": "solid", "color": { "r": 0, "g": 0, "b": 0 } }, "layers": [
    { "type": "image", "asset": "<asset id>", "fit": "contain", "inset": 0.22, "tint": { "r": 255, "g": 255, "b": 255 }, "icon": "mdi:lightbulb" },
    { "type": "text", "content": "Cocina", "font_family": "Inter", "font_weight": 700, "size": 22, "color": { "r": 255, "g": 255, "b": 255 }, "position": "bottom", "outline": null }
  ] },
  "actions": [{ "type": "home_assistant", "server": "<hass server id>", "command": { "kind": "switch", "entity": "light.kitchen", "mode": "toggle" } }],
  "second_state": { "appearance": { "background": { "type": "solid", "color": { "r": 217, "g": 119, "b": 6 } }, "layers": [
    { "type": "image", "asset": "<asset id>", "fit": "contain", "inset": 0.22, "tint": { "r": 255, "g": 255, "b": 255 }, "icon": "mdi:lightbulb-on" },
    { "type": "text", "content": "Cocina", "font_family": "Inter", "font_weight": 700, "size": 22, "color": { "r": 255, "g": 255, "b": 255 }, "position": "bottom", "outline": null }
  ] }, "actions": [{ "type": "home_assistant", "server": "<hass server id>", "command": { "kind": "switch", "entity": "light.kitchen", "mode": "toggle" } }] }
}
```

Call any action (`hass_services` lists the fields of `data`):

```json key
{ "kind": "action",
  "appearance": { "background": { "type": "solid", "color": { "r": 30, "g": 64, "b": 175 } }, "layers": [
    { "type": "text", "content": "Cine", "font_family": "Inter", "font_weight": 700, "size": 30, "color": { "r": 255, "g": 255, "b": 255 }, "position": "middle", "outline": null }
  ] },
  "actions": [{ "type": "home_assistant", "server": "<hass server id>", "command": { "kind": "call", "action": "light.turn_on", "target": { "entities": ["light.living_room"], "devices": [], "areas": [], "floors": [], "labels": [] }, "data": { "brightness_pct": 20, "color_temp_kelvin": 2700 } } }]
}
```

Home Assistant widgets (all with `server` and `style`): `hass_entity {entity, attribute: null,
label, icon: null, icons: {}}` (a card with the state), `hass_graph {entity, hours 1-168, label}`,
`hass_entities {entities: 1-3 {entity, label}}`, `hass_picture {entity, seconds 2-3600}` (cameras,
images), `hass_weather {entity, forecast}`, `hass_template {template, label}`, `hass_tile`. Several
sensors on the wide key:

```json key wide
{ "kind": "widget",
  "appearance": { "background": { "type": "solid", "color": { "r": 0, "g": 0, "b": 0 } }, "layers": [
    { "type": "widget", "widget": { "kind": "hass_entities", "server": "<hass server id>", "entities": [{ "entity": "sensor.living_room_temperature", "label": "Salón" }, { "entity": "sensor.outdoor_temperature", "label": "Fuera" }, { "entity": "sensor.living_room_humidity", "label": "Humedad" }], "style": { "color": { "r": 255, "g": 255, "b": 255 }, "accent": { "r": 56, "g": 189, "b": 248 }, "font_family": "Inter" } } }
  ] }
}
```

### Plugin keys

1. `list_plugins`: installed plugins and their actions. Use an action with `"supported": true`
   whose plugin is `running`. If none fits, `search_plugin_store` and `recommend_plugin`.
2. `get_plugin_action` with its `action` id: what its settings mean (the property inspector) and
   the settings it starts with.
3. Put the key, with the plugin action alone as `actions[0]` of an `action` key and no layers: the
   plugin draws the key. Leave out `id` and `settings`: a new placement gets its own id and the
   plugin's default settings.
4. `set_plugin_key_settings {page, index, settings}` with the whole settings object, if the
   defaults are not what the user wants. Not undoable: the settings belong to the plugin.
   What a plugin shares between its keys (accounts, addresses) is in
   `get_plugin_global_settings` / `set_plugin_global_settings`. To move or copy a plugin key, keep its
   `id` and `settings` as `get_key` shows them. If it stays blank or misbehaves, `restart_plugin`.

```json key
{ "kind": "action",
  "appearance": { "background": { "type": "solid", "color": { "r": 0, "g": 0, "b": 0 } }, "layers": [] },
  "actions": [{ "type": "plugin", "action": "<plugin action id>" }]
}
```

## Settings

`set_preferences` takes only the fields to change; nested objects merge with what there is.

The screensaver: after 10 minutes without touching the dock, dark after an hour more. `image` is
`{ "type": "scene", "scene": "sunset" | "aurora" | "ocean" | "neon" | "mountains" | "graphite" }`
or `{ "type": "custom", "asset": "<asset id>" }` (an image from `import_image`, covering the whole
panel). `idle_minutes` is one of 0 (never), 1, 2, 5, 10, 15, 30, 60; `off_after_minutes` one of
0 (never), 5, 10, 15, 30, 60, 120; `brightness` 5-100; `on_lock` is `keep`, `screensaver` or `off`.
`preview_screensaver` shows it for a moment.

```json preferences
{ "screensaver": { "idle_minutes": 10, "off_after_minutes": 60, "brightness": 40, "on_lock": "screensaver", "image": { "type": "scene", "scene": "aurora" } } }
```

The look of new keys (also of the back key of new pages): text size 10-80, weight 400-900.

```json preferences
{ "new_keys": { "background": { "r": 24, "g": 24, "b": 27 }, "text_color": { "r": 250, "g": 250, "b": 250 }, "font_family": "Inter", "font_weight": 600, "text_size": 26, "text_position": "bottom" } }
```

Other preferences: `language` (`system`, `es`, `en`), `gestures`
(`{ "long_press_ms": 250-2000, "double_press_ms": 150-800 }`), `profiles.follow_apps` (whether
linked apps switch profiles).

`set_device_settings`: `brightness` 0-100, `sleep_minutes` one of 0 (never), 1, 5, 10, 15, 30, 60.

```json device
{ "brightness": 60, "sleep_minutes": 30 }
```

`pause_dock` darkens the dock and stops its keys until resumed.

## Profiles that follow apps

`list_running_apps` gives each app's `id` and `name` (or `app_at_path` for one that is not
running). Link it to a profile, and that profile comes to the dock whenever the app is in front,
while `profiles.follow_apps` is on. An app belongs to one profile at most.

```json changes
{ "changes": [
  { "op": "link_app", "profile": "<profile id>", "app_id": "com.obsproject.obs-studio", "app_name": "OBS" }
] }
```

`unlink_app {profile, app_id}` undoes it. `set_profile_local` keeps a profile on this computer only,
out of the user's cloud sync.
