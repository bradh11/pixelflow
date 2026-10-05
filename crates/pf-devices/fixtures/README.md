# Device fixtures

- `fpp/`: real responses from an FPP 9.3 player (Raspberry Pi 3 B+) that sends DDP to a Falcon.
  IP addresses were replaced with RFC 5737 documentation addresses (192.0.2.x) and the
  hardware serial in `uuid` with a placeholder. Only safe read-only endpoints are recorded;
  FPP endpoints that return Wi-Fi passwords are never read.
- `fpp-hat/`: an FPP with a pixel hat, built from the `co-pixelStrings` format in FPP's
  sample configs (synthetic).
- `falcon/`: an F16V5, built from the request/response formats in xLights' `Falcon.cpp`
  (synthetic until recorded from a real controller; Wi-Fi fields are blank).
- `wled/`: a WLED controller, built from WLED's `/json/info` and `/json/cfg` formats (synthetic).
