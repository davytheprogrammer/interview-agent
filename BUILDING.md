# Building Linux and Windows packages

The desktop UI, session logic and NVIDIA adapter are shared. Linux captures audio through PipeWire/PulseAudio; Windows uses the separate `scripts/platform/windows/audio.py` WASAPI helper. The speech model and runtimes are included in the packages, not downloaded while the user is recording.

On a Linux build host, install Tauri 2 prerequisites, Python 3.10 with `venv`, Node.js, Rust, `curl` and `unzip`. Run:

```sh
npm ci
scripts/prepare_resources.sh all
npm run tauri build -- --bundles deb
```

For Windows x64 cross-compilation, install `cargo-xwin`, the `x86_64-pc-windows-msvc` Rust target, LLVM/LLD and NSIS as described by the [Tauri Windows installer guide](https://v2.tauri.app/distribute/windows-installer/). Then run:

```sh
npm run tauri build -- --runner cargo-xwin --target x86_64-pc-windows-msvc --bundles nsis
```

The platform configurations map their own dependencies into `speech/` under Tauri's resource directory. The Debian package requires `python3.10` and `pulseaudio-utils`. The Windows x64 installer bundles Python and requires WebView2; Tauri's default installer may need internet to install WebView2 if absent. Builds are unsigned unless a signing process is configured.

NVIDIA credentials are stored in each operating system's credential store and are not shipped in either installer. Enter the key again in Settings on a different machine. The Windows build selects the Windows default speaker for meeting loopback and the Windows default microphone for coding; reconnecting or changing devices during a session may require stopping and starting capture again.

Run `cargo test` inside `src-tauri`, `npm run build`, `.venv/bin/python tests/test_windows_audio.py`, and `.venv/bin/python tests/test_continuous_transcription.py` for local checks. Source-routing mocks and cross-compilation cannot verify real WASAPI, Windows privacy settings, NVIDIA access, or what a meeting application shares. Test microphone, loopback, start/stop, permissions, screen-share capture and NVIDIA separately on a real Windows machine. Screen-capture exclusion is a best-effort OS request and is not a guarantee of invisibility.
