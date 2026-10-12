//! What PixelFlow opens outside its own window: the project's page, xLights' page, the licence
//! and notices that come inside the app, and the ways to support the project.
//!
//! The window never hands over an address. It names a link, and the shell opens the address or
//! the bundled file it has for that name, with the system's own opener: a link in the browser, a
//! file in whatever opens text. Nothing is fetched by PixelFlow itself.
//!
//! The ways to support the project are account names in `app/support.json`, the one place they
//! are set (the window reads the same file, and `.github/FUNDING.yml` says the same for the
//! repository's page). A way whose name is empty has no link: asking for it is refused, and the
//! menu has no item for it.

use crate::Reply;
use serde::Deserialize;
use std::sync::LazyLock;
use tauri::{AppHandle, Manager, Runtime};

/// Where PixelFlow's source is.
const SOURCE: &str = "https://github.com/bradh11/pixelflow";
/// The project PixelFlow was inspired by, and parts of it are derived from.
const XLIGHTS: &str = "https://github.com/xLightsSequencer/xLights";

/// The ways to give, as the window names them, with what each one's address begins with.
const WAYS: [(&str, &str); 4] = [
    ("githubSponsors", "https://github.com/sponsors/"),
    ("koFi", "https://ko-fi.com/"),
    ("buyMeACoffee", "https://buymeacoffee.com/"),
    ("payPal", "https://paypal.me/"),
];

/// The account name for each way to give; empty where there is none.
#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct Support {
    github_sponsors: String,
    ko_fi: String,
    buy_me_a_coffee: String,
    pay_pal: String,
}

/// The accounts this build was made with.
pub(crate) static SUPPORT: LazyLock<Support> =
    LazyLock::new(|| Support::read(include_str!("../../support.json")));

impl Support {
    /// The accounts in `text` (the contents of `support.json`); none if it can't be read.
    fn read(text: &str) -> Self {
        serde_json::from_str(text).unwrap_or_default()
    }

    fn account(&self, way: &str) -> Option<&str> {
        Some(match way {
            "githubSponsors" => &self.github_sponsors,
            "koFi" => &self.ko_fi,
            "buyMeACoffee" => &self.buy_me_a_coffee,
            "payPal" => &self.pay_pal,
            _ => return None,
        })
    }

    /// The address for one way to give: only when it has an account, and the account is a
    /// plain name.
    fn url(&self, way: &str) -> Option<String> {
        let name = self.account(way).filter(|name| plain_name(name))?;
        let (_, site) = WAYS.iter().find(|(known, _)| *known == way)?;
        Some(format!("{site}{name}"))
    }

    /// Whether any way to give is set up.
    pub(crate) fn any(&self) -> bool {
        WAYS.iter().any(|(way, _)| self.url(way).is_some())
    }
}

/// An account name as these services write one: no slashes, dots, or spaces, so it can only ever
/// be the last part of the address.
fn plain_name(name: &str) -> bool {
    (1..=64).contains(&name.len())
        && name
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_'))
}

