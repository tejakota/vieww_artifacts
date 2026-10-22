# main @ 42c6d92f7d92432ce450f6717ee441efe3d65414

run: https://github.com/tejakota/vieww_artifacts/actions/runs/39789365904

legs: all

- success: macOS and iOS (Metal)
- success: Windows (D3D12 on WARP)
- success: Wire check (root ↔ vieww_base)
- success: GPU on software Vulkan (lavapipe)
- success: Android (compile)
- in_progress: checks.sh (Linux)
- success: Web (wasm32; Chromium, Firefox, WebKit)
- success: Android emulator (APK, device and accessibility suites)
- success: Audio decoders under cargo-fuzz (nightly)
- success: iOS simulator (tests in the sim, a frame on its screen)
- success: Coverage floor
- success: Type-check for macOS, iOS, Windows, Android (from Linux)
- skipped: Android device suite (self-hosted)
