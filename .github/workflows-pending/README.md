# Workflows waiting to be placed

The three workflow files in this directory belong in
`../../workflows/` (i.e. `.github/workflows/`). They are **not** there yet,
and this directory exists so that the branch could be pushed at all.

## Why

The session that pushed this branch had only a `public_repo`-scoped personal
access token, and GitHub refuses to let such a token create or update
anything under `.github/workflows/`:

```
remote: refusing to allow a Personal Access Token to create or update
remote: workflow `.github/workflows/audio-backends.yml` without `workflow` scope
```

Everything else in the branch — the code, the tests, the docs — went through
untouched. Only the live registration of the three workflows could not.

## What each one is

| file | what it runs |
|---|---|
| `framework-checks.yml` | the framework's own gate: checks + coverage floor + cross type-check + `gpu-lavapipe` (the Vulkan suites on Mesa's software Vulkan, **including `vieww-3d`'s GPU parity suite**, added 2026-10-11) + macOS/Metal + Windows/D3D12 + Android + web parity |
| `audio-backends.yml` | `vieww-audio`'s device backends across platforms (PipeWire/JACK/PulseAudio/ALSA, Core Audio, WASAPI, AAudio, Web Audio) |
| `audio-plugins.yml` | `vieww-audio-plugin`'s CLAP and VST3 validation |

## How to place them (one command, needs a token with `workflow` scope)

From the repository root:

```sh
bash .github/workflows-pending/restore.sh
git add .github/workflows
git commit -m "ci: place the three workflows from workflows-pending"
git push
```

`restore.sh` copies the three files into `.github/workflows/` and deletes
this directory's payloads (this README and the script itself stay until the
commit that removes them). After that push, the workflows are live and
`PENDING.md` §1.7 and §5.1 read as written; until then, read every "the
`framework-checks` workflow runs…" in this branch as "will run, once placed".

If the push still lacks a `workflow`-scoped token, the alternative is the
web UI: create each file on GitHub's "Actions" page (or the file editor) by
pasting the contents of the corresponding file here, on branch
`feature/vieww_code_base2` — the editor runs with your session's authority,
not the token's.
