# Taurilanzi

A free desktop app to configure and drive Ulanzi stream docks (the D200 and its family), built
with Tauri. It replaces Ulanzi Studio: keys, pages, profiles, actions, live content on the wide
key, and the plugins published for Ulanzi Studio, which run unchanged.

`CLAUDE.md` is a symbolic link to this file: both are read by coding agents.

## Language

Everything in the repository is in **English**: code, comments, commit messages, README and this
file.

The app itself speaks **Spanish and English**: the system's language by default, or the one chosen
in Settings → General (`preferences.language`). Every user-visible text exists in both:

- UI: `vue-i18n`. Messages in `src-nuxt/app/locales/es.json` and `en.json`, grouped by area, read as
  `t('<area>.<key>')`; in components `const { t } = useI18n()`, elsewhere `t` from
  `src-nuxt/app/lib/i18n.ts`, called when the text is shown (never stored at module load, so it
  follows a change of language). `tests/unit/locales.test.ts` checks both languages have the same
  keys.
- Engine: messages in `src-tauri/locales/es.json` and `en.json`, nested by area like the UI's and
  built into the binary. `tr!("tray.quit")` gives a text, `trf!("tray.updates.ready", version = v)`
  fills its `{placeholders}`; for everything drawn on keys, shown in menus or returned as an error.
  Logs stay in English. Tests in `src/i18n/messages.rs` check both files have the same keys and
  placeholders and that every key in the code exists.
- The tests read the Spanish texts: unit tests set Spanish in their setup, the e2e tests run the app
  with `TAURILANZI_LANGUAGE=es`, and Rust tests are in Spanish unless they set another language.

## Structure

```
src-nuxt/       The UI, a whole Nuxt project (SPA mode): nuxt.config.ts, app/, public/
                src-nuxt/app/ipc/bindings.ts is generated from Rust
src-tauri/      Rust: engine, D200 driver, renderer, store, IPC, plugin host, tray…
tests/          unit/ (bun test) and e2e/ (WebdriverIO against the real app)
scripts/        app build, local update server, bundled Node download
```

One project, **no workspaces**: Bun at the root for the UI (`package.json` + `bun.lock`, never
npm/pnpm/yarn) and Cargo in `src-tauri` (`Cargo.toml` + `Cargo.lock`). Nuxt lives in `src-nuxt`
but has no `package.json` of its own: the root scripts point Nuxt at it (`nuxt dev src-nuxt`), and
any Nuxt command run by hand needs that argument too.

Bun installs packages and runs the scripts and the unit tests; the tools (Nuxt, the Tauri CLI,
`vue-tsc`, wdio) run on Node, as in the official `create-tauri-app` starter. Do not force Bun as
the runtime (`--bun`, `bunfig.toml`): it gains nothing and `vue-tsc` does not work with it.
Node ≥ 24.11 (nvm), or Nuxt warns about an unsupported version.

## Commands

