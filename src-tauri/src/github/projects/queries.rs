//! GitHub Projects v2 GraphQL query strings + typed response/variables.
//!
//! Hand-rolled rather than codegen'd so the build doesn't need to fetch the
//! 30 MB GitHub schema. Responses use serde structs scoped to the fields we
//! actually consume — keeps the API surface small and stable.

use serde::{Deserialize, Serialize};

// ── ListProjectsForUser ─────────────────────────────────────────────────────

pub const LIST_PROJECTS_FOR_USER: &str = "
query ListProjectsForUser($login: String!, $first: Int!) {
  user(login: $login) {
    projectsV2(first: $first, orderBy: {field: TITLE, direction: ASC}) {
      nodes {
        id
        number
        title
        url
        closed
      }
    }
  }
}
";

#[derive(Debug, Serialize)]
pub struct ListProjectsForUserVars<'a> {
    pub login: &'a str,
    pub first: u32,
}

#[derive(Debug, Deserialize)]
pub struct ListProjectsForUserResponse {
    pub user: ProjectListContainer,
}

// ── ListProjectsForOrg ──────────────────────────────────────────────────────

pub const LIST_PROJECTS_FOR_ORG: &str = "
query ListProjectsForOrg($login: String!, $first: Int!) {
  organization(login: $login) {
    projectsV2(first: $first, orderBy: {field: TITLE, direction: ASC}) {
      nodes {
        id
        number
        title
        url
        closed
      }
    }
  }
}
";

#[derive(Debug, Serialize)]
pub struct ListProjectsForOrgVars<'a> {
    pub login: &'a str,
    pub first: u32,
}

#[derive(Debug, Deserialize)]
pub struct ListProjectsForOrgResponse {
    pub organization: ProjectListContainer,
}

#[derive(Debug, Deserialize)]
pub struct ProjectListContainer {
    #[serde(rename = "projectsV2")]
    pub projects: ProjectsV2Connection,
}

#[derive(Debug, Deserialize)]
pub struct ProjectsV2Connection {
    pub nodes: Vec<ProjectSummary>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ProjectSummary {
    pub id: String,
    pub number: i32,
    pub title: String,
    pub url: String,
    pub closed: bool,
}

// ── GetProjectFields ────────────────────────────────────────────────────────

pub const GET_PROJECT_FIELDS: &str = "
query GetProjectFields($projectId: ID!) {
  node(id: $projectId) {
    ... on ProjectV2 {
      fields(first: 50) {
        nodes {
          __typename
          ... on ProjectV2Field {
            id
            name
            dataType
          }
          ... on ProjectV2IterationField {
            id
            name
            dataType
          }
          ... on ProjectV2SingleSelectField {
            id
            name
            dataType
            options { id name }
          }
        }
      }
    }
  }
}
";

#[derive(Debug, Serialize)]
pub struct GetProjectFieldsVars<'a> {
    #[serde(rename = "projectId")]
    pub project_id: &'a str,
}

#[derive(Debug, Deserialize)]
pub struct GetProjectFieldsResponse {
    pub node: Option<ProjectFieldsNode>,
}

#[derive(Debug, Deserialize)]
pub struct ProjectFieldsNode {
    pub fields: ProjectFieldsConnection,
}

