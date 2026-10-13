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

## Requirements

Windows 10/11. No runtime to install — the `.exe` is self-contained.

## Usage

> Before you start, make sure the Honour Mode save is set to **"Custom"** in
> game. If you already clicked **"Delete save"**, this tool can't help — you must
> have clicked **"Continue"** instead. **Close the game before running the tool.**

1. Open Reclaim Honour.
2. Choose your BG3 profile folder
   (`%LOCALAPPDATA%\Larian Studios\Baldur's Gate 3\PlayerProfiles\Public`).
3. Click **Load Saves** and pick the GUID of the Honour Mode save to recover.
4. Click **Reactivate Honour Mode**.
5. Relaunch the game — the save should be Honour Mode again.

A backup is created as `profile8.lsf.backup` next to the file. To revert, delete
the modified `profile8.lsf` and rename `profile8.lsf.backup` to `profile8.lsf`.

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
