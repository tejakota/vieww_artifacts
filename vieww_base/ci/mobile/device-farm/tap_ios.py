#!/usr/bin/env python3
"""Taps on an iPhone through Appium's XCUITest driver, for run-ios.sh.

Speaks the W3C WebDriver protocol to the local Appium server with nothing but
the standard library, so the Device Farm test package needs no Appium client
wheels (and their validator has nothing to reject).

    tap_ios.py open  --udid UDID [--wda DERIVED_DATA]   prints the session id
    tap_ios.py tap   --session ID --line "__VIEWW_TAPS_PT__ scale=3 button=x,y field=x,y"
    tap_ios.py close --session ID

The session is opened without an app, so XCUITest attaches to whatever is in
the foreground — vieww, launched separately by devicectl — and taps land at
screen points, which is the unit `__VIEWW_TAPS_PT__` reports.
"""

import argparse
import json
import re
import sys
import urllib.request

SERVER = "http://127.0.0.1:4723"


def call(method, path, body=None, timeout=300):
    data = None if body is None else json.dumps(body).encode()
    req = urllib.request.Request(
        SERVER + path, data=data, method=method, headers={"Content-Type": "application/json"}
    )
    with urllib.request.urlopen(req, timeout=timeout) as resp:
        return json.loads(resp.read() or b"{}")


def open_session(udid, wda):
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
    reply = call("POST", "/session", {"capabilities": {"alwaysMatch": caps, "firstMatch": [{}]}})
    value = reply.get("value", {})
    session = value.get("sessionId") or reply.get("sessionId")
    if not session:
        raise SystemExit(f"tap_ios: no session: {json.dumps(reply)[:2000]}")
    print(session)


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
    parser.add_argument("command", choices=["open", "tap", "close"])
    parser.add_argument("--udid")
    parser.add_argument("--wda", default="")
    parser.add_argument("--session")
    parser.add_argument("--line", default="")
    args = parser.parse_args()
    if args.command == "open":
        open_session(args.udid, args.wda)
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
