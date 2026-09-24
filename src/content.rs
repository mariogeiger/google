//! Content blocks: what a caller sends, and a typed view of what the model sent.
//!
//! One wire vocabulary serves `user_input`, `model_output`, thought summaries and
//! function results, but the two directions are different types. A caller
//! writes [`InputContent`]; the model's blocks are replayed as received (see
//! [`crate::step`]) and read through [`OutputContent`], which tolerates values
//! newer than the crate because nothing is ever rebuilt from it.

use base64::Engine as _;
use base64::engine::general_purpose::STANDARD;
use serde::ser::SerializeMap;
use serde::{Deserialize, Serialize, Serializer};
use serde_json::{Map, Value};

use crate::values::{AudioMime, DocumentMime, ImageMime, MediaResolution, VideoMime, VideoProcessing};

// ── Media bytes ──────────────────────────────────────────────────────────────

/// Media bytes in the standard base64 alphabet, as the wire carries them.
///
/// Holding the encoded form means a block serializes without re-encoding, and
/// a string that is not base64 is refused where it enters rather than by a 400.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Base64(String);

/// A string offered as base64 that is not standard padded base64.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InvalidBase64;

impl std::fmt::Display for InvalidBase64 {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("not standard padded base64")
    }
}

impl std::error::Error for InvalidBase64 {}

impl Base64 {
    /// Encode raw bytes.
    pub fn encode(bytes: &[u8]) -> Self {
        Self(STANDARD.encode(bytes))
    }

    /// Accept an already-encoded string, checking that it decodes.
    pub fn from_encoded(encoded: impl Into<String>) -> Result<Self, InvalidBase64> {
        let encoded = encoded.into();
        STANDARD.decode(&encoded).map_err(|_| InvalidBase64)?;
        Ok(Self(encoded))
    }

    /// The encoded text, as sent.
    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// The raw bytes.
    pub fn decode(&self) -> Vec<u8> {
        STANDARD.decode(&self.0).expect("checked at construction")
    }
}

/// Where a media block's bytes come from.
///
/// The wire has two optional fields, `data` and `uri`; a block with both or
/// neither means nothing, so the type has exactly one.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Source {
    /// The bytes themselves.
    Inline(Base64),
    /// A URI the API can fetch, such as an uploaded file's.
    Uri(String),
}

impl Source {
    fn write<M: SerializeMap>(&self, map: &mut M) -> Result<(), M::Error> {
        match self {
            Source::Inline(data) => map.serialize_entry("data", data.as_str()),
            Source::Uri(uri) => map.serialize_entry("uri", uri),
        }
    }
}

// ── Caller-authored blocks ───────────────────────────────────────────────────

/// An image, in a user input or a function result.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Image {
    /// The bytes or where to fetch them.
    pub source: Source,
    /// What the bytes are.
    pub mime: ImageMime,
    /// How finely to tokenize, when the caller says.
    pub resolution: Option<MediaResolution>,
}

impl Image {
    /// An image with the model's own choice of resolution.
    pub fn new(source: Source, mime: ImageMime) -> Self {
        Self { source, mime, resolution: None }
    }

    fn write<M: SerializeMap>(&self, map: &mut M) -> Result<(), M::Error> {
        map.serialize_entry("type", "image")?;
        self.source.write(map)?;
        map.serialize_entry("mime_type", &self.mime)?;
        if let Some(resolution) = self.resolution {
            map.serialize_entry("resolution", &resolution)?;
        }
        Ok(())
    }
}

/// Audio in a user input.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Audio {
    /// The bytes or where to fetch them.
    pub source: Source,
    /// What the bytes are.
    pub mime: AudioMime,
    /// Samples per second, needed for headerless formats such as `audio/l16`.
    pub sample_rate: Option<u32>,
    /// Channel count, needed for headerless formats such as `audio/l16`.
    pub channels: Option<u32>,
}

/// A document in a user input.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Document {
    /// The bytes or where to fetch them.
    pub source: Source,
    /// What the bytes are.
    pub mime: DocumentMime,
}

/// A video in a user input.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Video {
    /// The bytes or where to fetch them.
    pub source: Source,
    /// What the bytes are.
    pub mime: VideoMime,
    /// How finely to tokenize, when the caller says.
    pub resolution: Option<MediaResolution>,
    /// How to process it, when the caller says.
    pub processing: Option<VideoProcessing>,
    /// A name the model may use to refer to it in its answer.
    pub name: Option<String>,
}

