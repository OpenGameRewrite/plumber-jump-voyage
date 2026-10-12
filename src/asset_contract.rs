//! The local export format. Runtime behavior uses roles, not display names.
use bevy::prelude::*;
use serde::Deserialize;

#[derive(Clone, Copy, Debug, Default, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum VisibilityRole {
    #[default]
    Unchanged,
    Hidden,
    Visible,
    ClosedLeft,
    ClosedRight,
    PupilLeft,
    PupilRight,
    Track,
    WornCap,
    UncappedHair,
}
#[derive(Clone, Copy, Debug, Default, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum Surface {
    #[default]
    Ordinary,
    Glass,
    Hair,
}
#[derive(Clone, Copy, Debug, Default, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum Stain {
    #[default]
    None,
    Cap,
    Body,
}
#[derive(Component, Clone, Debug, Default, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct Part {
    pub version: u32,
    pub role: VisibilityRole,
    pub track: Option<String>,
    pub pupil: Option<String>,
    pub surface: Surface,
    pub stain: Stain,
    pub exported: bool,
    pub metal: bool,
}
#[derive(Component, Clone, Debug, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct Bone {
    pub version: u32,
    pub key: [String; 2],
    pub leg: bool,
    pub hip: bool,
}

pub fn decode<T: serde::de::DeserializeOwned>(json: &str, key: &str) -> Result<Option<T>, String> {
    let document: serde_json::Value = serde_json::from_str(json).map_err(|e| e.to_string())?;
    let Some(value) = document.get(key) else {
        return Ok(None);
    };
    if value.get("version").and_then(serde_json::Value::as_u64) != Some(1) {
        return Err(format!("Unsupported {key} metadata version"));
    }
    if key == "pjv_part"
        && matches!(
            value.get("role").and_then(serde_json::Value::as_str),
            Some("track" | "worn_cap")
        )
        && value
            .get("track")
            .and_then(serde_json::Value::as_str)
            .is_none_or(|s| s.is_empty())
    {
        return Err("A visibility role requires its track identity".into());
    }
    if key == "pjv_bone"
        && value
            .get("key")
            .and_then(serde_json::Value::as_array)
            .and_then(|key| key.first())
            .and_then(serde_json::Value::as_str)
            .is_none_or(|s| s.is_empty())
    {
        return Err("A bone requires a nonempty identity".into());
    }
    serde_json::from_value(value.clone())
        .map(Some)
        .map_err(|e| e.to_string())
}

#[path = "tests/asset_contract.rs"]
mod checks;
pub fn self_check() {
    checks::run();
}
