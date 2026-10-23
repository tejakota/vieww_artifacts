# Hardware runs

Three scripts, one per kind of machine, each runnable by hand or by the
`Hardware (self-hosted Linux and Windows)` workflow — `.github/workflows/hardware.yml`
at the repository root once placed; until a token with `workflow` scope
pushes it, it waits in `.github/workflows-pending/` (`restore.sh hardware.yml`):

| Script | Machine | Attended? | Closes |
|---|---|---|---|
| `linux.sh` | your Linux PC/laptop with its GPU | no | Vulkan suites on the real driver, GPU census with receipts, the `baseline` window (start-up, RSS, idle, fps, frame stages, RAPL energy), shipped size, stress p95 ×5, desktop suite |
| `windows.ps1` | your Windows PC with its GPU | no | the same (energy excepted), plus D3D12 on a hardware adapter and the studio's remote preview |

Both also have an `audio` step: vieww-audio on the machine's **real sound
card** — what CI's virtual devices (`audio-backends.yml`) cannot prove. It
lists devices and soaks the default output under full-core load (silent by
default: `VIEWW_HW_AUDIO_GAIN=0` streams every frame without sound;
`VIEWW_HW_AUDIO_SOAK=86400` makes it the 24-hour soak). Two optional
cables close the rest, each enabled by an environment variable on the
runner (put it in the runner's `.env` file, next to `run.sh`/`run.cmd`):

| Cable | Variable | Then checked |
|---|---|---|
| line out → line in (or a USB interface's out → in), both the defaults | `VIEWW_HW_AUDIO_LOOP=pipewire` (Linux: the backend; Windows: `1`) | every frame of a two-tone signal comes back: level, phase continuity (no dropped frames), noise floor — shared and exclusive on Windows |
| MIDI out → MIDI in | `VIEWW_MIDI_LOOP='<out id>><in id>'` | notes, controllers, pitch bend and SysEx round trip in order |

| `close-remaining.sh` | the Linux machine **with the Redmi on USB** | **yes** | the phone's device suite on CPU and GPU (now with `frame_stages`), TalkBack by ear, Maestro, the two-monitor refresh check |

Each writes a results directory with `summary.txt`, one log per step, receipts
(`*/receipt.json`), planner shadow logs (`*.jsonl`) and baseline reports
(`baseline/*.json` + `summary.json`). Exit status is non-zero if any step failed;
a step the machine cannot run says `SKIP` with the reason, never `PASS`.

## By hand

```console
# Linux, from vieww_base/
ci/hardware/linux.sh                       # everything
ci/hardware/linux.sh baseline size         # some steps

# Windows, from vieww_base\ in PowerShell
powershell -ExecutionPolicy Bypass -File ci\hardware\windows.ps1
powershell -ExecutionPolicy Bypass -File ci\hardware\windows.ps1 -Steps baseline,size
```

## As self-hosted GitHub runners

1. **Register each machine** (repository → Settings → Actions → Runners → New
   self-hosted runner) and give it the extra label the workflow asks for:
   `vieww-linux` on the Linux machine, `vieww-windows` on the Windows one. GitHub
   adds `self-hosted`, the OS and the architecture labels itself.
2. **Start the runner inside your logged-in desktop session**, with `./run.sh`
   (Linux) or `run.cmd` (Windows) in a terminal — **not** as a service. A service
   has no display: on Linux there is no `DISPLAY`/`WAYLAND_DISPLAY`, and on
   Windows a service runs in session 0, which cannot open windows. The window
   steps (`gpu-window`, `baseline`, `desktop`) would then SKIP or fail. Leave the
   machine unlocked and awake for the run; a locked screen can stop presentation.
3. **Run it**: Actions → *Hardware (self-hosted Linux and Windows)* → Run
   workflow. Untick a machine to run only the other. Results land in the run's
   artifacts and, as text, on the branch `ci-results/hardware-<branch>`.

### Security — read once

A self-hosted runner runs whatever a workflow tells it to, as you, on your
machine. This workflow can only be started by hand (`workflow_dispatch`); keep
it that way. If this repository is ever **public**, also set Settings → Actions
→ General → *Fork pull request workflows* to require approval for all outside
contributors — otherwise a fork's pull request could add a workflow that
targets these labels.

## Prerequisites

**Both: a `git clone`, not GitHub's "Download ZIP".** A ZIP has no `.git`, so
every receipt and summary says `commit: unknown` and a result cannot be tied to
the code that produced it.

**Both:** the pinned toolchain from `rust-toolchain.toml` (rustup installs it
on the first `cargo` call — check `rustc -V` says the pinned version, or every
number is for a different compiler), Python 3 (baseline summaries).

**Linux:** your GPU's Vulkan driver (Mesa or NVIDIA) and `vulkaninfo`
(`vulkan-tools`); a graphical session. For energy, RAPL is root-only since
2020; once per boot:

```console
sudo chmod o+r /sys/class/powercap/intel-rapl:*/energy_uj
```

For a true cold start, `VIEWW_HW_DROP_CACHES=1` drops the page cache before
each launch (needs passwordless `sudo`); otherwise start-up is reported as
warm-cache.

**Windows:** Visual Studio Build Tools (MSVC), the GPU vendor's current driver
(it installs the Vulkan driver), optionally the LunarG Vulkan SDK for
`vulkaninfo`, Git for Windows (its `bash` runs the desktop suite; the script
finds it beside `git.exe` even though Git's installer leaves `bash` off PATH,
and never uses WSL's `System32\bash.exe`). Energy is
not measured on Windows and is reported as `null` rather than estimated.

**Both, for steady numbers:** on mains power, a performance power plan /
`performance` governor, nothing heavy running. The `env` step records all of
these, so a noisy run can be recognised afterwards.

## What the numbers are

`baseline` reports are explained in `crates/vieww-platform-winit/examples/baseline.rs`;
receipts in `examples/fixtures/src/receipt.rs`; shadow logs in
`crates/vieww-render-planner/src/shadow.rs`. `summarize_baseline.py` compares the
medians with the audit's desktop targets (RSS under 40 MB; zero frames and zero
pixels while idle) and marks each *meets* or *MISSES*.
