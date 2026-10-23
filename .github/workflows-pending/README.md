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

# Waiting (2026-10-23): audio-ci.yml

`audio-ci.yml` — the one workflow that runs **on push**, gating the audio and
music crates (tests and clippy with `-D warnings`, nightly cargo-fuzz). The
vieww-audio / vieww-music series of 2026-10-23 reached `main` without it: the
push token lacked `workflow` scope, so the file rides here as an ordinary
file, exactly as the five above once did. It is the only automated gate the
audio crates have, so placing it is not optional housekeeping.

Place it at **both** paths, mirrored byte-for-byte like `framework-checks.yml`:

    .github/workflows/audio-ci.yml
    vieww_base/.github/workflows/audio-ci.yml

(via the web editor, or `restore.sh audio-ci.yml` under a workflow-scoped
token — then mirror the `vieww_base` copy by hand; `restore.sh` only fills
the root). The file's own `on:` block already covers both copies' contract:
push and pull_request paths on the audio and music crates, a nightly fuzz
schedule at 02:47 IST, and `workflow_dispatch`.
