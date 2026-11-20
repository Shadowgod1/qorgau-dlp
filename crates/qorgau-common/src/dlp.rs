use std::sync::OnceLock;

use regex::Regex;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::Severity;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct DlpMatch {
    pub rule_name: String,
    pub severity: Severity,
    pub action: String, // "block" | "alert" | "log"
    pub matched_sample: String,
}

/// Validates a Kazakhstan 12-digit IIN (Индивидуальный идентификационный номер)
/// according to the official modulo-11 two-pass checksum algorithm + birth date & century digit check.
pub fn is_valid_kz_iin(s: &str) -> bool {
    let bytes = s.as_bytes();
    if bytes.len() != 12 || !bytes.iter().all(|b| b.is_ascii_digit()) {
        return false;
    }
    let d: Vec<u32> = bytes.iter().map(|b| (b - b'0') as u32).collect();

    // Month 01..=12, Day 01..=31
    let month = d[2] * 10 + d[3];
    let day = d[4] * 10 + d[5];
    if !(1..=12).contains(&month) || !(1..=31).contains(&day) {
        return false;
    }

    // 7th digit (index 6): 1..=6 encodes gender & century (1800s/1900s/2000s)
    if !(1..=6).contains(&d[6]) {
        return false;
    }

    kz_mod11_checksum(&d)
}

/// Validates a Kazakhstan 12-digit BIN (Бизнес-идентификационный номер).
/// Month 01..=12, 5th digit (entity type) in 4..=6, 6th digit (structure) in 0..=4, plus mod-11 checksum.
pub fn is_valid_kz_bin(s: &str) -> bool {
    let bytes = s.as_bytes();
    if bytes.len() != 12 || !bytes.iter().all(|b| b.is_ascii_digit()) {
        return false;
    }
    let d: Vec<u32> = bytes.iter().map(|b| (b - b'0') as u32).collect();

    let month = d[2] * 10 + d[3];
    if !(1..=12).contains(&month) {
        return false;
    }
    // 5th digit (index 4): 4 = resident legal entity, 5 = non-resident, 6 = individual entrepreneur (joint)
    if !(4..=6).contains(&d[4]) {
        return false;
    }
    // 6th digit (index 5): 0..=4
    if d[5] > 4 {
        return false;
    }

    kz_mod11_checksum(&d)
}

fn kz_mod11_checksum(d: &[u32]) -> bool {
    const W1: [u32; 11] = [1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11];
    const W2: [u32; 11] = [3, 4, 5, 6, 7, 8, 9, 10, 11, 1, 2];

    let sum1: u32 = d.iter().take(11).zip(W1).map(|(a, w)| a * w).sum();
    let mut ctrl = sum1 % 11;
    if ctrl == 10 {
        let sum2: u32 = d.iter().take(11).zip(W2).map(|(a, w)| a * w).sum();
        ctrl = sum2 % 11;
    }
    ctrl != 10 && ctrl == d[11]
}

/// Luhn algorithm for 13..=19 digit payment cards (ignoring spaces/hyphens).
pub fn is_valid_luhn_card(raw: &str) -> bool {
    let digits: Vec<u32> = raw
        .bytes()
        .filter(|b| b.is_ascii_digit())
        .map(|b| (b - b'0') as u32)
        .collect();
    if !(13..=19).contains(&digits.len()) {
        return false;
    }
    // Reject trivial all-identical digits (e.g. 0000000000000000)
    if digits.iter().all(|&x| x == digits[0]) {
        return false;
    }
    let mut sum = 0u32;
    for (idx, &digit) in digits.iter().rev().enumerate() {
        if idx % 2 == 1 {
            let mut doubled = digit * 2;
            if doubled > 9 {
                doubled -= 9;
            }
            sum += doubled;
        } else {
            sum += digit;
        }
    }
    sum % 10 == 0
}

