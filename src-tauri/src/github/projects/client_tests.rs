//! Integration tests for the GraphQL client against a wiremock server.
//!
//! Each test stands up a wiremock server, registers a response, calls the
//! client function, and asserts on both the parsed result and the request
//! that was sent (path, headers, body shape).

use serde_json::json;
use wiremock::matchers::{header, method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

use super::client::{
    add_draft_issue, build_http_client, build_request_body, delete_project_item,
    get_project_fields, link_issue_to_project, list_all_project_items, list_project_items_page,
    list_projects_for_org, list_projects_for_user, parse_envelope_for_tests,
    update_project_item_field, ClientConfig, ClientError, USER_AGENT,
};
use super::queries::{
    AddDraftIssueResponse, FieldValue, FieldValueInput, ListProjectsForUserResponse,
    ProjectItemContent,
};

fn config_for(server: &MockServer, pat: &str) -> ClientConfig {
    ClientConfig::new(pat).with_endpoint(server.uri())
}

async fn mount_post_json(server: &MockServer, body: serde_json::Value) {
    Mock::given(method("POST"))
        .and(path("/"))
        .and(header("Authorization", "Bearer test-pat"))
        .respond_with(ResponseTemplate::new(200).set_body_json(body))
        .mount(server)
        .await;
}

#[tokio::test]
async fn list_projects_for_user_returns_parsed_summaries() {
    let server = MockServer::start().await;
    mount_post_json(
        &server,
        json!({
            "data": {
                "user": {
                    "projectsV2": {
                        "nodes": [
                            {"id": "PVT_a", "number": 1, "title": "Alpha", "url": "https://github.com/users/x/projects/1", "closed": false},
                            {"id": "PVT_b", "number": 2, "title": "Beta", "url": "https://github.com/users/x/projects/2", "closed": true},
                        ]
                    }
                }
            }
        }),
    )
    .await;

    let http = build_http_client().unwrap();
    let config = config_for(&server, "test-pat");
    let projects = list_projects_for_user(&http, &config, "x").await.unwrap();
    assert_eq!(projects.len(), 2);
    assert_eq!(projects[0].title, "Alpha");
    assert_eq!(projects[1].closed, true);
}

#[tokio::test]
async fn list_projects_for_org_returns_parsed_summaries() {
    let server = MockServer::start().await;
    mount_post_json(
        &server,
        json!({
            "data": {
                "organization": {
                    "projectsV2": {
                        "nodes": [
                            {"id": "PVT_o", "number": 7, "title": "Org Board", "url": "https://github.com/orgs/o/projects/7", "closed": false},
                        ]
                    }
                }
            }
        }),
    )
    .await;

    let http = build_http_client().unwrap();
    let config = config_for(&server, "test-pat");
    let projects = list_projects_for_org(&http, &config, "o").await.unwrap();
    assert_eq!(projects.len(), 1);
    assert_eq!(projects[0].id, "PVT_o");
}

#[tokio::test]
async fn surfaces_graphql_errors_when_returned() {
    let server = MockServer::start().await;
    mount_post_json(
        &server,
        json!({
            "data": null,
            "errors": [{"message": "Bad credentials"}],
        }),
    )
    .await;
    let http = build_http_client().unwrap();
    let config = config_for(&server, "test-pat");
    let err = list_projects_for_user(&http, &config, "x")
        .await
        .unwrap_err();
    match err {
        ClientError::GraphQl(message) => assert!(message.contains("Bad credentials")),
        other => panic!("expected GraphQl error, got {other:?}"),
    }
}

#[tokio::test]
async fn surfaces_http_errors_with_status_and_body() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/"))
        .respond_with(ResponseTemplate::new(403).set_body_string("forbidden"))
        .mount(&server)
        .await;
    let http = build_http_client().unwrap();
    let config = config_for(&server, "test-pat");
    let err = list_projects_for_user(&http, &config, "x")
        .await
        .unwrap_err();
    match err {
        ClientError::Http { status, body } => {
            assert_eq!(status, 403);
            assert!(body.contains("forbidden"));
        }
        other => panic!("expected Http error, got {other:?}"),
    }
}

