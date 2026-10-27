#!/bin/sh
# Applies the udev rule the package installed to the dock and the virtual keyboard right away,
# without waiting for a reboot.
modprobe uinput 2>/dev/null || true
if command -v udevadm >/dev/null 2>&1; then
  udevadm control --reload-rules || true
  udevadm trigger --action=change --subsystem-match=misc --subsystem-match=hidraw --subsystem-match=usb || true
fi
exit 0
