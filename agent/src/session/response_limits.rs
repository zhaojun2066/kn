#[cfg(test)]
mod tests {
    use super::truncate_utf8;

    #[test]
    fn truncates_utf8_without_splitting_a_character() {
        let value = format!("{}中文", "a".repeat(4094));
        let truncated = truncate_utf8(&value, 4095);
        assert_eq!(truncated.len(), 4094);
        assert!(truncated.is_char_boundary(truncated.len()));
    }
}

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
