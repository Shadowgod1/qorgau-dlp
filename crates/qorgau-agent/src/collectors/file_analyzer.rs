use std::fs::File;
use std::io::Read;
use std::path::Path;

use quick_xml::events::Event;
use quick_xml::Reader;
use sha2::{Digest, Sha256};

const MAX_SNIPPET_CHARS: usize = 2000;
const MAX_ANALYZE_BYTES: u64 = 25 * 1024 * 1024;

#[derive(Debug, Clone)]
pub struct FileAnalysis {
    pub sha256: String,
    pub size_bytes: u64,
    pub text_snippet: String,
}

pub fn analyze_file(path: &Path) -> Option<FileAnalysis> {
    let meta = std::fs::metadata(path).ok()?;
    if !meta.is_file() {
        return None;
    }
    let size_bytes = meta.len();
    let sha256 = compute_sha256(path).unwrap_or_default();

    let text_snippet = if size_bytes > 0 && size_bytes <= MAX_ANALYZE_BYTES {
        extract_text(path).chars().take(MAX_SNIPPET_CHARS).collect()
    } else {
        String::new()
    };

    Some(FileAnalysis {
        sha256,
        size_bytes,
        text_snippet,
    })
}

pub fn compute_sha256(path: &Path) -> std::io::Result<String> {
    let mut f = File::open(path)?;
    let mut hasher = Sha256::new();
    let mut buf = [0u8; 65536];
    loop {
        let n = f.read(&mut buf)?;
        if n == 0 {
            break;
        }
        hasher.update(&buf[..n]);
    }
    Ok(hex::encode(hasher.finalize()))
}

pub fn extract_text(path: &Path) -> String {
    let ext = path
        .extension()
        .and_then(|s| s.to_str())
        .map(|s| s.to_ascii_lowercase())
        .unwrap_or_default();

    match ext.as_str() {
        "txt" | "csv" | "json" | "xml" | "log" | "md" | "ini" | "yaml" | "yml" | "sql" | "env" => {
            read_plain_text(path, 65536)
        }
        "docx" => extract_office_zip_xml(path, &["word/document.xml"]),
        "xlsx" => extract_office_zip_xml(path, &["xl/sharedStrings.xml"]),
        "pptx" => extract_pptx_text(path),
        "zip" => extract_zip_text(path),
        "pdf" => extract_pdf_ascii_text(path),
        _ => String::new(),
    }
}

fn read_plain_text(path: &Path, max_bytes: usize) -> String {
    let Ok(f) = File::open(path) else {
        return String::new();
    };
    let mut buf = Vec::with_capacity(max_bytes.min(16384));
    let _ = f.take(max_bytes as u64).read_to_end(&mut buf);
    String::from_utf8_lossy(&buf).into_owned()
}

fn extract_office_zip_xml(path: &Path, entry_names: &[&str]) -> String {
    let Ok(f) = File::open(path) else {
        return String::new();
    };
    let Ok(mut archive) = zip::ZipArchive::new(f) else {
        return String::new();
    };
    let mut out = String::new();
    for name in entry_names {
        if let Ok(mut entry) = archive.by_name(name) {
            let mut xml = String::new();
            let _ = entry.by_ref().take(512 * 1024).read_to_string(&mut xml);
            out.push_str(&strip_xml_tags(&xml));
            out.push('\n');
        }
    }
    out
}

fn extract_pptx_text(path: &Path) -> String {
    let Ok(f) = File::open(path) else {
        return String::new();
    };
    let Ok(mut archive) = zip::ZipArchive::new(f) else {
        return String::new();
    };
    let mut out = String::new();
    for i in 0..archive.len() {
        if out.len() >= MAX_SNIPPET_CHARS * 2 {
            break;
        }
        if let Ok(mut entry) = archive.by_index(i) {
            if entry.name().starts_with("ppt/slides/slide") && entry.name().ends_with(".xml") {
                let mut xml = String::new();
                let _ = entry.by_ref().take(128 * 1024).read_to_string(&mut xml);
                out.push_str(&strip_xml_tags(&xml));
                out.push('\n');
            }
        }
    }
    out
}

fn extract_zip_text(path: &Path) -> String {
    let Ok(f) = File::open(path) else {
        return String::new();
    };
    let Ok(mut archive) = zip::ZipArchive::new(f) else {
        return String::new();
    };
    let mut out = String::new();
    for i in 0..archive.len().min(30) {
        if out.len() >= MAX_SNIPPET_CHARS * 2 {
            break;
        }
        if let Ok(mut entry) = archive.by_index(i) {
            let name = entry.name().to_ascii_lowercase();
            if [".txt", ".csv", ".json", ".xml", ".log", ".sql"]
                .iter()
                .any(|e| name.ends_with(e))
            {
                let mut buf = Vec::new();
                let _ = entry.by_ref().take(32 * 1024).read_to_end(&mut buf);
                out.push_str(&String::from_utf8_lossy(&buf));
                out.push('\n');
            }
        }
    }
    out
}

/// Lightweight zero-dependency PDF text extraction for uncompressed literal text runs & metadata.
fn extract_pdf_ascii_text(path: &Path) -> String {
    let Ok(f) = File::open(path) else {
        return String::new();
    };
    let mut buf = Vec::new();
    let _ = f.take(512 * 1024).read_to_end(&mut buf);
    let raw = String::from_utf8_lossy(&buf);
    let mut out = String::new();
    let mut in_paren = false;
    let mut escaped = false;
    for ch in raw.chars() {
        if out.len() >= MAX_SNIPPET_CHARS * 2 {
            break;
        }
        if escaped {
            out.push(ch);
            escaped = false;
            continue;
        }
        match ch {
            '\\' if in_paren => escaped = true,
            '(' => in_paren = true,
            ')' => {
                in_paren = false;
                out.push(' ');
            }
            c if in_paren && !c.is_control() => out.push(c),
            _ => {}
        }
    }
    out
}

fn strip_xml_tags(xml: &str) -> String {
    let mut reader = Reader::from_str(xml);
    let mut out = String::new();
    loop {
        match reader.read_event() {
            Ok(Event::Text(e)) => {
                if let Ok(t) = e.unescape() {
                    let trimmed = t.trim();
                    if !trimmed.is_empty() {
                        out.push_str(trimmed);
                        out.push(' ');
                    }
                }
            }
            Ok(Event::Eof) | Err(_) => break,
            _ => {}
        }
        if out.len() >= MAX_SNIPPET_CHARS * 2 {
            break;
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    #[test]
    fn analyzes_text_and_docx_zip() {
        let dir = std::env::temp_dir().join(format!("qorgau_fa_{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();

        let txt_path = dir.join("secret.txt");
        std::fs::write(&txt_path, "Коммерческая тайна: 4111-1111-1111-1111").unwrap();
        let res = analyze_file(&txt_path).unwrap();
        assert_eq!(res.sha256.len(), 64);
        assert!(res.text_snippet.contains("Коммерческая тайна"));

        // Create a synthetic minimal .docx zip
        let docx_path = dir.join("doc.docx");
        let f = File::create(&docx_path).unwrap();
        let mut zw = zip::ZipWriter::new(f);
        zw.start_file("word/document.xml", zip::write::SimpleFileOptions::default())
            .unwrap();
        zw.write_all(b"<w:document><w:body><w:p><w:t>Secret DOCX text</w:t></w:p></w:body></w:document>")
            .unwrap();
        zw.finish().unwrap();

        let docx_res = analyze_file(&docx_path).unwrap();
        assert!(docx_res.text_snippet.contains("Secret DOCX text"));

        let _ = std::fs::remove_dir_all(&dir);
    }
}
