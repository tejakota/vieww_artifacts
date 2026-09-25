# Three implementation tracker

## 2026-09-12 — social product pass

Status: implemented; runtime verification requires a Rust toolchain.

### Completed

- Replaced the placeholder `three-social` command enum with an executable,
  transport-independent social domain and `SocialBackend` contract.
- Added profiles, `.3` asset upload/download, posts, cursor feeds, likes,
  comments, follows, search, notifications, privacy, deletion and remixes.
- Added `MemorySocialBackend` as a deterministic offline/reference backend.
- Rebuilt `three-app` around product routes, tabs, feed state, composer state,
  search, profile feeds and media opening.
- Made publishing an end-to-end transaction: encode/validate `.3` -> upload
  asset -> publish post -> refresh feed.
- Added a Vieww `three-social-app` reference application with Home, Explore,
  Create, Activity and Profile surfaces.
- Embedded the existing interactive `.3` Vieww viewer into the Home feed so
  the native social post is spatial media, not a disconnected viewer demo.
- Preserved `three-viewer` as a focused media diagnostic.
- Added social architecture/product documentation and verification script.

### Verification state

The execution container used for this pass does not provide `cargo`, `rustc`
or `rustfmt`, so no build/test result is claimed here. Run `./verify.sh` in a
Rust 1.89+ environment; the script is the canonical gate.

### Production-service work that remains deployment-specific

- authenticated remote `SocialBackend` adapter;
- durable user/post relational store;
- object storage/CDN and progressive `.3` streaming;
- moderation/abuse systems and server-side privacy authorization;
- push notification transport;
- real Android/iOS capture backends and reconstruction acceleration.

These are service/device adapters, not missing social product semantics in the
client architecture.
