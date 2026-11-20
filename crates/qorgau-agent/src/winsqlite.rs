//! Minimal dynamic loader for Windows' built-in `C:\Windows\System32\winsqlite3.dll`.
//! Allows reading Chrome/Edge/Brave/Firefox SQLite history databases with zero C compilation
//! and zero binary size overhead.

#[cfg(windows)]
use std::ffi::{CStr, CString};
#[cfg(windows)]
use std::os::raw::{c_char, c_int, c_void};
#[cfg(windows)]
use std::path::Path;

#[cfg(windows)]
const SQLITE_OK: c_int = 0;
#[cfg(windows)]
const SQLITE_ROW: c_int = 100;
#[cfg(windows)]
const SQLITE_OPEN_READONLY: c_int = 0x0000_0001;

#[cfg(windows)]
type OpenV2Fn = unsafe extern "C" fn(*const c_char, *mut *mut c_void, c_int, *const c_char) -> c_int;
#[cfg(windows)]
type CloseV2Fn = unsafe extern "C" fn(*mut c_void) -> c_int;
#[cfg(windows)]
type PrepareV2Fn =
    unsafe extern "C" fn(*mut c_void, *const c_char, c_int, *mut *mut c_void, *mut *const c_char) -> c_int;
#[cfg(windows)]
type StepFn = unsafe extern "C" fn(*mut c_void) -> c_int;
#[cfg(windows)]
type FinalizeFn = unsafe extern "C" fn(*mut c_void) -> c_int;
#[cfg(windows)]
type ColumnInt64Fn = unsafe extern "C" fn(*mut c_void, c_int) -> i64;
#[cfg(windows)]
type ColumnTextFn = unsafe extern "C" fn(*mut c_void, c_int) -> *const c_char;

#[cfg(windows)]
pub fn query_history_rows(db_path: &Path, sql: &str) -> Vec<(i64, String)> {
    let mut rows = Vec::new();
    let Ok(lib) = (unsafe { libloading::Library::new("winsqlite3.dll") }) else {
        return rows;
    };

    unsafe {
        let Ok(open_v2) = lib.get::<OpenV2Fn>(b"sqlite3_open_v2") else { return rows };
        let Ok(close_v2) = lib.get::<CloseV2Fn>(b"sqlite3_close_v2") else { return rows };
        let Ok(prepare_v2) = lib.get::<PrepareV2Fn>(b"sqlite3_prepare_v2") else { return rows };
        let Ok(step) = lib.get::<StepFn>(b"sqlite3_step") else { return rows };
        let Ok(finalize) = lib.get::<FinalizeFn>(b"sqlite3_finalize") else { return rows };
        let Ok(col_i64) = lib.get::<ColumnInt64Fn>(b"sqlite3_column_int64") else { return rows };
        let Ok(col_text) = lib.get::<ColumnTextFn>(b"sqlite3_column_text") else { return rows };

        let Ok(c_path) = CString::new(db_path.to_string_lossy().as_bytes()) else {
            return rows;
        };
        let Ok(c_sql) = CString::new(sql) else {
            return rows;
        };

        let mut db: *mut c_void = std::ptr::null_mut();
        if open_v2(c_path.as_ptr(), &mut db, SQLITE_OPEN_READONLY, std::ptr::null()) != SQLITE_OK
            || db.is_null()
        {
            if !db.is_null() {
                close_v2(db);
            }
            return rows;
        }

        let mut stmt: *mut c_void = std::ptr::null_mut();
        if prepare_v2(db, c_sql.as_ptr(), -1, &mut stmt, std::ptr::null_mut()) == SQLITE_OK
            && !stmt.is_null()
        {
            while step(stmt) == SQLITE_ROW {
                let id = col_i64(stmt, 0);
                let ptr = col_text(stmt, 1);
                if !ptr.is_null() {
                    let url = CStr::from_ptr(ptr).to_string_lossy().into_owned();
                    rows.push((id, url));
                }
            }
            finalize(stmt);
        }

        close_v2(db);
    }

    rows
}

#[cfg(not(windows))]
pub fn query_history_rows(_db_path: &std::path::Path, _sql: &str) -> Vec<(i64, String)> {
    Vec::new()
}
