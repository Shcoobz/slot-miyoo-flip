SLOT FOR MIYOO FLIP
===================

This is an unofficial fork of slot, a Game Boy-centric frontend made by
Brandon Kowalski for the Anbernic RG SP (https://github.com/BrandonKowalski/slot).
This fork runs it on the Miyoo Flip. It plays Game Boy, Game Boy Color and
Game Boy Advance games only. It is not made or supported by the original
author: please report Flip problems to this fork, not to upstream slot.


WHAT YOU NEED
-------------

  1. A Miyoo Flip.
  2. baseos-my355 on SD card 1:
       https://github.com/apommel/baseos-my355
     Follow its install guide first. This fork does not replace it.
  3. A second SD card, formatted FAT32, for the left slot. This is where
     slot and your games live.


INSTALL
-------

Copy everything inside the "flip-card" folder (not the folder itself) to
the root of card 2. When you are done, the root of card 2 should hold
.tmp_update, System, Config, Games and the other folders.

Note: .tmp_update is a hidden folder. Make sure your file manager shows
hidden files, or it may not be copied.

Put card 2 in the left slot and turn the Flip on. baseos-my355 starts slot
from .tmp_update/updater.

Nothing needs to be built. Updating is the same: copy a newer flip-card
over the old one. Your games, saves, labels and settings are kept, as long
as you do not delete them.


WHERE THINGS GO
---------------

Games
  Games/GBA     .gba files
  Games/GB      .gb files
  Games/GBC     .gbc files

  Games must be unzipped: slot does not read .zip or .7z files.
  A file loose at the top of Games/ is never shown; put it in the folder
  of its system.

  You can use one level of subfolders, for example
      Games/GBA/Pokemon Hacks/My Hack.gba
  Each subfolder shows up as a folder on the shelf. Folders inside
  folders are not read.

Labels (cart art)
  Labels/GBA/<game>.png, Labels/GB/<game>.png, Labels/GBC/<game>.png
  Named exactly like the game file, for example
      Games/GBA/Metroid Fusion.gba  ->  Labels/GBA/Metroid Fusion.png
  See Labels/README.txt for sizes.

Bezels
  Bezels/GB, Bezels/GBC, Bezels/GBA
  Put your own bezel .png files here (for example drkhrse's bezels), then
  pick one in Settings > Screen. Third-party bezels are not included; get
  them from whoever made them. See Bezels/README.txt for sizes.

BIOS (all optional, never included)
  BIOS/gba_bios.bin    16384 bytes  (Game Boy Advance)
  BIOS/gb_bios.bin       256 bytes  (Game Boy)
  BIOS/gbc_bios.bin     2304 bytes  (Game Boy Color)
  The names must match exactly (lowercase). Games run without them.
  You must dump these from your own hardware; they are not part of this
  download and never will be.

Cheats
  Cheats/GBA/<game>.cht, Cheats/GB/<game>.cht, Cheats/GBC/<game>.cht
  Libretro cheat format, named like the game file. Turn cheats on and
  off in the in-game menu.

Stick calibration (recommended)
  The Flip's analog sticks need the calibration files from its stock
  firmware. baseos-my355 does not provide them, so slot lends the system
  the folder Config/userdata on card 2 for them.

  Copy these two files into Config/userdata/ on card 2:
      joypad.config
      joypad_right.config

  Where to get them: the stock Miyoo firmware keeps them in /userdata.
  If you used spruce before, it keeps its own copy of these files on its
  card too. Each Flip has its own calibration, so use the files from your
  own device if you can.

  Without these files the sticks are uncalibrated: they can swing fully
  at the smallest touch. The d-pad and buttons work either way.


WHAT THIS FORK ADDS
-------------------

  Hardware
  - Miyoo Flip support: display, buttons and sticks, lid, rumble, LED,
    headphones and HDMI.
  - TV output over HDMI: NES and SNES fill the screen at 4:3, with a
    sharp 2x mode.
  - Real sleep when you close the lid.
  - Battery savings: Dim Screen and Auto Sleep settings, a CPU cap while
    a light core plays (lifted for fast-forward), and volume set on the
    sound chip, with the amplifier off at mute.
  - Tilt for games such as WarioWare: Twisted!, steered with a stick.

  Systems and cores
  - NES (FCEUmm, Nestopia) and Super Nintendo (Snes9x, Snes9x 2005 Plus)
    on their own shelves, with their own button layouts.
  - Gambatte for Game Boy and Game Boy Color, with slot's palettes.
    mGBA and gpSP are there for Game Boy Advance, as in upstream slot.
  - Core per game, per folder or per system.
  - Turbo A and B on the Game Boy and NES pads.

  Shelf and menus
  - Folders on the shelf, drawn as taped stacks of carts. A opens a
    folder. START on a folder opens its menu: Rename, Cover, Sort, Core,
    Open at, Paper.
  - Game switcher: the recently played games, newest first. X opens a
    game's save states.
  - An Extras shelf with Activity report, Get lucky and Never played.
  - Play-time tracking, shown on the open cartridge.
  - Delete a game from its cartridge view with Y.
  - Cheats in the in-game menu.


WHAT IS IN THE DOWNLOAD
-----------------------

  System/            slot, the emulator cores and helper scripts
  System/licenses/   licences and source archives of the bundled parts
  .tmp_update/       the file baseos-my355 starts slot through

slot and this fork are free software under the GPL-3.0. The bundled cores
are mGBA (MPL-2.0), gpSP (GPL-2.0) and Gambatte (GPL-2.0). The built-in
SP bezel is "Perfect GBA SP (no grid)" by mugwomp93 (CC BY-SA 4.0). No
games, BIOS files or third-party bezels are included.

Game Boy is a trademark of Nintendo. Not affiliated with Nintendo,
Miyoo, Anbernic or the original slot project.
