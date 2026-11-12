# CXSSM — CarX Street Sound Mod

Sound editor for **CarX Street** (PC), written in Rust with the [GPUI](https://gpui.rs) UI framework.

CXSSM opens the game's `fmod_assets_all.bundle`, finds the **FMOD** banks it contains
(engine RPM loops, turbo, exhaust, music, voices, UI…) and lets you replace each sound with an
audio file that has the same characteristics. Your work lives in a **`.cxssm` project folder**
(a copy of the original bundle, your audio files and the rebuilt bundle), and CXSSM installs
the mod into the game with one click — and puts the original file back just as easily.

> Replaced sounds have been validated in game. Research notes (in French):
> [docs/RECHERCHE_FMOD.md](docs/RECHERCHE_FMOD.md).

---

## How it works

```
fmod_assets_all.bundle (UnityFS)
 └─ SerializedFile "CAB-…"
     └─ TextAsset  "V8.bank" …            ← one FMOD Studio bank per TextAsset
         └─ RIFF "FEV " (.bank)
             ├─ events / parameters       ← kept untouched (RPM, load, buses…)
             └─ "SND " chunk → FSB5        ← the sounds, replaced one by one
```

1. **Open**: decompresses the bundle (LZ4/LZMA), reads the TextAssets, then parses the banks
   and their FSB5 sound banks (codec, sample rate, channels, length and loop of every sound).
2. **Replace**: pick a WAV, OGG, FLAC or MP3 file. CXSSM checks its sample rate and channel
   count against the original (green or red indicator).
3. **Build**: re-encodes to the original codec, rebuilds the FSB5, the bank, the
   SerializedFile and the bundle, written to the project's `mod/fmod_assets_all.bundle`
   (or `<chosen folder>/` without a project) together with a `cxssm-mod.txt` summary.
4. **Install**: copies the built bundle over the game's file; **Restore** puts the original back.

## Projects (`.cxssm`)

**New project** copies the game's original bundle into a new folder:

```
My mod/
├─ My mod.cxssm                    project file (TOML): replaced sounds, edit settings, game file
├─ original/fmod_assets_all.bundle copy of the game's original bundle
├─ mod/fmod_assets_all.bundle      bundle built by CXSSM
└─ audio/
   ├─ imported/    files you imported (copied into the project)
   ├─ edited/      tracks rendered by the track editor (volume, pitch, recordings)
   ├─ exported/    original sounds you exported
   └─ recordings/  car recordings (acceleration, deceleration)
```

- The project is saved automatically; the last one reopens on startup.
- Sounds are matched by bank and sample name, so they survive a game update.
- The **Project** window shows the state of the game file — *mod installed*, *original*,
  *game updated*, *modified by something else* — and offers install, restore and
  **Update the project's original** (after a game update; installing is blocked until then,
  so a mod built on an old bundle is never installed on a new game version).
- CXSSM refuses to use an already modified bundle as the original: it compares the file size
  with the size declared in the game's Addressables catalog (`aa/catalog.json`).

## Track editor

The **Edit** tab of a sound adjusts its volume (±12 dB) and pitch (±6 semitones). The source
can be the game's **original** sound (to make a sound quieter or deeper without any other
file), an imported **file**, or — for engine sounds, experimental — a car **recording**. For engine
sounds it can also rebuild the loops from two continuous recordings of the car (full
acceleration and full deceleration): CXSSM tracks the RPM in the recording, picks the passage
closest to each loop's natural RPM, corrects its pitch and cuts a seamless loop of whole
engine cycles. **Generate all of the car's tracks** does it for every loop of the car.

Several sounds can be selected at once (Ctrl / Cmd + click to add or remove, Shift + click for
a range, Ctrl+A for the whole list): the volume and pitch set in the Edit tab then apply to all
of them. Every change can be undone and redone (Ctrl+Z, Ctrl+Shift+Z, or the arrows in the app
bar).

Cars that use a replaced sound — including cars sharing a sound with the one you edited — get a
dot in the tree (their bank is mapped in the background).

### Keyboard shortcuts