/// Validates Kazakhstan IBAN (`KZ` + 18 alphanumeric chars = 20 chars) using ISO 7064 Mod-97-10.
pub fn is_valid_kz_iban(raw: &str) -> bool {
    let cleaned: String = raw
        .chars()
        .filter(|c| c.is_ascii_alphanumeric())
        .map(|c| c.to_ascii_uppercase())
        .collect();
    if cleaned.len() != 20 || !cleaned.starts_with("KZ") {
        return false;
    }
    // Move first 4 chars to the end
    let rearranged = format!("{}{}", &cleaned[4..], &cleaned[..4]);
    let mut rem: u32 = 0;
    for ch in rearranged.bytes() {
        if ch.is_ascii_digit() {
            rem = (rem * 10 + (ch - b'0') as u32) % 97;
        } else if ch.is_ascii_uppercase() {
            let val = (ch - b'A' + 10) as u32;
            rem = (rem * 100 + val) % 97;
        } else {
            return false;
        }
    }
    rem == 1
}

struct BuiltinRegexes {
    twelve_digits: Regex,
    card_candidate: Regex,
    iban_kz: Regex,
    phone_kz: Regex,
    secrecy: Regex,
    passwords: Regex,
}

fn regexes() -> &'static BuiltinRegexes {
    static RE: OnceLock<BuiltinRegexes> = OnceLock::new();
    RE.get_or_init(|| BuiltinRegexes {
        twelve_digits: Regex::new(r"\b\d{12}\b").unwrap(),
        card_candidate: Regex::new(r"\b(?:\d[ -]?){15,18}\d\b").unwrap(),
        iban_kz: Regex::new(r"\bKZ\d{2}[A-Z0-9]{16}\b").unwrap(),
        // Matches +7 or 8 followed by 10 digits with optional spaces/dashes/parens;
        // avoids the broken `\b\+7` bug from the legacy Python code.
        phone_kz: Regex::new(
            r"(?:^|[^\dA-Za-z])(\+7|8)[\s\-]?\(?(7\d{2})\)?[\s\-]?\d{3}[\s\-]?\d{2}[\s\-]?\d{2}\b",
        )
        .unwrap(),
        secrecy: Regex::new(
            r"(?i)(?:совершенно\s+секретно|коммерческая\s+тайна|для\s+служебного\s+пользования|\bДСП\b|конфиденциально|құпия|қызметтік\s+пайдалану\s+үшін|confidential|strictly\s+confidential)",
        )
        .unwrap(),
        passwords: Regex::new(
            r"(?i)(?:-----BEGIN\s+(?:RSA\s+|EC\s+|OPENSSH\s+)?PRIVATE\s+KEY-----|AKIA[0-9A-Z]{16}|(?:password|пароль|secret_key|api_key)\s*[:=]\s*\S{6,})",
        )
        .unwrap(),
    })
}

