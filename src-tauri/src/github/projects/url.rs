//! Parse the canonical GitHub Projects v2 URL into an owner-kind / login /
//! project-number tuple.
//!
//! Accepts both shapes the GitHub UI hands out:
//! - `https://github.com/users/<login>/projects/<n>` (user-owned)
//! - `https://github.com/orgs/<login>/projects/<n>`  (org-owned)
//!
//! Trailing query strings, fragments, and `/views/<id>` suffixes are
//! tolerated since real-world URLs often carry them.

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProjectUrl {
    pub owner: ProjectOwner,
    pub login: String,
    pub number: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProjectOwner {
    User,
    Org,
}

pub fn parse_project_url(input: &str) -> Result<ProjectUrl, String> {
    let trimmed = input.trim();
    if trimmed.is_empty() {
        return Err("Project URL is empty.".into());
    }
    let without_scheme = trimmed
        .strip_prefix("https://")
        .or_else(|| trimmed.strip_prefix("http://"))
        .unwrap_or(trimmed);
    let after_host = without_scheme
        .strip_prefix("github.com/")
        .or_else(|| without_scheme.strip_prefix("www.github.com/"))
        .ok_or_else(|| "URL must point at github.com.".to_string())?;
    let path = after_host
        .split(['?', '#'])
        .next()
        .unwrap_or("")
        .trim_end_matches('/');

    let parts: Vec<&str> = path.split('/').collect();
    if parts.len() < 4 {
        return Err("URL is missing the projects/<number> segments.".into());
    }
    let owner = match parts[0] {
        "users" => ProjectOwner::User,
        "orgs" => ProjectOwner::Org,
        other => return Err(format!("Unsupported owner segment `{other}`.")),
    };
    let login = parts[1].to_string();
    if login.is_empty() {
        return Err("Owner login is empty.".into());
    }
    if parts[2] != "projects" {
        return Err("URL must contain `/projects/<number>`.".into());
    }
    let number: u32 = parts[3]
        .parse()
        .map_err(|_| format!("`{}` is not a project number.", parts[3]))?;
    Ok(ProjectUrl {
        owner,
        login,
        number,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_a_user_project_url() {
        let url = parse_project_url("https://github.com/users/seltzdesign/projects/7").unwrap();
        assert_eq!(url.owner, ProjectOwner::User);
        assert_eq!(url.login, "seltzdesign");
        assert_eq!(url.number, 7);
    }

    #[test]
    fn parses_an_org_project_url() {
        let url = parse_project_url("https://github.com/orgs/acme/projects/42").unwrap();
        assert_eq!(url.owner, ProjectOwner::Org);
        assert_eq!(url.login, "acme");
        assert_eq!(url.number, 42);
    }

    #[test]
    fn tolerates_a_trailing_view_segment() {
        let url =
            parse_project_url("https://github.com/users/seltzdesign/projects/7/views/3").unwrap();
        assert_eq!(url.number, 7);
    }

    #[test]
    fn tolerates_query_strings_and_fragments() {
        let url =
            parse_project_url("https://github.com/users/seltzdesign/projects/7?layout=board#x")
                .unwrap();
        assert_eq!(url.number, 7);
    }

    #[test]
    fn tolerates_a_missing_scheme() {
        let url = parse_project_url("github.com/users/seltzdesign/projects/7").unwrap();
        assert_eq!(url.login, "seltzdesign");
    }

    #[test]
    fn rejects_a_non_github_host() {
        let err = parse_project_url("https://gitlab.com/users/x/projects/1").unwrap_err();
        assert!(err.contains("github.com"));
    }

    #[test]
    fn rejects_a_url_without_a_projects_segment() {
        let err = parse_project_url("https://github.com/users/x/repositories/1").unwrap_err();
        assert!(err.contains("projects"));
    }

    #[test]
    fn rejects_a_non_numeric_project_number() {
        let err = parse_project_url("https://github.com/users/x/projects/abc").unwrap_err();
        assert!(err.contains("project number"));
    }

    #[test]
    fn rejects_an_unsupported_owner_kind() {
        let err = parse_project_url("https://github.com/repos/x/projects/1").unwrap_err();
        assert!(err.contains("owner"));
    }

    #[test]
    fn rejects_an_empty_url() {
        assert!(parse_project_url("   ").is_err());
    }
}