| Action | Default |
|---|---|
| Start / stop the engine | `E` |
| Sweep up to the limiter | `S` |
| Throttle (hold) | `Space` |
| Undo / redo | `Ctrl+Z` / `Ctrl+Shift+Z` |
| Select all sounds | `Ctrl+A` |

All of them can be changed in **Preferences › Keyboard shortcuts** by pressing the new key, which
also makes them work with any keyboard layout.

## Engine simulator: finding which sounds to replace

Sound names are stripped from the game's banks (`sample_0000`…). To find out which sounds
make up a car's engine, CXSSM replays the **game's own FMOD events** through the official
FMOD Studio API, exactly as the game does:

- every car has an event `event:/GameEvents/Engines/<car>` (130 in total, in the `V6`, `V8`,
  `V10`, `V12`, `I4`, `I6`, `Flat` and `Wankel` banks);
- it is driven by **`RPM`** (0 → 10,500) and **`Load`** (0 = off throttle, 1 = on throttle),
  plus, depending on the car, `Turbo`, `Exhaust`, `CameraView`, `HP`…;
- FMOD layers 1 to 3 loops and adjusts their volume and pitch according to RPM.

In the app, **Simulateur → Moteur (RPM)**:
1. pick a car, then **▶ DÉMARRER** (start);
2. move the RPM / load sliders, or **hold « ACCÉLÉRER »** (throttle) to rev up; the
   adjustable **rev limiter** (« Rupteur ») makes the engine bounce at the chosen RPM;
3. **SONS ENTENDUS** shows the loops being played in real time (volume, pitch); click one to
   select it;
4. **CARTOGRAPHIER** sweeps the whole RPM range and shows the RPM range of every loop:
   this is the exact list of sounds to replace;
5. once you have replaced sounds, enable **MES SONS** to hear your mod before exporting it.

From the command line:
```sh
cxssm engine-map <bundle>                      # list the 130 engine events
cxssm engine-map <bundle> --event z28          # sound map of the z28
cxssm extract-banks <bundle> --out banks/      # extract the raw .bank files (Master, V8…)
cxssm find-game                                # detected game bundles and FMOD Engine installs
cxssm check-update                             # newer release on GitHub?
```

The simulator uses `libfmodstudio` from the FMOD Engine (`api/` folder), loaded at runtime:
`CXSSM_FMOD_API=…/fmodstudioapi…/api`, or derived automatically from `CXSSM_FSBANKCL`.

## Preferences and game detection

The **⚙ PREFERENCES** button (top bar) opens the settings panel:

- **Language**: French or English (default: system language).
- **Simulator rev limiter**: on (RPM bounces on the adjustable limiter) or off (RPM climbs to the maximum).
- **Visible categories**: hide the sidebar categories you don't use.
- **Game**: open the game bundle automatically on startup, pick one of the detected
  installations, or choose a file manually.
- **FMOD Engine**: detected automatically (Downloads, `/opt`, `C:\Program Files (x86)\FMOD SoundSystem`,
  `/Applications`), or choose its folder manually (root, `api/` or `bin/` all work).
- **Updates**: check GitHub Releases on startup, then install the new version and restart
  from the panel (Nix installs are updated with Nix).

CXSSM looks for `fmod_assets_all.bundle` in the default Steam folders and in every Steam library
listed in `steamapps/libraryfolders.vdf` (games installed on other drives):

| System | Default Steam folders |
|---|---|
| Windows | `C:\Program Files (x86)\Steam` |
| Linux | `~/.local/share/Steam`, `~/.steam/steam`, Flatpak `~/.var/app/com.valvesoftware.Steam/.local/share/Steam` |
| macOS | `~/Library/Application Support/Steam` |

Inside the game folder it checks `CarX Street_Data/StreamingAssets/aa/*/` (Windows, Linux) and
`CarX Street.app/Contents/Resources/Data/StreamingAssets/aa/*/` (macOS). `cxssm find-game` prints
the folders examined and the bundles found.

