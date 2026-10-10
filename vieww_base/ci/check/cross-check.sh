#!/usr/bin/env bash
#
# Type-check the whole workspace for macOS, iOS, Windows and Android **from a
# Linux machine with none of those SDKs installed**.
#
#     ci/check/cross-check.sh                  # every foreign target
#     ci/check/cross-check.sh macos ios        # some of them
#
# # How, when rustup cannot add a target
#
# A foreign target normally needs its precompiled `std` (`rustup target add`),
# which needs `static.rust-lang.org`. This builds `std` from source instead
# (`-Zbuild-std`, enabled on a stable compiler by `RUSTC_BOOTSTRAP=1` — the
# same switch the Rust project's own bootstrap uses), so the only thing
# required is the `library/` sources matching this exact rustc, which
# `rustup component add rust-src` provides — or, where rustup cannot reach its
# server either, a checkout of `rust-lang/rust` at the commit
# `rustc -vV` prints (plus its `library/backtrace` submodule), linked to
# `$(rustc --print sysroot)/lib/rustlib/src/rust`.
#
# Build scripts that compile C through `cc-rs` (tree-sitter, criterion's
# `alloca`) would still look for an Apple SDK or MSVC. `ci/tools/stub-cc.sh`,
# `stub-ar.sh` and `stub-bin/xcrun` stand in for those: they produce empty
# objects, which is harmless because a type-check never links.
#
# # What passing means
#
# Every crate, test, example and bench compiles for that target, with rustc's
# own warnings treated as errors. It does not mean anything links, runs, or
# draws: that is `platform-check.sh` on the real machine and the workflow
# legs in `.github/workflows/framework-checks.yml` that run on GitHub's macOS
# and Windows runners. Clippy is not run here — under `-Zbuild-std` it
# reports `thread_local!` initialisers that are already `const` (a clippy
# false positive specific to a from-source `std`), so the lint pass belongs on
# the real runners.

set -uo pipefail

root="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$root"

# Lean builds: a type-check of four targets' worth of the workspace, each
# with its own from-source std, is several gigabytes even without debuginfo.
# shellcheck source=ci/lib/lean.sh
. "$root/ci/lib/lean.sh"
vieww_lean_env

export RUSTC_BOOTSTRAP=1
export CARGO_TARGET_DIR="${CARGO_TARGET_DIR:-$root/target/xcheck}"
export PATH="$root/ci/tools/stub-bin:$PATH"
export RUSTFLAGS="${RUSTFLAGS:-} -D warnings"

sysroot="$(rustc --print sysroot)"
if [ ! -f "$sysroot/lib/rustlib/src/rust/library/std/src/lib.rs" ]; then
	echo "error: no std sources for this rustc ($(rustc -V))." >&2
	echo "       rustup component add rust-src — or, offline, check out" >&2
	echo "       rust-lang/rust at $(rustc -vV | sed -n 's/commit-hash: //p')" >&2
	echo "       and link it to $sysroot/lib/rustlib/src/rust" >&2
	exit 2
fi

declare -A triple=(
	[macos]=aarch64-apple-darwin
	[ios]=aarch64-apple-ios
	[windows]=x86_64-pc-windows-msvc
	[android]=aarch64-linux-android
)
# Per platform, the feature-gated backends that only exist there.
declare -A extra=(
	[macos]="-p vieww-hal --features metal"
	[ios]="-p vieww-hal --features metal"
	[windows]="-p vieww-hal --features d3d12,vulkan"
	[android]="-p vieww-hal --features vulkan"
)

platforms=("$@")
[ ${#platforms[@]} -eq 0 ] && platforms=(macos ios windows android)

failures=0
for p in "${platforms[@]}"; do
	t="${triple[$p]:-}"
	if [ -z "$t" ]; then
		echo "unknown platform: $p (one of: ${!triple[*]})" >&2
		exit 2
	fi
	tt="${t//-/_}"
	export "CC_$tt=$root/ci/tools/stub-cc.sh" "CXX_$tt=$root/ci/tools/stub-cc.sh" \
		"AR_$tt=$root/ci/tools/stub-ar.sh"
	printf '\n\033[1m== %s (%s)\033[0m\n' "$p" "$t"
	# shellcheck disable=SC2086 # `extra` is a word list by design.
	if cargo check -Zbuild-std=std,panic_abort --target "$t" --workspace --all-targets --quiet &&
		cargo check -Zbuild-std=std,panic_abort --target "$t" ${extra[$p]} --all-targets --quiet; then
		printf '   \033[32mOK\033[0m — the workspace type-checks for %s\n' "$t"
		# A passed target's metadata is not needed again by this run; keep
		# it only when asked, so four targets fit where one would.
		[ "${VIEWW_CROSS_KEEP:-0}" = 1 ] || rm -rf "${CARGO_TARGET_DIR:?}/$t"
	else
		printf '   \033[31mFAILED\033[0m — see above\n'
		failures=$((failures + 1))
	fi
done

echo
if [ "$failures" -eq 0 ]; then
	echo "every requested target type-checks. Type-checking is not running: see the header."
fi
exit "$failures"
