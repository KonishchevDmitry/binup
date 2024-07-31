pub fn format_multiline(text: &str) -> String {
    let text = text.trim_end();

    if text.find('\n').is_some() {
        format!("\n{text}")
    } else {
        format!(" {text}")
    }
}