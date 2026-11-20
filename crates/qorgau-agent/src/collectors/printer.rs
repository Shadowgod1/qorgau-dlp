//! Native Win32 Print Spooler monitor (`EnumPrintersW` + `EnumJobsW`).
//! Zero WMI / COM dependencies! Tracks `(printer, job_id, document_name)` with TTL so reused JobIds are never dropped.

use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};

use serde_json::json;

use crate::transport::AgentTransport;

#[derive(Debug)]
struct PrintJobInfo {
    printer_name: String,
    job_id: u32,
    document_name: String,
    pages: u32,
    size_bytes: u64,
    status: String,
}

pub async fn run(transport: Arc<AgentTransport>, mut enabled: tokio::sync::watch::Receiver<bool>) {
    let mut seen: HashMap<String, Instant> = HashMap::new();
    let mut ticker = tokio::time::interval(Duration::from_secs(3));

    loop {
        ticker.tick().await;
        if !*enabled.borrow_and_update() {
            continue;
        }

        let now = Instant::now();
        seen.retain(|_, ts| now.duration_since(*ts) < Duration::from_secs(600));

        for job in enumerate_print_jobs() {
            let key = format!("{}:{}:{}", job.printer_name, job.job_id, job.document_name);
            if seen.contains_key(&key) {
                continue;
            }
            seen.insert(key, now);

            transport.enqueue(
                "print",
                json!({
                    "printer_name": job.printer_name,
                    "document_name": job.document_name,
                    "pages": job.pages,
                    "size_bytes": job.size_bytes,
                    "status": job.status,
                }),
            );
        }
    }
}

#[cfg(windows)]
fn enumerate_print_jobs() -> Vec<PrintJobInfo> {
    use windows::core::PCWSTR;
    use windows::Win32::Foundation::HANDLE;
    use windows::Win32::Graphics::Printing::{
        ClosePrinter, EnumJobsW, EnumPrintersW, OpenPrinterW, JOB_INFO_1W,
        PRINTER_ENUM_CONNECTIONS, PRINTER_ENUM_LOCAL, PRINTER_INFO_4W,
    };

    let mut out = Vec::new();
    let flags = PRINTER_ENUM_LOCAL | PRINTER_ENUM_CONNECTIONS;

    unsafe {
        let mut needed = 0u32;
        let mut returned = 0u32;
        let _ = EnumPrintersW(flags, None, 4, None, &mut needed, &mut returned);
        if needed == 0 {
            return out;
        }
        let mut buf = vec![0u8; needed as usize];
        if EnumPrintersW(flags, None, 4, Some(&mut buf), &mut needed, &mut returned).is_err() {
            return out;
        }

        let printers = std::slice::from_raw_parts(
            buf.as_ptr() as *const PRINTER_INFO_4W,
            returned as usize,
        );

        for p in printers {
            if p.pPrinterName.is_null() {
                continue;
            }
            let pname = p.pPrinterName.to_string().unwrap_or_default();
            let mut handle = HANDLE::default();
            if OpenPrinterW(PCWSTR(p.pPrinterName.0), &mut handle, None).is_err() {
                continue;
            }

            let mut j_needed = 0u32;
            let mut j_returned = 0u32;
            let _ = EnumJobsW(handle, 0, 100, 1, None, &mut j_needed, &mut j_returned);
            if j_needed > 0 {
                let mut jbuf = vec![0u8; j_needed as usize];
                if EnumJobsW(
                    handle,
                    0,
                    100,
                    1,
                    Some(&mut jbuf),
                    &mut j_needed,
                    &mut j_returned,
                )
                .is_ok()
                {
                    let jobs = std::slice::from_raw_parts(
                        jbuf.as_ptr() as *const JOB_INFO_1W,
                        j_returned as usize,
                    );
                    for j in jobs {
                        let doc = if j.pDocument.is_null() {
                            "Untitled".to_string()
                        } else {
                            j.pDocument.to_string().unwrap_or_else(|_| "Untitled".to_string())
                        };
                        let st = if j.pStatus.is_null() {
                            "Printing".to_string()
                        } else {
                            j.pStatus.to_string().unwrap_or_else(|_| "Printing".to_string())
                        };
                        out.push(PrintJobInfo {
                            printer_name: pname.clone(),
                            job_id: j.JobId,
                            document_name: doc,
                            pages: j.TotalPages.max(j.PagesPrinted),
                            size_bytes: 0,
                            status: st,
                        });
                    }
                }
            }
            let _ = ClosePrinter(handle);
        }
    }
    out
}

#[cfg(not(windows))]
fn enumerate_print_jobs() -> Vec<PrintJobInfo> {
    Vec::new()
}
