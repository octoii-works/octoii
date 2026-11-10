//! The providers' drivers. Those that speak the Responses API are its driver with their dialects — how each takes
//! developer messages, whether it takes a cache's name and gives back its reasoning encrypted, and the kinds its codes
//! stand for; Anthropic's API and Claude through its agent are Anthropic's driver, at their addresses.

use crate::driver::Connection;
use crate::protocols::anthropic::AnthropicDriver;
use crate::protocols::chat_completions::{self, ChatDriver, Thinking};
use crate::protocols::responses::{DeveloperRole, Dialect, ResponsesDriver};
use crate::vocabulary::FailureKind;

/// The kind of failure one of a table's codes stands for.
fn kind_in(table: &[(&str, FailureKind)], code: Option<&str>) -> Option<FailureKind> {
    let code = code?;
    table.iter().find(|(each, _)| *each == code).map(|(_, kind)| *kind)
}

const OPENAI_CODES: [(&str, FailureKind); 14] = [
    ("credit_balance_exhausted", FailureKind::Quota),
    ("organization_spend_limit_exceeded", FailureKind::Quota),
    ("project_spend_limit_exceeded", FailureKind::Quota),
    ("organization_usage_limit_exceeded", FailureKind::Quota),
    ("rate_limit_exceeded", FailureKind::RateLimit),
    ("slow_down", FailureKind::RateLimit),
    ("invalid_api_key", FailureKind::Auth),
    ("bio_policy", FailureKind::ContentFilter),
    ("cyber_policy", FailureKind::ContentFilter),
    ("misalignment_policy_violation", FailureKind::ContentFilter),
    ("server_error", FailureKind::Server),
    ("server_is_overloaded", FailureKind::Server),
    ("vector_store_timeout", FailureKind::Server),
    ("invalid_prompt", FailureKind::InvalidRequest),
];

/// OpenAI's Responses API, which the vocabulary is.
pub const OPENAI: ResponsesDriver = ResponsesDriver {
    id: "openai",
    name: "OpenAI",
    base_url: "https://api.openai.com/v1",
    key_variable: "OPENAI_API_KEY",
    dialect: Dialect {
        developer_role: DeveloperRole::Developer,
        cache_key: true,
        encrypted_reasoning: true,
        takes_text_files: true,
        kind_of: |code, _| kind_in(&OPENAI_CODES, code),
    },
};

const OPENROUTER_CODES: [(&str, FailureKind); 31] = [
    ("context_length_exceeded", FailureKind::ContextOverflow),
    ("max_tokens_exceeded", FailureKind::InvalidRequest),
    ("token_limit_exceeded", FailureKind::Quota),
    ("string_too_long", FailureKind::PayloadTooLarge),
    ("authentication", FailureKind::Auth),
    ("invalid_api_key", FailureKind::Auth),
    ("permission_denied", FailureKind::Forbidden),
    ("payment_required", FailureKind::Quota),
    ("rate_limit_exceeded", FailureKind::RateLimit),
    ("provider_overloaded", FailureKind::Server),
    ("provider_unavailable", FailureKind::Server),
    ("invalid_request", FailureKind::InvalidRequest),
    ("invalid_prompt", FailureKind::InvalidRequest),
    ("not_found", FailureKind::InvalidRequest),
    ("precondition_failed", FailureKind::InvalidRequest),
    ("payload_too_large", FailureKind::PayloadTooLarge),
    ("unprocessable", FailureKind::InvalidRequest),
    ("content_policy_violation", FailureKind::ContentFilter),
    ("refusal", FailureKind::ContentFilter),
    ("bio_policy", FailureKind::ContentFilter),
    ("cyber_policy", FailureKind::ContentFilter),
    ("misalignment_policy_violation", FailureKind::ContentFilter),
    ("invalid_image", FailureKind::InvalidRequest),
    ("image_too_large", FailureKind::PayloadTooLarge),
    ("image_too_small", FailureKind::InvalidRequest),
    ("unsupported_image_format", FailureKind::InvalidRequest),
    ("image_not_found", FailureKind::InvalidRequest),
    ("image_download_failed", FailureKind::InvalidRequest),
    ("server", FailureKind::Server),
    ("server_error", FailureKind::Server),
    ("timeout", FailureKind::Server),
];

