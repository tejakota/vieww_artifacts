# `3` social application

`3` is the product. `.3` is its native moving-spatial media format.

The social application is deliberately split into three layers so capture,
rendering, product state, and transport do not become one inseparable crate.

```text
Vieww application (`examples/social-app`)
        |
        v
three-app                     product state + navigation + orchestration
        |
        v
three-social                  accounts, assets, posts, feed, graph, activity
        |
        +---- SocialBackend trait ---- remote service / persistent adapter
        |
        `---- MemorySocialBackend ---- deterministic offline/demo implementation

Media path:
camera -> reconstruction -> Capture3D -> .3 -> upload -> post -> feed
                                               |                 |
                                               `--- download ----+-> ThreeView -> Vieww
```

## Product surfaces

The reference Vieww application has five root destinations:

* **Home** — For You / Following feeds. A post's media is an interactive `.3`,
  not a video thumbnail pretending to be the final medium.
* **Explore** — profile discovery/search surface backed by profile search.
* **Create** — the composer contract: capture, reconstruct, preview, caption,
  privacy, publish. `ThreeApp::attach_capture` and `publish_composer` implement
  the product transaction.
* **Activity** — likes, comments, follows and remixes from the notification
  domain.
* **Profile** — identity, social counts and a profile-specific `.3` feed.

Post-detail, comments, user-detail and composer routes are first-class `Route`
variants rather than booleans sprinkled through widgets.

## Social semantics already implemented

`three-social` implements:

* profiles and active identity;
* validated `.3` asset upload/download;
* public, followers-only and private posts;
* For You, Following and Profile feeds with cursors;
* likes/unlikes;
* comments;
* follow/unfollow with counts;
* user search;
* activity notifications;
* post deletion with ownership checks;
* remix ancestry and remix notifications;
* deterministic offline behavior for tests and development.

The For You ranking in the local implementation is intentionally simple and
transparent: engagement score followed by creation time. Production ranking
belongs behind the same service boundary; it does not leak into UI code.

## Why the backend is a trait

The UI must never know whether social data came from an in-process development
backend, a disk cache, or the production service. `SocialBackend` is that
boundary. A production HTTP/WebSocket adapter can therefore replace
`MemorySocialBackend` without rewriting Vieww widgets or `.3` playback.

The bundled backend is not represented as a production Internet service. It is
the executable reference/social semantics and offline development backend.
Authentication, abuse prevention, database durability, CDN/object storage,
server-side ranking and production transport are deployment concerns and must
be implemented as service adapters rather than contaminating the media stack.

## Run

With the Vieww checkout next to this workspace at `../vieww-develop`:

```bash
cargo run -p three-social-app
```

The older standalone viewer remains useful as a media diagnostic:

```bash
cargo run -p vieww-integration
```

but it is no longer the product entry point.
