# Test plan

The automated unit tests cover deterministic encode/decode and header validation.
Before calling the format production-ready, add:

- malformed length fuzzing;
- truncated input tests for every field;
- large-file resource limits;
- version compatibility tests;
- property-based round-trip tests;
- golden files for old versions;
- cross-platform byte-for-byte tests.
