# Takumi Anything changes

Takumi Anything (formerly OSCraft) is based on Craft Anything by the ArtCraft team
(https://github.com/storytold/craft-anything), modified by Aleksandr Mitroshenkov. The fork starts
from upstream commit `4f0f744b`; everything below is on top of it. The original history is kept in
full. Entries below this first one say OSCraft, the name they were made under.

## The Takumi Anything name
- New name **Takumi Anything** (匠 *takumi*, master craftsman). Binaries `takumi-anything` and
  `takumi-anything-cli`, app id `io.github.mitroshenkov87.takumi-anything`, MCP server name
  `takumi-anything`, repository `github.com/Mitroshenkov87/takumi-anything`.
- New flat icon, a mason's trowel laying a brick wall (`assets/app-icon`), with a small drawing for
  16-32 px and the title bar, and a new wordmark (`docs/brand/takumi-anything-wordmark.svg`). The
  hydra-and-anvil icon and the OSCraft wordmark are gone.
- Settings live in `%APPDATA%\takumi-anything`, the engine and models in
  `%LOCALAPPDATA%\takumi-anything` (`TakumiAnythingData` beside a portable exe), new projects in
  `Documents\Takumi Anything`; `TAKUMI_ANYTHING_CONFIG_DIR` replaces `OSCRAFT_CONFIG_DIR`.
- The first start copies (never moves) what the new folders lack from the OSCraft ones, once, and
  leaves `migrated-from-oscraft.txt`; big files are hard-linked when on the same drive. Craft
  Anything's folders are still never read.
- The MSI keeps the OSCraft UpgradeCode, so installing Takumi Anything removes an OSCraft install
  and installs to `Program Files\Takumi Anything`.

- Irreversible phone changes at your own risk: a red option at the bottom of a project's
  Approvals menu, off by default, lets flashing, rooting, unlocking the bootloader and wiping
  partitions go ahead without a card each time. It is only switched on after a warning that the
  phone can be damaged for good and that it is the person's own risk; switching it off is one click.
  The CLI's `rebuild --yes-irreversible` prints the same warning and turns it on for the project,
  `--unattended` included. Builders can turn it off but never on. Nothing else changes: other
  kinds keep their approvals and computer-side work (disks, system changes) keeps its usual auto
  approval.
- Two button sizes, and no others: every one-line action is 168x40 whatever its label (a long one
  wraps onto two lines in the label size, then ends in "…" with the whole as a tooltip), and every
  described choice tile (Make's starts, Devices, Clean room/Deep, folders, pins) is 80 px tall,
  room for three lines. A narrow window that has less room than 168 gives the button what there is.
  Big first-run cards are all 200 px tall.

## Devices, Clean room tiles, Platform-Tools
- Make has a fifth start, **Devices**, with four starters: an Android app (Kotlin, builds an APK),
  looking inside an app or firmware image (unpack, decompile, report permissions), a per-model
  root and flash plan with checked adb/fastboot scripts, and phone utilities over adb.
- Changing a connected phone is a new approval kind, `device`: `fastboot flash/erase/oem/flashing`,
  `adb sideload/root/remount`, rebooting into the bootloader, recovery or EDL, raw writes from
  `adb shell`, and the Samsung, Qualcomm EDL and MediaTek flashing tools. Auto mode refuses it;
  otherwise each one asks on a card that says it can't be undone, with no "always".
- Settings → Plugins installs Android Platform-Tools 37.0.3 (Google's official zip, checked by
  SHA-256) with one button. Building Android apps also needs the Android SDK and Gradle.
- The Clean room / Deep tiles on Make always switch the mode (the title-bar switch follows). When
  the chosen AI can't hold a Clean room, the tile and a failed build offer "Build with …" for one
  that can.

## Kimi builder fix (`f1c1e55f`)
MCP tool input schemas may contain boolean sub-schemas (`true` / `false`). The Kimi coding agent
rejected them (Moonshot API error 400), so Kimi could not run as a builder. Boolean sub-schemas are
now written as equivalent object schemas in `tools/list`.

## Cyrillic headings and Ubuntu UI font (`0b448a95`, `18595333`, `6731d6fe`)
Cyrillic and Greek headings stay in egui's face before the craft-fonts bold, and Segoe UI Bold is
loaded for Cyrillic headings (0.6.1 fell back to Ubuntu Light at every weight). The interface uses
the official Ubuntu 0.83 Regular, Medium, Bold and Light faces.

## Self-repair (`1c3e4009`, `7916df34`)
Fixes found while repairing a broken local install: formatting, clippy and Windows-only test
failures; the prompt box keeps three rows with the Ubuntu font; demo and test runs no longer read
or write the user's own `config.toml`.

## Ghidra 12.2 with Java 25 (`1cde8bce`)
The managed Ghidra engine is Ghidra 12.2 with Eclipse Temurin JDK 25. A managed Java older than the
one Ghidra requires is replaced instead of reused.

## REA MCP integration (tested configuration)
The REA toolkit (`rea-agents`) runs as an MCP plugin next to the built-in tools, pointed at the
managed Ghidra 12.2 and JDK 25 through `GHIDRA_INSTALL_DIR` and `JAVA_HOME`.

## OSCraft rename
- Name OSCraft, binaries `oscraft` and `oscraft-cli`, app id `io.github.mitroshenkov87.oscraft`.
- Windows installer: product OSCraft, manufacturer Aleksandr Mitroshenkov, its own UpgradeCode and
  component GUIDs, `C:\Program Files\OSCraft`, registry key `HKLM\Software\OSCraft`. It installs side
  by side with the official app and never upgrades or removes it.
- Settings in `%APPDATA%\oscraft` (env override `OSCRAFT_CONFIG_DIR`, portable data `OSCraftData`).
  OSCraft always starts from zero: nothing is imported from Craft Anything's settings, engines or
  plugins, so the first start shows the first-run setup.
- New projects default to `Documents\OSCraft`.
- Update check and links point to `Mitroshenkov87/oscraft`; until releases exist the check reports
  "up to date" instead of an error.
- The ArtCraft name, logos and the raven icon were removed; a new flat icon set and wordmark were
  drawn for OSCraft (`assets/app-icon`, `docs/brand/oscraft-wordmark.svg`).
- Internal crate names (`craft-anything-*`), the WASM plugin interface (`craft-anything:plugin`)
  and other `CRAFT_ANYTHING_*` environment variables are unchanged, so plugins and scripts keep working.
- The translated READMEs described the upstream project and community; they were removed. The
  upstream repository has them.

## Header label, plain pages, CLI parity
- The title bar only shows Clean room or Deep as a label: no switch, no arrow, nothing to press.
  The choice is made on the two big cards on Make, or in Settings.
- A plain page (an `index.html` with no build script and no dependencies, with or without a
  `package.json` that only has `start` or `check` scripts) is built as it is and served by OSCraft:
  the build check passes, the screenshot check pictures the served page in the private browser, and
  the game self-test passes when the page loads without script errors. Before, a plain HTML/JS game
  failed with "Missing script: build" and "no built program to start".
- CLI parity for the Devices start and the Clean room switch: `rebuild --device
  <android-app|research|root-flash|utilities>` asks for the same request as the tile; a Clean room
  run whose builder works only in Deep mode is refused before anything is made, naming the AIs that
  can build there (`--backend "<AI>" --clean-room`); the terminal prompt for changing a phone says it
  can't be undone, takes only the whole word `yes`, and has no "always".