/// What a link's name stands for.
#[derive(Debug, PartialEq, Eq)]
enum Target {
    /// A page, for the browser.
    Page(String),
    /// A text file that comes inside the app (see `bundle.resources` in `tauri.conf.json`).
    Bundled(&'static str),
}

fn target(link: &str, support: &Support) -> Option<Target> {
    Some(match link {
        "source" => Target::Page(SOURCE.into()),
        "xlights" => Target::Page(XLIGHTS.into()),
        "licence" => Target::Bundled("LICENSE.txt"),
        "credits" => Target::Bundled("NOTICE.txt"),
        "notices" => Target::Bundled("THIRD-PARTY-NOTICES.txt"),
        way => Target::Page(support.url(way)?),
    })
}

/// Hands an address or a file to the system's own opener.
pub(crate) fn open_outside(what: &str) -> std::io::Result<()> {
    #[cfg(target_os = "macos")]
    let mut command = std::process::Command::new("open");
    #[cfg(target_os = "windows")]
    let mut command = {
        let mut c = std::process::Command::new("rundll32");
        c.arg("url.dll,FileProtocolHandler");
        c
    };
    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    let mut command = std::process::Command::new("xdg-open");
    command.arg(what).spawn().map(|_| ())
}

/// Opens one of the project's links by name: "source", "xlights", "licence", "credits",
/// "notices", or a way to give that is set up.
#[tauri::command]
pub(crate) fn open_project_link<R: Runtime>(app: AppHandle<R>, link: String) -> Reply<()> {
    let target = target(&link, &SUPPORT).ok_or_else(|| format!("{link} isn't a link PixelFlow opens."))?;
    match target {
        Target::Page(url) => {
            open_outside(&url).map_err(|e| format!("Couldn't open {url} in your browser: {e}"))
        }
        Target::Bundled(name) => {
            let missing = || format!("{name} should be inside the app, and isn't.");
            let file = app.path().resource_dir().map_err(|_| missing())?.join(name);
            if !file.is_file() {
                return Err(missing());
            }
            open_outside(&pf_model::path_to_text(&file)).map_err(|e| format!("Couldn't open {name}: {e}"))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn made_up() -> Support {
        Support::read(
            r#"{ "githubSponsors": "some-one", "koFi": "someone_1", "buyMeACoffee": "someone", "payPal": "SomeOne" }"#,
        )
    }

    #[test]
    fn a_way_with_an_account_has_a_well_formed_address() {
        let support = made_up();
        assert_eq!(
            support.url("githubSponsors").as_deref(),
            Some("https://github.com/sponsors/some-one")
        );
        assert_eq!(
            support.url("koFi").as_deref(),
            Some("https://ko-fi.com/someone_1")
        );
        assert_eq!(
            support.url("buyMeACoffee").as_deref(),
            Some("https://buymeacoffee.com/someone")
        );
        assert_eq!(
            support.url("payPal").as_deref(),
            Some("https://paypal.me/SomeOne")
        );
        assert!(support.any());
        assert_eq!(
            target("koFi", &support),
            Some(Target::Page("https://ko-fi.com/someone_1".into()))
        );
    }

    #[test]
    fn a_way_with_no_account_has_no_address_and_nothing_to_open() {
        let none = Support::default();
        for (way, _) in WAYS {
            assert_eq!(none.url(way), None, "{way}");
            assert_eq!(target(way, &none), None, "{way}");
        }
        assert!(!none.any());
        let one =
            Support::read(r#"{ "githubSponsors": "", "koFi": "someone", "buyMeACoffee": "", "payPal": "" }"#);
        assert_eq!(one.url("githubSponsors"), None);
        assert!(one.any());
    }

    #[test]
    fn an_account_that_is_not_a_plain_name_is_never_put_in_an_address() {
        for name in [
            "a/b",
            "../x",
            "a b",
            "a.b",
            "https://example.com",
            "a?b=c",
            "a#b",
            "é",
            &"x".repeat(65),
        ] {
            let support = Support {
                ko_fi: name.to_string(),
                ..Support::default()
            };
            assert_eq!(support.url("koFi"), None, "{name}");
            assert!(!support.any(), "{name}");
        }
    }

    #[test]
    fn only_the_named_links_open_and_none_is_an_address_from_the_window() {
        let support = made_up();
        assert_eq!(target("source", &support), Some(Target::Page(SOURCE.into())));
        assert_eq!(target("xlights", &support), Some(Target::Page(XLIGHTS.into())));
        assert_eq!(target("licence", &support), Some(Target::Bundled("LICENSE.txt")));
        assert_eq!(target("credits", &support), Some(Target::Bundled("NOTICE.txt")));
        assert_eq!(
            target("notices", &support),
            Some(Target::Bundled("THIRD-PARTY-NOTICES.txt"))
        );
        for link in [
            "https://example.com",
            "file:///etc/passwd",
            "kofi",
            "",
            "source/../x",
        ] {
            assert_eq!(target(link, &support), None, "{link}");
        }
    }

    #[test]
    fn the_accounts_file_reads_and_what_cannot_be_read_sets_up_nothing() {
        let text = include_str!("../../support.json");
        let read: Support = serde_json::from_str(text).expect("app/support.json reads");
        assert_eq!(read, *SUPPORT);
        // Every name in it is either empty or one that makes an address.
        for (way, _) in WAYS {
            let name = read.account(way).unwrap();
            assert!(name.is_empty() || read.url(way).is_some(), "{way}: {name}");
        }
        assert_eq!(Support::read("not json"), Support::default());
        assert_eq!(
            Support::read(r#"{ "koFi": "x", "other": "y" }"#),
            Support::default()
        );
    }

    #[test]
    fn the_bundled_files_are_the_ones_the_app_is_built_with() {
        let conf: serde_json::Value = serde_json::from_str(include_str!("../tauri.conf.json")).unwrap();
        let bundled: Vec<&str> = conf["bundle"]["resources"]
            .as_object()
            .unwrap()
            .values()
            .map(|name| name.as_str().unwrap())
            .collect();
        for link in ["licence", "credits", "notices"] {
            let Some(Target::Bundled(name)) = target(link, &Support::default()) else {
                panic!("{link} is a bundled file");
            };
            assert!(bundled.contains(&name), "{name} is not in bundle.resources");
        }
    }
}
