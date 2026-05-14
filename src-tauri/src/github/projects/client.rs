//! Async GraphQL client for GitHub Projects v2.
//!
//! All public functions take an explicit `endpoint` so tests can point at a
//! wiremock server; production callers should use `GITHUB_GRAPHQL_ENDPOINT`.
//! The PAT is read from the keychain (`super::auth::load_pat`) by the
//! higher-level Tauri commands — this module never touches the keychain
//! directly so the queries stay independently testable.

use std::time::Duration;

use reqwest::Client;
use serde::{Deserialize, Serialize};
use serde_json::Value as JsonValue;

use super::queries::{
    AddDraftIssueResponse, AddDraftIssueVars, DeleteProjectItemResponse, DeleteProjectItemVars,
    GetProjectFieldsResponse, GetProjectFieldsVars, LinkIssueToProjectResponse,
    LinkIssueToProjectVars, ListProjectItemsResponse, ListProjectItemsVars,
    ListProjectsForOrgResponse, ListProjectsForOrgVars, ListProjectsForUserResponse,
    ListProjectsForUserVars, ProjectField, ProjectItem, ProjectSummary,
    UpdateProjectItemFieldResponse, UpdateProjectItemFieldVars, ADD_DRAFT_ISSUE,
    DELETE_PROJECT_ITEM, GET_PROJECT_FIELDS, LINK_ISSUE_TO_PROJECT, LIST_PROJECTS_FOR_ORG,
    LIST_PROJECTS_FOR_USER, LIST_PROJECT_ITEMS, UPDATE_PROJECT_ITEM_FIELD,
};
use super::rate_limit::{self, RateLimitAction, RateLimitSnapshot};

pub const GITHUB_GRAPHQL_ENDPOINT: &str = "https://api.github.com/graphql";
pub const USER_AGENT: &str = concat!("tolaria/", env!("CARGO_PKG_VERSION"));

const DEFAULT_TIMEOUT_SECS: u64 = 30;
const PROJECT_LIST_PAGE_SIZE: u32 = 50;
const PROJECT_ITEM_PAGE_SIZE: u32 = 100;

/// Errors returned by every GraphQL call. The variants are deliberately
/// narrow so the caller can switch on them — unknown server states fall
/// through to `Transport` or `GraphQl`.
#[derive(Debug, thiserror::Error)]
pub enum ClientError {
    #[error("HTTP transport error: {0}")]
    Transport(String),
    #[error("HTTP {status}: {body}")]
    Http { status: u16, body: String },
    #[error("GraphQL errors: {0}")]
    GraphQl(String),
    #[error("Response missing `data` field")]
    EmptyData,
    #[error("Failed to decode response: {0}")]
    Decode(String),
    #[error("Rate limited; retry after {0:?}")]
    RateLimited(Duration),
}

#[derive(Debug, Clone)]
pub struct ClientConfig {
    pub endpoint: String,
    pub pat: String,
    pub timeout: Duration,
}

impl ClientConfig {
    pub fn new(pat: impl Into<String>) -> Self {
        Self {
            endpoint: GITHUB_GRAPHQL_ENDPOINT.to_string(),
            pat: pat.into(),
            timeout: Duration::from_secs(DEFAULT_TIMEOUT_SECS),
        }
    }

    pub fn with_endpoint(mut self, endpoint: impl Into<String>) -> Self {
        self.endpoint = endpoint.into();
        self
    }
}

#[derive(Debug, Serialize)]
struct GraphQlRequest<'a, V: Serialize> {
    query: &'a str,
    variables: V,
}

#[derive(Debug, Deserialize)]
struct GraphQlEnvelope<T> {
    data: Option<T>,
    #[serde(default)]
    errors: Vec<GraphQlErrorEntry>,
}

#[derive(Debug, Deserialize)]
struct GraphQlErrorEntry {
    message: String,
}

