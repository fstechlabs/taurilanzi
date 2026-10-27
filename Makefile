RELEASED_APP := com.taurilanzi.desktop
RELEASED_PROCESS := Taurilanzi.app/Contents/MacOS/
QUIT_TIMEOUT := 10

.PHONY: dev

# Quits the installed Taurilanzi, which holds the dock, and runs the development app with it.
# Quitting through the app itself makes it exit cleanly, so its LaunchAgent does not start it again.
dev:
	@if pgrep -qf '$(RELEASED_PROCESS)'; then \
		echo "Quitting the installed Taurilanzi"; \
		osascript -e 'tell application id "$(RELEASED_APP)" to quit'; \
		waited=0; \
		while pgrep -qf '$(RELEASED_PROCESS)'; do \
			if [ $$waited -ge $$(( $(QUIT_TIMEOUT) * 5 )) ]; then \
				echo "The installed Taurilanzi did not quit in $(QUIT_TIMEOUT) s"; exit 1; \
			fi; \
			sleep 0.2; waited=$$(( waited + 1 )); \
		done; \
	fi
	bun run tauri dev
