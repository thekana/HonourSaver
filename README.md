# Reclaim Honour

**Reclaim your Honour Mode.** Restore **Baldur's Gate 3 Honour Mode** on a save
that's already been lost — no save-editing tools or manual hex surgery required.

Reclaim Honour edits `profile8.lsf` (the profile that records which saves are in
Honour Mode) to flip a chosen save back on. A `.backup` of the original file is
written before anything is changed, so you can always undo it.

This is a Rust port of the original
[C# HonourSaver](https://github.com/nay-cat/HonourSaver) by **nay** (see
[Attribution](#attribution)). It ships as a native Windows desktop app —
download `ReclaimHonour.exe` from [Releases](../../releases).

---

## Table of contents

- [Is this tool right for me?](#is-this-tool-right-for-me)
- [Install (2 minutes)](#install-2-minutes)
- [Before you start (important)](#before-you-start-important)
- [Step-by-step usage](#step-by-step-usage)
- [How to undo](#how-to-undo)
- [Troubleshooting](#troubleshooting)
- [Building from source](#building-from-source)
- [How it works](#how-it-works)
- [Known issues](#known-issues)
- [Attribution](#attribution)
- [License](#license)

---

## Is this tool right for me?

Answer **yes** to all three:

1. You died (or otherwise lost) an **Honour Mode** run in Baldur's Gate 3.
2. In game, you clicked **"Continue"** on the warning — **not** "Delete save".
   If you clicked "Delete save", the save files are gone and this tool cannot
   bring them back.
3. The save is currently marked as **"Custom"** in your save list.

If any answer is **no**, this tool can't help — sorry!

## Install (2 minutes)

**Requirements:** Windows 10 or 11. Nothing else to install — the program is a
single, self-contained `.exe`.

1. Go to the [Releases page](../../releases).
2. Download **`ReclaimHonour.exe`**.
3. Put it anywhere you like — your Desktop or Downloads folder is fine.
   There is no installer: **double-click the file to run it.**

> **Windows SmartScreen warning?** The app isn't digitally signed, so Windows
> may show "Windows protected your PC". Click **More info → Run anyway**. This
> is expected for a small open-source tool — you can check the source code
> here on GitHub.

## Before you start (important)

Do these three things first — they're the most common reasons it doesn't work:

1. **Close Baldur's Gate 3 completely.** The tool edits a file the game keeps
   open. If the game is running, your changes may be overwritten or ignored.
2. **Don't use "Delete save".** You must have clicked **"Continue"** when the
   Honour Mode warning appeared. If you already deleted the save, stop here.
3. **Set the save to "Custom".** Launch the game, and make sure the affected
   save is set to **"Custom"** mode rather than being fully deleted.

**Optional but recommended:** turn off **Larian Cloud saves** (in the game's
settings menu) while you do this. Cloud sync can restore the old file over your
fixed one. See [Known issues](#known-issues).

## Step-by-step usage

1. **Open Reclaim Honour** (double-click `ReclaimHonour.exe`).

2. **Check the folder path.** The field labelled **BG3 Appdata Path** is already
   filled in for you:

   ```
   %LOCALAPPDATA%\Larian Studios\Baldur's Gate 3\PlayerProfiles\Public\
   ```

   Leave it as-is unless your game files live somewhere unusual.

   *Where is that folder?* Press **Windows + R**, paste
   `%LOCALAPPDATA%\Larian Studios\Baldur's Gate 3\PlayerProfiles\Public` and
   press Enter. It should contain a file called `profile8.lsf`.

3. **Click "Load Saves".** A popup tells you how many lost Honour Mode entries
   were found. Click **OK**.

4. **Pick your save** from the **"Save to reactivate"** dropdown:
   - Entries marked **✓** have save files on disk — these are the real,
     recoverable saves.
   - Entries marked **✗ (orphaned)** point to save files that no longer exist.
     Reactivating one of those only clears a stale entry; it won't restore the
     save.
   - Hover over an entry to see its full ID (GUID).
   - A **thumbnail preview** appears below the list so you can confirm it's the
     run you remember.

5. **Click "Reactivate Honour Mode".** You'll get a confirmation popup.

6. **Relaunch Baldur's Gate 3.** The save should show as Honour Mode again.

That's it — the achievement should be awarded on completion of the run.

## How to undo

Every run writes a safety copy called `profile8.lsf.backup` right next to the
original file. To put everything back the way it was:

1. Close the game and the tool.
2. Open the profile folder (see step 2 above).
3. **Delete** the modified `profile8.lsf`.
4. **Rename** `profile8.lsf.backup` → `profile8.lsf`.

Your file is now exactly as it was before the tool touched it.

## Troubleshooting

**"profile8.lsf not found" popup**
The path is wrong, or the folder structure is different on your machine. Open
the folder yourself (Windows + R → paste the path) and confirm `profile8.lsf`
is inside. Fix the path in the app to point at that folder, including the
trailing `\`, then click **Load Saves** again.

**"Found 0 disabled Honour Mode session(s)."**
- BG3 was running — close it fully and try again.
- The save was deleted rather than set to "Custom". Unfortunately the save
  files are gone in that case.
- You're looking at the wrong profile. If you use more than one BG3 profile,
  check the other folder in `PlayerProfiles\`.

**The save loads but still says "Custom" in game**
Sometimes a save shows as "Custom" even though Honour Mode is active and the
achievement is awarded anyway — see [Known issues](#known-issues).

**My changes disappeared after launching the game**
Larian Cloud saves likely synced the old file back. Disable cloud sync, then
run the tool again.

**An entry is marked ✗ (orphaned)**
The profile remembers the save, but the save files themselves were deleted.
Reactivating it only removes the stale entry — it can't recreate the save.

**Antivirus deleted or blocked the exe**
Some antivirus tools flag unsigned executables heuristically. This tool only
reads and writes one local file (`profile8.lsf`). Add an exception if you trust
the source, or build it yourself from source below.

## Building from source

Requires the [Rust toolchain](https://rustup.rs). On Windows you also need the
MSVC build tools (the default `rustup` install pulls them in).

```powershell
powershell -ExecutionPolicy Bypass -File build-windows.ps1
# -> rust\target\release\ReclaimHonour.exe
```

Or `cd rust; cargo build --release -p reclaimhonour-app`.

### Tests

```bash
cd rust
cargo test --workspace
```

## How it works

`profile8.lsf` is a Larian binary file (LSF). The core crate decodes it to LSX
(XML), marks the chosen save's GUID as Honour Mode, and re-encodes it — the same
approach the original tool used. The decoding/encoding is powered by a vendored,
trimmed build of the [`larian-formats`](https://crates.io/crates/larian-formats)
crate (Apache-2.0). The desktop app is [egui](https://github.com/emilk/egui)/eframe.

## Known issues

- Larian Cloud saves can behave oddly; the tool was tested with cloud sync
  disabled.
- Sometimes a save still shows as "Custom" even though Honour Mode is active and
  the achievement is awarded anyway.

## Attribution

- Original concept and C# implementation: [nay-cat/HonourSaver](https://github.com/nay-cat/HonourSaver).
- Game-file format handling based on [LSLib](https://github.com/Norbyte/lslib) /
  [`larian-formats`](https://crates.io/crates/larian-formats).

## License

MIT — see [LICENSE](LICENSE). The vendored `larian-formats` crate remains under
its own Apache-2.0 license (`rust/vendor/larian-formats/LICENSE`).
