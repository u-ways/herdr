use unicode_width::{UnicodeWidthChar, UnicodeWidthStr};

pub(crate) fn display_width(text: &str) -> usize {
    UnicodeWidthStr::width(text)
}

pub(crate) fn truncate_end(text: &str, max_width: usize) -> String {
    if display_width(text) <= max_width {
        return text.to_string();
    }
    if max_width == 0 {
        return String::new();
    }
    if max_width == 1 {
        return "…".to_string();
    }

    let prefix = take_prefix_width(text, max_width.saturating_sub(1));
    format!("{prefix}…")
}

/// Like `truncate_end`, but keeps the end of `text`.
pub(crate) fn truncate_start(text: &str, max_width: usize) -> String {
    if display_width(text) <= max_width {
        return text.to_string();
    }
    if max_width == 0 {
        return String::new();
    }

    let mut width = 0usize;
    let mut start = text.len();
    for (index, ch) in text.char_indices().rev() {
        width += UnicodeWidthChar::width(ch).unwrap_or(0);
        if width > max_width - 1 {
            break;
        }
        start = index;
    }
    format!("…{}", &text[start..])
}

fn take_prefix_width(text: &str, max_width: usize) -> String {
    let mut output = String::new();
    let mut width = 0usize;
    for ch in text.chars() {
        let ch_width = UnicodeWidthChar::width(ch).unwrap_or(0);
        if width + ch_width > max_width {
            break;
        }
        output.push(ch);
        width += ch_width;
    }
    output
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn truncate_end_uses_display_width() {
        let text = truncate_end("提交 herdr 的反馈", 16);

        assert_eq!(text, "提交 herdr 的反…");
        assert!(display_width(&text) <= 16);
    }

    #[test]
    fn truncate_start_keeps_the_end_by_display_width() {
        let text = truncate_start("提交 herdr 的反馈", 10);

        assert_eq!(text, "…dr 的反馈");
        assert!(display_width(&text) <= 10);
        assert_eq!(truncate_start("abc", 3), "abc");
        assert_eq!(truncate_start("abc", 1), "…");
        assert_eq!(truncate_start("abc", 0), "");
    }
}