async fn execute<V, T>(
    http: &Client,
    config: &ClientConfig,
    query: &str,
    variables: V,
) -> Result<T, ClientError>
where
    V: Serialize,
    T: for<'de> Deserialize<'de>,
{
    let request = GraphQlRequest { query, variables };
    let response = http
        .post(&config.endpoint)
        .bearer_auth(&config.pat)
        .header("Accept", "application/vnd.github+json")
        .header("User-Agent", USER_AGENT)
        .timeout(config.timeout)
        .json(&request)
        .send()
        .await
        .map_err(|e| ClientError::Transport(e.to_string()))?;

    let status = response.status();
    let headers = response.headers().clone();
    let body_text = response
        .text()
        .await
        .map_err(|e| ClientError::Transport(e.to_string()))?;

    if !status.is_success() {
        if let Some(delay) = retry_after_from_headers(&headers, &body_text) {
            return Err(ClientError::RateLimited(delay));
        }
        return Err(ClientError::Http {
            status: status.as_u16(),
            body: body_text,
        });
    }

    let envelope: GraphQlEnvelope<T> =
        serde_json::from_str(&body_text).map_err(|e| ClientError::Decode(e.to_string()))?;

    if !envelope.errors.is_empty() {
        let joined = envelope
            .errors
            .into_iter()
            .map(|e| e.message)
            .collect::<Vec<_>>()
            .join("; ");
        return Err(ClientError::GraphQl(joined));
    }

    envelope.data.ok_or(ClientError::EmptyData)
}

fn retry_after_from_headers(headers: &reqwest::header::HeaderMap, body: &str) -> Option<Duration> {
    let limit = headers
        .get("x-ratelimit-limit")
        .and_then(|v| v.to_str().ok());
    let remaining = headers
        .get("x-ratelimit-remaining")
        .and_then(|v| v.to_str().ok());
    let reset = headers
        .get("x-ratelimit-reset")
        .and_then(|v| v.to_str().ok());
    let snapshot = rate_limit::parse_snapshot(limit, remaining, reset)?;
    if !body.to_lowercase().contains("rate limit") && snapshot.remaining > 0 {
        return None;
    }
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    match rate_limit::decide_action(snapshot, now) {
        RateLimitAction::Exhausted { delay } | RateLimitAction::SlowDown { delay } => Some(delay),
        RateLimitAction::Proceed => None,
    }
}

/// Build an HTTP client preconfigured with our user-agent. Each top-level
/// command should create one and reuse it across calls.
pub fn build_http_client() -> Result<Client, ClientError> {
    Client::builder()
        .user_agent(USER_AGENT)
        .build()
        .map_err(|e| ClientError::Transport(e.to_string()))
}

pub async fn list_projects_for_user(
    http: &Client,
    config: &ClientConfig,
    login: &str,
) -> Result<Vec<ProjectSummary>, ClientError> {
    let vars = ListProjectsForUserVars {
        login,
        first: PROJECT_LIST_PAGE_SIZE,
    };
    let data: ListProjectsForUserResponse =
        execute(http, config, LIST_PROJECTS_FOR_USER, vars).await?;
    Ok(data.user.projects.nodes)
}

pub async fn list_projects_for_org(
    http: &Client,
    config: &ClientConfig,
    login: &str,
) -> Result<Vec<ProjectSummary>, ClientError> {
    let vars = ListProjectsForOrgVars {
        login,
        first: PROJECT_LIST_PAGE_SIZE,
    };
    let data: ListProjectsForOrgResponse =
        execute(http, config, LIST_PROJECTS_FOR_ORG, vars).await?;
    Ok(data.organization.projects.nodes)
}

pub async fn get_project_fields(
    http: &Client,
    config: &ClientConfig,
    project_id: &str,
) -> Result<Vec<ProjectField>, ClientError> {
    let vars = GetProjectFieldsVars { project_id };
    let data: GetProjectFieldsResponse = execute(http, config, GET_PROJECT_FIELDS, vars).await?;
    Ok(data.node.map(|n| n.fields.nodes).unwrap_or_default())
}

/// Single page of project items. Pass `cursor = None` for the first page.
pub struct ItemsPage {
    pub items: Vec<ProjectItem>,
    pub next_cursor: Option<String>,
}

pub async fn list_project_items_page(
    http: &Client,
    config: &ClientConfig,
    project_id: &str,
    cursor: Option<&str>,
) -> Result<ItemsPage, ClientError> {
    let vars = ListProjectItemsVars {
        project_id,
        first: PROJECT_ITEM_PAGE_SIZE,
        after: cursor,
    };
    let data: ListProjectItemsResponse = execute(http, config, LIST_PROJECT_ITEMS, vars).await?;
    match data.node {
        Some(node) => Ok(ItemsPage {
            next_cursor: node
                .items
                .page_info
                .has_next_page
                .then_some(())
                .and_then(|_| node.items.page_info.end_cursor.clone()),
            items: node.items.nodes,
        }),
        None => Ok(ItemsPage {
            items: Vec::new(),
            next_cursor: None,
        }),
    }
}