```bash
bun install                      # also downloads the Node that runs plugins (scripts/fetch-node.ts)
bun run tauri dev                # full app: Nuxt on :1420 + window, with the dock
make dev                         # same, quitting the installed Taurilanzi first so the dock is free
bun run dev:no-dock              # same without touching the dock (TAURILANZI_NO_DOCK=1), to work
                                 # on the UI while Ulanzi Studio uses the dock
bun run typecheck
bun run test                     # UI unit tests
cd src-tauri && cargo test       # Rust tests; also regenerates src-nuxt/app/ipc/bindings.ts
cd src-tauri && cargo clippy --all-targets

# Manual checks on the real dock (close Ulanzi Studio and the app first: only one process can
# talk to the dock)
cd src-tauri && cargo test --lib dock_check -- --ignored --nocapture
cd src-tauri && cargo test --release --lib frame_rate -- --ignored --nocapture

# End-to-end tests against the real app (built with the e2e feature, temporary data, no dock)
bun run e2e
bun run e2e --skip-build --spec specs/canvas.e2e.ts   # a single one, without building

# The README's screenshots (docs/screenshots), from a demo setup in English; needs internet for
# the icons. Not part of `bun run e2e`
bun run screenshots

# Packaged app for this system, with its signed update, in src-tauri/target/release/bundle (or
# target/<target>/release/bundle with --target): .app + .dmg on macOS (the .dmg from
# scripts/build-dmg.py, dmgbuild through uv, which lays its window out without Finder), the NSIS
# installer on Windows, .AppImage + .deb + .rpm on Linux. Each system's bundle targets are in
# src-tauri/tauri.<system>.conf.json
bun scripts/build-app.ts
bun scripts/build-app.ts --target x86_64-apple-darwin   # for Intel Macs, from an Apple silicon one

# Clippy for Windows from a Mac (once: brew install llvm; cargo install cargo-xwin). For Linux,
# an Ubuntu container with the packages the Linux job of .github/workflows/tests.yml installs
cd src-tauri && PATH="/opt/homebrew/opt/llvm/bin:$PATH" cargo xwin clippy \
  --target x86_64-pc-windows-msvc --all-targets --target-dir target/xwin

# Try the updater locally: build against the local server, then publish that build
bun scripts/build-app.ts --config src-tauri/tauri.local-updates.json
bun scripts/serve-updates.ts     # http://127.0.0.1:8787/latest.json
```

Development builds are another app: `tauri.conf.json` names them "Taurilanzi Local Dev"
(`com.taurilanzi.dev`), with their own data folder, keychain entries, single-instance lock and
login item, so they run next to the released app without touching it. Only `scripts/build-app.ts`
gives the released name and identifier, from `src-tauri/tauri.bundle.json`. Texts that name the
app read it from the build (`package_info().name` in Rust, `name` of `useEngine()` in the UI);
never write "Taurilanzi" in them.

`tauri dev` watches `src-tauri`: every Rust change rebuilds and relaunches the app, and the window
jumps to the front. So it is stopped before starting work (see workflow) and launched again at the
end for the user to try.

## Workflow

1. **Stop any running `tauri dev`** before touching code.
2. **Build everything that was asked**, whole, without running tests halfway.
3. **Check it**: build, `typecheck`, clippy, and that it does what it should.
4. **Ask the user to try it**, with `tauri dev` running and simple, concrete steps.
5. **Write the tests at the end**, once the user is happy with it.

The e2e tests **only run complete and right before a commit**, never one spec at a time while
developing, except to take screenshots for a UI review (below).

App environment variables:

- `TAURILANZI_NO_DOCK=1` does not open the USB device (the dock stays with whoever has it).
- `TAURILANZI_DATA_DIR=<folder>` uses another data folder instead of the real one, and turns off
  the single-instance lock. Without it, `tauri dev` uses the development app's own data folder,
  apart from the released app's.
- `TAURILANZI_NODE=<path>` runs plugins with another Node.
- `TAURILANZI_COMMUNITY_CATALOG=<url>` reads another community plugin catalog (the e2e tests serve
  their own).
- `TAURILANZI_LANGUAGE=es|en` stands in for the system's language.

Python always through `uv` (`uv run`, scripts with PEP 723 dependencies), never `python`/`pip`
directly.

The user may spend days with Ulanzi Studio driving the dock: for UI work, `dev:no-dock`. Never
close Ulanzi Studio or connect to the real dock unless asked.

Never run tests or scripts that create, change or delete data against the real configuration:
the user may be using the app at the same time. That is what `TAURILANZI_DATA_DIR` and the e2e
tests (which use it) are for.

Commits, versions and releases:

- Commit messages follow **Conventional Commits**: `type(scope): description`, with `!` for a
  breaking change. Types: `feat`, `fix`, `perf`, `refactor`, `build`, `docs`, `test`, `ci`,
  `style`, `chore`, `revert`. `bun install` sets up a `commit-msg` hook that refuses anything else
  (`scripts/check-commits.ts`), and the `commits` job of the tests checks every push.
