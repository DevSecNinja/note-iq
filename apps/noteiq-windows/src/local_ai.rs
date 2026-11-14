//! The Windows on-device language model, used to tidy up short notes. It needs package
//! identity (see docs/setup.md) and a supported NPU or GPU; without them, Tidy up stays
//! unavailable and the rest of NoteIQ is unaffected. Calls block, so run them off the UI thread.

use crate::local_ai_bindings::Microsoft::Windows::AI::{
    AIFeatureReadyState,
    Text::{LanguageModel, LanguageModelResponseStatus, TextRewriteTone, TextRewriter},
};
use std::{
    sync::atomic::{AtomicBool, Ordering},
    time::{Duration, Instant},
};
use windows_core::{HRESULT, HSTRING};
use windows_future::AsyncStatus;

/// A tidy-up that runs longer than this is cancelled; short notes took about 5 seconds.
pub const REWRITE_TIMEOUT: Duration = Duration::from_secs(60);
const POLL_INTERVAL: Duration = Duration::from_millis(50);
const CLASS_NOT_REGISTERED: HRESULT = HRESULT(0x8004_0154_u32 as i32);

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Availability {
    Checking,
    Ready,
    Unavailable(String),
}

impl Availability {
    pub fn description(&self) -> &str {
        match self {
            Self::Checking => "Checking whether this PC can run the Windows on-device AI model…",
            Self::Ready => "Available on this PC.",
            Self::Unavailable(reason) => reason,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum RewriteFailure {
    Cancelled,
    Failed(String),
}

/// Whether the model can run now. It never starts a model download.
pub fn availability() -> Availability {
    if windows::ApplicationModel::Package::Current().is_err() {
        return Availability::Unavailable(
            "Not available: NoteIQ isn't registered with Windows yet. See \"On-device AI\" in docs/setup.md."
                .to_owned(),
        );
    }
    let state = match LanguageModel::GetReadyState() {
        Ok(state) => state,
        Err(error) if error.code() == CLASS_NOT_REGISTERED => {
            return Availability::Unavailable(
                "Not available: install the Windows App Runtime 2 to use on-device AI.".to_owned(),
            );
        }
        Err(error) => {
            tracing::warn!(code = ?error.code(), "On-device AI readiness check failed");
            return Availability::Unavailable(
                "Not available: Windows couldn't check the on-device AI model.".to_owned(),
            );
        }
    };
    let reason = match state {
        AIFeatureReadyState::Ready => return Availability::Ready,
        AIFeatureReadyState::NotReady => {
            "Not available yet: Windows hasn't installed its on-device language model. Check Windows Update."
        }
        AIFeatureReadyState::DisabledByUser => {
            "Not available: on-device AI is turned off in Windows settings."
        }
        AIFeatureReadyState::CapabilityMissing => {
            "Not available: NoteIQ's Windows registration lacks the AI capability. Register NoteIQ again."
        }
        AIFeatureReadyState::OSUpdateNeeded => {
            "Not available: install the latest Windows updates to use on-device AI."
        }
        _ => "Not available: this PC can't run the Windows on-device language model.",
    };
    Availability::Unavailable(reason.to_owned())
}

/// Rewrites `text` concisely on this PC. Setting `cancel` stops the model promptly.
pub fn rewrite(text: &str, cancel: &AtomicBool) -> Result<String, RewriteFailure> {
    let failed = |error: windows_core::Error| {
        tracing::warn!(code = ?error.code(), "On-device tidy up failed");
        RewriteFailure::Failed("The on-device AI model couldn't tidy this note.".to_owned())
    };
    let model = LanguageModel::CreateAsync()
        .and_then(|operation| operation.join())
        .map_err(failed)?;
    let result = (|| {
        let rewriter = TextRewriter::CreateInstance(&model)?;
        let operation = rewriter.RewriteAsync2(&HSTRING::from(text), TextRewriteTone::Concise)?;
        let started = Instant::now();
        while operation.Status()? == AsyncStatus::Started {
            if cancel.load(Ordering::Relaxed) {
                operation.Cancel()?;
                return Ok(Err(RewriteFailure::Cancelled));
            }
            if started.elapsed() >= REWRITE_TIMEOUT {
                operation.Cancel()?;
                return Ok(Err(RewriteFailure::Failed(
                    "The on-device AI model took too long. Try again later.".to_owned(),
                )));
            }
            std::thread::sleep(POLL_INTERVAL);
        }
        let response = operation.GetResults()?;
        let status = response.Status()?;
        Ok(if status == LanguageModelResponseStatus::Complete {
            Ok(response.Text()?.to_string_lossy())
        } else {
            Err(RewriteFailure::Failed(response_message(status).to_owned()))
        })
    })()
    .map_err(failed)
    .and_then(|outcome| outcome);
    let _ = model.Close();
    result
}

fn response_message(status: LanguageModelResponseStatus) -> &'static str {
    match status {
        LanguageModelResponseStatus::PromptBlockedByContentModeration
        | LanguageModelResponseStatus::ResponseBlockedByContentModeration => {
            "Windows content moderation blocked this tidy-up."
        }
        LanguageModelResponseStatus::PromptLargerThanContext => {
            "This note is too long for the on-device AI model."
        }
        LanguageModelResponseStatus::UnsupportedLanguage
        | LanguageModelResponseStatus::LanguageMismatch => {
            "The on-device AI model doesn't support this note's language."
        }
        LanguageModelResponseStatus::BlockedByPolicy => {
            "Your organization's policy blocks the on-device AI model."
        }
        _ => "The on-device AI model couldn't tidy this note.",
    }
}
