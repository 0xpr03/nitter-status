use std::collections::HashMap;

use reqwest::Url;
use scraper::{ElementRef, Html, Selector};
use thiserror::Error;

pub type Result<T> = std::result::Result<T, InstaceListError>;

pub static EXPECT_CSS_SELCTOR: &'static str = "failed to parse css selector";
static CHECKBOX: &'static str = "✅";

type InstanceMap = HashMap<String, InstanceParsed>;

#[derive(Error, Debug)]
pub enum InstaceListError {
    #[error("No div #wiki-body found!")]
    NoWikiDiv,
    #[error("No table found containing instances!")]
    NoInstanceTable,
    #[error("Abort-on-err on, malformed table row found!")]
    MalformedRow,
}
#[derive(Debug, Eq, PartialEq)]
pub struct InstanceParsed {
    /// URL without any login stuff
    pub domain: String,
    /// connection URL
    pub url: String,
    /// whether this instance is marked as online
    pub online: bool,
    /// whether this is marked as up to date
    pub up_to_date: bool,
    /// the SSL provider this instance supposedly has
    pub ssl_provider: String,
}

impl InstanceParsed {
    /// just for testing purposes
    #[cfg(test)]
    fn from(domain: &str, url: &str, online: bool, up_to_date: bool, ssl_provider: &str) -> Self {
        Self {
            domain: domain.to_owned(),
            url: url.to_owned(),
            online,
            up_to_date,
            ssl_provider: ssl_provider.to_owned(),
        }
    }
}

/// Instance parser.
pub(crate) struct InstanceParser {
    selector_wiki: Selector,
    selector_table: Selector,
    selector_tr: Selector,
    selector_td: Selector,
    selector_a: Selector,
}

impl InstanceParser {
    pub fn new() -> Self {
        Self {
            selector_wiki: Selector::parse(r#"div[id="wiki-body"]"#).expect(EXPECT_CSS_SELCTOR),
            selector_table: Selector::parse("table").expect(EXPECT_CSS_SELCTOR),
            selector_tr: Selector::parse("tbody > tr").expect(EXPECT_CSS_SELCTOR),
            selector_td: Selector::parse("td").expect(EXPECT_CSS_SELCTOR),
            selector_a: Selector::parse("a").expect(EXPECT_CSS_SELCTOR),
        }
    }

    /// Parse a html rendered version of the instance list
    ///
    /// *abort_on_err* is just for testing and return an error for any malformed table entry
    pub fn parse_instancelist(
        &self,
        html: &str,
        additional_instances: &[String],
        abort_on_err: bool,
    ) -> Result<InstanceMap> {
        let fragment = Html::parse_fragment(html);
        // wiki body by div ID
        let mut wiki_divs = fragment.select(&self.selector_wiki);
        // first result
        let first_wiki = wiki_divs.next().ok_or(InstaceListError::NoWikiDiv)?;
        // all <table> element
        let mut tables = first_wiki.select(&self.selector_table);
        // find the one with "Online" text inside
        let instance_table = tables
            .find(|t| t.text().any(|text| text.contains("Online")))
            .ok_or(InstaceListError::NoInstanceTable)?;

        let mut instances = HashMap::with_capacity(50);
        // iterate over all <body> > <tr> inside
        for row in instance_table.select(&self.selector_tr) {
            match self.parse_row(row) {
                Ok(instance) => {
                    if let Some(old) = instances.insert(instance.domain.clone(), instance) {
                        tracing::warn!(domain = old.domain, "Parsed duplicate instance domain!");
                    }
                }
                Err(e) => {
                    if abort_on_err {
                        return Err(e);
                    }
                    continue;
                }
            }
        }

        for entry in additional_instances {
            match Url::parse(entry.as_ref()) {
                Ok(v) => {
                    if let Some(domain) = v.domain() {
                        instances.insert(
                            domain.to_owned(),
                            InstanceParsed {
                                domain: domain.to_owned(),
                                url: entry.clone(),
                                online: true,
                                up_to_date: true,
                                ssl_provider: String::new(),
                            },
                        );
                    }
                }
                Err(e) => tracing::warn!(instance=entry,error=?e,"Ignoring additional instance"),
            }
        }

        Ok(instances)
    }

    /// Parse a single instance table row
    fn parse_row(&self, row: ElementRef) -> Result<InstanceParsed> {
        let mut cols = row.select(&self.selector_td);
        // get first URL column
        let url: String = match cols.next() {
            None => {
                tracing::error!(row=?row.html(),"Parsed instance missing URL row, skipping!");
                return Err(InstaceListError::MalformedRow);
            }
            Some(col) => {
                // find first <a> inside and get its "href" attribute
                let mut a_elems = col.select(&self.selector_a);
                match a_elems.next().and_then(|v| v.value().attr("href")) {
                    None => {
                        tracing::error!(row=?row.html(),"Parsed instance missing valid URL <a> element, skipping!");
                        return Err(InstaceListError::MalformedRow);
                    }
                    Some(url_value) => {
                        // trim whitespace and strip `/` at the end
                        let trimmed = url_value.trim();
                        trimmed.strip_suffix("/").unwrap_or(trimmed).to_owned()
                    }
                }
            }
        };
        // parse URL to strip everything apart from the domain
        let domain = match Url::parse(&url) {
            Ok(parsed_url) => parsed_url.domain().map(|v| v.to_owned()).ok_or_else(|| {
                tracing::error!(url = url, "Parsed instance URL has no domain");
                InstaceListError::MalformedRow
            })?,
            Err(e) => {
                tracing::error!(url=url,error=?e,"Parsed instance URL is not valid");
                return Err(InstaceListError::MalformedRow);
            }
        };

        // map all remaining cols into Strings
        let columns: Vec<_> = cols
            .map(|col| {
                col.text().fold(String::new(), |mut acc, text| {
                    acc.push_str(text);
                    acc
                })
            })
            .collect();
        if columns.len() < 4 {
            tracing::error!(instance_data=?columns,"Parsed instance missing fields, skipping!");
            return Err(InstaceListError::MalformedRow);
        }
        let instance = InstanceParsed {
            domain,
            url,
            online: columns[0] == CHECKBOX,
            up_to_date: columns[1] == CHECKBOX,
            ssl_provider: columns[3].clone(),
        };
        Ok(instance)
    }
}

#[cfg(test)]
mod test {
    use std::collections::HashMap;

