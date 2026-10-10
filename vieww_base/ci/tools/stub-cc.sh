#!/bin/sh
# A C compiler that compiles nothing — for `ci/check/cross-check.sh` only.
#
# A cross type-check (`cargo check --target <foreign>`) never links, but a
# build script that uses the `cc` crate still runs the target's C compiler,
# and on a Linux machine there is no Apple or MSVC SDK for it to find. This
# stands in: it writes an empty object wherever `-o` says and succeeds, so
# the build script finishes and rustc gets to the Rust — which is the only
# thing a type-check is about. Nothing built with it can link, by design.
out=""
while [ $# -gt 0 ]; do
	case "$1" in
	-o) out="$2"; shift ;;
	-o*) out="${1#-o}" ;;
	# cc-rs preprocesses a probe to learn the compiler family; answer clang.
	-E) echo "clang"; exit 0 ;;
	-Fo*) out="${1#-Fo}" ;;
	--version | -v | -dumpversion) echo "stub-cc 1.0 (clang compatible)"; exit 0 ;;
	esac
	shift
done
[ -n "$out" ] && : >"$out"
exit 0
