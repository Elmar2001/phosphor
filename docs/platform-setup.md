# Platform setup

Phosphor drives the Iceman `proxmark3` client. The Windows installer ships
the client; on macOS and Linux you install it yourself.

## Where Phosphor looks for the client

In order, first match wins:

1. **Settings > PM3 RUNTIME > PM3 CLIENT PATH**, if set. Must be an absolute
   path to a file named `proxmark3` (or `proxmark3.exe`).
2. **Bundled client** next to the Phosphor executable (Windows installer).
3. **`proxmark3` on `PATH`.**
4. **Known install locations:**
   - Windows: `C:\proxmark3\proxmark3.exe`, `C:\Program Files\proxmark3\proxmark3.exe`
   - macOS: `/opt/homebrew/bin/proxmark3`, `/usr/local/bin/proxmark3`, `/opt/local/bin/proxmark3`
   - Linux: `/usr/local/bin/proxmark3`, `/usr/bin/proxmark3`

The **DIAG** tab shows every candidate, which one is in use, and whether it
actually starts.

## Windows

Install Phosphor and plug in the Proxmark3. If it isn't found:

- Device Manager > Ports should list a COM port for it. If not, try another
  cable (many are charge-only).
- Some PM3 Easy clones need the CH340 driver (wch-ic.com).
- Antivirus sometimes quarantines `proxmark3.exe`. DIAG reports a client
  that can't start, and names a missing DLL if that's the cause.

## macOS

```bash
brew install rfidresearchgroup/proxmark3/proxmark3
```

Apps started from Finder don't see your shell `PATH`, which is why the
Homebrew and MacPorts locations are checked explicitly. A self-built client
can be selected in Settings. The device shows up as
`/dev/tty.usbmodemiceman1` or similar.

## Linux

Build and install the client following the Iceman repository's
instructions, then:

- **Serial permissions:** add yourself to the group that owns
  `/dev/ttyACM*` (usually `dialout`; `uucp` on Arch), then log out and in:
  `sudo usermod -aG dialout $USER`
- **ModemManager** probes new ACM devices and interferes with the PM3.
  Stop it (`sudo systemctl stop ModemManager`) or exclude the device with a
  udev rule:

  ```
  # /etc/udev/rules.d/77-pm3-usb-device-blacklist.rules
  ATTRS{idVendor}=="9ac4", ATTRS{idProduct}=="4b8f", ENV{ID_MM_DEVICE_IGNORE}="1"
  ```

  `make udev` in the Iceman source tree installs an equivalent rule.

- RDV4 Bluetooth add-on users can pick their `/dev/rfcomm*` port in Settings.

## Detection and overrides

Detection probes serial ports in this order: the preferred port from
Settings, ports whose USB ID identifies a Proxmark3 (`9AC4:4B8F`,
`2D2D:504D`), other USB serial ports, Bluetooth ports, then a fixed list of
common names as a fallback. Set **PREFERRED PORT** when several serial
devices are connected or the PM3 doesn't report its USB ID (some clones,
Bluetooth).