    use tracing_test::traced_test;

    use super::*;
    #[test]
    #[traced_test]
    fn parse() {
        let html = include_str!("../test_data/instancelist.html");
        let parser = InstanceParser::new();
        let res = parser.parse_instancelist(html, &[], true).unwrap();
        dbg!(&res);

        // adjust when updating the test html
        let expected: HashMap<String, InstanceParsed> = expected_data()
            .into_iter()
            .map(|instance| (instance.domain.clone(), instance))
            .collect();
        assert_eq!(res.len(), expected.len());
        for (_, instance) in res.iter() {
            assert_eq!(Some(instance), expected.get(&instance.domain));
        }
    }

    fn expected_data() -> Vec<InstanceParsed> {
        vec![
            InstanceParsed::from(
                "nitter.lacontrevoie.fr",
                "https://nitter.lacontrevoie.fr",
                true,
                true,
                "Let's Encrypt",
            ),
            InstanceParsed::from(
                "nitter.nixnet.services",
                "https://nitter:nitter@nitter.nixnet.services",
                true,
                true,
                "Let's Encrypt",
            ),
            InstanceParsed::from(
                "nitter.fdn.fr",
                "https://nitter.fdn.fr",
                true,
                true,
                "Let's Encrypt",
            ),
            InstanceParsed::from(
                "nitter.1d4.us",
                "https://nitter.1d4.us",
                true,
                true,
                "Let's Encrypt",
            ),
            InstanceParsed::from(
                "nitter.kavin.rocks",
                "https://nitter.kavin.rocks",
                true,
                true,
                "Let's Encrypt",
            ),
            InstanceParsed::from(
                "nitter.unixfox.eu",
                "https://nitter.unixfox.eu",
                true,
                true,
                "Buypass AS-983163327",
            ),
            InstanceParsed::from(
                "nitter.domain.glass",
                "https://nitter.domain.glass",
                true,
                false,
                "Cloudflare",
            ),
            InstanceParsed::from(
                "birdsite.xanny.family",
                "https://birdsite.xanny.family",
                true,
                false,
                "Cloudflare",
            ),
            InstanceParsed::from(
                "nitter.moomoo.me",
                "https://nitter.moomoo.me",
                true,
                true,
                "Let's Encrypt",
            ),
            InstanceParsed::from(
                "bird.trom.tf",
                "https://bird.trom.tf",
                false,
                false,
                "Let's Encrypt",
            ),
            InstanceParsed::from(
                "nitter.it",
                "https://nitter.it",
                true,
                false,
                "Let's Encrypt",
            ),
            InstanceParsed::from(
                "twitter.owacon.moe",
                "https://twitter.owacon.moe",
                true,
                true,
                "Let's Encrypt",
            ),
            InstanceParsed::from(
                "notabird.site",
                "https://notabird.site",
                false,
                false,
                "Cloudflare",
            ),
            InstanceParsed::from(
                "nitter.weiler.rocks",
                "https://nitter.weiler.rocks",
                true,
                true,
                "Let's Encrypt",
            ),
            InstanceParsed::from(
                "nitter.sethforprivacy.com",
                "https://nitter.sethforprivacy.com",
                true,
                true,
                "Let's Encrypt",
            ),
            InstanceParsed::from(
                "nitter.nl",
                "https://nitter.nl",
                true,
                true,
                "Let's Encrypt",
            ),
            InstanceParsed::from(
                "nitter.mint.lgbt",
                "https://nitter.mint.lgbt",
                true,
                true,
                "Let's Encrypt",
            ),
            InstanceParsed::from(
                "nitter.esmailelbob.xyz",
                "https://nitter.esmailelbob.xyz",
                true,
                false,
                "Let's Encrypt",
            ),
            InstanceParsed::from(
                "tw.artemislena.eu",
                "https://tw.artemislena.eu",
                true,
                true,
                "Let's Encrypt",
            ),
            InstanceParsed::from(
                "nitter.tiekoetter.com",
                "https://nitter.tiekoetter.com",
                true,
                false,
                "Let's Encrypt",
            ),
            InstanceParsed::from(
                "nitter.privacy.com.de",
                "https://nitter.privacy.com.de",
                true,
                true,
                "ZeroSSL",
            ),
            InstanceParsed::from(
                "nitter.poast.org",
                "https://nitter.poast.org",
                true,
                true,
                "Let's Encrypt",
            ),
            InstanceParsed::from(
                "nitter.bird.froth.zone",
                "https://nitter.bird.froth.zone",
                true,
                false,
                "Let's Encrypt",
            ),
            InstanceParsed::from(
                "nitter.cz",
                "https://nitter.cz",
                true,
                true,
                "Let's Encrypt",
            ),
            InstanceParsed::from(
                "nitter.privacydev.net",
                "https://nitter.privacydev.net",
                true,
                true,
                "Let's Encrypt",
            ),
            InstanceParsed::from(
                "tweet.lambda.dance",
                "https://tweet.lambda.dance",
                true,
                false,
                "Let's Encrypt",
            ),
            InstanceParsed::from(
                "nitter.kylrth.com",
                "https://nitter.kylrth.com",
                true,
                false,
                "Let's Encrypt",
            ),
            InstanceParsed::from(
                "nitter.tokhmi.xyz",
                "https://nitter.tokhmi.xyz",
                true,
                false,
                "Let's Encrypt",
            ),
            InstanceParsed::from(
                "nitter.catalyst.sx",
                "https://nitter.catalyst.sx",
                false,
                false,
                "Let's Encrypt",
            ),
            InstanceParsed::from(
                "unofficialbird.com",
                "https://unofficialbird.com",
                true,
                true,
                "ZeroSSL",
            ),
            InstanceParsed::from(
                "nitter.projectsegfau.lt",
                "https://nitter.projectsegfau.lt",
                true,
                true,
                "Let's Encrypt",
            ),
            InstanceParsed::from(
                "nitter.eu.projectsegfau.lt",
                "https://nitter.eu.projectsegfau.lt",
                true,
                true,
                "Let's Encrypt",
            ),
            InstanceParsed::from(
                "nitter.in.projectsegfau.lt",
                "https://nitter.in.projectsegfau.lt",
                true,
                true,
                "Let's Encrypt",
            ),
            InstanceParsed::from(
                "singapore.unofficialbird.com",
                "https://singapore.unofficialbird.com",
                true,
                true,
                "Let's Encrypt",
            ),
            InstanceParsed::from(
                "canada.unofficialbird.com",
                "https://canada.unofficialbird.com",
                true,
                true,
                "Let's Encrypt",
            ),
            InstanceParsed::from(
                "india.unofficialbird.com",
                "https://india.unofficialbird.com",
                true,
                true,
                "Let's Encrypt",
            ),
            InstanceParsed::from(
                "nederland.unofficialbird.com",
                "https://nederland.unofficialbird.com",
                true,
                true,
                "Let's Encrypt",
            ),
            InstanceParsed::from(
                "uk.unofficialbird.com",
                "https://uk.unofficialbird.com",
                true,
                true,
                "Let's Encrypt",
            ),
            InstanceParsed::from("n.l5.ca", "https://n.l5.ca", true, false, "Let's Encrypt"),
            InstanceParsed::from(
                "nitter.slipfox.xyz",
                "https://nitter.slipfox.xyz",
                true,
                false,
                "Let's Encrypt",
            ),
            InstanceParsed::from(
                "nitter.soopy.moe",
                "https://nitter.soopy.moe",
                true,
                false,
                "Let's Encrypt",
            ),
            InstanceParsed::from(
                "nitter.qwik.space",
                "https://nitter.qwik.space",
                true,
                false,
                "Let's Encrypt",
            ),
            InstanceParsed::from(
                "read.whatever.social",
                "https://read.whatever.social",
                true,
                false,
                "Let's Encrypt",
            ),
            InstanceParsed::from(
                "nitter.rawbit.ninja",
                "https://nitter.rawbit.ninja",
                true,
                false,
                "Cloudflare",
            ),
            InstanceParsed::from(
                "nt.vern.cc",
                "https://nt.vern.cc",
                false,
                false,
                "Let's Encrypt",
            ),
            InstanceParsed::from(
                "nitter.ir",
                "https://nitter.ir",
                true,
                false,
                "Let's Encrypt",
            ),
            InstanceParsed::from(
                "nitter.privacytools.io",
                "https://nitter.privacytools.io",
                true,
                false,
                "Cloudflare",
            ),
            InstanceParsed::from(
                "n.sneed.network",
                "https://n.sneed.network",
                true,
                true,
                "Cloudflare",
            ),
            InstanceParsed::from(
                "nitter.smnz.de",
                "https://nitter.smnz.de",
                false,
                false,
                "Let's Encrypt",
            ),
            InstanceParsed::from(
                "nitter.twei.space",
                "https://nitter.twei.space",
                true,
                false,
                "Let's Encrypt",
            ),
            InstanceParsed::from(
                "nitter.inpt.fr",
                "https://nitter.inpt.fr",
                true,
                false,
                "Let's Encrypt",
            ),
            InstanceParsed::from(
                "nitter.d420.de",
                "https://nitter.d420.de",
                true,
                true,
                "Cloudflare",
            ),
            InstanceParsed::from(
                "nitter.caioalonso.com",
                "https://nitter.caioalonso.com",
                true,
                false,
                "Let's Encrypt",
            ),
            InstanceParsed::from(
                "nitter.at",
                "https://nitter.at",
                true,
                true,
                "Let's Encrypt",
            ),
            InstanceParsed::from(
                "nitter.pw",
                "https://nitter.pw",
                true,
                false,
                "Let's Encrypt",
            ),
            InstanceParsed::from(
                "nitter.nicfab.eu",
                "https://nitter.nicfab.eu",
                true,
                true,
                "Let's Encrypt",
            ),
            InstanceParsed::from(
                "bird.habedieeh.re",
                "https://bird.habedieeh.re",
                true,
                false,
                "Let's Encrypt",
            ),
            InstanceParsed::from(
                "nitter.hostux.net",
                "https://nitter.hostux.net",
                true,
                true,
                "Let's Encrypt",
            ),
            InstanceParsed::from(
                "nitter.adminforge.de",
                "https://nitter.adminforge.de",
                true,
                false,
                "Let's Encrypt",
            ),
            InstanceParsed::from(
                "nitter.platypush.tech",
                "https://nitter.platypush.tech",
                false,
                false,
                "Let's Encrypt",
            ),
            InstanceParsed::from(
                "nitter.pufe.org",
                "https://nitter.pufe.org",
                false,
                false,
                "Let's Encrypt",
            ),
            InstanceParsed::from(
                "nitter.us.projectsegfau.lt",
                "https://nitter.us.projectsegfau.lt",
                true,
                true,
                "Let's Encrypt",
            ),
            InstanceParsed::from("t.com.sb", "https://t.com.sb", true, false, "ZeroSSL"),
            InstanceParsed::from(
                "nitter.kling.gg",
                "https://nitter.kling.gg",
                true,
                true,
                "Let's Encrypt",
            ),
            InstanceParsed::from(
                "nitter.riverside.rocks",
                "https://nitter.riverside.rocks",
                false,
                false,
                "Let's Encrypt",
            ),
            InstanceParsed::from(
                "nitter.lunar.icu",
                "https://nitter.lunar.icu",
                true,
                false,
                "Cloudflare",
            ),
            InstanceParsed::from(
                "twitter.moe.ngo",
                "https://twitter.moe.ngo",
                true,
                false,
                "Google Trust Services LLC",
            ),
            InstanceParsed::from(
                "nitter.freedit.eu",
                "https://nitter.freedit.eu",
                true,
                false,
                "Let's Encrypt",
            ),
            InstanceParsed::from(
                "ntr.frail.duckdns.org",
                "https://ntr.frail.duckdns.org",
                false,
                false,
                "Let's Encrypt",
            ),
            InstanceParsed::from(
                "n.opnxng.com",
                "https://n.opnxng.com",
                true,
                false,
                "Let's Encrypt",
            ),
            InstanceParsed::from(
                "nitter.tux.pizza",
                "https://nitter.tux.pizza",
                true,
                true,
                "Let's Encrypt",
            ),
            InstanceParsed::from(
                "t.floss.media",
                "https://t.floss.media",
                false,
                false,
                "Let's Encrypt",
            ),
            InstanceParsed::from(
                "twit.hell.rodeo",
                "https://twit.hell.rodeo",
                false,
                false,
                "Let's Encrypt",
            ),
            InstanceParsed::from(
                "nitter.nachtalb.io",
                "https://nitter.nachtalb.io",
                false,
                false,
                "Cloudflare",
            ),
            InstanceParsed::from(
                "n.quadtr.ee",
                "https://n.quadtr.ee",
                false,
                false,
                "Cloudflare",
            ),
            InstanceParsed::from(
                "nitter.altgr.xyz",
                "https://nitter.altgr.xyz",
                true,
                false,
                "Cloudflare",
            ),
            InstanceParsed::from(
                "jote.lile.cl",
                "https://jote.lile.cl",
                true,
                false,
                "Cloudflare",
            ),
            InstanceParsed::from(
                "nitter.one",
                "https://nitter.one",
                true,
                false,
                "Let's Encrypt",
            ),
            InstanceParsed::from(
                "nitter.instances.cc",
                "https://nitter.instances.cc",
                false,
                false,
                "Cloudflare",
            ),
            InstanceParsed::from(
                "nitter.io.lol",
                "https://nitter.io.lol",
                true,
                false,
                "Let's Encrypt",
            ),
            InstanceParsed::from(
                "nitter.hu",
                "https://nitter.hu",
                true,
                false,
                "Let's Encrypt",
            ),
            InstanceParsed::from(
                "nitter.no-logs.com",
                "https://nitter.no-logs.com",
                true,
                false,
                "Let's Encrypt",
            ),
            InstanceParsed::from(
                "nitter.ftw.lol",
                "https://nitter:nitter@nitter.ftw.lol",
                false,
                false,
                "Let's Encrypt",
            ),
            InstanceParsed::from(
                "tweet.whateveritworks.org",
                "https://tweet.whateveritworks.org",
                true,
                false,
                "Let's Encrypt + Cloudflare",
            ),
            InstanceParsed::from(
                "nitter.fediflix.org",
                "https://nitter.fediflix.org",
                true,
                false,
                "Let's Encrypt",
            ),
            InstanceParsed::from(
                "nitter.nohost.network",
                "https://nitter.nohost.network",
                true,
                false,
                "Let's Encrypt",
            ),
            InstanceParsed::from(
                "twt.funami.tech",
                "https://twt.funami.tech",
                false,
                false,
                "Let's Encrypt",
            ),
            InstanceParsed::from(
                "nitter.simpleprivacy.fr",
                "https://nitter.simpleprivacy.fr",
                true,
                false,
                "Let's Encrypt",
            ),
            InstanceParsed::from(
                "nitter.onthescent.xyz",
                "https://nitter.onthescent.xyz",
                true,
                false,
                "Let's Encrypt",
            ),
            InstanceParsed::from(
                "nitter.x86-64-unknown-linux-gnu.zip",
                "https://nitter.x86-64-unknown-linux-gnu.zip",
                true,
                true,
                "Let's Encrypt",
            ),
            InstanceParsed::from(
                "nitter.private.coffee",
                "https://nitter.private.coffee",
                true,
                true,
                "Let's Encrypt",
            ),
            InstanceParsed::from(
                "nitter.oksocial.net",
                "https://nitter.oksocial.net",
                true,
                true,
                "Let's Encrypt",
            ),
            InstanceParsed::from(
                "nitter.services.woodland.cafe",
                "https://nitter.services.woodland.cafe",
                true,
                true,
                "Let's Encrypt",
            ),
            InstanceParsed::from(
                "nitter.datura.network",
                "https://nitter.datura.network",
                true,
                true,
                "Let's Encrypt",
            ),
        ]
    }
}
