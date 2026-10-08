//! Typed forms of the JSON documents (clean-room work item C16, task P1), for the serde groups
//! that deserialize them into a struct with `serde_ucl` and with `serde_json`. Each one covers
//! every field of its document; a field that is always `null` or an empty array there is
//! `IgnoredAny`. The irregular documents have no typed form, as their keys are random, and
//! neither do the rspamd configurations.

use serde::Deserialize;
use serde::de::IgnoredAny;
use std::collections::BTreeMap;

/// The typed form of [`super::json`].
#[derive(Debug, Deserialize)]
pub struct JsonItems {
    pub items: Vec<JsonItem>,
}

#[derive(Debug, Deserialize)]
pub struct JsonItem {
    pub id: u64,
    pub name: String,
    pub price: f64,
    pub active: bool,
    pub tags: Vec<String>,
    pub dims: Dims,
    pub note: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct Dims {
    pub w: u64,
    pub h: u64,
    pub d: f64,
}

/// `canada.json`: a GeoJSON feature collection, mostly coordinate pairs.
#[derive(Debug, Deserialize)]
pub struct Canada {
    #[serde(rename = "type")]
    pub kind: String,
    pub features: Vec<Feature>,
}

#[derive(Debug, Deserialize)]
pub struct Feature {
    #[serde(rename = "type")]
    pub kind: String,
    pub properties: BTreeMap<String, String>,
    pub geometry: Geometry,
}

#[derive(Debug, Deserialize)]
pub struct Geometry {
    #[serde(rename = "type")]
    pub kind: String,
    pub coordinates: Vec<Vec<(f64, f64)>>,
}

/// `citm_catalog.json`: maps from numeric ids to names, events and performances.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CitmCatalog {
    pub area_names: BTreeMap<String, String>,
    pub audience_sub_category_names: BTreeMap<String, String>,
    pub block_names: BTreeMap<String, String>,
    pub events: BTreeMap<String, Event>,
    pub performances: Vec<Performance>,
    pub seat_category_names: BTreeMap<String, String>,
    pub sub_topic_names: BTreeMap<String, String>,
    pub subject_names: BTreeMap<String, String>,
    pub topic_names: BTreeMap<String, String>,
    pub topic_sub_topics: BTreeMap<String, Vec<u64>>,
    pub venue_names: BTreeMap<String, String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Event {
    pub description: Option<String>,
    pub id: u64,
    pub logo: Option<String>,
    pub name: String,
    pub sub_topic_ids: Vec<u64>,
    pub subject_code: Option<String>,
    pub subtitle: Option<String>,
    pub topic_ids: Vec<u64>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Performance {
    pub event_id: u64,
    pub id: u64,
    pub logo: Option<String>,
    pub name: Option<String>,
    pub prices: Vec<Price>,
    pub seat_categories: Vec<SeatCategory>,
    pub seat_map_image: Option<String>,
    pub start: u64,
    pub venue_code: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Price {
    pub amount: u64,
    pub audience_sub_category_id: u64,
    pub seat_category_id: u64,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SeatCategory {
    pub areas: Vec<Area>,
    pub seat_category_id: u64,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Area {
    pub area_id: u64,
    pub block_ids: Vec<u64>,
}

/// `twitter.json`: a search result of 100 statuses, with users, entities and retweets.
#[derive(Debug, Deserialize)]
pub struct Twitter {
    pub statuses: Vec<Status>,
    pub search_metadata: SearchMetadata,
}

#[derive(Debug, Deserialize)]
pub struct Status {
    pub metadata: StatusMetadata,
    pub created_at: String,
    pub id: u64,
    pub id_str: String,
    pub text: String,
    pub source: String,
    pub truncated: bool,
    pub in_reply_to_status_id: Option<u64>,
    pub in_reply_to_status_id_str: Option<String>,
    pub in_reply_to_user_id: Option<u64>,
    pub in_reply_to_user_id_str: Option<String>,
    pub in_reply_to_screen_name: Option<String>,
    pub user: User,
    pub geo: Option<IgnoredAny>,
    pub coordinates: Option<IgnoredAny>,
    pub place: Option<IgnoredAny>,
    pub contributors: Option<IgnoredAny>,
    pub retweeted_status: Option<Box<Status>>,
    pub retweet_count: u32,
    pub favorite_count: u32,
    pub entities: StatusEntities,
    pub favorited: bool,
    pub retweeted: bool,
    pub possibly_sensitive: Option<bool>,
    pub lang: String,
}

#[derive(Debug, Deserialize)]
pub struct StatusMetadata {
    pub result_type: String,
    pub iso_language_code: String,
}

#[derive(Debug, Deserialize)]
pub struct User {
    pub id: u64,
    pub id_str: String,
    pub name: String,
    pub screen_name: String,
    pub location: String,
    pub description: String,
    pub url: Option<String>,
    pub entities: UserEntities,
    pub protected: bool,
    pub followers_count: u32,
    pub friends_count: u32,
    pub listed_count: u32,
    pub created_at: String,
    pub favourites_count: u32,
    pub utc_offset: Option<i32>,
    pub time_zone: Option<String>,
    pub geo_enabled: bool,
    pub verified: bool,
    pub statuses_count: u32,
    pub lang: String,
    pub contributors_enabled: bool,
    pub is_translator: bool,
    pub is_translation_enabled: bool,
    pub profile_background_color: String,
    pub profile_background_image_url: String,
    pub profile_background_image_url_https: String,
    pub profile_background_tile: bool,
    pub profile_image_url: String,
    pub profile_image_url_https: String,
    pub profile_banner_url: Option<String>,
    pub profile_link_color: String,
    pub profile_sidebar_border_color: String,
    pub profile_sidebar_fill_color: String,
    pub profile_text_color: String,
    pub profile_use_background_image: bool,
    pub default_profile: bool,
    pub default_profile_image: bool,
    pub following: bool,
    pub follow_request_sent: bool,
    pub notifications: bool,
}

#[derive(Debug, Deserialize)]
pub struct UserEntities {
    pub url: Option<UserUrls>,
    pub description: UserUrls,
}

#[derive(Debug, Deserialize)]
pub struct UserUrls {
    pub urls: Vec<Url>,
}

#[derive(Debug, Deserialize)]
pub struct Url {
    pub url: String,
    pub expanded_url: String,
    pub display_url: String,
    pub indices: (u32, u32),
}

#[derive(Debug, Deserialize)]
pub struct StatusEntities {
    pub hashtags: Vec<Hashtag>,
    pub symbols: Vec<IgnoredAny>,
    pub urls: Vec<Url>,
    pub user_mentions: Vec<UserMention>,
    pub media: Option<Vec<Media>>,
}

#[derive(Debug, Deserialize)]
pub struct Hashtag {
    pub text: String,
    pub indices: (u32, u32),
}

#[derive(Debug, Deserialize)]
pub struct UserMention {
    pub screen_name: String,
    pub name: String,
    pub id: u64,
    pub id_str: String,
    pub indices: (u32, u32),
}

#[derive(Debug, Deserialize)]
pub struct Media {
    pub id: u64,
    pub id_str: String,
    pub indices: (u32, u32),
    pub media_url: String,
    pub media_url_https: String,
    pub url: String,
    pub display_url: String,
    pub expanded_url: String,
    #[serde(rename = "type")]
    pub kind: String,
    pub sizes: BTreeMap<String, MediaSize>,
    pub source_status_id: Option<u64>,
    pub source_status_id_str: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct MediaSize {
    pub w: u32,
    pub h: u32,
    pub resize: String,
}

#[derive(Debug, Deserialize)]
pub struct SearchMetadata {
    pub completed_in: f64,
    pub max_id: u64,
    pub max_id_str: String,
    pub next_results: String,
    pub query: String,
    pub refresh_url: String,
    pub count: u32,
    pub since_id: u64,
    pub since_id_str: String,
}
