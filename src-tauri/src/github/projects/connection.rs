//! Minimal GitHub GraphQL ping used by the Settings UI's "Test connection"
//! button. Calls the `viewer { login }` query with the stored PAT and returns
//! the resolved username (or an error string the UI can surface).
//!
//! We hand-roll the JSON request body here instead of pulling in
//! `graphql_client`; the latter arrives in P9 alongside the real client.

use serde::Deserialize;

use super::auth::load_pat;

const GITHUB_GRAPHQL_ENDPOINT: &str = "https://api.github.com/graphql";
const VIEWER_LOGIN_QUERY: &str = r#"{"query":"query { viewer { login } }"}"#;
const USER_AGENT: &str = concat!("tolaria/", env!("CARGO_PKG_VERSION"));

#[derive(Deserialize)]
struct ViewerResponse {
    data: Option<ViewerData>,
    errors: Option<Vec<GraphQlError>>,
}

#[derive(Deserialize)]
struct ViewerData {
    viewer: Viewer,
}

#[derive(Deserialize)]
struct Viewer {
    login: String,
}

#[derive(Deserialize)]
struct GraphQlError {
    message: String,
}

pub fn test_connection() -> Result<String, String> {
    let pat =
        load_pat()?.ok_or_else(|| "No GitHub personal access token is stored.".to_string())?;
    let body = call_viewer(&pat, GITHUB_GRAPHQL_ENDPOINT)?;
    parse_viewer_login(&body)
}

fn call_viewer(pat: &str, endpoint: &str) -> Result<String, String> {
    let client = reqwest::blocking::Client::builder()
        .user_agent(USER_AGENT)
        .build()
        .map_err(|e| format!("Failed to build HTTP client: {e}"))?;
    let response = client
        .post(endpoint)
        .bearer_auth(pat)
        .header("Accept", "application/vnd.github+json")
        .body(VIEWER_LOGIN_QUERY)
        .send()
        .map_err(|e| format!("GitHub request failed: {e}"))?;
    if !response.status().is_success() {
        return Err(format!("GitHub returned HTTP {}", response.status()));
    }
    response
        .text()
        .map_err(|e| format!("Failed to read GitHub response body: {e}"))
}

fn parse_viewer_login(body: &str) -> Result<String, String> {
    let parsed: ViewerResponse =
        serde_json::from_str(body).map_err(|e| format!("Failed to parse GitHub response: {e}"))?;
    if let Some(errors) = parsed.errors {
        let joined = errors
            .into_iter()
            .map(|e| e.message)
            .collect::<Vec<_>>()
            .join("; ");
        return Err(format!("GitHub returned errors: {joined}"));
    }
    let data = parsed
        .data
        .ok_or_else(|| "GitHub response missing 'data' field.".to_string())?;
    Ok(data.viewer.login)
}

#[cfg(test)]
mod tests {
    use super::parse_viewer_login;

    #[test]
    fn parses_a_successful_viewer_response() {
        let body = r#"{"data":{"viewer":{"login":"seltzdesign"}}}"#;
        assert_eq!(parse_viewer_login(body).unwrap(), "seltzdesign");
    }

    #[test]
    fn surfaces_graphql_errors_when_data_is_null() {
        let body = r#"{"data":null,"errors":[{"message":"Bad credentials"}]}"#;
        let err = parse_viewer_login(body).unwrap_err();
        assert!(err.contains("Bad credentials"));
    }

    #[test]
    fn joins_multiple_graphql_errors() {
        let body = r#"{"data":null,"errors":[{"message":"one"},{"message":"two"}]}"#;
        let err = parse_viewer_login(body).unwrap_err();
        assert!(err.contains("one"));
        assert!(err.contains("two"));
    }

    #[test]
    fn rejects_an_unparseable_response_body() {
        assert!(parse_viewer_login("not json").is_err());
    }

    #[test]
    fn rejects_a_response_missing_data_and_errors() {
        let body = r#"{}"#;
        let err = parse_viewer_login(body).unwrap_err();
        assert!(err.contains("data"));
    }
}
