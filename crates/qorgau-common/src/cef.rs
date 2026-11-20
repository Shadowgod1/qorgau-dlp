use chrono::{DateTime, Utc};

use crate::{dlp::mask_text, Severity};

/// Escapes CEF header fields (`|` and `\`, plus strips CR/LF).
fn escape_cef_header(s: &str) -> String {
    s.replace('\\', "\\\\")
        .replace('|', "\\|")
        .replace(['\r', '\n'], " ")
}

/// Escapes CEF extension values (`=`, `\`, `\r`, `\n`).
fn escape_cef_ext(s: &str) -> String {
    s.replace('\\', "\\\\")
        .replace('=', "\\=")
        .replace('\r', "\\r")
        .replace('\n', "\\n")
}

/// Sanitizes RFC 5424 syslog HOSTNAME (printable ASCII without spaces/controls).
fn sanitize_syslog_host(s: &str) -> String {
    let cleaned: String = s
        .chars()
        .filter(|c| c.is_ascii_graphic() && !c.is_whitespace())
        .take(64)
        .collect();
    if cleaned.is_empty() {
        "-".to_string()
    } else {
        cleaned
    }
}

#[derive(Debug, Clone)]
pub struct CefIncident<'a> {
    pub id: i64,
    pub ts: DateTime<Utc>,
    pub hostname: &'a str,
    pub username: &'a str,
    pub agent_id: &'a str,
    pub channel: &'a str,
    pub severity: Severity,
    pub rules: &'a [String],
    pub snippet: &'a str,
}

/// Formats an incident as an RFC 5424 Syslog message wrapping ArcSight `CEF:0`.
/// Masks PII in snippet before emitting to external SIEM/ОЦИБ collectors and caps UDP payload size.
pub fn format_syslog_cef(inc: &CefIncident<'_>, max_bytes: usize) -> String {
    let sig_id = escape_cef_header(&format!("{}-{}", inc.channel, inc.severity.as_str()));
    let name = escape_cef_header(&format!("DLP Incident ({})", inc.channel));
    let rules_str = escape_cef_ext(&inc.rules.join(", "));
    let masked_snippet = mask_text(inc.snippet);
    let snippet_short: String = masked_snippet.chars().take(200).collect();
    let msg = escape_cef_ext(&snippet_short);

    let cef = format!(
        "CEF:0|Qorgau|DLP|2.0|{}|{}|{}|externalId={} suser={} shost={} cs1={} cs1Label=AgentID cs2={} cs2Label=Channel cs3={} cs3Label=MatchedRules msg={}",
        sig_id,
        name,
        inc.severity.cef_level(),
        inc.id,
        escape_cef_ext(inc.username),
        escape_cef_ext(inc.hostname),
        escape_cef_ext(inc.agent_id),
        escape_cef_ext(inc.channel),
        rules_str,
        msg
    );

    // Facility local4 (20) * 8 + severity (warning=4..critical=2)
    let syslog_sev = match inc.severity {
        Severity::Critical => 2,
        Severity::High => 3,
        Severity::Medium => 4,
        Severity::Low => 6,
    };
    let pri = 20 * 8 + syslog_sev;
    let host = sanitize_syslog_host(inc.hostname);
    let full = format!(
        "<{}>1 {} {} qorgau-dlp - - - {}",
        pri,
        inc.ts.to_rfc3339(),
        host,
        cef
    );

    if full.len() <= max_bytes {
        full
    } else {
        // Safely truncate on UTF-8 char boundary
        let mut end = max_bytes;
        while end > 0 && !full.is_char_boundary(end) {
            end -= 1;
        }
        full[..end].to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cef_escapes_headers_and_strips_crlf() {
        let rules = vec!["Rule=1".to_string()];
        let msg = format_syslog_cef(
            &CefIncident {
                id: 42,
                ts: Utc::now(),
                hostname: "WIN PC\r\nEVIL",
                username: "ivanov",
                agent_id: "agent123456",
                channel: "usb|evil",
                severity: Severity::Critical,
                rules: &rules,
                snippet: "card 4111 1111 1111 1111",
            },
            1400,
        );
        assert!(!msg.contains('\r'));
        assert!(!msg.contains('\n'));
        assert!(msg.contains(r"usb\|evil-critical"));
        assert!(msg.contains(r"Rule\=1"));
        assert!(!msg.contains("4111 1111 1111 1111"));
    }
}
