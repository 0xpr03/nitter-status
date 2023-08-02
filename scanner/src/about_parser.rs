// SPDX-License-Identifier: AGPL-3.0-only
use regex::{Regex, RegexBuilder};
use reqwest::Url;
use scraper::{Html, Selector};
use thiserror::Error;

use crate::instance_parser::EXPECT_CSS_SELCTOR;

pub type Result<T> = std::result::Result<T, AboutParseError>;

#[derive(Error, Debug)]
pub enum AboutParseError {
    #[error("No p containing a version found!")]
    NoAboutElement,
    #[error("No a element found!")]
    NoCommitLinkFound,
    #[error("Missing test! Found '{0}'")]
    InvalidCommitFormat(String),
    #[error("No valid href!")]
    NoValidHref,
}

pub(crate) struct AboutParser {
    selector_p: Selector,
    selector_a: Selector,
    regex: Regex,
}

pub struct AboutParsed {
    pub version_name: String,
    pub url: String,
}

impl AboutParser {
    /// Returns the text of the version <a> element of nitters about site
    pub fn parse_about_version(&self, html: &str) -> Result<AboutParsed> {
        let fragment = Html::parse_fragment(html);
        // get all <p> elements
        let p_elem = fragment
            .select(&self.selector_p)
            .find(|t| t.text().any(|text| text.contains("Version")))
            .ok_or(AboutParseError::NoAboutElement)?;

        let mut a_elems = p_elem.select(&self.selector_a);
        let link = a_elems.next().ok_or(AboutParseError::NoCommitLinkFound)?;
        let url = link.value().attr("href").map(|v|v.trim().to_owned())
            .ok_or(AboutParseError::NoValidHref)?;
        let link_text = link.text().fold(String::new(), |mut acc, text| {
            acc.push_str(text);
            acc
        });
        if !self.regex.is_match(&link_text) {
            return Err(AboutParseError::InvalidCommitFormat(link_text));
        }
        Ok(AboutParsed {
            url,
            version_name: link_text
        })
    }

    pub fn new() -> Self {
        let mut builder = RegexBuilder::new(r#"^((\d+\.\d+\.\d+)|[a-zA-Z0-9]{7,})"#);
        builder.case_insensitive(true);
        Self {
            selector_p: Selector::parse("p").expect(EXPECT_CSS_SELCTOR),
            selector_a: Selector::parse("a").expect(EXPECT_CSS_SELCTOR),
            regex: builder.build().expect("failed to generate regex"),
        }
    }
}

#[cfg(test)]
mod test {
    use super::*;
    #[test]
    fn parse() {
        let html = include_str!("../test_data/about.html");
        let parser = AboutParser::new();
        let res = parser.parse_about_version(html).unwrap();
        assert_eq!(&res.version_name, "2023.07.22-72d8f35");
        assert_eq!(res.url, String::from("https://github.com/zedeus/nitter/commit/72d8f35"));
    }
}
