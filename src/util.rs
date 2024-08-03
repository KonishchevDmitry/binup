use const_format::formatcp;

pub static USER_AGENT: &str = formatcp!(
    "{name}/{version} ({homepage})",
    name=env!("CARGO_PKG_NAME"), version=env!("CARGO_PKG_VERSION"), homepage=env!("CARGO_PKG_REPOSITORY"),
);

pub fn format_multiline(text: &str) -> String {
    let text = text.trim_end();

    if text.find('\n').is_some() {
        format!("\n{text}")
    } else {
        format!(" {text}")
    }
}