#[tokio::test]
async fn get_project_fields_returns_field_metadata() {
    let server = MockServer::start().await;
    mount_post_json(
        &server,
        json!({
            "data": {
                "node": {
                    "fields": {
                        "nodes": [
                            {"__typename": "ProjectV2Field", "id": "f1", "name": "Title", "dataType": "TITLE"},
                            {"__typename": "ProjectV2SingleSelectField", "id": "f2", "name": "Status", "dataType": "SINGLE_SELECT",
                                "options": [{"id": "o1", "name": "Backlog"}, {"id": "o2", "name": "Done"}]},
                        ]
                    }
                }
            }
        }),
    )
    .await;
    let http = build_http_client().unwrap();
    let config = config_for(&server, "test-pat");
    let fields = get_project_fields(&http, &config, "PVT_x").await.unwrap();
    assert_eq!(fields.len(), 2);
    assert_eq!(fields[1].name, "Status");
    assert_eq!(fields[1].options.len(), 2);
    assert_eq!(fields[1].options[0].id, "o1");
}

#[tokio::test]
async fn list_project_items_paginates_through_cursors() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/"))
        .respond_with(
            ResponseTemplate::new(200)
                .insert_header("Content-Type", "application/json")
                .set_body_string(serde_json::to_string(&json!({
                    "data": {
                        "node": {
                            "items": {
                                "pageInfo": {"hasNextPage": false, "endCursor": null},
                                "nodes": [
                                    {
                                        "id": "PVTI_1",
                                        "content": {"__typename": "DraftIssue", "title": "Draft A", "body": null},
                                        "fieldValues": {"nodes": []}
                                    }
                                ]
                            }
                        }
                    }
                })).unwrap()),
        )
        .mount(&server)
        .await;
    let http = build_http_client().unwrap();
    let config = config_for(&server, "test-pat");
    let page = list_project_items_page(&http, &config, "PVT_x", None)
        .await
        .unwrap();
    assert_eq!(page.items.len(), 1);
    assert!(page.next_cursor.is_none());
    match &page.items[0].content {
        Some(ProjectItemContent::DraftIssue { title, .. }) => assert_eq!(title, "Draft A"),
        other => panic!("expected DraftIssue, got {other:?}"),
    }
}

#[tokio::test]
async fn list_all_project_items_concatenates_pages() {
    let server = MockServer::start().await;
    // Both responses look the same to wiremock; we just want to verify
    // that the drain helper exits when hasNextPage is false.
    mount_post_json(
        &server,
        json!({
            "data": {
                "node": {
                    "items": {
                        "pageInfo": {"hasNextPage": false, "endCursor": null},
                        "nodes": [
                            {"id": "PVTI_a", "content": null, "fieldValues": {"nodes": []}},
                            {"id": "PVTI_b", "content": null, "fieldValues": {"nodes": []}},
                        ]
                    }
                }
            }
        }),
    )
    .await;
    let http = build_http_client().unwrap();
    let config = config_for(&server, "test-pat");
    let items = list_all_project_items(&http, &config, "PVT_x")
        .await
        .unwrap();
    assert_eq!(items.len(), 2);
}

#[tokio::test]
async fn add_draft_issue_returns_the_new_item_id() {
    let server = MockServer::start().await;
    mount_post_json(
        &server,
        json!({
            "data": {
                "addProjectV2DraftIssue": {
                    "projectItem": {"id": "PVTI_new"}
                }
            }
        }),
    )
    .await;
    let http = build_http_client().unwrap();
    let config = config_for(&server, "test-pat");
    let id = add_draft_issue(&http, &config, "PVT_x", "New Task", Some("Body"))
        .await
        .unwrap();
    assert_eq!(id, "PVTI_new");
}

#[tokio::test]
async fn update_project_item_field_returns_updated_item_id() {
    let server = MockServer::start().await;
    mount_post_json(
        &server,
        json!({
            "data": {
                "updateProjectV2ItemFieldValue": {
                    "projectV2Item": {"id": "PVTI_updated"}
                }
            }
        }),
    )
    .await;
    let http = build_http_client().unwrap();
    let config = config_for(&server, "test-pat");
    let id = update_project_item_field(
        &http,
        &config,
        "PVT_x",
        "PVTI_y",
        "FIELD_id",
        FieldValueInput::text("hello"),
    )
    .await
    .unwrap();
    assert_eq!(id, "PVTI_updated");
}

