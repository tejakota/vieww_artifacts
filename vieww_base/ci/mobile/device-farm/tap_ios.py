#!/usr/bin/env python3
"""Taps on an iPhone through Appium's XCUITest driver, for run-ios.sh.

Speaks the W3C WebDriver protocol to the local Appium server with nothing but
the standard library, so the Device Farm test package needs no Appium client
wheels (and their validator has nothing to reject).

    tap_ios.py open  --udid UDID [--wda DERIVED_DATA]   prints the session id
    tap_ios.py tap   --session ID --line "__VIEWW_TAPS_PT__ scale=3 button=x,y field=x,y"
    tap_ios.py close --session ID
    tap_ios.py suite --udid UDID [--wda DERIVED_DATA] --bundle ID --gpu 0|1 --out LOG

`suite` is the whole run for an iPhone **below iOS 17**, where Xcode's
`devicectl` cannot reach the device (CoreDevice is iOS 17+; on Device Farm's
iPhone 13 at iOS 16.0.2 it answered "the specified device was not found"):
Appium launches the app itself, with the suite's environment
(`processArguments.env`), its lines are read from the device's syslog through
Appium, the taps go in as above, and every vieww line is written to LOG.

The session is opened without an app, so XCUITest attaches to whatever is in
the foreground — vieww, launched separately by devicectl — and taps land at
screen points, which is the unit `__VIEWW_TAPS_PT__` reports.
"""

import argparse
import json
import re
import sys
import time
import urllib.error
import urllib.request

SERVER = "http://127.0.0.1:4723"


def call(method, path, body=None, timeout=300):
    data = None if body is None else json.dumps(body).encode()
    req = urllib.request.Request(
        SERVER + path, data=data, method=method, headers={"Content-Type": "application/json"}
    )
    with urllib.request.urlopen(req, timeout=timeout) as resp:
        return json.loads(resp.read() or b"{}")


def open_session(udid, wda, extra=None, quiet=False):
    caps = {
        "platformName": "iOS",
        "appium:automationName": "XCUITest",
        "appium:udid": udid,
        "appium:newCommandTimeout": 600,
        "appium:wdaLaunchTimeout": 180000,
        "appium:noReset": True,
    }
    if wda:
        caps["appium:usePrebuiltWDA"] = True
        caps["appium:derivedDataPath"] = wda
    caps.update(extra or {})
    try:
        reply = call("POST", "/session", {"capabilities": {"alwaysMatch": caps, "firstMatch": [{}]}})
    except urllib.error.HTTPError as error:
        body = error.read().decode(errors="replace")
        # "Unknown device or simulator UDID" (iPhone 13, iOS 16.0.2, 2026-10-16):
        # Appium's own device list disagreed with Device Farm's UDID. With
        # one phone attached, `auto` lets Appium take the one it can see.
        if "Unknown device" not in body:
            raise SystemExit(f"tap_ios: session refused: {body[:2000]}")
        print(f"tap_ios: {caps['appium:udid']} unknown to Appium; retrying with udid=auto", file=sys.stderr)
        caps["appium:udid"] = "auto"
        reply = call("POST", "/session", {"capabilities": {"alwaysMatch": caps, "firstMatch": [{}]}})
    value = reply.get("value", {})
    session = value.get("sessionId") or reply.get("sessionId")
    if not session:
        raise SystemExit(f"tap_ios: no session: {json.dumps(reply)[:2000]}")
    if not quiet:
        print(session)
    return session


def syslog_lines(session):
    """Device log entries since the last call (Appium keeps the cursor)."""
    try:
        reply = call("POST", f"/session/{session}/se/log", {"type": "syslog"}, timeout=60)
    except Exception as error:  # a transient failure is a missed poll, not a run
        return [f"tap_ios: syslog poll failed: {error}"]
    return [entry.get("message", "") for entry in reply.get("value", [])]


def run_suite(args):
    """Launch, tap and read the suite through Appium alone (iOS below 17)."""
    extra = {
        "appium:bundleId": args.bundle,
        "appium:processArguments": {"env": {"VIEWW_GPU": args.gpu}},
        "appium:autoLaunch": True,
        "appium:forceAppLaunch": True,
    }
    started = time.time()
    session = open_session(args.udid, args.wda, extra, quiet=True)
    deadline = time.time() + args.timeout
    taps_line, taps_sent, result = None, 0, None
    with open(args.out, "a") as out:
        out.write(f"tap_ios: session {session}, app launched after {time.time() - started:.1f}s\n")
        while time.time() < deadline and result is None:
            for line in syslog_lines(session):
                if "VIEWW" in line or "vieww" in line or line.startswith("tap_ios"):
                    out.write(line.rstrip("\r\n") + "\n")
                if "__VIEWW_TAPS_PT__" in line and taps_line is None:
                    taps_line = line
                if "__VIEWW_SUITE__ result" in line:
                    result = line.split()[-1]
            out.flush()
            if taps_line and taps_sent < 6 and result is None:
                for name in ("button", "field"):
                    match = re.search(rf"{name}=(-?\d+),(-?\d+)", taps_line)
                    if match:
                        tap(session, int(match.group(1)), int(match.group(2)))
                taps_sent += 1
                out.write(f"tap_ios: tap round {taps_sent} sent\n")
            time.sleep(1)
        out.write(f"tap_ios: result {result or 'TIMEOUT'}\n")
    try:
        call("DELETE", f"/session/{session}", timeout=60)
    except Exception:
        pass
    print(result or "TIMEOUT")


def tap(session, x, y):
    actions = {
        "actions": [
            {
                "type": "pointer",
                "id": "finger",
                "parameters": {"pointerType": "touch"},
                "actions": [
                    {"type": "pointerMove", "duration": 0, "x": x, "y": y},
                    {"type": "pointerDown", "button": 0},
                    {"type": "pause", "duration": 60},
                    {"type": "pointerUp", "button": 0},
                ],
            }
        ]
    }
    call("POST", f"/session/{session}/actions", actions, timeout=60)


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("command", choices=["open", "tap", "close", "suite"])
    parser.add_argument("--bundle", default="dev.vieww.demo")
    parser.add_argument("--gpu", default="0")
    parser.add_argument("--out", default="suite.log")
    parser.add_argument("--timeout", type=int, default=240)
    parser.add_argument("--udid")
    parser.add_argument("--wda", default="")
    parser.add_argument("--session")
    parser.add_argument("--line", default="")
    args = parser.parse_args()
    if args.command == "open":
        open_session(args.udid, args.wda)
    elif args.command == "suite":
        run_suite(args)
    elif args.command == "tap":
        for name in ("button", "field"):
            match = re.search(rf"{name}=(-?\d+),(-?\d+)", args.line)
            if match:
                x, y = int(match.group(1)), int(match.group(2))
                tap(args.session, x, y)
                print(f"tapped {name} at {x},{y} (points)")
    else:
        call("DELETE", f"/session/{args.session}", timeout=60)
    return 0


if __name__ == "__main__":
    sys.exit(main())