#[derive(Debug, Deserialize)]
pub struct ProjectFieldsConnection {
    pub nodes: Vec<ProjectField>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ProjectField {
    pub id: String,
    pub name: String,
    #[serde(rename = "dataType")]
    pub data_type: String,
    #[serde(default)]
    pub options: Vec<ProjectFieldOption>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ProjectFieldOption {
    pub id: String,
    pub name: String,
}

// ── ListProjectItems (paginated) ────────────────────────────────────────────

pub const LIST_PROJECT_ITEMS: &str = "
query ListProjectItems($projectId: ID!, $first: Int!, $after: String) {
  node(id: $projectId) {
    ... on ProjectV2 {
      items(first: $first, after: $after) {
        pageInfo { hasNextPage endCursor }
        nodes {
          id
          updatedAt
          content {
            __typename
            ... on DraftIssue { title body }
            ... on Issue { number title url repository { nameWithOwner } }
            ... on PullRequest { number title url repository { nameWithOwner } }
          }
          fieldValues(first: 50) {
            nodes {
              __typename
              ... on ProjectV2ItemFieldTextValue {
                text
                field { ... on ProjectV2Field { id name } }
              }
              ... on ProjectV2ItemFieldNumberValue {
                number
                field { ... on ProjectV2Field { id name } }
              }
              ... on ProjectV2ItemFieldDateValue {
                date
                field { ... on ProjectV2Field { id name } }
              }
              ... on ProjectV2ItemFieldSingleSelectValue {
                optionId
                name
                field { ... on ProjectV2SingleSelectField { id name } }
              }
            }
          }
        }
      }
    }
  }
}
";

#[derive(Debug, Serialize)]
pub struct ListProjectItemsVars<'a> {
    #[serde(rename = "projectId")]
    pub project_id: &'a str,
    pub first: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub after: Option<&'a str>,
}

#[derive(Debug, Deserialize)]
pub struct ListProjectItemsResponse {
    pub node: Option<ProjectItemsNode>,
}

#[derive(Debug, Deserialize)]
pub struct ProjectItemsNode {
    pub items: ProjectItemsConnection,
}

#[derive(Debug, Deserialize)]
pub struct ProjectItemsConnection {
    #[serde(rename = "pageInfo")]
    pub page_info: PageInfo,
    pub nodes: Vec<ProjectItem>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct PageInfo {
    #[serde(rename = "hasNextPage")]
    pub has_next_page: bool,
    #[serde(rename = "endCursor")]
    pub end_cursor: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ProjectItem {
    pub id: String,
    /// Item-level "when did this item last change on github.com" timestamp.
    /// Used by the reconciler as the remote side of an LWW comparison
    /// when both local and remote have diverged from the snapshot.
    #[serde(rename = "updatedAt", default)]
    pub updated_at: Option<String>,
    pub content: Option<ProjectItemContent>,
    #[serde(rename = "fieldValues")]
    pub field_values: FieldValuesConnection,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "__typename")]
pub enum ProjectItemContent {
    DraftIssue {
        title: String,
        body: Option<String>,
    },
    Issue {
        number: i32,
        title: String,
        url: String,
        repository: RepositoryRef,
    },
    PullRequest {
        number: i32,
        title: String,
        url: String,
        repository: RepositoryRef,
    },
}

#[derive(Debug, Clone, Deserialize)]
pub struct RepositoryRef {
    #[serde(rename = "nameWithOwner")]
    pub name_with_owner: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct FieldValuesConnection {
    pub nodes: Vec<FieldValue>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "__typename")]
pub enum FieldValue {
    ProjectV2ItemFieldTextValue {
        text: Option<String>,
        field: FieldRef,
    },
    ProjectV2ItemFieldNumberValue {
        number: Option<f64>,
        field: FieldRef,
    },
    ProjectV2ItemFieldDateValue {
        date: Option<String>,
        field: FieldRef,
    },
    ProjectV2ItemFieldSingleSelectValue {
        #[serde(rename = "optionId")]
        option_id: Option<String>,
        name: Option<String>,
        field: FieldRef,
    },
    #[serde(other)]
    Unknown,
}

#[derive(Debug, Clone, Deserialize)]
pub struct FieldRef {
    pub id: String,
    pub name: String,
}

// ── AddDraftIssue ───────────────────────────────────────────────────────────

pub const ADD_DRAFT_ISSUE: &str = "
mutation AddDraftIssue($projectId: ID!, $title: String!, $body: String) {
  addProjectV2DraftIssue(input: {projectId: $projectId, title: $title, body: $body}) {
    projectItem { id }
  }
}
";

#[derive(Debug, Serialize)]
pub struct AddDraftIssueVars<'a> {
    #[serde(rename = "projectId")]
    pub project_id: &'a str,
    pub title: &'a str,
    pub body: Option<&'a str>,
}

#[derive(Debug, Deserialize)]
pub struct AddDraftIssueResponse {
    #[serde(rename = "addProjectV2DraftIssue")]
    pub add_project_v2_draft_issue: AddDraftIssuePayload,
}

#[derive(Debug, Deserialize)]
pub struct AddDraftIssuePayload {
    #[serde(rename = "projectItem")]
    pub project_item: ProjectItemRef,
}

#[derive(Debug, Deserialize)]
pub struct ProjectItemRef {
    pub id: String,
}

// ── UpdateProjectItemField ──────────────────────────────────────────────────

pub const UPDATE_PROJECT_ITEM_FIELD: &str = "
mutation UpdateProjectItemField($projectId: ID!, $itemId: ID!, $fieldId: ID!, $value: ProjectV2FieldValue!) {
  updateProjectV2ItemFieldValue(input: {projectId: $projectId, itemId: $itemId, fieldId: $fieldId, value: $value}) {
    projectV2Item { id }
  }
}
";

#[derive(Debug, Serialize)]
pub struct UpdateProjectItemFieldVars<'a> {
    #[serde(rename = "projectId")]
    pub project_id: &'a str,
    #[serde(rename = "itemId")]
    pub item_id: &'a str,
    #[serde(rename = "fieldId")]
    pub field_id: &'a str,
    pub value: FieldValueInput,
}