Settings are stored in `settings.toml` in the system config folder
(`~/.config/cxssm/`, `%APPDATA%\cxssm\`, `~/Library/Application Support/cxssm/`).

## Preparing a replacement sound

| Rule | Details |
|---|---|
| **Same sample rate** | e.g. 48,000 Hz if the original is 48,000 Hz, otherwise the pitch changes. |
| **Same channel count** | engine sounds are usually mono. |
| **Similar length** | especially for **engine loops**: seamless loop, no click, no fade. |
| **Same pitch at the same RPM** | a loop used around 3,000 rpm must be recorded at ~3,000 rpm. |
| **16-bit WAV** recommended | the most reliable source for re-encoding. |

Engine loops come in *on load* (throttle) and *off load* (deceleration) sets, one per RPM
range. Always replace a **complete set** (every range of the same engine; use the simulator
map), otherwise the blend between your sounds and the original ones is audible.

To prepare files with [Audacity](https://www.audacityteam.org/): set the project rate,
mix down to mono if needed, then export as 16-bit WAV.

## Installation

### Binaries (Windows, Linux, macOS)

Download the archive for your system from the
[GitHub Releases](https://github.com/vabyz971/CXSSM/releases):

| System | Archive | Contents |
|---|---|---|
| Windows 10/11 x64 | `cxssm-vX.Y.Z-windows-x86_64.zip` | `cxssm-app.exe` (GUI), `cxssm.exe` (CLI) |
| Linux x86_64 | `cxssm-vX.Y.Z-linux-x86_64.tar.gz` | `cxssm-app`, `cxssm` |
| macOS 11+ (Intel and Apple Silicon) | `cxssm-vX.Y.Z-macos-universal.zip` | `CXSSM.app`, `cxssm` |

> **macOS**: the app is not notarized by Apple. On first launch, right-click → **Open**, or
> remove the quarantine flag: `xattr -dr com.apple.quarantine CXSSM.app`

FMOD is **not** included in these archives (see [FMOD licensing](#fmod-licensing)):
install the FMOD Engine separately.

### NixOS / Nix (flake)

```sh
nix run github:vabyz971/CXSSM                 # run the GUI
nix run github:vabyz971/CXSSM#cli -- --help   # command-line tool
nix profile install github:vabyz971/CXSSM     # install cxssm-app and cxssm
```

In a NixOS flake configuration:
```nix
inputs.cxssm.url = "github:vabyz971/CXSSM";
# …
environment.systemPackages = [ inputs.cxssm.packages.${pkgs.system}.default ];
```

The Nix package ships the libraries CXSSM needs (Vulkan, Wayland/X11, audio) as well as those
required by the FMOD `fsbank` tool (glib, zlib, libstdc++). Development shell: `nix develop`.

### Publishing a release (maintainers)

```sh
git tag v0.3.0 && git push Github main v0.3.0
```
The `.github/workflows/release.yml` workflow builds on Linux, Windows and macOS and attaches
the archives to a GitHub Release.

## Requirements

- **FMOD Engine** (free download, account required: <https://www.fmod.com/download>). It provides:
  - the official encoder (`fsbank` / `fsbankcl`), required because **every CarX Street sound
    is FMOD Vorbis** (not standard Ogg);
  - `libfmodstudio`, used by the engine simulator.

  CXSSM finds it automatically in the usual places, or you can choose its folder in
  **⚙ Preferences → FMOD Engine**. If it is not found, CXSSM opens a step-by-step
  installation guide on startup: the download link, how to extract or install it on your
  system, and a button to point to its folder. The guide can also be opened from the status
  bar and stays available in Preferences → FMOD. Environment variables still take precedence if set:
  ```sh
  # Linux: the binary is called `fsbank` (bin/ folder of the fmodstudioapi…linux archive)
  export CXSSM_FSBANKCL=~/Downloads/fmodstudioapi20400linux/bin/fsbank
  # Windows
  setx CXSSM_FSBANKCL "C:\Program Files (x86)\FMOD SoundSystem\FMOD Studio API Windows\bin\fsbankcl.exe"
  ```
- To build from source: **Rust** 1.85+ (`rustup`). On Linux, GPUI needs the Vulkan,
  Wayland/X11, xkbcommon and fontconfig development packages.
- **NixOS**: `nix develop` (flake) or `nix-shell` (`shell.nix`) provides everything (Rust,
  cargo, clippy, rustfmt, rust-analyzer and the libraries); no rustup needed.

### Linux display note

GPUI 0.2.2 on Wayland only redraws on compositor frame callbacks; with some compositors
(e.g. niri) the window can stay frozen after background work until it is resized. CXSSM
therefore runs through **XWayland** when it is available. Set `CXSSM_WAYLAND=1` to force Wayland.

## Build and run

```sh
cargo run -p cxssm-app --release [-- <bundle>]   # GUI (opens the bundle directly if given)
cargo run -p cxssm-cli -- list   <path>/fmod_assets_all.bundle
cargo run -p cxssm-cli -- bundle <path>/fmod_assets_all.bundle
cargo run -p cxssm-cli -- replace <bundle> --bank V8 --sound sample_0003 --file my_sound.wav --out output/
cargo test                                     # unit tests (the GPUI crate is excluded by default)
CXSSM_TEST_BUNDLE=<bundle> cargo test -p cxssm-core -- --ignored   # tests on the real game file
```

## Why not the Steam Workshop?

CarX Street's Workshop mods cannot replace sounds. The game's Addressables catalog
(`StreamingAssets/aa/catalog.json`) loads `fmod_assets_all.bundle` and its banks with Unity's
standard provider; only the bundles assigned to `CarX.Street.Mods.AssetBundleProviderForMods`
(cars, vinyls, UI…) look for Workshop overrides. A subscribed sound mod is downloaded into
`<game>/Mods/` but never read, whatever its size.

CXSSM therefore installs the mod in place of the game file (**Install in the game**). Steam
restores the original file on every game update or file verification: reinstall the mod
afterwards (the Project window shows it). To share a mod, share the built
`mod/fmod_assets_all.bundle` with instructions to copy it into
`CarX Street_Data/StreamingAssets/aa/<platform>/` — or the `.cxssm` project folder.

## Project layout

```
CXSSM/
├── Cargo.toml              workspace: shared dependencies and lints
├── flake.nix               Nix package (cxssm-app + cxssm), `nix run`, `nix develop`
├── shell.nix               NixOS dev shell (without flakes)
├── .github/workflows/      CI (clippy + tests) and Release (Windows, Linux, macOS)
├── packaging/macos/        Info.plist for CXSSM.app
├── crates/
│   ├── cxssm-unity/        UnityFS (LZ4/LZMA read, LZ4 write), SerializedFile, TextAsset
│   ├── cxssm-fmod/         RIFF/FEV bank, FSB5 (read, byte-identical rewrite), FMOD Vorbis decoder
│   ├── cxssm-fmod-studio/  runtime-loaded binding to the FMOD Studio API (the only `unsafe` crate)
│   ├── cxssm-core/         domain logic: ModProject, .cxssm workspace, audio checks, encoder, export, engine simulator, loop generator
│   ├── cxssm-cli/          `cxssm` binary (scripts, tests on the real file)
│   └── cxssm-app/          GPUI binary (see below)
└── docs/
    ├── RECHERCHE_FMOD.md   formats, constraints, risks, validation plan (French)
    ├── design/             reference UI design: Main.dc.html, Preferences.dc.html
    └── mockup/index.html   first UI mockup (open in a browser)
```

The GPUI app (`crates/cxssm-app/src`) follows the design in `docs/design/`:

```text
main.rs            window, embedded fonts and icons
theme.rs           colours, fonts, sizes and spacing (constants)
assets.rs          SVG icons + Inter / JetBrains Mono (OFL, assets/fonts)
widgets.rs         buttons, switch, segmented control, scrollbar, spectrum strip
state/             shared GPUI model `AppState`: bundle, project (.cxssm), selection,
                   simulator (RPM, load, limiter), playback, track editor, preferences, updates
ui/mod.rs          root view: lays out the components below
ui/tree.rs         category tree (Engine ▸ V8/V6… ▸ cars) with search
ui/rpm_simulator.rs  RPM readout, tach, load, limiter, throttle and sweep
ui/sound_list.rs   sounds of the selection: spectrum + RPM range, live volume/pitch
ui/side_panel.rs   collapsible panel: Info / Edit / Import-Export tabs
ui/track_editor.rs Edit tab: source, recordings, volume / pitch sliders
ui/project_dialog.rs  Project window: folder content, game state, install / restore
ui/preferences.rs  modal dialog with section navigation
```

Principles:
- **Layered crates**: the binary formats (`unity`, `fmod`) know nothing about the UI or audio
  output, and `core` knows nothing about GPUI. The CLI and the app share the same logic.
- Typed errors with `thiserror` in libraries, `anyhow` in binaries.
- `unsafe_code = "forbid"` and clippy on the whole workspace, except `cxssm-fmod-studio` (FFI).
- Heavy work (opening, export, FMOD analysis) runs on GPUI's background executor so the UI
  never blocks.

The user interface is available in French and English (`crates/cxssm-app/src/i18n.rs`:
every text is written in both languages where it is used, e.g. `self.t("Ouvrir", "Open")`).

## Roadmap

- [x] UnityFS, SerializedFile and TextAsset read/write
- [x] FMOD bank and FSB5 parsing, FSB5 rewrite
- [x] Native PCM16 encoding, Vorbis/FADPCM through `fsbank`
- [x] GPUI interface: categories, list, details, replacement, export
- [x] Read/rewrite validated on the real `fmod_assets_all.bundle`
- [x] Preview of the original sounds and WAV export (native FMOD Vorbis decoder)
- [x] Engine simulator (FMOD Studio API): live RPM/load, active layers, sound map, rev limiter
- [x] In-game test of a replaced sound: the replaced sounds play correctly in CarX Street
- [ ] Native FMOD Vorbis encoder (would remove the `fsbank` dependency)
- [x] Preferences: language (FR/EN), rev limiter on/off, visible categories, game auto-detection
- [x] Appearance: dark / light theme and accent colour
- [x] Multi-selection (Ctrl / Shift + click, Ctrl+A) and undo / redo (Ctrl+Z, Ctrl+Shift+Z)
- [x] Configurable keyboard shortcuts (engine E, sweep S, throttle Space…) and modified-car indicator
- [x] Category tree with car search, collapsible details panel (design in `docs/design/`)
- [ ] Drag & drop of replacement files
- [ ] Automatic resampling and mono/stereo conversion
- [x] FMOD Engine auto-detection and folder setting in the UI
- [x] Update check and install from GitHub Releases
- [ ] Encoding quality setting in the UI
- [x] Save / reopen a mod project (`.cxssm`), install into / restore the game
- [x] Track editor: volume, pitch, engine loops rebuilt from car recordings (experimental)
- [ ] Credits / About screen (FMOD attribution)

## FMOD licensing

CXSSM's own code is MIT-licensed, but **FMOD is proprietary software** owned by Firelight
Technologies. CXSSM does not ship any FMOD file: it loads the FMOD Engine that each user
downloads, accepting its licence. In short, according to the EULA shipped with FMOD 2.04
(`doc/LICENSE.TXT`):

- the FMOD Engine may be used free of charge for personal (hobbyist) or non-commercial use,
  with no monetisation, and with a credit line naming "FMOD" and "Firelight Technologies Pty Ltd.";
- only the **run-time libraries** (`.dll`, `.so`…) may be redistributed, integrated into an
  application, and FMOD must **not** be distributed "as part of a game engine or tool set";
- other SDK files, including the `fsbank` tool, must **not** be redistributed.

Bundling the FMOD libraries with CXSSM would therefore need written confirmation from Firelight
Technologies (sales@fmod.com) that a free modding tool qualifies. Until then: **never commit
FMOD binaries to this repository**, and let users install the FMOD Engine themselves.

## Disclaimer

Fan project, not affiliated with CarX Technologies or Firelight Technologies (FMOD).
Only distribute your own sounds, or sounds you have the rights to.

## Credits

- FMOD Vorbis setup-header table from [fsbex](https://github.com/astral4/fsbex)
  (MIT/Apache-2.0). See [THIRD_PARTY_LICENSES.md](THIRD_PARTY_LICENSES.md).
- Audio engine: FMOD Studio by Firelight Technologies Pty Ltd. (installed separately by the user).

## License

MIT, see [LICENSE](LICENSE).
