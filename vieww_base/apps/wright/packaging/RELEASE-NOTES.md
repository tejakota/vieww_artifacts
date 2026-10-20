## Wright @VERSION@

Describe the outcome; agents build, check and deliver it.

### Downloads

| Platform | Installer | Portable |
|---|---|---|
| Windows 10/11 (x64) | `Wright-Setup-@VERSION@-x64.exe` — per-user, no admin prompt | `wright-@VERSION@-windows-x64.zip` |
| macOS 11+ (Apple silicon and Intel) | `Wright-@VERSION@-macos-universal.dmg` | `Wright-@VERSION@-macos-universal.app.zip` |
| Linux (x64, glibc 2.35+) | `wright_@VERSION@_amd64.deb` | `wright-@VERSION@-linux-x64.tar.gz` |

`SHA256SUMS.txt` lists a checksum for every file.

### What works where

| | Linux | macOS | Windows |
|---|---|---|---|
| The window (splash, sessions, engine, settings, voice, files) | yes | yes | yes |
| Plans, the run store, history, schedules | yes | yes | yes |
| Local model servers over `http://` (Ollama, LM Studio, vLLM, the mock) | yes | yes | yes |
| Cloud model providers over `https://` | yes (OS OpenSSL) | **not yet** | **not yet** |
| Sandboxed tools: shell, git, run-tests | yes (Landlock) | **refuse** | **refuse** |
| Keys in the OS keychain | `secret-tool` | Keychain | environment variables only (`wright secret env <ref>` names them) |

On macOS and Windows, TLS and the sandbox refuse loudly rather than run
unprotected; they are the next platform work (W-5b, W-6b in `docs/SPIKES.md`).
Until then, point Wright at a local model server in Settings → Models & keys.

### First launch

- **macOS:** the app is signed ad hoc, not notarized. Right-click Wright → Open
  the first time, or run `xattr -dr com.apple.quarantine /Applications/Wright.app`.
- **Windows:** SmartScreen may say the publisher is unknown (the installer is
  unsigned). Choose *More info → Run anyway*.
- **Linux:** `sudo apt install ./wright_@VERSION@_amd64.deb`, then open Wright
  from your applications menu, or run `wright`.

Everything Wright remembers lives in `~/.wright`; uninstalling never touches
it or your projects.
