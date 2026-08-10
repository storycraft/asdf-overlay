use asdf_overlay_client::common::event::window::input;
use napi_derive::napi;

/// Describe a IME event.
#[napi]
pub enum Ime {
    Enabled {
        /// Initial IME language in ETF language tag(BCP 47) format.
        lang: String,

        /// Initial IME conversion mode
        conversion: u16,
    },

    /// IME language is changed.
    Changed {
        /// Changed IME language in ETF language tag(BCP 47) format.
        lang: String,
    },

    /// IME conversion mode is changed.
    ConversionChanged {
        /// Changed IME conversion mode.
        conversion: u16,
    },

    /// IME candidate is added/updated.
    CandidateChanged { list: ImeCandidateList },

    /// IME candidate window is closed.
    CandidateClosed,

    /// IME is composing text.
    Compose {
        /// Composing text.
        text: String,

        /// Current caret index in composing text.
        caret: u32,
    },

    /// IME has committed text.
    Commit {
        /// Committed text.
        text: String,
    },

    /// IME is disabled due to losing focus or etc.
    Disabled,
}

impl From<input::Ime> for Ime {
    fn from(ime: input::Ime) -> Self {
        match ime {
            input::Ime::Enabled { lang, conversion } => Ime::Enabled {
                lang,
                conversion: conversion.bits(),
            },
            input::Ime::Changed(lang) => Ime::Changed { lang },
            input::Ime::ConversionChanged(conversion) => Ime::ConversionChanged {
                conversion: conversion.bits(),
            },
            input::Ime::CandidateChanged(list) => Ime::CandidateChanged { list: list.into() },
            input::Ime::CandidateClosed => Ime::CandidateClosed,
            input::Ime::Compose { text, caret } => Ime::Compose {
                text,
                caret: caret as _,
            },
            input::Ime::Commit(text) => Ime::Commit { text },
            input::Ime::Disabled => Ime::Disabled,
        }
    }
}

/// IME candidate list.
#[napi(object)]
pub struct ImeCandidateList {
    /// Start index of current page.
    pub page_start_index: u32,

    /// Count of candidate item per page.
    pub page_size: u32,

    /// Currently selected candidate index.
    pub selected_index: u32,

    /// Candidate list.
    pub candidates: Vec<String>,
}

impl From<input::ImeCandidateList> for ImeCandidateList {
    fn from(list: input::ImeCandidateList) -> Self {
        ImeCandidateList {
            page_start_index: list.page_start_index,
            page_size: list.page_size,
            selected_index: list.selected_index,
            candidates: list.candidates,
        }
    }
}
