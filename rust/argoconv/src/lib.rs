//! 오피스 문서를 PDF 로 바꾸는 C 창구.
//!
//! 파일을 건드리지 않는다. 바이트를 받아 바이트를 돌려준다 —
//! tvOS 의 std 는 Tier 3 라 파일 메타데이터 쪽이 아직 비어 있고,
//! 그 빈 곳을 아예 지나가지 않기 위해서다. 읽고 쓰는 일은 Swift 가 한다.

mod hwp;

use std::panic;
use std::sync::Mutex;

pub const ARGOCONV_PANICKED: c_int = -3;

/// 마지막 패닉 메시지. 무엇이 왜 죽었는지 로그로 꺼내 보기 위한 것이다.
static LAST_PANIC: Mutex<String> = Mutex::new(String::new());

/// 패닉을 잡아 오류 코드로 바꾼다.
///
/// 변환기는 남이 만든 파서를 두 개 품고 있고, 그것들은 낯선 바이너리를
/// 읽는다. 어딘가에서 범위를 벗어나는 것은 시간문제다. 그때 앱이 죽는
/// 대신 코드를 돌려주면, Swift 가 조용히 기존 뷰어로 되돌아간다.
fn guard<F: FnOnce() -> c_int + panic::UnwindSafe>(work: F) -> c_int {
    let previous = panic::take_hook();
    panic::set_hook(Box::new(|info| {
        if let Ok(mut slot) = LAST_PANIC.lock() {
            *slot = info.to_string();
        }
    }));
    let outcome = panic::catch_unwind(work);
    panic::set_hook(previous);
    outcome.unwrap_or(ARGOCONV_PANICKED)
}

/// 마지막 패닉 메시지를 버퍼에 담는다. 담은 바이트 수를 돌려준다.
///
/// # Safety
/// `buffer`/`cap` 은 쓸 수 있는 버퍼여야 한다.
#[no_mangle]
pub unsafe extern "C" fn argoconv_last_error(buffer: *mut u8, cap: usize) -> usize {
    if buffer.is_null() || cap == 0 {
        return 0;
    }
    let Ok(message) = LAST_PANIC.lock() else {
        return 0;
    };
    let bytes = message.as_bytes();
    let n = bytes.len().min(cap);
    std::ptr::copy_nonoverlapping(bytes.as_ptr(), buffer, n);
    n
}

use std::os::raw::{c_char, c_int};
use std::slice;

/// 변환기가 무엇을 봤는지 한 줄. 기기 로그로 넘겨 눈으로 확인한다.
pub(crate) static LAST_NOTE: Mutex<String> = Mutex::new(String::new());

pub(crate) fn note(line: String) {
    if let Ok(mut slot) = LAST_NOTE.lock() {
        *slot = line;
    }
}

/// 마지막 변환이 남긴 한 줄을 버퍼에 담는다.
///
/// # Safety
/// `buffer`/`cap` 은 쓸 수 있는 버퍼여야 한다.
#[no_mangle]
pub unsafe extern "C" fn argoconv_last_note(buffer: *mut u8, cap: usize) -> usize {
    if buffer.is_null() || cap == 0 {
        return 0;
    }
    let Ok(line) = LAST_NOTE.lock() else {
        return 0;
    };
    let bytes = line.as_bytes();
    let n = bytes.len().min(cap);
    std::ptr::copy_nonoverlapping(bytes.as_ptr(), buffer, n);
    n
}

/// 0 성공. 음수는 실패 이유.
pub const ARGOCONV_OK: c_int = 0;
pub const ARGOCONV_BAD_ARGS: c_int = -1;
pub const ARGOCONV_CONVERT_FAILED: c_int = -2;

/// 한글 폰트를 등록한다.
///
/// tvOS 에는 fontconfig 가 없고 minipdf 의 시스템 폰트 탐색도 돌지 않는다.
/// 대신 Swift 가 /System/Library/Fonts/AppleSDGothicNeo.ttc 를 읽어
/// 이리로 넘긴다. 사용자가 폰트 폴더에 넣은 것도 같은 길로 들어온다.
///
/// # Safety
/// `name` 은 NUL 로 끝나는 문자열, `data`/`len` 은 유효한 버퍼여야 한다.
#[no_mangle]
pub unsafe extern "C" fn argoconv_register_font(
    name: *const c_char,
    data: *const u8,
    len: usize,
) -> c_int {
    if name.is_null() || data.is_null() || len == 0 {
        return ARGOCONV_BAD_ARGS;
    }
    let Ok(name) = std::ffi::CStr::from_ptr(name).to_str() else {
        return ARGOCONV_BAD_ARGS;
    };
    let bytes = slice::from_raw_parts(data, len).to_vec();
    // SVG 쪽(한글 문서)에도 같은 폰트가 필요합니다.
    if let Ok(mut fonts) = hwp::SVG_FONTS.lock() {
        fonts.push(bytes.clone());
    }
    minipdf::register_font(name.to_owned(), bytes);
    ARGOCONV_OK
}

/// docx · xlsx · pptx 를 PDF 로 바꾼다.
///
/// 성공하면 `out` 에 버퍼를, `out_len` 에 길이를 넣는다.
/// 그 버퍼는 반드시 `argoconv_free` 로 돌려줘야 한다.
///
/// # Safety
/// `input`/`len` 은 유효한 버퍼, `out`/`out_len` 은 쓸 수 있는 포인터여야 한다.
#[no_mangle]
pub unsafe extern "C" fn argoconv_to_pdf(
    input: *const u8,
    len: usize,
    out: *mut *mut u8,
    out_len: *mut usize,
) -> c_int {
    if input.is_null() || len == 0 || out.is_null() || out_len.is_null() {
        return ARGOCONV_BAD_ARGS;
    }
    *out = std::ptr::null_mut();
    *out_len = 0;

    let bytes = slice::from_raw_parts(input, len);
    let (out_ptr, out_size) = (out, out_len);
    guard(move || {
        // 시트는 한 쪽에 통째로 넣는다. 인쇄가 아니라 화면이므로 열이
        // 여러 쪽으로 쪼개지면 따라 읽을 수가 없다.
        let options = minipdf::ConversionOptions {
            fit_sheet_width: true,
            ..Default::default()
        };
        let Ok(pdf) = minipdf::convert_bytes_to_pdf_with_options(bytes, &options) else {
            return ARGOCONV_CONVERT_FAILED;
        };
        note(format!("office {} 바이트 PDF", pdf.len()));
        let mut boxed = pdf.into_boxed_slice();
        *out_size = boxed.len();
        *out_ptr = boxed.as_mut_ptr();
        std::mem::forget(boxed);
        ARGOCONV_OK
    })
}

/// `argoconv_to_pdf` 가 준 버퍼를 돌려준다.
///
/// # Safety
/// `argoconv_to_pdf` 가 준 포인터와 길이를 그대로 넘겨야 한다.
#[no_mangle]
pub unsafe extern "C" fn argoconv_free(ptr: *mut u8, len: usize) {
    if ptr.is_null() || len == 0 {
        return;
    }
    drop(Box::from_raw(slice::from_raw_parts_mut(ptr, len)));
}