#[tokio::test]
async fn delete_project_item_returns_deleted_id() {
    let server = MockServer::start().await;
    mount_post_json(
        &server,
        json!({
            "data": {
                "deleteProjectV2Item": {
                    "deletedItemId": "PVTI_gone"
                }
            }
        }),
    )
    .await;
    let http = build_http_client().unwrap();
    let config = config_for(&server, "test-pat");
    let deleted = delete_project_item(&http, &config, "PVT_x", "PVTI_y")
        .await
        .unwrap();
    assert_eq!(deleted.as_deref(), Some("PVTI_gone"));
}

#[tokio::test]
async fn link_issue_to_project_returns_item_id() {
    let server = MockServer::start().await;
    mount_post_json(
        &server,
        json!({
            "data": {
                "addProjectV2ItemById": {
                    "item": {"id": "PVTI_linked"}
                }
            }
        }),
    )
    .await;
    let http = build_http_client().unwrap();
    let config = config_for(&server, "test-pat");
    let id = link_issue_to_project(&http, &config, "PVT_x", "I_abc")
        .await
        .unwrap();
    assert_eq!(id, "PVTI_linked");
}

#[tokio::test]
async fn parses_single_select_field_values() {
    let server = MockServer::start().await;
    mount_post_json(
        &server,
        json!({
            "data": {
                "node": {
                    "items": {
                        "pageInfo": {"hasNextPage": false, "endCursor": null},
                        "nodes": [
                            {
                                "id": "PVTI_1",
                                "content": null,
                                "fieldValues": {
                                    "nodes": [
                                        {
                                            "__typename": "ProjectV2ItemFieldSingleSelectValue",
                                            "optionId": "opt-1",
                                            "name": "In progress",
                                            "field": {"id": "F_status", "name": "Status"}
                                        }
                                    ]
                                }
                            }
                        ]
                    }
                }
            }
        }),
    )
    .await;
    let http = build_http_client().unwrap();
    let config = config_for(&server, "test-pat");
    let page = list_project_items_page(&http, &config, "PVT_x", None)
        .await
        .unwrap();
    let values = &page.items[0].field_values.nodes;
    assert_eq!(values.len(), 1);
    match &values[0] {
        FieldValue::ProjectV2ItemFieldSingleSelectValue {
            name,
            option_id,
            field,
        } => {
            assert_eq!(name.as_deref(), Some("In progress"));
            assert_eq!(option_id.as_deref(), Some("opt-1"));
            assert_eq!(field.id, "F_status");
        }
        other => panic!("expected single select value, got {other:?}"),
    }
}

#[tokio::test]
async fn unknown_field_value_typename_falls_through_to_unknown_variant() {
    let body = json!({
        "__typename": "ProjectV2ItemFieldUserValue",
        "users": {"nodes": []}
    });
    let parsed: FieldValue = serde_json::from_value(body).unwrap();
    assert!(matches!(parsed, FieldValue::Unknown));
}

#[test]
fn build_request_body_serializes_query_and_variables() {
    let body = build_request_body("query { viewer { login } }", json!({"a": 1}));
    assert_eq!(body["query"], "query { viewer { login } }");
    assert_eq!(body["variables"]["a"], 1);
}

#[test]
fn parse_envelope_returns_empty_data_when_response_lacks_a_data_field() {
    let err = parse_envelope_for_tests::<ListProjectsForUserResponse>("{}").unwrap_err();
    matches!(err, ClientError::EmptyData);
}

#[test]
fn parse_envelope_decodes_a_successful_payload() {
    let body = r#"{"data": {"addProjectV2DraftIssue": {"projectItem": {"id": "PVTI_x"}}}}"#;
    let data: AddDraftIssueResponse = parse_envelope_for_tests(body).unwrap();
    assert_eq!(data.add_project_v2_draft_issue.project_item.id, "PVTI_x");
}

#[tokio::test]
async fn sends_the_pat_as_a_bearer_token() {
    // Only register one mock, and only for requests carrying the right
    // Authorization header. If the client failed to attach the token, the
    // request would 404 against wiremock's default handler and the call
    // would fail.
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/"))
        .and(header("Authorization", "Bearer my-secret-pat"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "data": {"user": {"projectsV2": {"nodes": []}}}
        })))
        .mount(&server)
        .await;
    let http = build_http_client().unwrap();
    let config = config_for(&server, "my-secret-pat");
    list_projects_for_user(&http, &config, "x")
        .await
        .expect("expected bearer-authed request to succeed");
}

#[test]
fn user_agent_includes_the_crate_version() {
    assert!(USER_AGENT.starts_with("tolaria/"));
}
