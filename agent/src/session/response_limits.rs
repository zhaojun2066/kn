#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn truncates_utf8_without_splitting_a_character() {
        let value = format!("{}中文", "a".repeat(4094));
        let truncated = truncate_utf8(&value, 4095);
        assert_eq!(truncated.len(), 4094);
        assert!(truncated.is_char_boundary(truncated.len()));
    }

    #[test]
    fn bounds_log_lines_by_line_and_window_budget() {
        let input = (1..=100).map(|line| (line, "界".repeat(2_000))).collect();
        let (lines, truncated) = bounded_log_lines(input);
        assert!(truncated);
        assert!(lines.iter().all(|line| line["text"].as_str().unwrap().len() <= LOG_LINE_MAX_BYTES));
        assert!(serde_json::to_vec(&lines).unwrap().len() <= LOG_WINDOW_MAX_BYTES);
    }

    #[test]
    fn bounded_log_window_retains_requested_center_when_earlier_lines_are_large() {
        let input = (1..=201)
            .map(|line| (line, "x".repeat(LOG_LINE_MAX_BYTES)))
            .collect();

        let (lines, truncated) = bounded_log_lines_around_center(input, 101);

        assert!(truncated);
        assert!(lines.iter().any(|line| line["lineNumber"] == 101));
        assert!(serde_json::to_vec(&lines).unwrap().len() <= LOG_WINDOW_MAX_BYTES);
    }

    #[test]
    fn bounded_log_window_never_skips_a_line_inside_the_returned_range() {
        let mut input: Vec<_> = (1..=201)
            .map(|line| (line, "x".repeat(LOG_LINE_MAX_BYTES)))
            .collect();
        input[100 - 16].1.clear();

        let (lines, _) = bounded_log_lines_around_center(input, 101);
        let line_numbers: Vec<_> = lines
            .iter()
            .filter_map(|line| line["lineNumber"].as_u64())
            .collect();

        assert!(line_numbers.windows(2).all(|pair| pair[1] == pair[0] + 1));
    }

    #[test]
    fn bounds_issue_preview_and_total_response_budget() {
        let mut issues = Vec::new();
        let (added, truncated) = append_bounded_issue(
            &mut issues,
            serde_json::json!({"preview": "界".repeat(2_000)}),
        );
        assert!(added);
        assert!(truncated);
        assert!(issues[0]["preview"].as_str().unwrap().len() <= ISSUE_CONTEXT_MAX_BYTES);

        for _ in 0..100 {
            let _ = append_bounded_issue(&mut issues, serde_json::json!({"preview": "x".repeat(4096)}));
        }
        assert!(serde_json::to_vec(&issues).unwrap().len() < ISSUE_RESULT_MAX_BYTES);
    }

    #[test]
    fn issue_budget_bounds_non_preview_fields_before_serializing() {
        let mut issues = Vec::new();
        let (added, truncated) = append_bounded_issue(
            &mut issues,
            serde_json::json!({
                "issueId": "i".repeat(200 * 1024),
                "ruleId": "r".repeat(200 * 1024),
                "level": "l".repeat(200 * 1024),
                "preview": "ok"
            }),
        );

        assert!(added);
        assert!(truncated);
        assert!(serde_json::to_vec(&issues).unwrap().len() < ISSUE_RESULT_MAX_BYTES);
    }

    #[test]
    fn final_issue_response_stays_bounded_with_oversized_metadata() {
        let response = bounded_issue_response(
            &"s".repeat(200 * 1024),
            &"r".repeat(200 * 1024),
            &"v".repeat(200 * 1024),
            vec![serde_json::json!({"preview": "x".repeat(4096)})],
            false,
        );

        assert_eq!(response["truncated"], true);
        assert!(serde_json::to_vec(&response).unwrap().len() <= ISSUE_RESULT_MAX_BYTES);
    }
}

pub const LOG_WINDOW_MAX_BYTES: usize = 128 * 1024;
pub const LOG_LINE_MAX_BYTES: usize = 4 * 1024;
pub const ISSUE_CONTEXT_MAX_BYTES: usize = 4 * 1024;
pub const ISSUE_RESULT_MAX_BYTES: usize = 128 * 1024;
const LOG_LINES_JSON_BUDGET: usize = 120 * 1024;
const ISSUE_ENVELOPE_RESERVE_BYTES: usize = 8 * 1024;
const ISSUE_METADATA_MAX_BYTES: usize = 4 * 1024;

pub fn truncate_utf8(text: &str, max_bytes: usize) -> String {
    if text.len() <= max_bytes {
        return text.to_string();
    }
    let mut end = max_bytes;
    while end > 0 && !text.is_char_boundary(end) {
        end -= 1;
    }
    text[..end].to_string()
}

pub fn bounded_log_lines(lines: Vec<(usize, String)>) -> (Vec<serde_json::Value>, bool) {
    let center_line = lines.first().map(|(line, _)| *line).unwrap_or(0);
    bounded_log_lines_around_center(lines, center_line)
}

