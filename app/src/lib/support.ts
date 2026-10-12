// The ways to support the project. The account names are set in one file, app/support.json
// (the shell reads the same file: app/src-tauri/src/links.rs), and .github/FUNDING.yml says
// the same for the repository's page. A name left empty there is not listed anywhere here.
//
// The window never opens an address itself. It names a way, and the shell opens the address it
// has for that name in the system's browser (Backend.openProjectLink).

import accounts from "../../support.json";
import type { SupportWay } from "../api/types";

/** Each service, in the order it is listed, and what its address begins with. */
const SERVICES: { way: SupportWay; label: string; site: string }[] = [
  { way: "githubSponsors", label: "GitHub Sponsors", site: "github.com/sponsors/" },
  { way: "koFi", label: "Ko-fi", site: "ko-fi.com/" },
  { way: "buyMeACoffee", label: "Buy Me a Coffee", site: "buymeacoffee.com/" },
  { way: "payPal", label: "PayPal", site: "paypal.me/" },
];

/** An account name as these services write one: no slashes, dots, or spaces. */
const PLAIN_NAME = /^[A-Za-z0-9_-]{1,64}$/;

/** The account name for each way; empty where there is none. */
export type SupportAccounts = Record<SupportWay, string>;

/** A way to give that is set up. */
export interface SupportLink {
  way: SupportWay;
  /** The service's name, for the button. */
  label: string;
  /** Where it goes, without the "https://": for saying so before it opens. */
  address: string;
}

/** The ways that are set up, in order. One with no account, or a name that isn't plain, is left out. */
export function supportLinks(names: SupportAccounts = accounts): SupportLink[] {
  return SERVICES.flatMap(({ way, label, site }) => {
    const name: string = names[way] ?? "";
    return PLAIN_NAME.test(name) ? [{ way, label, address: `${site}${name}` }] : [];
  });
}
