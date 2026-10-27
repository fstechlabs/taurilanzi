# Contributing to Taurilanzi

Thanks for wanting to help. Bug reports, ideas and pull requests are all welcome.

## Before you start

- **Bugs and ideas**: open an issue describing what happens, what you expected and, for bugs, your
  dock model and macOS version.
- **Pull requests**: for anything beyond a small fix, open an issue first so we can agree on the
  approach before you spend time on it.

## License of contributions

Taurilanzi is licensed under the [GNU AGPL v3.0](LICENSE). By opening a pull request you agree that
your contribution is released under the same license. You keep the copyright of your work and it
stays credited to you in the project's history.

## Working on the code

[`AGENTS.md`](AGENTS.md) describes the layout of the project, the commands to run it and test it,
and the conventions the code follows. In short:

```bash
bun install                  # dependencies and the Node that runs plugins
bun run tauri dev            # run the app (quit Ulanzi Studio first: one app per dock)
bun run typecheck && bun run test
cd src-tauri && cargo clippy --all-targets && cargo test
bun run e2e                  # end-to-end tests, before opening the pull request
```

- Code, comments and commit messages are in English.
- Commit messages follow [Conventional Commits](https://www.conventionalcommits.org):
  `fix(driver): resend the keys the dock dropped`. Their type decides whether the change is
  released: `feat`, `fix`, `perf`, `refactor` and `build` are.
- Every text the app shows exists in Spanish and English (`src-nuxt/app/locales/` and
  `src-tauri/locales/`).
- Keep pull requests focused on one change, with tests for what they add.
- Pull requests must not touch `.github/`, `.githooks/`, `scripts/`, `src-tauri/tauri*.json` or any
  `.gitignore`: they hold the CI, the release that signs every update and the updater's settings,
  and only the maintainers change them. A pull request that does is closed automatically; open an
  issue instead.
