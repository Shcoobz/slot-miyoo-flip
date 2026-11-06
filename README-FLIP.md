# slot for Miyoo Flip (unofficial fork)

This repository is an unofficial fork of [slot](https://github.com/BrandonKowalski/slot), a
Game Boy-centric frontend for GB, GBC and GBA games, made to run on the **Miyoo Flip** under
[baseos-my355](https://github.com/apommel/baseos-my355).

It is not made or supported by the original author. Please report Flip problems here, not to
upstream slot.

## Install (no building needed)

The ready-made card is in the [`flip-card/`](flip-card) folder of this repository.

1. Install [baseos-my355](https://github.com/apommel/baseos-my355) on SD card 1 by following
   its install guide.
2. Format a second SD card as FAT32. It goes in the Flip's left slot.
3. Download this repository (Code > Download ZIP, or `git clone`).
4. Copy **everything inside** `flip-card/` (not the folder itself) to the root of card 2.
   `flip-card/` contains a hidden folder, `.tmp_update`, so make sure hidden files are copied
   too.
5. Add your games to `Games/GBA`, `Games/GB` and `Games/GBC` (unzipped), put card 2 in the
   left slot and turn the Flip on.

Where labels, bezels, BIOS files, cheats and the stick calibration files go is explained in
[`card/README-FLIP.txt`](card/README-FLIP.txt), which is also copied to the root of the card as
`README-FLIP.txt`.

The short version for the sticks: copy `joypad.config` and `joypad_right.config` from your
Flip's stock firmware (`/userdata`) into `Config/userdata/` on card 2. Without them the analog
sticks are uncalibrated.

## What this fork adds

- Miyoo Flip support: display, buttons and sticks, lid, rumble, LED, headphones and HDMI.
- Gambatte for Game Boy and Game Boy Color (mGBA and gpSP for GBA, as upstream).
- Folders on the shelf, drawn as taped stacks. A opens a folder; START opens its menu:
  Rename, Cover, Sort, Core, Open at, Paper.
- An Extras shelf: Activity report, Get lucky, Never played.
- Real sleep when the lid is closed.
- Play-time tracking.
- Delete a game from its cartridge view with Y.
- Cheats (libretro `.cht` files in `Cheats/<SYS>/`) in the in-game menu.

## Credits

- **slot** by Brandon Kowalski, GPL-3.0: <https://github.com/BrandonKowalski/slot>.
  Nearly everything here is his work; this fork adds the Flip port and the features above.
- **baseos-my355** by apommel: <https://github.com/apommel/baseos-my355>.
- Emulator cores: **mGBA** (MPL-2.0), **gpSP** (GPL-2.0), **Gambatte** (GPL-2.0).
- Fonts: **Permanent Marker** (Apache-2.0), **VT323** (OFL-1.1).
- Built-in SP bezel: "Perfect GBA SP (no grid)" by mugwomp93 (CC BY-SA 4.0).

## Licence notes

- The fork is licensed under the **GPL-3.0**, like slot. See [`LICENSE`](LICENSE).
- The corresponding source is this repository. The card also carries the source archives,
  patches and build stamps of the bundled cores and of `fsck.exfat` in `System/licenses/`.
- The licence texts of the bundled third-party parts are in [`licenses/`](licenses) (copied to
  `System/licenses/` on the card).
- **No BIOS files, ROMs or third-party bezels are ever included.** Bring your own; bezels made
  by others stay with whoever made them.
- Snes9x note: if SNES support is ever added, keep in mind that Snes9x is **not** GPL and its
  licence forbids commercial use. It could only be shipped as a separate, non-commercial core,
  never folded into the GPL build.

Game Boy is a trademark of Nintendo. This project is not affiliated with Nintendo, Miyoo,
Anbernic or the original slot project.

## For developers

### Building the card

```sh
task flip-card
```

This runs `dist:flip` with `OUT=flip-card`: it builds slot for the device, builds mGBA, gpSP,
Gambatte and a static `fsck.exfat` from pinned sources, and writes the whole card tree to
`flip-card/` (the folder is wiped first). `task dist:flip` does the same into `dist-flip/`,
which is not committed.

You need [Docker](https://www.docker.com/) and [go-task](https://taskfile.dev/), plus `curl`,
`file` and `shasum` on the host. The device build runs in an arm64 container automatically
(the `device:sh` task: `docker run --platform linux/arm64 ... rust:1-bullseye`) unless the host
is already Linux on aarch64. On an x86 machine that means Docker needs arm64 emulation
(QEMU/binfmt) set up.

`flip-card/` is committed on purpose, `.so` cores included (`.gitignore` has
`!/flip-card/**`). Rebuild and commit it whenever the source changes.

### Testing on a PC

- The tests run on x86 Linux. An easy way is Docker with `rust:1-bookworm`, using a separate
  target directory so it does not mix with the device build, for example:

  ```sh
  docker run --rm -v "$PWD":/src -w /src rust:1-bookworm \
    cargo test --workspace --target-dir target-x86b
  ```

  The host build pulls in audio, gamepad and EGL libraries, so you will likely need to
  `apt-get install` packages such as `pkg-config`, `libasound2-dev`, `libudev-dev` and Mesa's
  EGL (`libegl1`, `libegl-mesa0`) in the container first.
- `crates/slot/examples/shots.rs` draws slot off-screen and saves chosen frames as PNG, to
  check a screen without the handheld:
  `cargo run -p slot --example shots -- ROOT SCRIPT OUT_DIR [WxH]` (use `640x480` for the Flip).
  The script uses `wait N`, `tap BTN`, `down BTN`, `up BTN` and `shot NAME`.
- The upstream task list (`task --list`) and [`README.md`](README.md) cover everything else.

### Cart Studio

A local copy of Cart Studio (cart labels in the browser) lives next to this repository in
`slot-cart-studio/`; its `README.md` explains how to run it yourself.
