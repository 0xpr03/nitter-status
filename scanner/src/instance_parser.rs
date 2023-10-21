// SPDX-License-Identifier: AGPL-3.0-only
use std::collections::HashMap;

use reqwest::Url;
use scraper::{ElementRef, Html, Selector};
use thiserror::Error;

pub type Result<T> = std::result::Result<T, InstanceListError>;

pub static EXPECT_CSS_SELCTOR: &'static str = "failed to parse css selector";
static CHECKBOX: &'static str = "✅";

type InstanceMap = HashMap<String, InstanceParsed>;

#[derive(Error, Debug)]
pub enum InstanceListError {
    #[error("No div #wiki-body found!")]
    NoWikiDiv,
    #[error("No table found containing instances!")]
    NoInstanceTable,
    #[error("Abort-on-err on, malformed table row found!")]
    MalformedRow,
}
#[derive(Debug, Eq, PartialEq)]
#[cfg_attr(test, derive(serde::Serialize, serde::Deserialize))]
pub struct InstanceParsed {
    /// URL without any login stuff
    pub domain: String,
    /// connection URL
    pub url: String,
    /// whether this instance is marked as online
    pub online: bool,
    /// the SSL provider this instance supposedly has
    pub ssl_provider: String,
    /// the country for this instance
    pub country: String,
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
        additional_instances_country: &str,
        abort_on_err: bool,
    ) -> Result<InstanceMap> {
        let fragment = Html::parse_fragment(html);
        // wiki body by div ID
        let mut wiki_divs = fragment.select(&self.selector_wiki);
        // first result
        let first_wiki = wiki_divs.next().ok_or(InstanceListError::NoWikiDiv)?;
        // all <table> element
        let mut tables = first_wiki.select(&self.selector_table);
        // find the one with "Online" text inside
        let instance_table = tables
            .find(|t| t.text().any(|text| text.contains("Online")))
            .ok_or(InstanceListError::NoInstanceTable)?;

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
                                ssl_provider: String::new(),
                                country: additional_instances_country.to_owned(),
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
                return Err(InstanceListError::MalformedRow);
            }
            Some(col) => {
                // find first <a> inside and get its "href" attribute
                let mut a_elems = col.select(&self.selector_a);
                match a_elems.next().and_then(|v| v.value().attr("href")) {
                    None => {
                        tracing::error!(row=?row.html(),"Parsed instance missing valid URL <a> element, skipping!");
                        return Err(InstanceListError::MalformedRow);
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
                InstanceListError::MalformedRow
            })?,
            Err(e) => {
                tracing::error!(url=url,error=?e,"Parsed instance URL is not valid");
                return Err(InstanceListError::MalformedRow);
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
            return Err(InstanceListError::MalformedRow);
        }
        let instance = InstanceParsed {
            domain,
            url,
            online: columns[0] == CHECKBOX,
            ssl_provider: columns[3].clone(),
            country: columns[2].clone(),
        };
        Ok(instance)
    }
}

#[cfg(test)]
mod test {
    use csv;
    use std::collections::HashMap;
    use tracing_test::traced_test;

    use super::*;
    #[test]
    #[traced_test]
    fn parse() {
        let html = include_str!("../test_data/instancelist.html");
        let parser = InstanceParser::new();
        let res = parser.parse_instancelist(html, &[], "", true).unwrap();

        // writeback for new tests
        // write_data(res.values());

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
        let file = std::fs::File::open("test_data/instancelist_expected.csv").unwrap();
        let mut rdr = csv::Reader::from_reader(file);
        let mut vec = Vec::new();
        for result in rdr.deserialize() {
            // Notice that we need to provide a type hint for automatic
            // deserialization.
            let record: InstanceParsed = result.unwrap();
            vec.push(record);
        }
        vec
    }
}
