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
