use serde::{Deserialize, Serialize};
use ts_rs::TS;
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../frontend/src/types-generated/")]
pub struct DocumentId(pub Uuid);

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../frontend/src/types-generated/")]
pub struct DocumentInfo {
    pub id: DocumentId,
    pub path: String,
    pub title: String,
    pub page_count: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../frontend/src/types-generated/")]
pub struct PageSize {
    pub width: f32,
    pub height: f32,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../frontend/src/types-generated/")]
pub struct RecentFile {
    pub path: String,
    pub title: String,
    pub last_opened_ms: i64,
    pub pinned: bool,
    pub page_count: Option<u32>,
    pub thumb_path: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../frontend/src/types-generated/")]
#[serde(rename_all = "lowercase")]
pub enum Theme {
    System,
    Light,
    Dark,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../frontend/src/types-generated/")]
pub struct AppVersion {
    pub name: String,
    pub version: String,
    pub pdfium_version: Option<String>,
}