- The version is the moment of the build in UTC, `YEAR.MONTHDAY.HOURMINUTE` (`2026.1220.2008`;
  `2027.105.930` is 5 January at 09:30), set by `scripts/build-app.ts` (`scripts/release-version.ts`).
  `tauri.conf.json` and `Cargo.toml` keep `0.0.0`: never write a version there by hand.
- Every push to main that passes the tests is released by `.github/workflows/release.yml` if a
  commit since the last release is a `feat`, `fix`, `perf`, `refactor` or `build`, or marked as
  breaking (`scripts/release-plan.ts`). For now its notes only link to the changes since the
  release before. Running the workflow by hand releases main anyway. Each system and processor
  (macOS, Windows and Linux, on x64 and Arm) builds on a runner of its own
  (`scripts/release-assets.ts`), and the release is published once all of them are in, with one
  `latest.json` for every updater (`scripts/release-manifest.ts`).
- Updates are signed in GitHub Actions only: the private key and its password are secrets of the
  `release` environment (limited to main), with a backup kept by the owner outside any computer
  the project runs on. Only the public key is in the repo (`plugins.updater.pubkey`). Releases are
  immutable once published.
- Only the repository's admins change `.github/`, `.githooks/`, `scripts/`, `src-tauri/tauri*.json`
  and the `.gitignore` files: `.github/workflows/guard.yml` closes any other pull request that
  touches them. It runs on `pull_request_target`, so it must never check out or run the pull
  request's code.
- Builds made by hand with `scripts/build-app.ts` are signed with a test key of the computer's own
  (`~/.tauri/taurilanzi-test.key`, created on the first build) and trust it instead of the real
  one, which is how the updater is tried locally. Only release builds look for updates: the
  endpoint is in `src-tauri/tauri.bundle.json`.
- The installed app registers to start with the session as `com.taurilanzi.desktop`: a
  LaunchAgent in `~/Library/LaunchAgents` on macOS (to clean up a test: `launchctl bootout
  gui/$UID/com.taurilanzi.desktop` and delete the `.plist`), a value of
  `HKCU\Software\Microsoft\Windows\CurrentVersion\Run` on Windows, and
  `~/.config/autostart/com.taurilanzi.desktop.desktop` on Linux.

## Conventions

- Rust: a module with submodules lives entirely in its folder, with a `mod.rs` that only declares
  submodules and re-exports. Never a `module.rs` next to the folder. Constants and logic go in
  files with their own name (e.g. `hardware.rs`), not in `mod.rs`.
- File names say what they hold (`protocol.rs`, `connection.rs`), never generic ones.
- Comments only when they explain something the code cannot: a why, a non-obvious decision, a
  workaround. Never comments that repeat the code.
- macOS, Windows and Linux, on x64 and Arm, without Apple or Windows code signing. Whatever
  depends on the system has one file or folder per system behind `cfg` (`macos`, `windows`,
  `linux`, and `unsupported` for any other), with the same functions in each. Helpers several
  modules share live in `src/os/<system>/`: on Windows COM, processes and windows, shortcuts; on
  Linux desktop files, icons, D-Bus, systemd scopes. Files the Linux packages install are in
  `src-tauri/packaging/linux/`. The UI tells the systems apart with `os` from
  `src-nuxt/app/lib/os.ts`.
- Linux: keys are pressed through a uinput virtual keyboard, the same under X11 and Wayland, and
  text is typed with the user's own keyboard layout. The dock and that keyboard need the udev
  rule in `src-tauri/packaging/linux/70-taurilanzi.rules`: the packages install it, and otherwise
  the app installs it with `pkexec`. Which app is in front comes from X11, a KWin script, the
  wlr-foreign-toplevel protocol or, on GNOME, an extension of the app's, which it installs when
  asked; the scripts are next to their code in `src/foreground/linux/`.
