# `3` social application

`3` is the product. `.3` is its native depth-video media format.

The social application is deliberately split into three layers so capture,
rendering, product state, and transport do not become one inseparable crate.

```text
Vieww application (examples/social-app)
        |
        v
three-app          product state + navigation + orchestration
        |
        v
three-social       accounts, assets, posts, feed, graph, activity
        |
        +---- SocialBackend trait ---- remote service / persistent adapter
        |
        `---- MemorySocialBackend ---- deterministic offline/demo impl
```

## The Create tab is the camera

On a phone build, `android_main` constructs the `Camera2Source` from the
activity handle and hands it to the app (`set_capture_source`) before the
tree mounts; `SocialState::new` takes it. **Record a moment** starts a
`DynRecording` (three seconds, 24 fps, 4:3); the element's `tick` polls it
one frame per frame tick — the poll contract, so the capture never blocks
the UI thread; the progress line is the recording's own numbers. On
completion the capture is encoded and attached to the composer (the bytes
the composer holds are exactly the bytes a publish will upload), and the
flow routes to the caption/publish screen.

On a desktop run there is no camera source, and the Create tab says so —
with **Use the sample moment** as the dev path through the identical
publish pipeline.

## The feed card

The home feed's first post opens through `three_app.open_post_media`:
bytes downloaded, `LazyCapture::open` indexed, shared as an `Rc`. The
`ViewerScreen` (the same one the reference viewer uses) plays it with the
full gesture set: pan orbits, pinch dollies in depth, hold recentres. A
feed of posts holds compressed bytes and one decoded frame per visible
card — the lazy reader is what makes that a budget, not a fantasy.

## Publishing

`publish_composer` validates the attached bytes by decoding them (the
composer's proof it holds a real capture), uploads through the backend's
asset contract, and publishes the post. The offline backend makes the
whole loop real with no network: likes, comments, follows, notifications
and remixes all happen through the same calls a server would answer.

## The receipts

`cargo test -p three-social-app -- --ignored --nocapture` renders the app's
five screens through the native rasterizer — the same tree `App::run`
shows, no display, no window.
