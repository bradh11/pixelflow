//! Fixture devices for tests (feature `test-fixtures`). See `fixtures/README.md`.

use crate::http::FakeHttp;

pub use crate::fake_fpp::{
    Checksum, FakeFpp, FakeFppState, StoredFile, UploadFile, fppd_rejects, stripslashes,
};
pub use crate::fake_wled::{FakeWled, FakeWledState};

/// The FPP player (real responses, IPs scrubbed). It sends DDP to [`FALCON`].
pub const FPP: &str = "192.0.2.10";
/// A Falcon F16V5 (real responses, scrubbed) in DDP mode with five strings on ports 1–4 and 6.
/// [`falcon_synthetic`] swaps in a synthetic setup for edge cases.
pub const FALCON: &str = "192.0.2.20";
/// An FPP with a pixel hat (two strings on port 1).
pub const FPP_HAT: &str = "192.0.2.30";
/// A WLED with an RGB and an RGBW output.
pub const WLED: &str = "192.0.2.40";

/// A Falcon JSON API query body, exactly as the adapter sends it.
pub fn falcon_query(method: &str, batch: u32) -> String {
    format!(r#"{{"T":"Q","M":"{method}","B":{batch},"E":0,"I":0,"P":{{}}}}"#)
}

/// The FPP player only (no Falcon answering).
pub fn fpp_only() -> FakeHttp {
    FakeHttp::new()
        .with_get(FPP, "/", include_str!("../fixtures/fpp/home.html"))
        .with_get(
            FPP,
            "/api/system/info",
            include_str!("../fixtures/fpp/api_system_info.json"),
        )
        .with_get(
            FPP,
            "/api/fppd/multiSyncSystems",
            include_str!("../fixtures/fpp/api_fppd_multiSyncSystems.json"),
        )
        .with_get(
            FPP,
            "/api/channel/output/universeOutputs",
            include_str!("../fixtures/fpp/api_channel_output_universeOutputs.json"),
        )
        .with_get_status(FPP, "/api/channel/output/co-pixelStrings", 404)
        .with_get(
            FPP,
            "/api/fppd/status",
            include_str!("../fixtures/fpp/api_fppd_status.json"),
        )
        .with_get(
            FPP,
            "/api/sequence",
            include_str!("../fixtures/fpp/api_sequence.json"),
        )
        .with_get(
            FPP,
            "/api/sequence/Christmas%20Medley%202017/meta",
            include_str!("../fixtures/fpp/api_sequence_Christmas_Medley_2017_meta.json"),
        )
        .with_post(FPP, "/api/command", START_MEDLEY, "Playlist Starting")
        .with_post(
            FPP,
            "/api/command",
            r#"{"command":"Stop Now","args":[]}"#,
            "Stopped",
        )
        .with_post(
            FPP,
            "/api/command",
            r#"{"command":"Stop Gracefully","args":["false"]}"#,
            "Stopping",
        )
}

/// The FPP command that starts the fixture FPP's sequence.
pub const START_MEDLEY: &str =
    r#"{"command":"Start Playlist","args":["Christmas Medley 2017.fseq","false","false"]}"#;

/// Every fixture device, each answering on its own address.
pub fn network() -> FakeHttp {
    fpp_only()
        // The real F16V5 answers its home page with a 404.
        .with_get_status(FALCON, "/", 404)
        .with_get(
            FALCON,
            "/status.xml",
            include_str!("../fixtures/falcon/status.xml"),
        )
        .with_post(
            FALCON,
            "/api",
            &falcon_query("ST", 0),
            include_str!("../fixtures/falcon/st0.json"),
        )
        .with_post(
            FALCON,
            "/api",
            &falcon_query("ST", 1),
            include_str!("../fixtures/falcon/st1.json"),
        )
        .with_post(
            FALCON,
            "/api",
            &falcon_query("SP", 0),
            include_str!("../fixtures/falcon/sp0.json"),
        )
        .with_get(
            FPP_HAT,
            "/api/system/info",
            include_str!("../fixtures/fpp-hat/api_system_info.json"),
        )
        .with_get(
            FPP_HAT,
            "/api/channel/output/co-pixelStrings",
            include_str!("../fixtures/fpp-hat/api_channel_output_co-pixelStrings.json"),
        )
        .with_get(
            FPP_HAT,
            "/api/channel/output/universeOutputs",
            include_str!("../fixtures/fpp-hat/api_channel_output_universeOutputs.json"),
        )
        .with_get(WLED, "/", include_str!("../fixtures/wled/home.html"))
        .with_get(WLED, "/json/info", include_str!("../fixtures/wled/info.json"))
        .with_get(WLED, "/json/cfg", include_str!("../fixtures/wled/cfg.json"))
}

/// [`network`] with a synthetic setup on [`FALCON`] for edge cases: settings split over two `ST`
/// pages (older V4 firmware), E1.31 input universes, and two `SP` pages with strings on ports 1 and
/// 3 — a reversed string with null pixels, a smart receiver, grouping, and a white-first color
/// order.
pub fn falcon_synthetic() -> FakeHttp {
    let page = |method: &str, batch: u32, body: &str| (falcon_query(method, batch), body.to_string());
    [
        page("ST", 0, include_str!("../fixtures/falcon/synthetic/st0.json")),
        page("ST", 1, include_str!("../fixtures/falcon/synthetic/st1.json")),
        page("IN", 0, include_str!("../fixtures/falcon/synthetic/in.json")),
        page("SP", 0, include_str!("../fixtures/falcon/synthetic/sp0.json")),
        page("SP", 1, include_str!("../fixtures/falcon/synthetic/sp1.json")),
    ]
    .into_iter()
    .fold(network(), |http, (request, body)| {
        http.with_post(FALCON, "/api", &request, &body)
    })
}

/// FPP's public release list as recorded (stable and pre-release, trimmed to their OS files).
pub const FPP_RELEASES: &str = include_str!("../fixtures/fpp/github_releases.json");

/// Endpoints that return passwords; adapters must never request them.
pub const SECRET_ENDPOINTS: [&str; 6] = [
    "/api/system/status",
    "/api/network/interface",
    "/api/configfile",
    "/api/backups",
    "/api/settings",
    "wsec.json",
];

/// Panics if any request touched a [`SECRET_ENDPOINTS`] path.
pub fn assert_no_secret_endpoints(http: &FakeHttp) {
    for request in http.requests() {
        for forbidden in SECRET_ENDPOINTS {
            assert!(!request.contains(forbidden), "requested {request}");
        }
    }
}
