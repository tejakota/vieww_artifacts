# Development checklist

## Everyday commands

```bash
cargo test                 # the foundation, no Vieww checkout needed
cargo test --workspace     # everything, viewer included (needs Vieww alongside)
cargo clippy --workspace   # clean, keep it that way
```

The viewer's tests run headlessly through `vieww-test-harness` — a clock the
test controls, injected gestures, and the CPU rasterizer for pixel
assertions. Never assert on text through the *element* tree's `debug_tree`
(it prints widget names and build counts, not text data); use state reads or
the widget-level `vieww::debug_tree`.

## Before merging format changes

- update `docs/FORMAT.md`;
- add a round-trip test;
- add malformed-input coverage;
- preserve backward compatibility for the same major version;
- benchmark encode/decode on representative phone-sized captures.

## Before enabling a native camera backend

- verify camera permission failure;
- verify camera interruption/recovery;
- verify orientation changes;
- verify app background/foreground transitions;
- verify bounded memory usage;
- verify that capture never blocks the Vieww UI thread;
- capture logs with frame timestamps and dropped-frame counts.

## Before social launch

- authenticate uploads;
- validate and size-limit `.3` files server-side;
- strip or explicitly protect sensitive capture metadata;
- implement deletion;
- implement moderation/reporting;
- stream previews progressively;
- never trust client-provided mesh or texture dimensions.