/// Scans text using built-in high-precision KZ DLP detectors (with checksum validation).
pub fn scan_builtin_dlp(text: &str) -> Vec<DlpMatch> {
    if text.is_empty() {
        return Vec::new();
    }
    let re = regexes();
    let mut matches = Vec::new();

    // 1. IIN / BIN (with mod-11 validation)
    let mut found_iin = false;
    let mut found_bin = false;
    for m in re.twelve_digits.find_iter(text) {
        let s = m.as_str();
        if !found_iin && is_valid_kz_iin(s) {
            found_iin = true;
            matches.push(DlpMatch {
                rule_name: "ИИН РК (персональные данные)".to_string(),
                severity: Severity::High,
                action: "alert".to_string(),
                matched_sample: mask_digits_keep_ends(s, 2, 2),
            });
        } else if !found_bin && is_valid_kz_bin(s) {
            found_bin = true;
            matches.push(DlpMatch {
                rule_name: "БИН РК".to_string(),
                severity: Severity::Medium,
                action: "alert".to_string(),
                matched_sample: mask_digits_keep_ends(s, 2, 2),
            });
        }
    }

    // 2. Payment Cards (Luhn validated)
    for m in re.card_candidate.find_iter(text) {
        if is_valid_luhn_card(m.as_str()) {
            matches.push(DlpMatch {
                rule_name: "Номер банковской карты".to_string(),
                severity: Severity::Critical,
                action: "block".to_string(),
                matched_sample: mask_digits_keep_ends(m.as_str(), 4, 4),
            });
            break;
        }
    }

    // 3. KZ IBAN (Mod-97 validated)
    for m in re.iban_kz.find_iter(text) {
        if is_valid_kz_iban(m.as_str()) {
            matches.push(DlpMatch {
                rule_name: "IBAN (Казахстан)".to_string(),
                severity: Severity::High,
                action: "block".to_string(),
                matched_sample: mask_digits_keep_ends(m.as_str(), 4, 4),
            });
            break;
        }
    }

    // 4. Secrecy / Confidentiality labels
    if let Some(m) = re.secrecy.find(text) {
        matches.push(DlpMatch {
            rule_name: "Гриф конфиденциальности / ДСП".to_string(),
            severity: Severity::Critical,
            action: "block".to_string(),
            matched_sample: m.as_str().to_string(),
        });
    }

    // 5. Private keys / credentials
    if let Some(m) = re.passwords.find(text) {
        matches.push(DlpMatch {
            rule_name: "Учётные данные / приватный ключ".to_string(),
            severity: Severity::Critical,
            action: "block".to_string(),
            matched_sample: m.as_str().chars().take(24).collect(),
        });
    }

    // 6. KZ Phone numbers (low severity informational)
    if let Some(m) = re.phone_kz.find(text) {
        matches.push(DlpMatch {
            rule_name: "Телефон РК".to_string(),
            severity: Severity::Low,
            action: "log".to_string(),
            matched_sample: mask_digits_keep_ends(m.as_str().trim(), 2, 2),
        });
    }

    matches
}

fn mask_digits_keep_ends(s: &str, head: usize, tail: usize) -> String {
    let chars: Vec<char> = s.chars().collect();
    if chars.len() <= head + tail {
        return "*".repeat(chars.len());
    }
    let prefix: String = chars[..head].iter().collect();
    let suffix: String = chars[chars.len() - tail..].iter().collect();
    format!("{}****{}", prefix, suffix)
}

/// Masks sensitive PII in free text (for `viewer` role, Telegram/SIEM alerts, and audit logs).
pub fn mask_text(text: &str) -> String {
    if text.is_empty() {
        return String::new();
    }
    let re = regexes();
    let step1 = re.card_candidate.replace_all(text, |caps: &regex::Captures| {
        let s = caps.get(0).map(|m| m.as_str()).unwrap_or("");
        if is_valid_luhn_card(s) {
            mask_digits_keep_ends(s, 4, 4)
        } else {
            s.to_string()
        }
    });
    let step2 = re.iban_kz.replace_all(&step1, |caps: &regex::Captures| {
        let s = caps.get(0).map(|m| m.as_str()).unwrap_or("");
        if is_valid_kz_iban(s) {
            mask_digits_keep_ends(s, 4, 4)
        } else {
            s.to_string()
        }
    });
    let step3 = re.twelve_digits.replace_all(&step2, |caps: &regex::Captures| {
        let s = caps.get(0).map(|m| m.as_str()).unwrap_or("");
        if is_valid_kz_iin(s) || is_valid_kz_bin(s) {
            mask_digits_keep_ends(s, 2, 2)
        } else {
            s.to_string()
        }
    });
    let step4 = re.passwords.replace_all(&step3, "[REDACTED_SECRET]");
    let step5 = re.phone_kz.replace_all(&step4, |caps: &regex::Captures| {
        let full = caps.get(0).map(|m| m.as_str()).unwrap_or("");
        let trimmed = full.trim_start();
        let lead = &full[..full.len() - trimmed.len()];
        format!("{}{}", lead, mask_digits_keep_ends(trimmed, 2, 2))
    });
    step5.into_owned()
}

/// Recursively masks all string values inside a JSON object/array (prevents `incident.meta` PII leaks).
pub fn mask_json_value(val: &Value) -> Value {
    match val {
        Value::String(s) => Value::String(mask_text(s)),
        Value::Array(arr) => Value::Array(arr.iter().map(mask_json_value).collect()),
        Value::Object(map) => {
            let mut out = serde_json::Map::with_capacity(map.len());
            for (k, v) in map {
                // Redact obviously sensitive raw content keys entirely for viewers
                let lower = k.to_ascii_lowercase();
                if lower == "content" || lower == "content_snippet" || lower == "password" {
                    out.insert(k.clone(), Value::String(mask_text(&v.to_string())));
                } else {
                    out.insert(k.clone(), mask_json_value(v));
                }
            }
            Value::Object(out)
        }
        other => other.clone(),
    }
}