/// Shape mirrors GitHub's `ProjectV2FieldValue` input union. Only one field
/// should be populated per call.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct FieldValueInput {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub text: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub number: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub date: Option<String>,
    #[serde(
        rename = "singleSelectOptionId",
        skip_serializing_if = "Option::is_none"
    )]
    pub single_select_option_id: Option<String>,
}

impl FieldValueInput {
    pub fn text(value: impl Into<String>) -> Self {
        Self {
            text: Some(value.into()),
            number: None,
            date: None,
            single_select_option_id: None,
        }
    }
    pub fn number(value: f64) -> Self {
        Self {
            text: None,
            number: Some(value),
            date: None,
            single_select_option_id: None,
        }
    }
    pub fn date(value: impl Into<String>) -> Self {
        Self {
            text: None,
            number: None,
            date: Some(value.into()),
            single_select_option_id: None,
        }
    }
    pub fn single_select(option_id: impl Into<String>) -> Self {
        Self {
            text: None,
            number: None,
            date: None,
            single_select_option_id: Some(option_id.into()),
        }
    }
}

#[derive(Debug, Deserialize)]
pub struct UpdateProjectItemFieldResponse {
    #[serde(rename = "updateProjectV2ItemFieldValue")]
    pub update_project_v2_item_field_value: UpdateProjectItemFieldPayload,
}

#[derive(Debug, Deserialize)]
pub struct UpdateProjectItemFieldPayload {
    #[serde(rename = "projectV2Item")]
    pub project_v2_item: ProjectItemRef,
}

// ── DeleteProjectItem ───────────────────────────────────────────────────────

pub const DELETE_PROJECT_ITEM: &str = "
mutation DeleteProjectItem($projectId: ID!, $itemId: ID!) {
  deleteProjectV2Item(input: {projectId: $projectId, itemId: $itemId}) {
    deletedItemId
  }
}
";

#[derive(Debug, Serialize)]
pub struct DeleteProjectItemVars<'a> {
    #[serde(rename = "projectId")]
    pub project_id: &'a str,
    #[serde(rename = "itemId")]
    pub item_id: &'a str,
}

#[derive(Debug, Deserialize)]
pub struct DeleteProjectItemResponse {
    #[serde(rename = "deleteProjectV2Item")]
    pub delete_project_v2_item: DeleteProjectItemPayload,
}

#[derive(Debug, Deserialize)]
pub struct DeleteProjectItemPayload {
    #[serde(rename = "deletedItemId")]
    pub deleted_item_id: Option<String>,
}

// ── LinkIssueToProject (gated behind link_to_issues) ────────────────────────

pub const LINK_ISSUE_TO_PROJECT: &str = "
mutation LinkIssueToProject($projectId: ID!, $contentId: ID!) {
  addProjectV2ItemById(input: {projectId: $projectId, contentId: $contentId}) {
    item { id }
  }
}
";

#[derive(Debug, Serialize)]
pub struct LinkIssueToProjectVars<'a> {
    #[serde(rename = "projectId")]
    pub project_id: &'a str,
    #[serde(rename = "contentId")]
    pub content_id: &'a str,
}

#[derive(Debug, Deserialize)]
pub struct LinkIssueToProjectResponse {
    #[serde(rename = "addProjectV2ItemById")]
    pub add_project_v2_item_by_id: LinkIssueToProjectPayload,
}

#[derive(Debug, Deserialize)]
pub struct LinkIssueToProjectPayload {
    pub item: ProjectItemRef,
}
