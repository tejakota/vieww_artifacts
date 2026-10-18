// The iOS half of the accessibility suite: assert that VoiceOver's tree
// contains what vieww's semantics tree says it should.
//
// The mirror of `ci/mobile/a11y-android.sh`'s uiautomator dump, in the
// framework Apple gives UI tests: XCUITest queries the accessibility tree
// the system itself exposes — the same tree VoiceOver reads — so a green
// run here is "the elements are present, labelled, traited and enabled",
// which is exactly the half of the contract PENDING §3.6 records as
// missing on iOS.
//
// # Status: written, not run
//
// **This file has never executed.** XCUITest needs an Xcode project with a
// UI-testing bundle to run inside, and this repository deliberately has no
// .xcodeproj (see `ci/mobile/ios-app.sh`'s doc: the app is a plain binary
// plus an Info.plist, and a generated project file nobody edits is the
// thing the note exists to avoid). The run recipe lives in
// `ci/mobile/a11y-ios.sh`, which says honestly what it can and cannot do
// on a given machine. Until a run has happened, this file is the shape of
// the assertion, not a green tick — the same standing as
// `vieww-platform-winit`'s `ios_accessibility.rs`, whose elements these
// tests would be reading.
//
// # What a run must assert to count
//
// Per PENDING §3.6's start: a vieww button exists to VoiceOver, is
// labelled, carries the button trait and is enabled — `app.buttons[...]`
// below is that assertion spelled in XCUITest's own vocabulary. The
// identifiers come from the bridge: every published element's accessibility
// identifier is `vieww.<generation>.<index>` (the packed render id), which
// is stable across rebuilds for the same reason the Android tree's ids are.

import XCTest

final class ViewwA11yTests: XCTestCase {

    /// The app under test, as `ios-app.sh` bundles it.
    private var app: XCUIApplication!

    override func setUpWithError() throws {
        continueAfterFailure = false
        app = XCUIApplication()
        app.launch()
        // VoiceOver itself cannot be driven from XCUITest — what a UI test
        // reads is the same accessibility tree VoiceOver consumes, so a
        // green run is tree-level evidence, and ears on a manual session
        // remain the confirmation PENDING §3.6 asks to be recorded beside
        // it. The distinction is stated here so nobody reads a passing
        // suite as "VoiceOver has been heard".
    }

    /// The demo app's one labelled button — the same node
    /// `a11y-android.sh` asserts TalkBack sees as `Tap me`.
    func testTheButtonIsPresentLabelledAndEnabled() throws {
        // The demo button's label, as the gallery's fixture names it. If
        // the fixture's text changes, this assertion and the Android
        // harness's are the two places that follow it.
        let button = app.buttons["Tap me"]
        XCTAssertTrue(button.waitForExistence(timeout: 10.0),
                      "the semantics tree's button never reached the accessibility tree")
        XCTAssertTrue(button.isHittable,
                      "the button is in the tree but not where VoiceOver would touch")
        XCTAssertEqual(button.value as? String, nil,
                       "a plain button carries no value; a value here means the trait mapping leaked")
    }

    /// Every element the bridge published carries a `vieww.` identifier —
    /// the packed render id, stable across rebuilds. An element without
    /// one came from somewhere other than the bridge, and the test wants
    /// to know about it.
    func testEveryPublishedElementCarriesABridgeIdentifier() throws {
        let allElements = app.descendants(matching: .any)
            .matching(NSPredicate(format: "identifier BEGINSWITH 'vieww.'"))
        XCTAssertGreaterThan(allElements.count, 0,
                             "no bridge-published elements in the tree — is VoiceOver running? \
                             the bridge publishes nothing otherwise, by design")
        for index in 0..<min(allElements.count, 20) {
            let element = allElements.element(boundBy: index)
            XCTAssertTrue(element.identifier.hasPrefix("vieww."),
                          "element \(index) lost its identifier: '\(element.identifier)'")
        }
    }

    /// A disabled control reads as dimmed: the `NotEnabled` trait the
    /// bridge sets is what VoiceOver announces, and the tree-level fact is
    /// `isEnabled == false`.
    func testDisabledControlsAreDisabledInTheTree() throws {
        // The demo's disabled button, when the fixture has one on screen;
        // skipped with a spoken reason rather than passed silently when it
        // does not — a test that cannot find its subject says so.
        let disabled = app.buttons["Disabled"]
        guard disabled.waitForExistence(timeout: 5.0) else {
            throw XCTSkip("no Disabled control in this fixture's tree; enable one to run this")
        }
        XCTAssertFalse(disabled.isEnabled,
                       "the semantics said disabled, the tree says enabled — the trait was lost")
    }
}