/// Bounds a log window while keeping the requested center line first in the
/// allocation order. Returned lines are sorted so callers still receive one
/// continuous window around the center.
pub fn bounded_log_lines_around_center(
    lines: Vec<(usize, String)>,
    center_line: usize,
) -> (Vec<serde_json::Value>, bool) {
    if lines.is_empty() {
        return (Vec::new(), false);
    }
    let center_index = lines
        .iter()
        .position(|(line_number, _)| *line_number == center_line)
        .unwrap_or(lines.len() / 2);
    let mut selected: Vec<(usize, serde_json::Value)> = Vec::new();
    let mut omitted = false;

    let mut try_add = |index: usize| -> bool {
        let (line_number, text) = &lines[index];
        let bounded_text = truncate_utf8(text, LOG_LINE_MAX_BYTES);
        if bounded_text.len() < text.len() {
            omitted = true;
        }
        let value = serde_json::json!({ "lineNumber": line_number, "text": bounded_text });
        let mut candidate = selected.clone();
        candidate.push((*line_number, value));
        candidate.sort_by_key(|(line_number, _)| *line_number);
        let candidate_values: Vec<&serde_json::Value> = candidate.iter().map(|(_, value)| value).collect();
        if serde_json::to_vec(&candidate_values)
            .map(|bytes| bytes.len() <= LOG_LINES_JSON_BUDGET)
            .unwrap_or(false)
        {
            selected = candidate;
            true
        } else {
            omitted = true;
            false
        }
    };

    let _ = try_add(center_index);
    let mut distance = 1;
    let mut include_left = true;
    let mut include_right = true;
    while center_index >= distance || center_index + distance < lines.len() {
        if include_left && center_index >= distance {
            include_left = try_add(center_index - distance);
        }
        if include_right && center_index + distance < lines.len() {
            include_right = try_add(center_index + distance);
        }
        distance += 1;
    }
    if selected.len() < lines.len() {
        omitted = true;
    }
    (selected.into_iter().map(|(_, value)| value).collect(), omitted)
}

/// Adds an issue only if the response remains bounded. The returned flags are
/// `(added, truncated)`: a long preview is safely shortened before insertion.
pub fn append_bounded_issue(
    issues: &mut Vec<serde_json::Value>,
    mut issue: serde_json::Value,
) -> (bool, bool) {
    let mut truncated = false;
    if let Some(object) = issue.as_object_mut() {
        for value in object.values_mut() {
            if let Some(text) = value.as_str() {
                let bounded = truncate_utf8(text, ISSUE_CONTEXT_MAX_BYTES);
                truncated |= bounded.len() < text.len();
                *value = serde_json::Value::String(bounded);
            }
        }
    }
    // The current envelope includes a handful of identifier/status fields; reserve
    // a small fixed amount so its final JSON stays under the hard 128 KiB ceiling.
    let used = serde_json::to_vec(issues).map_or(ISSUE_RESULT_MAX_BYTES, |bytes| bytes.len());
    let next = serde_json::to_vec(&issue).map_or(ISSUE_RESULT_MAX_BYTES, |bytes| bytes.len());
    if used
        .saturating_add(next)
        .saturating_add(ISSUE_ENVELOPE_RESERVE_BYTES)
        > ISSUE_RESULT_MAX_BYTES
    {
        return (false, true);
    }
    issues.push(issue);
    (true, truncated)
}

/// Produces the complete error-extraction result under its 128 KiB contract.
/// Identifiers are bounded only for an otherwise malformed/oversized request;
/// normal protocol IDs are far below this threshold and remain unchanged.
pub fn bounded_issue_response(
    session_id: &str,
    run_id: &str,
    rules_version: &str,
    mut issues: Vec<serde_json::Value>,
    mut truncated: bool,
) -> serde_json::Value {
    let session_id_was_truncated = session_id.len() > ISSUE_METADATA_MAX_BYTES;
    let run_id_was_truncated = run_id.len() > ISSUE_METADATA_MAX_BYTES;
    let rules_version_was_truncated = rules_version.len() > ISSUE_METADATA_MAX_BYTES;
    let session_id = truncate_utf8(session_id, ISSUE_METADATA_MAX_BYTES);
    let run_id = truncate_utf8(run_id, ISSUE_METADATA_MAX_BYTES);
    let rules_version = truncate_utf8(rules_version, ISSUE_METADATA_MAX_BYTES);
    truncated |= session_id_was_truncated || run_id_was_truncated || rules_version_was_truncated;

    loop {
        let response = serde_json::json!({
            "sessionId": session_id,
            "runId": run_id,
            "status": "ok",
            "rulesVersion": rules_version,
            "issues": issues,
            "truncated": truncated,
        });
        if serde_json::to_vec(&response)
            .map(|bytes| bytes.len() <= ISSUE_RESULT_MAX_BYTES)
            .unwrap_or(false)
        {
            return response;
        }
        if issues.pop().is_none() {
            return serde_json::json!({
                "sessionId": truncate_utf8(&session_id, 512),
                "runId": truncate_utf8(&run_id, 512),
                "status": "ok",
                "rulesVersion": truncate_utf8(&rules_version, 512),
                "issues": [],
                "truncated": true,
            });
        }
        truncated = true;
    }
}