/// Neutralizes CSV/Excel formula injection (leading `=`, `+`, `-`, `@`, `\t`, `\r`).
pub fn sanitize_spreadsheet_cell(val: &str) -> String {
    if let Some(first) = val.chars().next() {
        if matches!(first, '=' | '+' | '-' | '@' | '\t' | '\r') {
            return format!("'{}", val);
        }
    }
    val.to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_valid_iin(prefix11: &str) -> String {
        let d: Vec<u32> = prefix11.bytes().map(|b| (b - b'0') as u32).collect();
        const W1: [u32; 11] = [1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11];
        const W2: [u32; 11] = [3, 4, 5, 6, 7, 8, 9, 10, 11, 1, 2];
        let mut c = d.iter().zip(W1).map(|(a, w)| a * w).sum::<u32>() % 11;
        if c == 10 {
            c = d.iter().zip(W2).map(|(a, w)| a * w).sum::<u32>() % 11;
        }
        format!("{}{}", prefix11, c)
    }

    #[test]
    fn kz_iin_checksum_filters_false_positives() {
        let valid = make_valid_iin("95051535012");
        assert!(is_valid_kz_iin(&valid));
        // Flip last digit -> invalid
        let mut invalid = valid.clone();
        let last = invalid.pop().unwrap();
        invalid.push(if last == '0' { '1' } else { '0' });
        assert!(!is_valid_kz_iin(&invalid));
        // Random timestamp-like 12 digits with month 99 -> invalid
        assert!(!is_valid_kz_iin("209912345678"));
    }

    #[test]
    fn kz_bin_checksum_works() {
        let valid_bin = make_valid_iin("18054001234");
        assert!(is_valid_kz_bin(&valid_bin));
        assert!(!is_valid_kz_bin("180510012340"));
    }

    #[test]
    fn luhn_card_validation_works() {
        assert!(is_valid_luhn_card("4111-1111-1111-1111"));
        assert!(!is_valid_luhn_card("4111-1111-1111-1112"));
        assert!(!is_valid_luhn_card("0000000000000000"));
    }

    #[test]
    fn kz_iban_mod97_works() {
        // Real format valid mod-97 KZ IBAN
        assert!(is_valid_kz_iban("KZ86125KZT5004100100"));
        assert!(!is_valid_kz_iban("KZ00125KZT5004100100"));
    }

    #[test]
    fn kz_phone_matches_plus7_and_8() {
        let m1 = scan_builtin_dlp("Звоните +7 (701) 555-12-34 срочно");
        assert!(m1.iter().any(|x| x.rule_name.contains("Телефон")));
        let m2 = scan_builtin_dlp("87771234567");
        assert!(m2.iter().any(|x| x.rule_name.contains("Телефон")));
    }

    #[test]
    fn masking_hides_pii_in_text_and_json() {
        let iin = make_valid_iin("95051535012");
        let txt = format!("ИИН {} карта 4111 1111 1111 1111", iin);
        let masked = mask_text(&txt);
        assert!(!masked.contains(&iin));
        assert!(!masked.contains("4111 1111 1111 1111"));

        let meta = serde_json::json!({ "content_snippet": txt, "nested": { "iin": iin } });
        let masked_meta = mask_json_value(&meta);
        assert!(!masked_meta.to_string().contains(&iin));
    }

    #[test]
    fn spreadsheet_formula_injection_is_neutralized() {
        assert_eq!(sanitize_spreadsheet_cell("=CMD|' /C calc'!A0"), "'=CMD|' /C calc'!A0");
        assert_eq!(sanitize_spreadsheet_cell("+123"), "'+123");
        assert_eq!(sanitize_spreadsheet_cell("normal text"), "normal text");
    }
}
