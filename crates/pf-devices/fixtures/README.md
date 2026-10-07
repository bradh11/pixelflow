# Device fixtures

- `fpp/`: real responses from an FPP 9.3 player (Raspberry Pi 3 B+) that sends DDP to a Falcon.
  IP addresses were replaced with RFC 5737 documentation addresses (192.0.2.x) and the
  hardware serial in `uuid` with a placeholder. Only safe read-only endpoints are recorded;
  FPP endpoints that return Wi-Fi passwords are never read. `api_fppd_status.json` and the
  `api_sequence*` files were recorded later from the same FPP (playing its scheduled show), with
  the same scrubbing plus a generic audio file path.
- `fpp-hat/`: an FPP with a pixel hat, built from the `co-pixelStrings` format in FPP's
  sample configs (synthetic).
- `falcon/`: real responses from an F16V5 on firmware Bld 32 in DDP mode (the Falcon the FPP
  above feeds), recorded read-only: `/status.xml` and the JSON API queries `ST` (pages 0 and 1,
  which this firmware answers alike) and `SP` page 0. Its home page `/` answers HTTP 404. The IP
  address and gateway were replaced with 192.0.2.x, the MAC address (`C`) with a placeholder, and
  the MAC-derived suffix of the controller name with the one the FPP fixture uses; the Wi-Fi
  fields (`WS`, `WP`, `CP`) were blank on the controller.
- `falcon/synthetic/`: Falcon edge cases built from the formats in xLights' `Falcon.cpp`:
  settings split over two `ST` pages (older V4 firmware), E1.31 input universes, and two `SP`
  pages with a reversed string with null pixels, a smart receiver, grouping, and a white-first
  color order.
- `wled/`: a WLED controller, built from WLED's `/json/info` and `/json/cfg` formats (synthetic;
  `cfg.json` follows `serializeConfig()` in `wled00/cfg.cpp`, with blank Wi-Fi and access point
  fields, so "Send setup" tests can check those sections are never sent).
