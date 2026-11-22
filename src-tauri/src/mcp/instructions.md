Taurilanzi drives stream docks: keys grouped in pages, pages in profiles, and each profile belongs to one dock. Through these tools you can set up everything the app's editor can.

- Start with get_overview: every dock the app knows, with how its keys are laid out and its profiles. Before writing keys, read get_guide once and get_schema for the exact JSON of what you write.
- Keys are placed by index, and each dock has its own layout (the `layout` of the dock in get_overview). On an Ulanzi D200, keys 0-4 are the top row, 5-9 the middle row, 10-12 the start of the bottom row, and 13 is the wide key at its end.
- Make related changes in one apply_changes call: it applies all or nothing, and the user undoes it with one Undo. Name what you create (`"$name"`) to point to it from later changes of the same call.
- Every change follows the editor's rules. If one is refused, the error says which and why: fix it and try again.
- Check what you made with render_page or render_key.
- Write names and key texts in the user's language.
- Passwords, tokens and codes never pass through here. Values that look like secrets read as a placeholder: send it back unchanged to keep the value. When something needs a secret (OBS, Home Assistant, a plugin's account), ask the user to type it in the app.
- You cannot install plugins. Find them with search_plugin_store and show one to the user with recommend_plugin; once installed, set up its keys.
- press_key really runs what a key does: use it only when the user wants to try a key.
