use std::path::PathBuf;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ItemKind {
    Text,
    RichText,
    Link,
    Color,
    Image,
    File,
}

impl ItemKind {
    pub fn as_str(&self) -> &'static str {
        match self {
            ItemKind::Text => "text",
            ItemKind::RichText => "rich",
            ItemKind::Link => "link",
            ItemKind::Color => "color",
            ItemKind::Image => "image",
            ItemKind::File => "file",
        }
    }

    pub fn parse(s: &str) -> ItemKind {
        match s {
            "rich" => ItemKind::RichText,
            "link" => ItemKind::Link,
            "color" => ItemKind::Color,
            "image" => ItemKind::Image,
            "file" => ItemKind::File,
            _ => ItemKind::Text,
        }
    }

    /// Label shown in the card header, matching Paste's naming.
    pub fn label(&self) -> &'static str {
        match self {
            ItemKind::Text | ItemKind::RichText => "Text",
            ItemKind::Link => "Link",
            ItemKind::Color => "Color",
            ItemKind::Image => "Image",
            ItemKind::File => "File",
        }
    }
}

/// Lightweight item representation kept in memory for the whole history.
/// Full payloads (complete text, RTF, HTML) are loaded on demand from the database.
#[derive(Clone, Debug)]
pub struct ClipItem {
    pub id: i64,
    pub kind: ItemKind,
    pub preview: String,
    pub title: Option<String>,
    pub color: Option<String>,
    pub image_path: Option<PathBuf>,
    pub thumb_path: Option<PathBuf>,
    pub favicon_path: Option<PathBuf>,
    pub link_image_path: Option<PathBuf>,
    pub files: Vec<PathBuf>,
    pub app_bundle: String,
    pub app_name: String,
    pub created_at: i64,
    pub char_count: i64,
    pub width: i64,
    pub height: i64,
    pub byte_size: i64,
    pub pinboard_id: Option<i64>,
    pub hash: String,
}

impl ClipItem {
    /// Display name for file items (first file name, plus count).
    pub fn file_label(&self) -> String {
        match self.files.len() {
            0 => String::new(),
            1 => self.files[0]
                .file_name()
                .map(|n| n.to_string_lossy().to_string())
                .unwrap_or_default(),
            n => format!(
                "{} and {} more",
                self.files[0]
                    .file_name()
                    .map(|n| n.to_string_lossy().to_string())
                    .unwrap_or_default(),
                n - 1
            ),
        }
    }

    pub fn domain(&self) -> Option<String> {
        if self.kind != ItemKind::Link {
            return None;
        }
        url::Url::parse(self.preview.trim())
            .ok()
            .and_then(|u| u.host_str().map(|h| h.trim_start_matches("www.").to_string()))
    }
}

/// Full content needed to write an item back to the pasteboard.
#[derive(Clone, Debug, Default)]
pub struct Payload {
    pub text: String,
    pub rtf: Option<Vec<u8>>,
    pub html: Option<String>,
}

#[derive(Clone, Debug)]
pub struct NewItem {
    pub kind: ItemKind,
    pub text: String,
    pub rtf: Option<Vec<u8>>,
    pub html: Option<String>,
    pub color: Option<String>,
    pub image_path: Option<PathBuf>,
    pub thumb_path: Option<PathBuf>,
    pub files: Vec<PathBuf>,
    pub app_bundle: String,
    pub app_name: String,
    pub char_count: i64,
    pub width: i64,
    pub height: i64,
    pub byte_size: i64,
    pub hash: String,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Pinboard {
    pub id: i64,
    pub name: String,
    pub color: String,
    pub position: i64,
}

pub const PINBOARD_COLORS: &[&str] = &[
    "#FF3B30", "#FF9500", "#FFCC00", "#34C759", "#007AFF", "#AF52DE", "#FF2D55", "#8E8E93",
];

pub const PREVIEW_CHARS: usize = 600;

pub fn make_preview(text: &str) -> String {
    let trimmed = text.trim_matches(|c: char| c == '\n' || c == '\r');
    if trimmed.chars().count() <= PREVIEW_CHARS {
        trimmed.to_string()
    } else {
        trimmed.chars().take(PREVIEW_CHARS).collect()
    }
}
