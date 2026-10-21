# wrightsite

Wright's product page, as a vieww widget tree rendered to DOM, with a canvas
island.

```sh
# the page, rasterised on the host — no browser, no wasm toolchain
cargo run -p wrightsite --example render          # -> site-shots/{desktop,tablet,phone}.png

# the real thing
WRIGHT_REPO=owner/name apps/wrightsite/build-site.sh   # -> apps/wrightsite/dist/
```

## What is in it

| file | what it is |
|---|---|
| `src/lib.rs` | the canvas half. `root()` mounts the tree on a `FrameDriver`; the palette is Wright's own (`apps/wright/app/src/ui/theme.rs`), the mark is the app's one-stroke W (`ui/logo.rs`), drawn as a path |
| `src/dom.rs` | the DOM half that serves: the same widgets through `vieww-platform-web-dom`, with a `Canvas` island for the live demo |
| `index.html` | the host page: fonts, the stylesheet (splash, staged arrival, scroll-driven reveals, hover, fluid scale), the fallback copy, and one `<script type="module">` |
| `build-site.sh` | cargo → wasm-bindgen → a directory a static host can serve |
| `examples/render.rs` | the same tree to PNG on the host, at three widths, with the firing held at the gate |

## The one line to change

`REPO` in `src/lib.rs` and `__REPO__` in `index.html` — `build-site.sh` fills
both from `WRIGHT_REPO`. Every download URL is
`releases/latest/download/<name>`, against the names
`packaging/RELEASE-NOTES.md` publishes, so a release with those assets makes
every link work at once. Until then the links 404 on a placeholder repository,
which is the intended failure: a wrong-but-plausible repository is worse.

The asset table's sizes are `Pending` chips. Delete them once a release
publishes real numbers.

## The live demo

The page's one thing a screenshot could not have been: the Wright window in
miniature, running as a real vieww application — signals, two animations,
vector stage glyphs — mounted on a `<canvas>` on the DOM page and rasterised
on the canvas half for the host review. It fires on arrival, holds at the
approval gate, and answers the buttons: **Allow** releases Mailer and
Operator and the receipt is written; **Don't** stops the run with nothing
sent; **Fire the run again** resets and replays. Idle, it draws zero frames a
second — the page presents a frame only when the scene was rebuilt.

## Serving it

Any static host. Two headers matter:

- `.wasm` → `Content-Type: application/wasm`, or the browser will not
  stream-compile it;
- `.wasm` → `Content-Encoding: br` or `gzip` — the wire is the wait.

On Cloudflare Workers, `[site] bucket = "apps/wrightsite/dist"` (or a Pages
project pointed at the same directory) does both by default.

## What a canvas costs, and what is done about it

The island is pixels, so its text is not selectable and a crawler reads
nothing — which is why the island is one screen inside a page that is
otherwise DOM, and why `index.html` carries the page's real copy in `#app`'s
fallback content. The `<title>`, the description and the Open Graph tags are
on the host page for the same reason. If the page's copy changes, change it in
both places — they are two renderings of one text, and the fallback silently
going stale is the failure mode to watch for.
