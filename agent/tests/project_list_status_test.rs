use kn_agent::session::project_list_status;

#[test]
fn parses_clean_branch_status_with_upstream_counts() {
    let output = "# branch.oid abcdef\0# branch.head feature/list\0# branch.upstream origin/feature/list\0# branch.ab +2 -1\0";

    let status = project_list_status::parse_porcelain_v2(output);

    assert_eq!(status.state, project_list_status::GitListState::Clean);
    assert_eq!(status.branch.as_deref(), Some("feature/list"));
    assert!(status.has_upstream);
    assert_eq!(status.ahead, 2);
    assert_eq!(status.behind, 1);
}

#[test]
fn parses_any_porcelain_entry_as_changed() {
    let output = "# branch.oid abcdef\0# branch.head main\0# branch.upstream origin/main\0# branch.ab +0 -0\0? new-file.txt\0";

    let status = project_list_status::parse_porcelain_v2(output);

    assert_eq!(status.state, project_list_status::GitListState::Changed);
    assert_eq!(status.ahead, 0);
    assert_eq!(status.behind, 0);
}

#[test]
fn parses_missing_upstream_without_treating_it_as_an_error() {
    let output = "# branch.oid abcdef\0# branch.head topic\0";

    let status = project_list_status::parse_porcelain_v2(output);

    assert_eq!(status.state, project_list_status::GitListState::Clean);
    assert_eq!(status.branch.as_deref(), Some("topic"));
    assert!(!status.has_upstream);
}

#[test]
fn ignores_non_branch_headers_when_worktree_is_clean() {
    let output = "# branch.oid abcdef\0# branch.head main\0# stash 1\0";

    let status = project_list_status::parse_porcelain_v2(output);

    assert_eq!(status.state, project_list_status::GitListState::Clean);
}
