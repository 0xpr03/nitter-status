// SPDX-License-Identifier: AGPL-3.0-only
//! Parse profile pages for verification
use regex::{Regex, RegexBuilder};
use reqwest::Url;
use scraper::{Html, Selector};
use thiserror::Error;

use crate::instance_parser::EXPECT_CSS_SELCTOR;

pub type Result<T> = std::result::Result<T, ProfileParseError>;

#[derive(Error, Debug)]
pub enum ProfileParseError {
    #[error("No profile-card div found!")]
    NoProfileCard,
    #[error("No timeline div found!")]
    NoTimeline,
    #[error("No timeline-item div found!")]
    NoTimelineItem,
}

pub(crate) struct ProfileParser {
    selector_profile_card_name: Selector,
    selector_timeline: Selector,
    selector_timeline_item: Selector,
    regex: Regex,
}

#[derive(Debug)]
pub struct ProfileParsed {
    pub post_count: usize,
    pub name: String,
}

impl ProfileParser {
    /// Returns the health-check relevant part of a nitter account profile
    pub fn parse_profile_content(&self, html: &str) -> Result<ProfileParsed> {
        let fragment = Html::parse_fragment(html);
        // get profile info div
        let mut profile_card_name_divs = fragment.select(&self.selector_profile_card_name);
        let first_card = profile_card_name_divs
            .next()
            .ok_or(ProfileParseError::NoProfileCard)?;
        let profile_name = first_card.text().fold(String::new(), |mut acc, text| {
            acc.push_str(text);
            acc
        });

        // find timeline div
        let mut timeline_divs = fragment.select(&self.selector_timeline);
        let first_timeline_div = timeline_divs.next().ok_or(ProfileParseError::NoTimeline)?;
        // select timeline-item divs inside
        let timeline_items = first_timeline_div.select(&self.selector_timeline_item);
        let timeline_item_count = timeline_items.count();

        Ok(ProfileParsed {
            post_count: timeline_item_count,
            name: profile_name,
        })
    }

    pub fn new() -> Self {
        let mut builder = RegexBuilder::new(r#"^((\d+\.\d+\.\d+)|[a-zA-Z0-9]{7,})"#);
        builder.case_insensitive(true);
        Self {
            selector_profile_card_name: Selector::parse(".profile-card-username")
                .expect(EXPECT_CSS_SELCTOR),
            selector_timeline: Selector::parse(".timeline").expect(EXPECT_CSS_SELCTOR),
            selector_timeline_item: Selector::parse(".timeline-item").expect(EXPECT_CSS_SELCTOR),
            regex: builder.build().expect("failed to generate regex"),
        }
    }
}

#[cfg(test)]
mod test {
    use super::*;
    #[test]
    fn parse() {
        let html = include_str!("../test_data/profile.html");
        let parser = ProfileParser::new();
        let res = parser.parse_profile_content(html).unwrap();
        assert_eq!(&res.name, "@jack");
        assert_eq!(res.post_count, 20);
    }
}