- Keys are drawn on the computer as whole images (layers → SVG → PNG with `resvg`); the legacy
  mode (title drawn by the firmware) only for compatibility.
- Ulanzi plugins run on Node, never on Bun: the app's own Node, pinned in
  `scripts/node-runtime.json` for each system and processor and bundled only by `build-app.ts`
  (`tauri.bundle.json`). To change its version, update the version and the nodejs.org SHA-256
  checksums in that file.
- The logo and icons come from the SVGs in `docs/brand/`. App icons: `bun run tauri icon
  docs/brand/app-icon.svg`, then delete the `android/`, `ios/` and `64x64.png` it also writes, and
  copy `icon.ico` to `src-nuxt/public/favicon.ico`. `src-tauri/icons/tray.png` is
  `tray-icon.svg` at 64 px, black and transparent: macOS tints it as a template. Windows and
  Linux show tray icons as they are, so there the tray has the app icon (`128x128.png`).
- Dev UI on port **1420** (3000 is used by other local projects).
- After `shadcn-vue add`, check `src-nuxt/app/assets/css/tailwind.css`: shadcn inserts a Google
  Fonts import again. The font is local (`@fontsource-variable/inter`), with no third-party requests.
- The UI only talks to the engine (Tauri IPC) and to `api.iconify.design` (icon search). Any
  other third party needs the CSP in `tauri.conf.json` opened and a discussion first. External
  links open in the browser with `openExternal` (`opener` plugin, limited to `https://`).
- Right click never shows the browser menu (blocked in the layout); custom context menus open
  with `useContextMenu().open(event, items)`.
- In development Nuxt does not register new shadcn components and Tailwind does not see new files
  until `bun run tauri dev` restarts.
- To review the UI without asking for screenshots: an e2e test with `shot()` (a capture of the
  real app through WebdriverIO) saves images in `tests/e2e/screens/`. Headless Chrome against
  `:1420` does not work: outside Tauri the UI has no IPC.

## The D200

What is easiest to forget:

- Every HID write carries the report ID `0x00` first (1025 bytes).
- No continuation report may start with `0x00` or `0x7C`: the firmware silently drops keys. The
  ZIP carries a padding entry first, adjusted until it holds.
- Screen of 1220×708: keys of 196 with gaps of 60. The wide key (`3_2`, index 13) is 452×196 and
  the firmware stretches whatever it gets to that size; it only shows without its own clock in
  background mode (`0x0006` with `2|0|0|00:00:00|0`).
- The firmware shrinks a key while it is held down, by itself: the app draws no press effect.
- A key image takes the dock about 36 ms; it does about 27 key updates a second for one key and
  12 for all thirteen at once.
- Only one process can talk to the dock at a time: to test against the device close Ulanzi Studio
  and open it again afterwards.

## Public repository: be extremely careful with commits and pushes

This repository is **public**. Whatever is committed or pushed is exposed for good (even if deleted
later, it stays in the history, forks and caches).

Before every `git add`, commit or push:

- Review the whole diff (`git diff --staged`), not just the file names.
- Never `git add -A` / `git add .` blindly; add files explicitly.
- Never commit or push unless the user asks.

Never let into the repository:

- Binaries, firmware, assets, resources or decompiled code of Ulanzi Studio. Describe the protocol
  and behaviour in our own words, never paste someone else's code.
- USB/HID traffic dumps or any capture of the device or the system.
- Secrets: tokens, API keys, credentials, `.env` files, signing keys.
- Personal or local data: absolute paths with the user name (`/Users/...`), emails, serial numbers
  or unique identifiers of a device, machine names.
- Local tool settings (`.claude/settings.local.json`, MCP configs with local paths).

`.gitignore` is a safety net, not a substitute for reviewing the diff. When in doubt about
whether something can be published, ask before committing.
