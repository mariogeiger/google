//! The enums that mirror closed API vocabularies, re-exported at the root.
//!
//! A closed set of wire strings is an enum, never a `&str`: a field of an enum
//! with no invalid variant accepts only what the API does. Vocabularies the
//! caller sends are *closed*: an unknown string is a decoding error, because
//! the crate would otherwise be unable to send it back. Vocabularies only the
//! server sends are *open*: Google may add a value without a new API version,
//! so an unknown string decodes as `Unrecognized` instead of failing the frame.

macro_rules! api_enum {
    (@base $(#[$enum_doc:meta])* $name:ident { $($(#[$doc:meta])* $variant:ident => $s:literal),* $(,)? }) => {
        $(#[$enum_doc])*
        #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
        pub enum $name { $($(#[$doc])* $variant),* }
        impl $name {
            /// Every value, in documentation order.
            pub const ALL: &'static [$name] = &[$($name::$variant),*];
            /// The string this value takes on the wire.
            pub fn as_str(self) -> &'static str {
                match self { $($name::$variant => $s),* }
            }
            /// The value a wire string names, if it names one.
            pub fn from_wire(s: &str) -> Option<Self> {
                match s { $($s => Some($name::$variant),)* _ => None }
            }
        }
        impl serde::Serialize for $name {
            fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
                s.serialize_str(self.as_str())
            }
        }
    };
    (closed $(#[$enum_doc:meta])* $name:ident { $($(#[$doc:meta])* $variant:ident => $s:literal),* $(,)? }) => {
        api_enum! { @base $(#[$enum_doc])* $name { $($(#[$doc])* $variant => $s),* } }
        impl<'de> serde::Deserialize<'de> for $name {
            fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
                let s = <std::borrow::Cow<'de, str> as serde::Deserialize>::deserialize(d)?;
                $name::from_wire(&s).ok_or_else(|| {
                    serde::de::Error::unknown_variant(&s, &[$($s),*])
                })
            }
        }
    };
    (open $(#[$enum_doc:meta])* $name:ident / $known:ident { $($(#[$doc:meta])* $variant:ident => $s:literal),* $(,)? }) => {
        api_enum! { @base
            /// The documented values of the open vocabulary of the same name.
            $known { $($(#[$doc])* $variant => $s),* }
        }
        $(#[$enum_doc])*
        #[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
        pub enum $name {
            /// A documented value.
            Known($known),
            /// A value newer than this crate, kept verbatim.
            Unrecognized(String),
        }
        impl $name {
            /// The string this value takes on the wire.
            pub fn as_str(&self) -> &str {
                match self { $name::Known(k) => k.as_str(), $name::Unrecognized(s) => s }
            }
        }
        impl From<$known> for $name {
            fn from(k: $known) -> Self { $name::Known(k) }
        }
        impl PartialEq<$known> for $name {
            fn eq(&self, other: &$known) -> bool { matches!(self, $name::Known(k) if k == other) }
        }
        impl serde::Serialize for $name {
            fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
                s.serialize_str(self.as_str())
            }
        }
        impl<'de> serde::Deserialize<'de> for $name {
            fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
                let s = <String as serde::Deserialize>::deserialize(d)?;
                Ok($known::from_wire(&s).map($name::Known).unwrap_or($name::Unrecognized(s)))
            }
        }
    };
}

pub(crate) use api_enum;

api_enum! { closed
    /// How much thinking a Gemini 3 model spends before it answers.
    ///
    /// Each model accepts a subset, so a model type carries its own level enum
    /// and converts into this one; this is the union the wire knows.
    ThinkingLevel {
        /// Little to no thinking.
        Minimal => "minimal",
        /// Low thinking.
        Low => "low",
        /// Medium thinking.
        Medium => "medium",
        /// High thinking.
        High => "high",
    }
}

api_enum! { closed
    /// Whether thought steps carry a readable summary.
    ///
    /// Summaries change what the response contains, never what the model
    /// reasons: the signature that preserves the reasoning is returned either
    /// way.
    ThinkingSummaries {
        /// Return summaries when the model produced enough reasoning to summarize.
        Auto => "auto",
        /// Return no summaries. The documented default.
        None => "none",
    }
}

api_enum! { closed
    /// Which capacity pool serves the request.
    ServiceTier {
        /// Cheaper, slower, and preemptible.
        Flex => "flex",
        /// The ordinary pool.
        Standard => "standard",
        /// Higher priority at a higher price.
        Priority => "priority",
    }
}

api_enum! { closed
    /// How the model may use the declared tools.
    ToolChoiceMode {
        /// The model decides whether to call a tool.
        Auto => "auto",
        /// The model must call at least one tool.
        Any => "any",
        /// The model must not call a tool.
        None => "none",
        /// The model decides, and a call it makes is checked against the
        /// declared schema before it is returned.
        Validated => "validated",
    }
}

api_enum! { closed
    /// How finely the model tokenizes an image or video.
    MediaResolution {
        /// The fewest tokens.
        Low => "low",
        /// Between low and high.
        Medium => "medium",
        /// Many tokens, for fine detail.
        High => "high",
        /// The most tokens.
        UltraHigh => "ultra_high",
    }
}

api_enum! { closed
    /// The MIME types an image block may declare.
    ImageMime {
        /// PNG.
        Png => "image/png",
        /// JPEG.
        Jpeg => "image/jpeg",
        /// WebP.
        Webp => "image/webp",
        /// HEIC.
        Heic => "image/heic",
        /// HEIF.
        Heif => "image/heif",
        /// GIF.
        Gif => "image/gif",
        /// BMP.
        Bmp => "image/bmp",
        /// TIFF.
        Tiff => "image/tiff",
    }
}

api_enum! { closed
    /// The MIME types an audio block may declare.
    AudioMime {
        /// WAV.
        Wav => "audio/wav",
        /// MP3.
        Mp3 => "audio/mp3",
        /// AIFF.
        Aiff => "audio/aiff",
        /// AAC.
        Aac => "audio/aac",
        /// Ogg.
        Ogg => "audio/ogg",
        /// FLAC.
        Flac => "audio/flac",
        /// MPEG audio.
        Mpeg => "audio/mpeg",
        /// M4A.
        M4a => "audio/m4a",
        /// Raw 16-bit linear PCM; `sample_rate` and `channels` describe it.
        L16 => "audio/l16",
        /// Opus.
        Opus => "audio/opus",
        /// A-law.
        Alaw => "audio/alaw",
        /// μ-law.
        Mulaw => "audio/mulaw",
        /// WebM audio.
        Webm => "audio/webm",
    }
}

api_enum! { closed
    /// The MIME types a document block may declare.
    DocumentMime {
        /// PDF.
        Pdf => "application/pdf",
        /// Comma-separated values.
        Csv => "text/csv",
    }
}

api_enum! { closed
    /// The MIME types a video block may declare.
    VideoMime {
        /// MP4.
        Mp4 => "video/mp4",
        /// MPEG.
        Mpeg => "video/mpeg",
        /// MPG.
        Mpg => "video/mpg",
        /// QuickTime.
        Mov => "video/mov",
        /// AVI.
        Avi => "video/avi",
        /// Flash video.
        Flv => "video/x-flv",
        /// WebM.
        Webm => "video/webm",
        /// Windows Media.
        Wmv => "video/wmv",
        /// 3GPP.
        ThreeGpp => "video/3gpp",
    }
}

api_enum! { closed
    /// How the model processes a video it is asked to understand.
    VideoProcessing {
        /// Sampled frames, analyzed once.
        Static => "static",
        /// The model decides where to look, and may look again.
        Agentic => "agentic",
    }
}

api_enum! { closed
    /// The language of code the code-execution tool runs.
    CodeLanguage {
        /// Python, the only language documented.
        Python => "python",
    }
}

api_enum! { closed
    /// Which kinds of Google Search grounding are enabled.
    SearchType {
        /// Web results.
        WebSearch => "web_search",
        /// Image results.
        ImageSearch => "image_search",
    }
}

api_enum! { closed
    /// The environment a computer-use model operates.
    ComputerEnvironment {
        /// A web browser.
        Browser => "browser",
        /// A mobile device.
        Mobile => "mobile",
        /// A desktop.
        Desktop => "desktop",
    }
}

api_enum! { closed
    /// A computer-use safety policy the caller may switch off.
    ComputerSafetyPolicy {
        /// Confirmation before financial transactions.
        FinancialTransactions => "financial_transactions",
        /// Confirmation before modifying sensitive data.
        SensitiveDataModification => "sensitive_data_modification",
        /// Confirmation before using communication tools.
        CommunicationTool => "communication_tool",
        /// Confirmation before creating accounts.
        AccountCreation => "account_creation",
        /// Confirmation before modifying data.
        DataModification => "data_modification",
        /// Confirmation before changing consent settings.
        UserConsentManagement => "user_consent_management",
        /// Confirmation before accepting legal terms.
        LegalTermsAndAgreements => "legal_terms_and_agreements",
    }
}

api_enum! { open
    /// Where an interaction stands.
    ///
    /// Only some are terminal; which ones end a stream is decided in
    /// [`crate::settle`], not here.
    Status / KnownStatus {
        /// Still running.
        InProgress => "in_progress",
        /// The model called functions and waits for their results.
        RequiresAction => "requires_action",
        /// Finished.
        Completed => "completed",
        /// Failed.
        Failed => "failed",
        /// Cancelled.
        Cancelled => "cancelled",
        /// Stopped early, for example at `max_output_tokens`.
        Incomplete => "incomplete",
        /// Stopped because an agent's token budget ran out.
        BudgetExceeded => "budget_exceeded",
        /// Waiting to start, for background interactions.
        Queued => "queued",
    }
}

api_enum! { open
    /// The modality a token count is for.
    Modality / KnownModality {
        /// Text.
        Text => "text",
        /// Images.
        Image => "image",
        /// Audio.
        Audio => "audio",
        /// Video.
        Video => "video",
        /// Documents.
        Document => "document",
    }
}

api_enum! { open
    /// Which grounding tool a usage count is for.
    GroundingTool / KnownGroundingTool {
        /// Google Search.
        GoogleSearch => "google_search",
        /// Google Maps.
        GoogleMaps => "google_maps",
    }
}

api_enum! { open
    /// What an error body says went wrong.
    ///
    /// The reference calls this "a URI that identifies the error type"; the
    /// endpoint sends short snake-case codes, and those are the documented
    /// values here. Measured 2026-09-24.
    ErrorCode / KnownErrorCode {
        /// The request is malformed or refused. A 400.
        InvalidRequest => "invalid_request",
        /// A per-minute or per-day rate limit. A 429.
        TooManyRequests => "too_many_requests",
        /// A rate limit, as a streamed request reports it. A 429.
        RateLimitExceeded => "rate_limit_exceeded",
        /// The plan's quota does not cover the request. A 429.
        QuotaExceeded => "quota_exceeded",
        /// The model is overloaded; retry later. A 503.
        ServiceUnavailable => "service_unavailable",
    }
}