/// One block of a user input.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum InputContent {
    /// Text.
    Text(String),
    /// An image.
    Image(Image),
    /// Audio.
    Audio(Audio),
    /// A PDF or CSV document.
    Document(Document),
    /// A video.
    Video(Video),
}

impl InputContent {
    /// A text block.
    pub fn text(text: impl Into<String>) -> Self {
        InputContent::Text(text.into())
    }
}

impl Serialize for InputContent {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        let mut map = s.serialize_map(None)?;
        match self {
            InputContent::Text(text) => {
                map.serialize_entry("type", "text")?;
                map.serialize_entry("text", text)?;
            }
            InputContent::Image(image) => image.write(&mut map)?,
            InputContent::Audio(audio) => {
                map.serialize_entry("type", "audio")?;
                audio.source.write(&mut map)?;
                map.serialize_entry("mime_type", &audio.mime)?;
                if let Some(rate) = audio.sample_rate {
                    map.serialize_entry("sample_rate", &rate)?;
                }
                if let Some(channels) = audio.channels {
                    map.serialize_entry("channels", &channels)?;
                }
            }
            InputContent::Document(document) => {
                map.serialize_entry("type", "document")?;
                document.source.write(&mut map)?;
                map.serialize_entry("mime_type", &document.mime)?;
            }
            InputContent::Video(video) => {
                map.serialize_entry("type", "video")?;
                video.source.write(&mut map)?;
                map.serialize_entry("mime_type", &video.mime)?;
                if let Some(resolution) = video.resolution {
                    map.serialize_entry("resolution", &resolution)?;
                }
                if let Some(processing) = video.processing {
                    map.serialize_entry("processing", &processing)?;
                }
                if let Some(name) = &video.name {
                    map.serialize_entry("name", name)?;
                }
            }
        }
        map.end()
    }
}

// ── Function results ─────────────────────────────────────────────────────────

/// One block of a function result: the result vocabulary is text and images.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ResultBlock {
    /// Text.
    Text(String),
    /// An image.
    Image(Image),
}

impl Serialize for ResultBlock {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        let mut map = s.serialize_map(None)?;
        match self {
            ResultBlock::Text(text) => {
                map.serialize_entry("type", "text")?;
                map.serialize_entry("text", text)?;
            }
            ResultBlock::Image(image) => image.write(&mut map)?,
        }
        map.end()
    }
}

/// What a function returned, in one of the three shapes the API accepts.
///
/// All three were accepted live on 2026-09-24, and they are different requests:
/// the model sees a bare string, a JSON object, or a list of blocks.
#[derive(Debug, Clone, PartialEq)]
pub enum FunctionOutput {
    /// A plain string.
    Text(String),
    /// A JSON object.
    Json(Map<String, Value>),
    /// Text and image blocks, in order.
    Blocks(Vec<ResultBlock>),
}

impl Serialize for FunctionOutput {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        match self {
            FunctionOutput::Text(text) => s.serialize_str(text),
            FunctionOutput::Json(object) => object.serialize(s),
            FunctionOutput::Blocks(blocks) => blocks.serialize(s),
        }
    }
}

// ── The model's blocks, read back ────────────────────────────────────────────

/// Which kind of media an [`OutputContent::Media`] block holds.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MediaKind {
    /// An image.
    Image,
    /// Audio.
    Audio,
    /// A document.
    Document,
    /// A video.
    Video,
}

/// A block the model produced, as a view for reading.
///
/// A view, not a value to send: the step it belongs to replays its received
/// JSON, so this type may be lossy and open-ended without making a replay
/// wrong.
#[derive(Debug, Clone, PartialEq)]
pub enum OutputContent {
    /// Text, with any citations attached to it.
    Text {
        /// The text.
        text: String,
        /// Sources for spans of the text, in received order.
        annotations: Vec<Annotation>,
    },
    /// An image, audio clip, document or video.
    Media {
        /// Which of the four.
        kind: MediaKind,
        /// The base64 bytes, when inline.
        data: Option<String>,
        /// Where to fetch the bytes, when not inline.
        uri: Option<String>,
        /// The declared MIME type, verbatim.
        mime_type: Option<String>,
    },
    /// A block kind newer than this crate.
    Unrecognized(Value),
}

