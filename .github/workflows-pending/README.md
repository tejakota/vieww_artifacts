# Placed

The three workflows that waited here — `framework-checks.yml`,
`audio-backends.yml`, `audio-plugins.yml` — are in `.github/workflows/` as of
2026-10-13. They rode here as ordinary files only because the push token of the
time lacked `workflow` scope; `restore.sh` is what moved them and is kept for
the record.

Results of every Framework checks run are published to the branch
`ci-results/<branch>` by its `report` job (`SUMMARY.md`, plus the tail of every
failed job's log under `logs/`) — readable with a plain `git fetch`, for sessions
that can reach git but not the GitHub API.

# Waiting (2026-10-15): every workflow, manual-only

All five files here wait for a push with `workflow` scope, and **every one is
triggered only by hand** (`workflow_dispatch`) — no push or pull-request
triggers anywhere, by the repository owner's choice:

| File | What it does |
|---|---|
| `ci.yml` | **replaces** the live `.github/workflows/ci.yml` ("Build Mobile Apps"), whose only change is dropping its push trigger |
| `hardware.yml` | your Linux and Windows machines as self-hosted runners (`vieww_base/ci/hardware/README.md`) |
| `device-farm.yml` | the device suite on AWS Device Farm phones, low / mid / high end (`vieww_base/ci/mobile/device_farm.py`) |
| `audio-backends.yml`, `audio-plugins.yml` | as before, now manual-only; `audio-backends.yml` also had a YAML error (an unquoted `backend::` at the end of a `run:` line) that would have stopped GitHub loading it |

Place them all with

    .github/workflows-pending/restore.sh

then commit and push with a token that has `workflow` scope, or move the
files in GitHub's web editor.

# Waiting (2026-10-23): audio-ci.yml, and two root copies behind

`audio-ci.yml` — the audio and music gate. The vieww-audio / vieww-music
series of 2026-10-23 reached `main` without it: the push token lacked
`workflow` scope, so the file rides here as an ordinary file, exactly as the
five above once did. The device-loopback commit of the same day then made it
**manual-only** (no push, pull_request or schedule trigger — the owner's
2026-10-15 rule for every workflow in this repository), so the copy here is
the manual-only version, and it is now mode 644.

Place it at **both** paths (via the web editor, or `restore.sh audio-ci.yml`
under a workflow-scoped token — then mirror by hand; `restore.sh` only fills
the root):

| Path | Content |
|---|---|
| `.github/workflows/audio-ci.yml` | **this directory's copy, verbatim** (blob `ccf6503`) |
| `vieww_base/.github/workflows/audio-ci.yml` | the same file, except its header comment is the two-line short form (blob `bbd0c17`; the author's own divergence, one comment's worth — paste from the session hand-off) |

Two more root copies are one web edit behind this directory:

| Path | Replace with |
|---|---|
| `.github/workflows/audio-backends.yml` | this directory's `audio-backends.yml` (blob `67f083d`; adds snd-aloop / snd-virmidi, PulseAudio loopback, VB-CABLE, BlackHole legs) |
| `.github/workflows/hardware.yml` | this directory's `hardware.yml` (blob `44f4f2d`; adds the `audio` step for the real sound cards) |

Until the first table's files are placed, the audio crates have no automated
gate at all — every audio workflow is manual, by design.
