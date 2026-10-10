# Third-party material in vieww-audio-plugin

vieww-audio-plugin has **no dependency** on any plugin SDK or bindings
crate. Its `sys` modules transcribe the two formats' application binary
interfaces — struct layouts, vtable method order, constants and interface
ids — from their published headers, which is what an ABI is. Both header
sets are MIT-licensed.

| Source | Used for | License |
|---|---|---|
| CLAP headers, <https://github.com/free-audio/clap> (revision `a47f6ba`) | `src/clap/sys.rs`: the entry, factory, plugin, host, process, event and extension definitions | MIT, © 2021 Alexandre Bique |
| VST3 interface headers, <https://github.com/steinbergmedia/vst3_pluginterfaces> (revision `4f547e8`, VST 3.8.1) | `src/vst3/sys.rs`: interface ids, vtables, `ProcessData`, `Event`, `ParameterInfo` and related structures | MIT, © 2026 Steinberg Media Technologies GmbH |

"VST" is a trademark of Steinberg Media Technologies GmbH. Shipping a
plugin under the VST3 name is subject to Steinberg's VST trademark usage
guidelines, separately from the code license.

## Test tools (not dependencies)

`tools/check-plugins.sh` fetches these at pinned revisions to check the
built plugins; nothing from them is compiled into this crate or its
plugins.

| Tool | Revision | License |
|---|---|---|
| CLAP headers (for `tools/clap_host.c`, `clap_layout.c`, `denormal_bench.c`) | `a47f6ba` | MIT |
| VST3 interface headers (for `tools/vst3_host.cpp`, `vst3_layout.cpp`) | `4f547e8` | MIT |
| free-audio/clap-validator | `b2f1d9b` (0.4.1) | MIT |
| Steinberg VST3 SDK validator, <https://github.com/steinbergmedia/vst3sdk> | `3cdf9ca` (3.8.1) | MIT |
