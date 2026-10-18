#!/usr/bin/env bash
#
# The iOS half of the accessibility suite — the counterpart of
# `a11y-android.sh`, and like it, honest about what a given machine can
# and cannot settle.
#
#   ci/mobile/a11y-ios.sh --check        # what this machine can run, and what is missing
#   ci/mobile/a11y-ios.sh --sim          # booted simulator: install, launch, read the tree
#   ci/mobile/a11y-ios.sh --xcuitest     # build and run the XCUITest bundle (needs a project, see below)
#
# Needs macOS with Xcode for anything beyond --check. This script never
# builds the app: `ci/mobile/ios-app.sh --sim` does that, and this script
# consumes its bundle.
#
# # What this can and cannot settle
#
# **The tree, not the ears.** `ios_accessibility.rs` publishes the
# semantics tree as `UIAccessibilityElement`s, and a UI test reads the same
# tree VoiceOver consumes — so a run can prove "the button is present,
# labelled, traited, enabled", which is the half of the contract PENDING
# §3.6 records as missing. What no automated run here proves is that
# VoiceOver *spoke*: VoiceOver itself cannot be driven from XCUITest, and
# the manual session recorded beside the TalkBack record (PENDING §3.6's
# own wording) remains the other half.
#
# **The XCUITest half needs an Xcode project, and this repository has
# deliberately none** (`ios-app.sh`'s doc explains why). `ci/mobile/xcuitest/
# ViewwA11yTests.swift` is the assertion set, written and reviewed but never
# executed; running it needs a throwaway project with a UI-testing bundle
# pointed at the installed app. When one exists — a real device run is the
# natural moment — this script's --xcuitest branch is where it plugs in, and
# the branch below says so rather than pretending.

set -uo pipefail

mode="${1:---check}"

have() { command -v "$1" >/dev/null 2>&1; }

report() { printf 'a11y-ios: %s\n' "$*"; }

fail() { printf 'a11y-ios: FAIL — %s\n' "$*" >&2; exit 1; }

case "$mode" in
--check)
	report "checking what this machine can run"
	if ! have xcrun; then
		report "xcrun not found — no Apple toolchain; the Rust bridge still type-checks:"
		report "  cargo check --target aarch64-apple-ios -p vieww-platform-winit"
		exit 0
	fi
	report "xcrun present — the simctl half can run: ios-app.sh --sim, then this script --sim"
	if have xcodebuild; then
		report "xcodebuild present — but the XCUITest half needs a project with a UI-testing"
		report "  bundle, which this repository deliberately does not carry (see header)."
		report "  The assertions live in ci/mobile/xcuitest/ViewwA11yTests.swift, never run."
	else
		report "xcodebuild not found — the XCUITest half cannot run here at all"
	fi
	report "record: no VoiceOver session has ever been heard on vieww (PENDING 3.6)"
	exit 0
	;;
--sim)
	have xcrun || fail "--sim needs macOS with Xcode (xcrun)"
	booted="$(xcrun simctl list devices booted | grep -c "Booted" || true)"
	[ "$booted" -ge 1 ] || fail "no booted simulator: open Simulator, or xcrun simctl boot '<device>'"
	# The bundle ios-app.sh --sim produced and installed. The launch is the
	# test's setup; what the tree holds afterwards is what the XCUITest
	# bundle would assert in detail.
	report "launching the installed vieww demo on the booted simulator"
	xcrun simctl launch booted dev.vieww.demo ||
		fail "dev.vieww.demo not installed — run ci/mobile/ios-app.sh --sim first"
	sleep 3
	report "the app is up; what the tree holds is asserted by the XCUITest set"
	report "  (ci/mobile/xcuitest/ViewwA11yTests.swift) — see --xcuitest and the header"
	report "simctl half done; this run alone does not move PENDING 3.6 without a"
	report "  recorded session or a green XCUITest run beside it"
	exit 0
	;;
--xcuitest)
	fail "no Xcode project exists to run the UI-testing bundle in — see this file's"
	;;
*)
	printf 'usage: %s [--check|--sim|--xcuitest]\n' "$0" >&2
	exit 2
	;;
esac