impl OutputContent {
    /// The text of a text block.
    pub fn as_text(&self) -> Option<&str> {
        match self {
            OutputContent::Text { text, .. } => Some(text),
            _ => None,
        }
    }

    pub(crate) fn from_value(value: &Value) -> Result<Self, serde_json::Error> {
        #[derive(Deserialize)]
        struct TextWire {
            text: String,
            #[serde(default)]
            annotations: Vec<Value>,
        }
        #[derive(Deserialize)]
        struct MediaWire {
            data: Option<String>,
            uri: Option<String>,
            mime_type: Option<String>,
        }
        let kind = match value.get("type").and_then(Value::as_str) {
            Some("text") => {
                let wire = TextWire::deserialize(value)?;
                let annotations = wire.annotations.iter().map(Annotation::from_value).collect::<Result<_, _>>()?;
                return Ok(OutputContent::Text { text: wire.text, annotations });
            }
            Some("image") => MediaKind::Image,
            Some("audio") => MediaKind::Audio,
            Some("document") => MediaKind::Document,
            Some("video") => MediaKind::Video,
            _ => return Ok(OutputContent::Unrecognized(value.clone())),
        };
        let wire = MediaWire::deserialize(value)?;
        Ok(OutputContent::Media { kind, data: wire.data, uri: wire.uri, mime_type: wire.mime_type })
    }
}

/// A byte span of model text and what it is attributed to.
///
/// Offsets are in bytes of the UTF-8 text, as the reference states.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Deserialize)]
pub struct Span {
    /// First byte, inclusive.
    #[serde(default)]
    pub start_index: u32,
    /// Last byte, exclusive.
    #[serde(default)]
    pub end_index: u32,
}

/// Where a span of model text came from.
#[derive(Debug, Clone, PartialEq)]
pub enum Annotation {
    /// A web page.
    Url {
        /// The span.
        span: Span,
        /// The page.
        url: String,
        /// Its title.
        title: Option<String>,
    },
    /// A file from a file-search store.
    File {
        /// The span.
        span: Span,
        /// The file's URI.
        document_uri: Option<String>,
        /// The file's name.
        file_name: Option<String>,
        /// The cited page, when the file has pages.
        page_number: Option<u32>,
        /// The quoted source text.
        source: Option<String>,
    },
    /// A Google Maps place.
    Place {
        /// The span.
        span: Span,
        /// The place's name.
        name: Option<String>,
        /// `places/{place_id}`.
        place_id: Option<String>,
        /// The place's Maps URL.
        url: Option<String>,
    },
    /// Any other annotation, verbatim: speech metadata, word timings, and
    /// kinds newer than this crate.
    Other(Value),
}

impl Annotation {
    fn from_value(value: &Value) -> Result<Self, serde_json::Error> {
        #[derive(Deserialize)]
        struct UrlWire {
            #[serde(flatten)]
            span: Span,
            url: String,
            title: Option<String>,
        }
        #[derive(Deserialize)]
        struct FileWire {
            #[serde(flatten)]
            span: Span,
            document_uri: Option<String>,
            file_name: Option<String>,
            page_number: Option<u32>,
            source: Option<String>,
        }
        #[derive(Deserialize)]
        struct PlaceWire {
            #[serde(flatten)]
            span: Span,
            name: Option<String>,
            place_id: Option<String>,
            url: Option<String>,
        }
        Ok(match value.get("type").and_then(Value::as_str) {
            Some("url_citation") => {
                let w = UrlWire::deserialize(value)?;
                Annotation::Url { span: w.span, url: w.url, title: w.title }
            }
            Some("file_citation") => {
                let w = FileWire::deserialize(value)?;
                Annotation::File {
                    span: w.span,
                    document_uri: w.document_uri,
                    file_name: w.file_name,
                    page_number: w.page_number,
                    source: w.source,
                }
            }
            Some("place_citation") => {
                let w = PlaceWire::deserialize(value)?;
                Annotation::Place { span: w.span, name: w.name, place_id: w.place_id, url: w.url }
            }
            _ => Annotation::Other(value.clone()),
        })
    }
}