/// OpenRouter's Responses API, which routes a model to the providers that serve it.
pub const OPENROUTER: ResponsesDriver = ResponsesDriver {
    id: "openrouter",
    name: "OpenRouter",
    base_url: "https://openrouter.ai/api/v1",
    key_variable: "OPENROUTER_API_KEY",
    dialect: Dialect {
        developer_role: DeveloperRole::Developer,
        cache_key: true,
        encrypted_reasoning: false,
        takes_text_files: true,
        kind_of: |code, _| kind_in(&OPENROUTER_CODES, code),
    },
};

/// The Responses API as DeepSeek serves it: developer messages as the system's, and no cache's name, as it caches by
/// itself.
pub const DEEPSEEK: ResponsesDriver = ResponsesDriver {
    id: "deepseek",
    name: "DeepSeek",
    base_url: "https://api.deepseek.com",
    key_variable: "DEEPSEEK_API_KEY",
    dialect: Dialect {
        developer_role: DeveloperRole::System,
        cache_key: false,
        encrypted_reasoning: false,
        takes_text_files: true,
        kind_of: |_, _| None,
    },
};

/// xAI's Responses API, which takes no text files: they go as the person's text.
pub const XAI: ResponsesDriver = ResponsesDriver {
    id: "xai",
    name: "xAI",
    base_url: "https://api.x.ai/v1",
    key_variable: "XAI_API_KEY",
    dialect: Dialect {
        developer_role: DeveloperRole::Developer,
        cache_key: true,
        encrypted_reasoning: false,
        takes_text_files: false,
        kind_of: |_, _| None,
    },
};

const META_CODES: [(&str, FailureKind); 13] = [
    ("invalid_api_key", FailureKind::Auth),
    ("authentication_error", FailureKind::Auth),
    ("billing_not_configured", FailureKind::Quota),
    ("billing_error", FailureKind::Quota),
    ("rate_limit_exceeded", FailureKind::RateLimit),
    ("rate_limit_error", FailureKind::RateLimit),
    ("payload_too_large", FailureKind::PayloadTooLarge),
    ("content_policy_violation", FailureKind::ContentFilter),
    ("service_overloaded", FailureKind::Server),
    ("backend_unavailable", FailureKind::Server),
    ("gateway_timeout", FailureKind::Server),
    ("server_error", FailureKind::Server),
    ("model_not_found", FailureKind::InvalidRequest),
];

/// Meta's Responses API.
pub const META: ResponsesDriver = ResponsesDriver {
    id: "meta",
    name: "Meta",
    base_url: "https://api.meta.ai/v1",
    key_variable: "META_API_KEY",
    dialect: Dialect {
        developer_role: DeveloperRole::Developer,
        cache_key: false,
        encrypted_reasoning: false,
        takes_text_files: true,
        kind_of: |code, _| kind_in(&META_CODES, code),
    },
};

/// Anthropic's Messages API, reached with an API key.
pub const ANTHROPIC: AnthropicDriver = AnthropicDriver {
    id: "anthropic",
    name: "Anthropic",
    base_url: "https://api.anthropic.com/v1",
    connection: Connection::ApiKey {
        variable: "ANTHROPIC_API_KEY",
    },
};

/// Claude through the person's own Claude Code: the agent gateway beside the host, which speaks Anthropic's Messages
/// API and is signed in to on its own.
pub const CLAUDE: AnthropicDriver = AnthropicDriver {
    id: "claude",
    name: "Claude",
    base_url: "gateway://claude/v1",
    connection: Connection::Agent { through: "Claude Code" },
};

/// Ollama, a server of the person's on this machine.
pub const OLLAMA: ChatDriver = ChatDriver {
    id: "ollama",
    name: "Ollama",
    base_url: "http://127.0.0.1:11434/v1",
    connection: Connection::Server,
    dialect: chat_completions::Dialect {
        thinking: Thinking::ReasoningEffort,
        tool_choices: true,
    },
};

/// LM Studio, a server of the person's on this machine.
pub const LMSTUDIO: ChatDriver = ChatDriver {
    id: "lmstudio",
    name: "LM Studio",
    base_url: "http://127.0.0.1:1234/v1",
    connection: Connection::Server,
    dialect: chat_completions::Dialect {
        thinking: Thinking::ReasoningEffort,
        tool_choices: true,
    },
};

/// Z.ai's Chat Completions: reasoning switched by `thinking`, and a tool choice of `auto` alone.
pub const ZAI: ChatDriver = ChatDriver {
    id: "zai",
    name: "Z.ai",
    base_url: "https://api.z.ai/api/paas/v4",
    connection: Connection::ApiKey { variable: "ZAI_API_KEY" },
    dialect: chat_completions::Dialect {
        thinking: Thinking::Switched,
        tool_choices: false,
    },
};