/// Drain every page into a single Vec. Convenience for callers that don't
/// need to stream — the sync engine in P11 will iterate manually.
pub async fn list_all_project_items(
    http: &Client,
    config: &ClientConfig,
    project_id: &str,
) -> Result<Vec<ProjectItem>, ClientError> {
    let mut out = Vec::new();
    let mut cursor: Option<String> = None;
    loop {
        let page = list_project_items_page(http, config, project_id, cursor.as_deref()).await?;
        out.extend(page.items);
        match page.next_cursor {
            Some(next) => cursor = Some(next),
            None => break,
        }
    }
    Ok(out)
}

pub async fn add_draft_issue(
    http: &Client,
    config: &ClientConfig,
    project_id: &str,
    title: &str,
    body: Option<&str>,
) -> Result<String, ClientError> {
    let vars = AddDraftIssueVars {
        project_id,
        title,
        body,
    };
    let data: AddDraftIssueResponse = execute(http, config, ADD_DRAFT_ISSUE, vars).await?;
    Ok(data.add_project_v2_draft_issue.project_item.id)
}

pub async fn update_project_item_field(
    http: &Client,
    config: &ClientConfig,
    project_id: &str,
    item_id: &str,
    field_id: &str,
    value: super::queries::FieldValueInput,
) -> Result<String, ClientError> {
    let vars = UpdateProjectItemFieldVars {
        project_id,
        item_id,
        field_id,
        value,
    };
    let data: UpdateProjectItemFieldResponse =
        execute(http, config, UPDATE_PROJECT_ITEM_FIELD, vars).await?;
    Ok(data.update_project_v2_item_field_value.project_v2_item.id)
}

pub async fn delete_project_item(
    http: &Client,
    config: &ClientConfig,
    project_id: &str,
    item_id: &str,
) -> Result<Option<String>, ClientError> {
    let vars = DeleteProjectItemVars {
        project_id,
        item_id,
    };
    let data: DeleteProjectItemResponse = execute(http, config, DELETE_PROJECT_ITEM, vars).await?;
    Ok(data.delete_project_v2_item.deleted_item_id)
}

pub async fn link_issue_to_project(
    http: &Client,
    config: &ClientConfig,
    project_id: &str,
    content_id: &str,
) -> Result<String, ClientError> {
    let vars = LinkIssueToProjectVars {
        project_id,
        content_id,
    };
    let data: LinkIssueToProjectResponse =
        execute(http, config, LINK_ISSUE_TO_PROJECT, vars).await?;
    Ok(data.add_project_v2_item_by_id.item.id)
}

/// Helper for callers (and tests) that want to record-and-react to GitHub's
/// rate-limit headers without making a request — given the latest snapshot,
/// decide how long to wait before the next call.
pub fn rate_limit_action(snapshot: RateLimitSnapshot) -> RateLimitAction {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    rate_limit::decide_action(snapshot, now)
}

/// JSON helper: deserialize a server payload that may be `{"data": ..., "errors": [...]}`
/// or `{"data": null, "errors": [...]}`. Exposed for tests that want to assert
/// what the response parser does without going through HTTP.
pub fn parse_envelope_for_tests<T>(body: &str) -> Result<T, ClientError>
where
    T: for<'de> Deserialize<'de>,
{
    let envelope: GraphQlEnvelope<T> =
        serde_json::from_str(body).map_err(|e| ClientError::Decode(e.to_string()))?;
    if !envelope.errors.is_empty() {
        let joined = envelope
            .errors
            .into_iter()
            .map(|e| e.message)
            .collect::<Vec<_>>()
            .join("; ");
        return Err(ClientError::GraphQl(joined));
    }
    envelope.data.ok_or(ClientError::EmptyData)
}

/// Shape of a raw GraphQL request body — used by tests that inspect what we
/// would have sent to the server.
pub fn build_request_body<V: Serialize>(query: &str, variables: V) -> JsonValue {
    serde_json::json!({
        "query": query,
        "variables": variables,
    })
}
