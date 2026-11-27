#![windows_subsystem = "windows"]
#![allow(unsafe_op_in_unsafe_fn)]

use std::ffi::c_void;
use std::mem::{size_of, transmute, zeroed};
use std::ptr::{null, null_mut};
use std::sync::Mutex;
use std::sync::atomic::{
    AtomicBool, AtomicI32, AtomicPtr, AtomicU16, AtomicU32, AtomicU64, AtomicUsize, Ordering,
};

use windows_sys::Win32::Foundation::{
    BOOL, COLORREF, CloseHandle, ERROR_ALREADY_EXISTS, ERROR_SUCCESS, FreeLibrary, GetLastError,
    HINSTANCE, HWND, LPARAM, LRESULT, POINT, RECT, SIZE, WPARAM,
};
use windows_sys::Win32::Graphics::Dwm::{
    DWM_WINDOW_CORNER_PREFERENCE, DWMWA_WINDOW_CORNER_PREFERENCE, DWMWCP_ROUND,
    DwmSetWindowAttribute,
};
use windows_sys::Win32::Graphics::Gdi::*;
use windows_sys::Win32::Security::{
    GetTokenInformation, TOKEN_ELEVATION, TOKEN_ELEVATION_TYPE, TOKEN_QUERY, TokenElevation,
    TokenElevationType, TokenElevationTypeDefault, TokenElevationTypeFull,
};
use windows_sys::Win32::System::LibraryLoader::{
    GetModuleFileNameW, GetModuleHandleW, GetProcAddress, LoadLibraryW,
};
use windows_sys::Win32::System::Registry::{
    HKEY_CURRENT_USER, KEY_SET_VALUE, REG_DWORD, REG_OPTION_NON_VOLATILE, RRF_RT_REG_DWORD,
    RegCloseKey, RegCreateKeyExW, RegGetValueW, RegSetValueExW,
};
use windows_sys::Win32::System::SystemInformation::GetTickCount;
use windows_sys::Win32::System::Threading::{
    CREATE_NO_WINDOW, CreateMutexW, CreateProcessW, GetCurrentProcess, GetExitCodeProcess,
    OpenProcessToken, PROCESS_INFORMATION, STARTUPINFOW, WaitForSingleObject,
};
use windows_sys::Win32::UI::Input::KeyboardAndMouse::{
    GetKeyboardLayout, GetKeyboardLayoutList, HKL, MOD_SHIFT, RegisterHotKey, ReleaseCapture,
    SetCapture, UnregisterHotKey, VK_CAPITAL, VK_F11,
};
use windows_sys::Win32::UI::Shell::{
    NIF_ICON, NIF_MESSAGE, NIF_TIP, NIM_ADD, NIM_DELETE, NIM_MODIFY, NOTIFYICONDATAW,
    Shell_NotifyIconW,
};
use windows_sys::Win32::UI::WindowsAndMessaging::*;

const ENGLISH: u16 = 0x0409;
const RUSSIAN: u16 = 0x0419;
const TRAY_MESSAGE: u32 = WM_APP + 1;
const SHOW_TRAY_MESSAGE: u32 = WM_APP + 2;
const SWITCH_LAYOUT_MESSAGE: u32 = WM_APP + 3;
const EXIT_ID: usize = 1002;
const HIDE_ID: usize = 1003;
const AUTOSTART_ID: usize = 1004;
const CAPS_DEFAULT_ID: usize = 1005;
const CAPS_SWITCH_ID: usize = 1006;
const FLAG_US_ID: usize = 1007;
const FLAG_UK_ID: usize = 1008;
const FLOATING_ID: usize = 1009;
const FLOATING_SIZE_75_ID: usize = 1010;
const FLOATING_SIZE_100_ID: usize = 1011;
const FLOATING_SIZE_150_ID: usize = 1012;
const FLOATING_SIZE_200_ID: usize = 1013;
const FLOATING_RESET_ID: usize = 1014;
const OSD_ID: usize = 1015;
const OSD_MONITOR_ALL_ID: usize = 1016;
const OSD_MONITOR_ACTIVE_ID: usize = 1018;
const OSD_CORNER_TOP_LEFT_ID: usize = 1100;
const OSD_CORNER_LAST_ID: usize = 1108;
const OSD_MODE_FLAGS_ID: usize = 1200;
const OSD_MODE_BOTH_ID: usize = 1202;
const OSD_SIZE_75_ID: usize = 1030;
const OSD_SIZE_LAST_ID: usize = 1033;
const OSD_DURATION_SLOW_ID: usize = 1040;
const OSD_CUSTOM_POS_ID: usize = 1050;
const OSD_RESET_POS_ID: usize = 1051;
const OSD_ANY_SWITCH_ID: usize = 1052;
const OSD_SHOW_MESSAGE: u32 = WM_APP + 4;
// Carries the language that was explicitly set (Caps Lock), so the OSD never
// has to guess it from a foreground window that may not have caught up yet.
const OSD_SWITCHED_MESSAGE: u32 = WM_APP + 5;
// Windows re-sends the same layout change; anything within this window is the
// same switch, not a second one.
const OSD_DUPLICATE_MS: u32 = 120;
// How long Windows may take to apply a layout change request.
const OSD_SWITCH_TIMEOUT_MS: u32 = 1000;
const OSD_PREVIEW_HOTKEY_ID: i32 = 1;
const OSD_FADE_TIMER: usize = 2;
const OSD_HIDE_TIMER: usize = 3;
const OSD_FADE_FRAME_MS: u32 = 16;
const OSD_FADE_STEPS: usize = 10;
const OSD_MONITOR_ALL: u16 = 0;
const OSD_MONITOR_CURSOR: u16 = 1;
const OSD_MONITOR_ACTIVE: u16 = 2;
const OSD_CORNER_TOP_LEFT: u16 = 0;
const OSD_CORNER_TOP_RIGHT: u16 = 1;
const OSD_CORNER_BOTTOM_LEFT: u16 = 2;
const OSD_CORNER_BOTTOM_RIGHT: u16 = 3;
const OSD_CORNER_BELOW_CENTER: u16 = 4;
const OSD_CORNER_ABOVE_CENTER: u16 = 5;
const OSD_CORNER_CENTER: u16 = 6;
const OSD_CORNER_TOP: u16 = 7;
const OSD_CORNER_BOTTOM: u16 = 8;
const OSD_MODE_FLAGS: u16 = 0;
const OSD_MODE_TEXT: u16 = 1;
const OSD_MODE_BOTH: u16 = 2;
const OSD_DURATION_FAST: u16 = 0;
const OSD_DURATION_NORMAL: u16 = 1;
const OSD_DURATION_SLOW: u16 = 2;
const MAX_OSD_WINDOWS: usize = 16;
const OSD_RADIUS: i32 = 16;
const OSD_FILL_ALPHA: u8 = 222;
const OSD_BORDER_ALPHA: u8 = 242;
const OSD_FONT_FAMILY: &str = "Segoe UI Variable Display";
const MAX_LAYOUTS: usize = 32;

static ENABLED: AtomicBool = AtomicBool::new(true);
static CAPS_DOWN: AtomicBool = AtomicBool::new(false);
static TRAY_VISIBLE: AtomicBool = AtomicBool::new(false);
static WINDOW: AtomicPtr<c_void> = AtomicPtr::new(null_mut());
static CURRENT_ICON: AtomicPtr<c_void> = AtomicPtr::new(null_mut());
static CURRENT_LANGUAGE: AtomicU16 = AtomicU16::new(0);
static ENGLISH_FLAG_SETTING: AtomicU16 = AtomicU16::new(0);
static FLOATING_VISIBLE: AtomicBool = AtomicBool::new(false);
static FLOAT_WINDOW: AtomicPtr<c_void> = AtomicPtr::new(null_mut());
static FLOATING_SIZE_PERCENT: AtomicU16 = AtomicU16::new(100);
static DRAGGING: AtomicBool = AtomicBool::new(false);
static DRAG_OFFSET_X: AtomicI32 = AtomicI32::new(0);
static DRAG_OFFSET_Y: AtomicI32 = AtomicI32::new(0);
static OSD_ENABLED: AtomicBool = AtomicBool::new(true);
static OSD_MONITOR: AtomicU16 = AtomicU16::new(OSD_MONITOR_CURSOR);
static OSD_CORNER: AtomicU16 = AtomicU16::new(OSD_CORNER_BELOW_CENTER);
static OSD_MODE: AtomicU16 = AtomicU16::new(OSD_MODE_BOTH);
static OSD_SIZE_PERCENT: AtomicU16 = AtomicU16::new(100);
static OSD_DURATION: AtomicU16 = AtomicU16::new(OSD_DURATION_NORMAL);
static OSD_WINDOWS: [AtomicPtr<c_void>; MAX_OSD_WINDOWS] =
    [const { AtomicPtr::new(null_mut()) }; MAX_OSD_WINDOWS];
// The language each OSD window last rendered, so a repaint never mixes the
// windows up and an unchanged frame is not redrawn.
static OSD_WINDOW_LANGUAGE: [AtomicU16; MAX_OSD_WINDOWS] =
    [const { AtomicU16::new(0) }; MAX_OSD_WINDOWS];
static OSD_WINDOW_GEOMETRY: [AtomicU64; MAX_OSD_WINDOWS] =
    [const { AtomicU64::new(0) }; MAX_OSD_WINDOWS];
// Tracks the HRGN set on each OSD window so the previous region can be
// deleted before SetWindowRgn replaces it (Win32 does not free the old one).
static OSD_REGIONS: [AtomicPtr<c_void>; MAX_OSD_WINDOWS] =
    [const { AtomicPtr::new(null_mut()) }; MAX_OSD_WINDOWS];
static OSD_LANGUAGE: AtomicU16 = AtomicU16::new(0);
static OSD_VISIBLE: AtomicBool = AtomicBool::new(false);
static OSD_OPACITY: AtomicU16 = AtomicU16::new(255);
static OSD_FADE_STEP: AtomicUsize = AtomicUsize::new(0);
static OSD_FADING: AtomicBool = AtomicBool::new(false);
// Debounce for the layout-change messages. Windows reports one layout switch
// more than once; a message closer than this to the previous one is the same
// switch being described twice, not a new one.
static OSD_LAST_SWITCH_MS: AtomicU32 = AtomicU32::new(0);
static OSD_LAST_MESSAGE_MS: AtomicU32 = AtomicU32::new(0);
// Drag state per OSD window for manual repositioning.
static OSD_DRAGGING: [AtomicBool; MAX_OSD_WINDOWS] =
    [const { AtomicBool::new(false) }; MAX_OSD_WINDOWS];
static OSD_DRAG_OFFSET_X: [AtomicI32; MAX_OSD_WINDOWS] =
    [const { AtomicI32::new(0) }; MAX_OSD_WINDOWS];
static OSD_DRAG_OFFSET_Y: [AtomicI32; MAX_OSD_WINDOWS] =
    [const { AtomicI32::new(0) }; MAX_OSD_WINDOWS];
// Saved custom position per OSD window (i32::MIN means "not set / auto").
static OSD_CUSTOM_X: [AtomicI32; MAX_OSD_WINDOWS] =
    [const { AtomicI32::new(i32::MIN) }; MAX_OSD_WINDOWS];
static OSD_CUSTOM_Y: [AtomicI32; MAX_OSD_WINDOWS] =
    [const { AtomicI32::new(i32::MIN) }; MAX_OSD_WINDOWS];
static OSD_CUSTOM_ENABLED: AtomicBool = AtomicBool::new(false);
// Show OSD on any layout change (Alt+Shift, Win+Space, …), not only Caps Lock.
static OSD_ANY_SWITCH: AtomicBool = AtomicBool::new(true);
// The layout this app has just asked Windows to apply, and until when that
// request may still be in flight. While a switch is pending the foreground
// layout still reports the OLD language; reading it and switching again would
// compute the same target and undo the previous switch, which is the "layout
// flips back" bug. The guard also tells the poller to trust this value instead
// of the stale foreground layout.
static PENDING_LANGUAGE: AtomicU16 = AtomicU16::new(0);
static PENDING_UNTIL_MS: AtomicU32 = AtomicU32::new(0);
// The foreground layout the poller last observed, so an external switch is
// detected even while the OSD is hidden (OSD_LANGUAGE is reset on hide).
static LAST_SEEN_LANGUAGE: AtomicU16 = AtomicU16::new(0);

fn next_language(current: u16, available: &[u16]) -> Option<u16> {
    let target = if current == RUSSIAN { ENGLISH } else { RUSSIAN };
    available.contains(&target).then_some(target)
}

/// The language a switch should start from. While a switch is in flight the
/// foreground window may still report the old layout; in that case base the
/// next switch on the pending target. Returns the effective current language
/// and whether the pending value was used.
fn switch_start_language(now: u32, foreground: u16, pending: u16, pending_until: u32) -> (u16, bool) {
    if pending != 0 && reference_is_in_future(now, pending_until) && pending_until != now {
        (pending, true)
    } else {
        (foreground, false)
    }
}

/// Records that a switch to `target` is in flight until `until`.
fn mark_switch_pending(target: u16, until: u32) {
    PENDING_LANGUAGE.store(target, Ordering::Relaxed);
    PENDING_UNTIL_MS.store(until, Ordering::Relaxed);
}

/// A switch is currently in flight (Windows has not applied it yet).
fn switch_pending(now: u32) -> bool {
    let pending = PENDING_LANGUAGE.load(Ordering::Relaxed);
    pending != 0 && reference_is_in_future(now, PENDING_UNTIL_MS.load(Ordering::Relaxed))
}

/// The language the OSD should show right now: the pending target while a
/// switch is in flight, otherwise the foreground layout.
fn display_language(now: u32, foreground: u16) -> u16 {
    if switch_pending(now) {
        PENDING_LANGUAGE.load(Ordering::Relaxed)
    } else {
        foreground
    }
}

/// True when a Caps Lock press must be ignored because Windows has not applied
/// the previous request yet AND the foreground layout still differs from the
/// pending target. As soon as the layout already equals the target the request
/// has landed, so the next press is a normal switch - not a blocked one.
fn should_ignore_press(now: u32, foreground: u16, pending: u16, pending_until: u32) -> bool {
    pending != 0
        && reference_is_in_future(now, pending_until)
        && pending_until != now
        && flag_language(foreground) != flag_language(pending)
}

/// Milliseconds between two tick counts, correct across the 32-bit wrap.
fn elapsed_ms(now: u32, since: u32) -> u32 {
    now.wrapping_sub(since)
}

/// Records the moment Caps Lock asked for the layout change. Layout-change
/// messages arriving while that call is still running - and shortly after -
/// are the same switch echoed back by Windows / the target application.
fn mark_layout_switched(now: u32) {
    OSD_LAST_SWITCH_MS.store(now.max(1), Ordering::Relaxed);
}

/// Called once the switch call returned: extends the echo window by the time
/// the call actually took, so a slow application cannot echo after it.
fn complete_layout_switch(started: u32) {
    let spent = elapsed_ms(now_ms(), started);
    OSD_LAST_SWITCH_MS.store(
        now_ms() + spent.min(OSD_SWITCH_TIMEOUT_MS) + OSD_DUPLICATE_MS,
        Ordering::Relaxed,
    );
}

/// True when `now` is the first report of a layout change: it is not the echo
/// of the Caps Lock switch and not a duplicate of an already handled message.
fn is_new_layout_report(now: u32) -> bool {
    let reference = OSD_LAST_SWITCH_MS.load(Ordering::Relaxed);
    // While the switch is in flight the reference is in the future; that gap
    // is unsigned-wrapped, so compare against zero explicitly.
    if reference_is_in_future(now, reference) {
        return false;
    }
    should_show_for_report(now, reference, OSD_LAST_MESSAGE_MS.load(Ordering::Relaxed))
}

/// True when `reference` lies ahead of `now`. The subtraction is done in the
/// wrapping 32-bit tick space and read as signed, which handles the counter
/// rollover correctly and keeps the in-flight check testable on its own.
fn reference_is_in_future(now: u32, reference: u32) -> bool {
    (now.wrapping_sub(reference) as i32) < 0
}

/// The rule behind `is_new_layout_report`, kept free of globals so it can be
/// tested: a report is new only when it is clearly later than the Caps Lock
/// switch itself and than the previous report that was already handled.
fn should_show_for_report(now: u32, last_switch: u32, last_report: u32) -> bool {
    if last_switch != 0 && elapsed_ms(now, last_switch) < OSD_DUPLICATE_MS {
        return false;
    }
    elapsed_ms(now, last_report) >= OSD_DUPLICATE_MS
}

fn should_switch(is_key_down: bool, was_down: bool) -> bool {
    is_key_down && !was_down
}

fn flag_language(language: u16) -> u16 {
    if language == RUSSIAN {
        RUSSIAN
    } else {
        ENGLISH
    }
}

fn wide(text: &str) -> Vec<u16> {
    text.encode_utf16().chain(Some(0)).collect()
}

// Cyrillic text is generated at runtime from UTF-16 code points so the file
// source transport cannot damage it.
const D0: &str = "\u{0420}\u{0423}\u{0421}";
const D1: &str = "\u{0420}\u{0443}\u{0441}\u{0441}\u{043a}\u{0438}\u{0439}";

// Flag colours, replaced magic numbers with named constants.
const COLOR_RUSSIAN_WHITE: COLORREF = 0x00ff_ffff;
const COLOR_RUSSIAN_BLUE: COLORREF = 0x00a6_3900;
const COLOR_RUSSIAN_RED: COLORREF = 0x001e_2bd5;
const COLOR_UK_BG: COLORREF = 0x0069_2101;
const COLOR_UK_WHITE: COLORREF = 0x00ff_ffff;
const COLOR_UK_RED: COLORREF = 0x002e_10c8;
const COLOR_US_STRIPE_DARK: COLORREF = 0x0034_22b2;
const COLOR_US_STRIPE_LIGHT: COLORREF = 0x00ff_ffff;
const COLOR_US_CANTON: COLORREF = 0x006e_3b3c;
const COLOR_US_STAR: COLORREF = 0x00ff_ffff;

// Cached flag icons so draw_flag does not recreate GDI brushes, pens and
// bitmaps on every OSD repaint. Index 0: EN (US), index 1: EN (UK), index 2: RU.
static FLAG_ICONS: [AtomicPtr<c_void>; 3] = [const { AtomicPtr::new(null_mut()) }; 3];

fn flag_icon_index(language: u16) -> usize {
    if language == RUSSIAN {
        2
    } else if is_uk_flag(ENGLISH_FLAG_SETTING.load(Ordering::Relaxed) as u32) {
        1
    } else {
        0
    }
}

/// Returns a cached flag icon, creating it on first use. The caller must not
/// DestroyIcon the result; it is owned by the cache and cleaned up when the
/// flag setting changes or the process exits.
unsafe fn cached_flag_icon(language: u16) -> HICON {
    let index = flag_icon_index(language);
    let cached = FLAG_ICONS[index].load(Ordering::Relaxed) as HICON;
    if !cached.is_null() {
        return cached;
    }
    let icon = create_flag_icon(language);
    FLAG_ICONS[index].store(icon, Ordering::Relaxed);
    icon
}

/// Drops every cached flag icon. Call when the English flag setting changes so
/// the UK/US icons are regenerated from the new artwork.
unsafe fn drop_cached_flag_icons() {
    for slot in &FLAG_ICONS {
        let icon = slot.swap(null_mut(), Ordering::Relaxed) as HICON;
        if !icon.is_null() {
            DestroyIcon(icon);
        }
    }
}

fn osd_short_name(language: u16) -> &'static str {
    if language == RUSSIAN { D0 } else { "ENG" }
}

fn osd_full_name(language: u16) -> &'static str {
    if language == RUSSIAN { D1 } else { "English" }
}

/// OSD width for the content mode at the chosen scale. The window is always
/// three times wider than it is high.
fn osd_width_for(mode: u16, size_percent: u16) -> i32 {
    (osd_window_size(mode) * size_percent as i32 / 100).max(60)
}

/// How long the OSD stays on screen before it fades out.
fn osd_display_ms(duration: u16) -> u32 {
    match duration {
        OSD_DURATION_FAST => 800,
        OSD_DURATION_SLOW => 3000,
        _ => 1500,
    }
}

fn valid_osd_duration(value: u16) -> u16 {
    match value {
        OSD_DURATION_FAST | OSD_DURATION_NORMAL | OSD_DURATION_SLOW => value,
        _ => OSD_DURATION_NORMAL,
    }
}

fn valid_osd_size(value: u16) -> u16 {
    if matches!(value, 75 | 100 | 125 | 150) {
        value
    } else {
        100
    }
}

fn osd_size_options() -> [(&'static str, u16); 4] {
    [("75%", 75), ("100%", 100), ("125%", 125), ("150%", 150)]
}

fn osd_duration_options() -> [(&'static str, u16); 3] {
    [
        ("Быстрое исчезновение", OSD_DURATION_FAST),
        ("Обычное исчезновение", OSD_DURATION_NORMAL),
        ("Медленное исчезновение", OSD_DURATION_SLOW),
    ]
}

fn osd_size_id(percent: u16) -> usize {
    OSD_SIZE_75_ID
        + osd_size_options()
            .iter()
            .position(|(_, value)| *value == percent)
            .unwrap_or(1)
}

fn osd_size_from_id(id: usize) -> Option<u16> {
    (OSD_SIZE_75_ID..=OSD_SIZE_LAST_ID)
        .contains(&id)
        .then(|| osd_size_options()[(id - OSD_SIZE_75_ID).min(3)].1)
}

fn osd_duration_id(duration: u16) -> usize {
    osd_duration_options()
        .iter()
        .position(|(_, value)| *value == duration)
        .map(|index| OSD_DURATION_SLOW_ID + index)
        .unwrap_or(OSD_DURATION_SLOW_ID + 1)
}

fn osd_duration_from_id(id: usize) -> Option<u16> {
    let options = osd_duration_options();
    (OSD_DURATION_SLOW_ID..OSD_DURATION_SLOW_ID + options.len())
        .contains(&id)
        .then(|| options[id - OSD_DURATION_SLOW_ID].1)
}

fn valid_osd_monitor_mode(value: u16) -> u16 {
    match value {
        OSD_MONITOR_ALL | OSD_MONITOR_CURSOR | OSD_MONITOR_ACTIVE => value,
        _ => OSD_MONITOR_CURSOR,
    }
}

fn valid_osd_corner(value: u16) -> u16 {
    if value <= OSD_CORNER_BOTTOM {
        value
    } else {
        OSD_CORNER_BELOW_CENTER
    }
}

fn valid_osd_mode(value: u16) -> u16 {
    if value <= OSD_MODE_BOTH {
        value
    } else {
        OSD_MODE_BOTH
    }
}

fn osd_shows_flags(mode: u16) -> bool {
    mode == OSD_MODE_FLAGS || mode == OSD_MODE_BOTH
}

fn osd_shows_text(mode: u16) -> bool {
    mode == OSD_MODE_TEXT || mode == OSD_MODE_BOTH
}

/// Opacity for fade step `step`: the first step is still visible and the
/// schedule always ends at 0, so the OSD is fully hidden.
fn fade_opacity(step: usize) -> u16 {
    if step >= OSD_FADE_STEPS {
        0
    } else {
        255 - (255 * step / OSD_FADE_STEPS) as u16
    }
}

fn osd_monitor_options() -> [(&'static str, u16); 3] {
    [
        ("Все мониторы", OSD_MONITOR_ALL),
        ("Монитор с курсором", OSD_MONITOR_CURSOR),
        ("Монитор активного окна", OSD_MONITOR_ACTIVE),
    ]
}

fn osd_corner_options() -> [(&'static str, u16); 8] {
    [
        ("Центр экрана", OSD_CORNER_CENTER),
        ("Ниже центра", OSD_CORNER_BELOW_CENTER),
        ("Выше центра", OSD_CORNER_ABOVE_CENTER),
        ("Сверху", OSD_CORNER_TOP),
        ("Снизу", OSD_CORNER_BOTTOM),
        ("Сверху слева", OSD_CORNER_TOP_LEFT),
        ("Сверху справа", OSD_CORNER_TOP_RIGHT),
        ("Снизу справа", OSD_CORNER_BOTTOM_RIGHT),
    ]
}

fn osd_mode_options() -> [(&'static str, u16); 3] {
    [
        ("Флаг", OSD_MODE_FLAGS),
        ("Текст", OSD_MODE_TEXT),
        ("Флаг + текст", OSD_MODE_BOTH),
    ]
}

fn osd_corner_id(corner: u16) -> usize {
    OSD_CORNER_TOP_LEFT_ID + corner as usize
}

fn osd_corner_from_id(id: usize) -> Option<u16> {
    (OSD_CORNER_TOP_LEFT_ID..=OSD_CORNER_LAST_ID)
        .contains(&id)
        .then(|| (id - OSD_CORNER_TOP_LEFT_ID) as u16)
}

fn osd_monitor_id(mode: u16) -> usize {
    OSD_MONITOR_ALL_ID + mode as usize
}

fn osd_mode_id(mode: u16) -> usize {
    OSD_MODE_FLAGS_ID + mode as usize
}

fn osd_mode_from_id(id: usize) -> Option<u16> {
    (OSD_MODE_FLAGS_ID..=OSD_MODE_BOTH_ID)
        .contains(&id)
        .then(|| (id - OSD_MODE_FLAGS_ID) as u16)
}

fn osd_monitor_from_id(id: usize) -> Option<u16> {
    (OSD_MONITOR_ALL_ID..=OSD_MONITOR_ACTIVE_ID)
        .contains(&id)
        .then(|| (id - OSD_MONITOR_ALL_ID) as u16)
}

fn osd_label(enabled: bool) -> &'static str {
    if enabled {
        "Отключить OSD языка"
    } else {
        "Включить OSD языка"
    }
}

fn osd_window_size(mode: u16) -> i32 {
    match mode {
        OSD_MODE_TEXT => 176,
        OSD_MODE_FLAGS => 128,
        _ => 240,
    }
}

unsafe fn foreground_language() -> u16 {
    let foreground = GetForegroundWindow();
    if foreground.is_null() {
        return ENGLISH;
    }
    let thread = GetWindowThreadProcessId(foreground, null_mut());
    if thread == 0 {
        return ENGLISH;
    }
    (GetKeyboardLayout(thread) as usize & 0xffff) as u16
}

unsafe fn switch_layout() {
    let foreground = GetForegroundWindow();
    if foreground.is_null() {
        return;
    }

    let mut layouts: [HKL; MAX_LAYOUTS] = [null_mut(); MAX_LAYOUTS];
    let received = GetKeyboardLayoutList(MAX_LAYOUTS as i32, layouts.as_mut_ptr());
    if received <= 0 {
        return;
    }
    let count = received as usize;
    let mut languages = [0u16; MAX_LAYOUTS];
    for index in 0..count {
        languages[index] = (layouts[index] as usize & 0xffff) as u16;
    }

    let thread = GetWindowThreadProcessId(foreground, null_mut());
    let foreground_language_now = if thread == 0 {
        ENGLISH
    } else {
        (GetKeyboardLayout(thread) as usize & 0xffff) as u16
    };
    let now = now_ms();
    let pending = PENDING_LANGUAGE.load(Ordering::Relaxed);
    let pending_until = PENDING_UNTIL_MS.load(Ordering::Relaxed);
    // The previous request already landed (the window reports its target):
    // clear the pending marker so this press switches normally instead of
    // being blocked. This is what made Caps Lock dead while the OSD was up.
    if pending != 0 && flag_language(foreground_language_now) == flag_language(pending) {
        mark_switch_pending(0, 0);
    }
    // Only ignore a press when the previous request genuinely has not applied
    // yet. Once the layout matches the pending target, switching continues.
    if should_ignore_press(now, foreground_language_now, pending, pending_until) {
        return;
    }
    let (current, _was_pending) = switch_start_language(
        now,
        foreground_language_now,
        PENDING_LANGUAGE.load(Ordering::Relaxed),
        PENDING_UNTIL_MS.load(Ordering::Relaxed),
    );
    if let Some(target) = next_language(current, &languages[..count]) {
        let handle = layouts[..count]
            .iter()
            .copied()
            .find(|layout| (*layout as usize & 0xffff) as u16 == target);
        if let Some(handle) = handle {
            let owner = GetWindow(foreground, GW_OWNER);
            let destination = if owner.is_null() { foreground } else { owner };
            let started = now_ms();
            // The request is sent synchronously below; the pending marker only
            // bridges the short gap before the window reports the new layout.
            // A press that arrives after the layout already changed clears it
            // (see should_ignore_press), so this never blocks normal switching.
            mark_switch_pending(target, started + OSD_SWITCH_TIMEOUT_MS);
            mark_layout_switched(started);
            let applied = SendMessageTimeoutW(
                destination,
                WM_INPUTLANGCHANGEREQUEST,
                0,
                handle as isize,
                SMTO_ABORTIFHUNG,
                OSD_SWITCH_TIMEOUT_MS,
                null_mut(),
            );
            complete_layout_switch(started);
            // Ask the foreground window what it now has. Some applications
            // answer the request but do not apply it (or Windows aborts the
            // request when the target hung), so announcing `target` blindly
            // made the tray and OSD show a language the window never got,
            // while the real layout stayed put. Trust the window, not the
            // request.
            let applied_language = (applied != 0)
                .then(|| {
                    let thread_now = GetWindowThreadProcessId(foreground, null_mut());
                    (thread_now != 0)
                        .then(|| (GetKeyboardLayout(thread_now) as usize & 0xffff) as u16)
                })
                .flatten();
            let announced = match applied_language {
                // The window reports the target: the switch really landed, so
                // the pending marker is no longer needed and must not block the
                // next Caps Lock press.
                Some(actual) if flag_language(actual) == flag_language(target) => {
                    mark_switch_pending(0, 0);
                    target
                }
                // The window still reports the old layout: the switch did not
                // apply, so do not claim it did. End the pending window now so
                // the poller settles on the real layout.
                Some(actual) => {
                    mark_switch_pending(0, 0);
                    actual
                }
                None => {
                    // No reliable answer: keep the pending target, the poller
                    // corrects it within its timeout if Windows did not apply.
                    target
                }
            };
            if announced == target {
                set_flag(WINDOW.load(Ordering::Relaxed), target);
                let main_window = WINDOW.load(Ordering::Relaxed);
                if !main_window.is_null() {
                    PostMessageW(main_window, OSD_SWITCHED_MESSAGE, target as usize, 0);
                }
            } else {
                // The switch failed; refresh the flag to the real layout only.
                set_flag(WINDOW.load(Ordering::Relaxed), announced);
            }
        }
    }
}

unsafe extern "system" fn keyboard_hook(code: i32, event: WPARAM, data: LPARAM) -> LRESULT {
    if code == HC_ACTION as i32 && ENABLED.load(Ordering::Relaxed) {
        let keyboard = &*(data as *const KBDLLHOOKSTRUCT);
        if keyboard.vkCode == VK_CAPITAL as u32 {
            let is_down = event == WM_KEYDOWN as usize || event == WM_SYSKEYDOWN as usize;
            let is_up = event == WM_KEYUP as usize || event == WM_SYSKEYUP as usize;
            if is_down || is_up {
                let was_down = CAPS_DOWN.swap(is_down, Ordering::Relaxed);
                if should_switch(is_down, was_down) {
                    PostMessageW(WINDOW.load(Ordering::Relaxed), SWITCH_LAYOUT_MESSAGE, 0, 0);
                }
                return 1;
            }
        }
    }
    CallNextHookEx(null_mut(), code, event, data)
}

unsafe fn fill_rect(dc: HDC, left: i32, top: i32, right: i32, bottom: i32, color: COLORREF) {
    let brush = CreateSolidBrush(color);
    let rect = RECT {
        left,
        top,
        right,
        bottom,
    };
    FillRect(dc, &rect, brush);
    DeleteObject(brush);
}

unsafe fn draw_line(dc: HDC, x1: i32, y1: i32, x2: i32, y2: i32, width: i32, color: COLORREF) {
    let pen = CreatePen(PS_SOLID, width, color);
    let previous = SelectObject(dc, pen);
    MoveToEx(dc, x1, y1, null_mut());
    LineTo(dc, x2, y2);
    SelectObject(dc, previous);
    DeleteObject(pen);
}

unsafe fn create_flag_icon(language: u16) -> HICON {
    let width = flag_width(GetSystemMetrics(SM_CXICON));
    let height = GetSystemMetrics(SM_CYICON);
    let screen = GetDC(null_mut());
    if screen.is_null() {
        return null_mut();
    }
    let dc = CreateCompatibleDC(screen);
    if dc.is_null() {
        ReleaseDC(null_mut(), screen);
        return null_mut();
    }
    let color = CreateCompatibleBitmap(screen, width, height);
    let mask = CreateBitmap(width, height, 1, 1, null());
    ReleaseDC(null_mut(), screen);
    if color.is_null() || mask.is_null() {
        if !color.is_null() {
            DeleteObject(color);
        }
        if !mask.is_null() {
            DeleteObject(mask);
        }
        DeleteDC(dc);
        return null_mut();
    }
    SelectObject(dc, color);

    if flag_language(language) == RUSSIAN {
        fill_rect(dc, 0, 0, width, height / 3, COLOR_RUSSIAN_WHITE);
        fill_rect(dc, 0, height / 3, width, height * 2 / 3, COLOR_RUSSIAN_BLUE);
        fill_rect(dc, 0, height * 2 / 3, width, height, COLOR_RUSSIAN_RED);
    } else if is_uk_flag(ENGLISH_FLAG_SETTING.load(Ordering::Relaxed) as u32) {
        fill_rect(dc, 0, 0, width, height, COLOR_UK_BG);
        let diagonal_white = (width / 6).max(2);
        let diagonal_red = (width / 12).max(1);
        for (x1, y1, x2, y2) in [(0, 0, width, height), (width, 0, 0, height)] {
            draw_line(dc, x1, y1, x2, y2, diagonal_white, COLOR_UK_WHITE);
            draw_line(dc, x1, y1, x2, y2, diagonal_red, COLOR_UK_RED);
        }
        fill_rect(dc, width * 3 / 8, 0, width * 5 / 8, height, COLOR_UK_WHITE);
        fill_rect(dc, 0, height * 3 / 8, width, height * 5 / 8, COLOR_UK_WHITE);
        fill_rect(dc, width * 7 / 16, 0, width * 9 / 16, height, COLOR_UK_RED);
        fill_rect(dc, 0, height * 7 / 16, width, height * 9 / 16, COLOR_UK_RED);
    } else {
        let stripe_height = (height + 12) / 13;
        for stripe in 0..13 {
            let color = if stripe % 2 == 0 {
                COLOR_US_STRIPE_DARK
            } else {
                COLOR_US_STRIPE_LIGHT
            };
            fill_rect(
                dc,
                0,
                stripe * stripe_height,
                width,
                ((stripe + 1) * stripe_height).min(height),
                color,
            );
        }
        let canton_width = width * 2 / 5;
        let canton_height = stripe_height * 7;
        fill_rect(dc, 0, 0, canton_width, canton_height, COLOR_US_CANTON);
        for row in 0..4 {
            for column in 0..5 {
                let x = 2 + column * (canton_width - 4) / 5;
                let y = 2 + row * (canton_height - 4) / 4;
                fill_rect(dc, x, y, x + 1, y + 1, COLOR_US_STAR);
            }
        }
    }

    let mut info: ICONINFO = zeroed();
    info.fIcon = 1;
    info.hbmMask = mask;
    info.hbmColor = color;
    let icon = CreateIconIndirect(&info);
    // Always release the temporary bitmaps, even when icon creation fails.
    DeleteObject(color);
    DeleteObject(mask);
    DeleteDC(dc);
    icon
}

unsafe fn tray_icon(window: HWND, icon: HICON) -> NOTIFYICONDATAW {
    let mut data: NOTIFYICONDATAW = zeroed();
    data.cbSize = size_of::<NOTIFYICONDATAW>() as u32;
    data.hWnd = window;
    data.uID = 1;
    data.uFlags = NIF_MESSAGE | NIF_ICON | NIF_TIP;
    data.uCallbackMessage = TRAY_MESSAGE;
    data.hIcon = icon;
    let tip = wide("CapsLock Layout: RU <-> EN");
    data.szTip[..tip.len()].copy_from_slice(&tip);
    data
}

unsafe fn set_flag(window: HWND, language: u16) {
    let language = flag_language(language);
    if window.is_null()
        || (CURRENT_LANGUAGE.load(Ordering::Relaxed) == language
            && !CURRENT_ICON.load(Ordering::Relaxed).is_null())
    {
        return;
    }
    let icon = create_flag_icon(language);
    if icon.is_null() {
        return;
    }
    if TRAY_VISIBLE.load(Ordering::Relaxed) {
        Shell_NotifyIconW(NIM_MODIFY, &tray_icon(window, icon));
    }
    let previous = CURRENT_ICON.swap(icon, Ordering::Relaxed);
    if !previous.is_null() {
        DestroyIcon(previous);
    }
    CURRENT_LANGUAGE.store(language, Ordering::Relaxed);
    InvalidateRect(FLOAT_WINDOW.load(Ordering::Relaxed), null(), 1);
}

unsafe extern "system" fn floating_proc(
    window: HWND,
    message: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    match message {
        WM_PAINT => {
            let mut paint: PAINTSTRUCT = zeroed();
            let dc = BeginPaint(window, &mut paint);
            let percent = FLOATING_SIZE_PERCENT.load(Ordering::Relaxed);
            DrawIconEx(
                dc,
                0,
                0,
                CURRENT_ICON.load(Ordering::Relaxed),
                floating_dimension(flag_width(GetSystemMetrics(SM_CXICON)), percent),
                floating_dimension(GetSystemMetrics(SM_CYICON), percent),
                0,
                null_mut(),
                DI_NORMAL,
            );
            EndPaint(window, &paint);
            0
        }
        WM_LBUTTONDOWN => {
            DRAG_OFFSET_X.store(
                (lparam as u32 & 0xffff) as u16 as i16 as i32,
                Ordering::Relaxed,
            );
            DRAG_OFFSET_Y.store(
                ((lparam as u32 >> 16) & 0xffff) as u16 as i16 as i32,
                Ordering::Relaxed,
            );
            DRAGGING.store(true, Ordering::Relaxed);
            SetCapture(window);
            0
        }
        WM_MOUSEMOVE if DRAGGING.load(Ordering::Relaxed) => {
            let mut point: POINT = zeroed();
            GetCursorPos(&mut point);
            SetWindowPos(
                window,
                null_mut(),
                point.x - DRAG_OFFSET_X.load(Ordering::Relaxed),
                point.y - DRAG_OFFSET_Y.load(Ordering::Relaxed),
                0,
                0,
                SWP_NOSIZE | SWP_NOZORDER | SWP_NOACTIVATE,
            );
            0
        }
        WM_LBUTTONUP if DRAGGING.load(Ordering::Relaxed) => {
            DRAGGING.store(false, Ordering::Relaxed);
            ReleaseCapture();
            let mut rect: RECT = zeroed();
            GetWindowRect(window, &mut rect);
            write_setting("FloatingX", rect.left as u32);
            write_setting("FloatingY", rect.top as u32);
            0
        }
        _ => DefWindowProcW(window, message, wparam, lparam),
    }
}

unsafe fn set_floating_size(percent: u16) {
    if !matches!(percent, 75 | 100 | 150 | 200) {
        return;
    }
    FLOATING_SIZE_PERCENT.store(percent, Ordering::Relaxed);
    write_setting("FloatingSize", percent as u32);
    let window = FLOAT_WINDOW.load(Ordering::Relaxed);
    if !window.is_null() {
        SetWindowPos(
            window,
            null_mut(),
            0,
            0,
            floating_dimension(flag_width(GetSystemMetrics(SM_CXICON)), percent),
            floating_dimension(GetSystemMetrics(SM_CYICON), percent),
            SWP_NOMOVE | SWP_NOZORDER | SWP_NOACTIVATE,
        );
        InvalidateRect(window, null(), 1);
    }
}

unsafe fn default_floating_position(width: i32, height: i32) -> (i32, i32) {
    let mut work_area: RECT = zeroed();
    SystemParametersInfoW(
        SPI_GETWORKAREA,
        0,
        &mut work_area as *mut _ as *mut c_void,
        0,
    );
    (work_area.right - width - 16, work_area.bottom - height - 16)
}

unsafe fn set_floating(visible: bool) {
    let mut window = FLOAT_WINDOW.load(Ordering::Relaxed);
    if visible && window.is_null() {
        let percent = FLOATING_SIZE_PERCENT.load(Ordering::Relaxed);
        let width = floating_dimension(flag_width(GetSystemMetrics(SM_CXICON)), percent);
        let height = floating_dimension(GetSystemMetrics(SM_CYICON), percent);
        let saved_x = read_setting("FloatingX", i32::MIN as u32) as i32;
        let saved_y = read_setting("FloatingY", i32::MIN as u32) as i32;
        let on_monitor = saved_x != i32::MIN
            && saved_y != i32::MIN
            && !MonitorFromPoint(
                POINT {
                    x: saved_x,
                    y: saved_y,
                },
                MONITOR_DEFAULTTONULL,
            )
            .is_null();
        let (x, y) = if on_monitor {
            (saved_x, saved_y)
        } else {
            default_floating_position(width, height)
        };
        window = CreateWindowExW(
            WS_EX_TOPMOST | WS_EX_TOOLWINDOW | WS_EX_NOACTIVATE,
            wide("CapsLockLayoutFloatingWindow").as_ptr(),
            wide("CapsLock Layout").as_ptr(),
            WS_POPUP,
            x,
            y,
            width,
            height,
            null_mut(),
            null_mut(),
            GetModuleHandleW(null()),
            null(),
        );
        FLOAT_WINDOW.store(window, Ordering::Relaxed);
    }
    if !window.is_null() {
        ShowWindow(window, if visible { SW_SHOWNOACTIVATE } else { SW_HIDE });
        if visible {
            InvalidateRect(window, null(), 1);
        }
    }
    FLOATING_VISIBLE.store(visible, Ordering::Relaxed);
    write_setting("FloatingWindow", visible as u32);
}

unsafe fn refresh_floating() {
    if !FLOATING_VISIBLE.load(Ordering::Relaxed) {
        return;
    }
    let window = FLOAT_WINDOW.load(Ordering::Relaxed);
    if window.is_null() {
        return;
    }
    let mut rect: RECT = zeroed();
    GetWindowRect(window, &mut rect);
    let center = POINT {
        x: rect.left + (rect.right - rect.left) / 2,
        y: rect.top + (rect.bottom - rect.top) / 2,
    };
    if MonitorFromPoint(center, MONITOR_DEFAULTTONULL).is_null() {
        reset_floating();
        return;
    }
    ShowWindow(window, SW_SHOWNOACTIVATE);
    SetWindowPos(
        window,
        HWND_TOPMOST,
        0,
        0,
        0,
        0,
        SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE,
    );
    InvalidateRect(window, null(), 1);
}

unsafe fn reset_floating() {
    set_floating_size(100);
    set_floating(true);
    let window = FLOAT_WINDOW.load(Ordering::Relaxed);
    if !window.is_null() {
        let (x, y) = default_floating_position(
            flag_width(GetSystemMetrics(SM_CXICON)),
            GetSystemMetrics(SM_CYICON),
        );
        SetWindowPos(
            window,
            null_mut(),
            x,
            y,
            0,
            0,
            SWP_NOSIZE | SWP_NOZORDER | SWP_NOACTIVATE,
        );
        write_setting("FloatingX", x as u32);
        write_setting("FloatingY", y as u32);
        InvalidateRect(window, null(), 1);
    }
}

/// The monitor that holds the foreground window. Windows places the keyboard
/// focus and the layout on that monitor, which is where KDE shows its OSD.
unsafe fn foreground_monitor() -> HMONITOR {
    let foreground = GetForegroundWindow();
    let monitor = MonitorFromWindow(foreground, MONITOR_DEFAULTTOPRIMARY);
    if !foreground.is_null() && foreground != WINDOW.load(Ordering::Relaxed) {
        return monitor;
    }
    let mut point: POINT = zeroed();
    GetCursorPos(&mut point);
    MonitorFromPoint(point, MONITOR_DEFAULTTOPRIMARY)
}

unsafe fn osd_monitor() -> HMONITOR {
    match OSD_MONITOR.load(Ordering::Relaxed) {
        OSD_MONITOR_ALL => MonitorFromPoint(POINT { x: 0, y: 0 }, MONITOR_DEFAULTTOPRIMARY),
        OSD_MONITOR_ACTIVE => foreground_monitor(),
        _ => {
            let mut point: POINT = zeroed();
            GetCursorPos(&mut point);
            MonitorFromPoint(point, MONITOR_DEFAULTTOPRIMARY)
        }
    }
}

fn osd_position(corner: u16, work: RECT, width: i32, height: i32) -> (i32, i32) {
    let margin = floating_dimension(height, 30);
    let center_x = work.left + (work.right - work.left - width) / 2;
    let middle_y = work.top + (work.bottom - work.top) / 2;
    let right = work.right - width - margin;
    let bottom = work.bottom - height - margin;
    match corner {
        OSD_CORNER_TOP_LEFT => (work.left + margin, work.top + margin),
        OSD_CORNER_TOP_RIGHT => (right, work.top + margin),
        OSD_CORNER_BOTTOM_LEFT => (work.left + margin, bottom),
        OSD_CORNER_BOTTOM_RIGHT => (right, bottom),
        OSD_CORNER_TOP => (center_x, work.top + margin),
        OSD_CORNER_BOTTOM => (center_x, bottom),
        // Exactly in the middle, so the panel covers the screen centre.
        OSD_CORNER_CENTER => (center_x, middle_y - height / 2),
        OSD_CORNER_ABOVE_CENTER => (center_x, middle_y - height / 2 - margin - height),
        _ => (center_x, middle_y - height / 2 + margin + height),
    }
}

/// Monitor DPI via shcore.dll. Windows 10 1703+ ships it; when the lookup is
/// unavailable the OSD keeps its base size and position.
unsafe fn monitor_dpi(monitor: HMONITOR) -> u32 {
    type GetDpiForMonitor = unsafe extern "system" fn(HMONITOR, i32, *mut u32, *mut u32) -> i32;
    let mut dpi = 96u32;
    let module = LoadLibraryW(wide("shcore.dll").as_ptr());
    if !module.is_null() {
        let address = GetProcAddress(module, c"GetDpiForMonitor".as_ptr() as *const u8);
        if let Some(procedure) = address {
            let get_dpi: GetDpiForMonitor = transmute(procedure);
            let (mut x, mut y) = (96u32, 96u32);
            if get_dpi(monitor, 0, &mut x, &mut y) == 0 {
                dpi = x.max(96);
            }
        }
        FreeLibrary(module);
    }
    dpi
}

/// A rounded-rectangle window region. The OSD frames are drawn with per-pixel
/// alpha already; the region keeps the layered window itself round.
unsafe fn rounded_region(dc: HDC, rect: RECT, radius: i32) -> HRGN {
    let radius = radius
        .min((rect.right - rect.left) / 2)
        .min((rect.bottom - rect.top) / 2)
        .max(0);
    let region = if BeginPath(dc) != 0 {
        RoundRect(
            dc,
            rect.left,
            rect.top,
            rect.right,
            rect.bottom,
            radius * 2,
            radius * 2,
        );
        CloseFigure(dc);
        if EndPath(dc) != 0 {
            PathToRegion(dc)
        } else {
            null_mut()
        }
    } else {
        null_mut()
    };
    if region.is_null() {
        CreateRoundRectRgn(
            rect.left,
            rect.top,
            rect.right + 1,
            rect.bottom + 1,
            radius * 2,
            radius * 2,
        )
    } else {
        region
    }
}

/// A light film grain, deterministic so frames do not shimmer.
fn osd_grain(x: i32, y: i32) -> u8 {
    let mut value = x.wrapping_mul(374_761_393) ^ y.wrapping_mul(668_265_263);
    value = (value ^ (value >> 13)).wrapping_mul(1_274_126_177);
    if value & 0x0100_0000 == 0 { 0 } else { 8 }
}

/// Sharpness of the corner arcs, shared by the fill and the window region so
/// the two never disagree (2 * cos 45 degrees).
const CORNER_COS: f32 = 0.707_106_77;

/// The KDE OSD shape: a thin top-left chamfer and a broad bottom-right sweep.
fn osd_corners(width: i32, height: i32) -> (f32, f32) {
    let limit = (height as f32 * 0.42).max(8.0);
    let top_left = (width as f32 * 0.07 + 4.0).clamp(6.0, limit).round();
    let bottom_right = (width as f32 * 0.25 + 6.0).clamp(6.0, limit).round();
    (top_left, bottom_right)
}

/// Distance from a point to the panel edge. The KDE shape keeps the straight
/// edges and rounds off the top-left and bottom-right corners: each corner is
/// the panel clipped by a circle centred on the opposite corner of its radius
/// square, so the pixel at the corner point itself is cut away.
fn osd_edge_distance(x: f32, y: f32, width: f32, height: f32) -> f32 {
    let (top_left_radius, bottom_right_radius) = osd_corners(width as i32, height as i32);
    let px = x + 0.5;
    let py = y + 0.5;
    let mut distance = px.min(py).min(width - px).min(height - py);
    if px < top_left_radius && py < top_left_radius {
        distance = distance.min((px * px + py * py).sqrt() - top_left_radius * CORNER_COS);
    }
    let right = width - px;
    let bottom = height - py;
    if right < bottom_right_radius && bottom < bottom_right_radius {
        distance = distance
            .min((right * right + bottom * bottom).sqrt() - bottom_right_radius * CORNER_COS);
    }
    distance
}

/// One pixel of the glass panel: a translucent dark fill, a brighter hairline
/// along the top edge, grain, and a feathered outer edge so the panel is
/// anti-aliased against the desktop behind it.
fn osd_glass_pixel(x: i32, y: i32, width: i32, height: i32) -> u32 {
    let distance = osd_edge_distance(x as f32, y as f32, width as f32, height as f32);
    if distance <= 0.0 {
        return 0;
    }
    let feather_alpha = (OSD_FILL_ALPHA as f32 * distance.min(1.0)).round() as u32;
    if feather_alpha == 0 {
        return 0;
    }
    let shade = ((y.max(0) * 8) / height.max(1)).min(7) as u32;
    let grain = osd_grain(x, y) as u32;
    let r = 20 + shade + grain;
    let g = 22 + shade + grain;
    let b = 27 + shade + grain;
    if distance < 1.4 {
        return (feather_alpha << 24) | (r << 16) | (g << 8) | b;
    }
    let alpha = if distance < 2.4 {
        OSD_BORDER_ALPHA
    } else {
        OSD_FILL_ALPHA
    };
    ((alpha as u32) << 24) | (r << 16) | (g << 8) | b
}

/// Composites a source pixel (r, g, b, a) over a destination pixel.
fn blend_pixel(destination: u32, source: (u8, u8, u8, u8)) -> u32 {
    let (sr, sg, sb, sa) = source;
    let source_alpha = sa as u32;
    let destination_alpha = (((destination >> 24) & 0xff) * (255 - source_alpha)) / 255;
    let out_alpha = source_alpha + destination_alpha;
    if out_alpha == 0 {
        return 0;
    }
    let mix = |s: u8, d: u8| -> u32 {
        (s as u32 * source_alpha / out_alpha + d as u32 * destination_alpha / out_alpha).min(255)
    };
    let r = mix(sr, (destination >> 16) as u8);
    let g = mix(sg, (destination >> 8) as u8);
    let b = mix(sb, destination as u8);
    (out_alpha << 24) | (r << 16) | (g << 8) | b
}

unsafe fn paint_glass(pixels: *mut u32, width: i32, height: i32) {
    for y in 0..height {
        for x in 0..width {
            *pixels.offset((y * width + x) as isize) = osd_glass_pixel(x, y, width, height);
        }
    }
}

/// Renders `text` into a cleared 32-bit DIB with GDI and returns the DIB and
/// the box the ink actually occupies, so the caller can centre the text by
/// its real bounds instead of the font's line box.
unsafe fn render_text_bitmap(
    text: &str,
    rect: RECT,
    align: u32,
    font: HFONT,
) -> Option<(HBITMAP, *mut u32, i32, i32, RECT)> {
    let label_width = (rect.right - rect.left).max(1);
    let label_height = (rect.bottom - rect.top).max(1);
    let mut info: BITMAPINFO = zeroed();
    info.bmiHeader.biSize = size_of::<BITMAPINFOHEADER>() as u32;
    info.bmiHeader.biWidth = label_width;
    info.bmiHeader.biHeight = -label_height;
    info.bmiHeader.biPlanes = 1;
    info.bmiHeader.biBitCount = 32;
    info.bmiHeader.biCompression = BI_RGB;
    let mut bits: *mut c_void = null_mut();
    let bitmap = CreateDIBSection(null_mut(), &info, DIB_RGB_COLORS, &mut bits, null_mut(), 0);
    if bitmap.is_null() {
        return None;
    }
    let dc = CreateCompatibleDC(null_mut());
    let previous_bitmap = SelectObject(dc, bitmap);
    let clear = CreateSolidBrush(0);
    let whole = RECT {
        left: 0,
        top: 0,
        right: label_width,
        bottom: label_height,
    };
    FillRect(dc, &whole, clear);
    DeleteObject(clear);
    let previous_font = SelectObject(dc, font);
    SetTextColor(dc, 0x00ff_ffff);
    SetBkMode(dc, TRANSPARENT as i32);
    let mut area = whole;
    let encoded: Vec<u16> = text.encode_utf16().collect();
    DrawTextW(
        dc,
        encoded.as_ptr(),
        encoded.len() as i32,
        &mut area,
        DT_SINGLELINE | DT_VCENTER | align,
    );
    GdiFlush();
    SelectObject(dc, previous_font);
    SelectObject(dc, previous_bitmap);
    DeleteDC(dc);

    let coverage = bits as *const u32;
    let mut ink = RECT {
        left: i32::MAX,
        top: i32::MAX,
        right: i32::MIN,
        bottom: i32::MIN,
    };
    for ly in 0..label_height {
        for lx in 0..label_width {
            let value = *coverage.offset((ly * label_width + lx) as isize);
            if ((value >> 8) & 0xff).max(value & 0xff) == 0 {
                continue;
            }
            ink.left = ink.left.min(lx);
            ink.top = ink.top.min(ly);
            ink.right = ink.right.max(lx + 1);
            ink.bottom = ink.bottom.max(ly + 1);
        }
    }
    if ink.left >= ink.right {
        return Some((bitmap, bits as *mut u32, label_width, label_height, whole));
    }
    Some((bitmap, bits as *mut u32, label_width, label_height, ink))
}

/// Renders one line centred by its visible pixels, both horizontally and
/// vertically inside the content area.
unsafe fn paint_centered_label(
    pixels: *mut u32,
    width: i32,
    height: i32,
    text: &str,
    area: RECT,
    font: HFONT,
    colour: (u8, u8, u8),
) {
    let area_width = (area.right - area.left).max(1);
    let area_height = (area.bottom - area.top).max(1);
    let Some((bitmap, bits, label_width, _label_height, ink)) =
        render_text_bitmap(text, area, DT_LEFT, font)
    else {
        return;
    };
    let coverage = bits as *const u32;
    let offset_x = area.left + (area_width - (ink.right - ink.left)) / 2 - ink.left;
    let offset_y = area.top + (area_height - (ink.bottom - ink.top)) / 2 - ink.top;
    for ly in ink.top..ink.bottom {
        for lx in ink.left..ink.right {
            let value = *coverage.offset((ly * label_width + lx) as isize);
            let coverage_value = ((value >> 8) & 0xff).max(value & 0xff);
            if coverage_value == 0 {
                continue;
            }
            let tx = offset_x + lx;
            let ty = offset_y + ly;
            if tx < 0 || ty < 0 || tx >= width || ty >= height {
                continue;
            }
            let alpha = coverage_value.min(255) as u8;
            let index = (ty * width + tx) as isize;
            *pixels.offset(index) =
                blend_pixel(*pixels.offset(index), (colour.0, colour.1, colour.2, alpha));
        }
    }
    DeleteObject(bitmap);
}

/// Draws the flag into the OSD using exactly the artwork of the tray icon and
/// the floating flag, so all three always show the same flag. The icon is
/// rendered off-screen and its pixels are blended into the frame, which keeps
/// the glass visible through it.
unsafe fn draw_flag(pixels: *mut u32, width: i32, height: i32, rect: RECT, language: u16) {
    let icon = cached_flag_icon(language);
    if icon.is_null() {
        return;
    }
    let mut info: ICONINFO = zeroed();
    if GetIconInfo(icon, &mut info) == 0 {
        return;
    }
    let flag_w = (rect.right - rect.left).max(1);
    let flag_h = (rect.bottom - rect.top).max(1);
    let mask = info.hbmMask;
    let colour = info.hbmColor;
    let screen = GetDC(null_mut());
    let dc = CreateCompatibleDC(screen);
    let icon_width = GetSystemMetrics(SM_CXICON);
    let icon_height = GetSystemMetrics(SM_CYICON);

    let mut info32: BITMAPINFO = zeroed();
    info32.bmiHeader.biSize = size_of::<BITMAPINFOHEADER>() as u32;
    info32.bmiHeader.biWidth = icon_width;
    info32.bmiHeader.biHeight = -icon_height;
    info32.bmiHeader.biPlanes = 1;
    info32.bmiHeader.biBitCount = 32;
    info32.bmiHeader.biCompression = BI_RGB;
    let mut bits: *mut c_void = null_mut();
    let bitmap = CreateDIBSection(dc, &info32, DIB_RGB_COLORS, &mut bits, null_mut(), 0);
    if !bitmap.is_null() {
        let previous = SelectObject(dc, bitmap);
        DrawIconEx(
            dc,
            0,
            0,
            icon,
            icon_width,
            icon_height,
            0,
            null_mut(),
            DI_NORMAL,
        );
        GdiFlush();
        let source = bits as *const u32;
        for fy in 0..flag_h {
            for fx in 0..flag_w {
                let tx = rect.left + fx;
                let ty = rect.top + fy;
                if tx < 0 || ty < 0 || tx >= width || ty >= height {
                    continue;
                }
                // Sample the icon with nearest-neighbour: the icon is small, the
                // OSD is large, and pixel art stays crisp.
                let sx = (fx * icon_width / flag_w).min(icon_width - 1);
                let sy = (fy * icon_height / flag_h).min(icon_height - 1);
                let value = *source.offset((sy * icon_width + sx) as isize);
                let index = (ty * width + tx) as isize;
                *pixels.offset(index) = blend_pixel(
                    *pixels.offset(index),
                    ((value >> 16) as u8, (value >> 8) as u8, value as u8, 255),
                );
            }
        }
        SelectObject(dc, previous);
        DeleteObject(bitmap);
    }

    DeleteDC(dc);
    ReleaseDC(null_mut(), screen);
    if !mask.is_null() {
        DeleteObject(mask);
    }
    if !colour.is_null() {
        DeleteObject(colour);
    }
    // Иконка кэшируется и не уничтожается здесь: она принадлежит FLAG_ICONS.
}

unsafe fn osd_font(height: i32, weight: i32) -> HFONT {
    CreateFontW(
        -height.max(1),
        0,
        0,
        0,
        weight,
        0,
        0,
        0,
        0,
        0,
        0,
        0,
        0,
        wide(OSD_FONT_FAMILY).as_ptr(),
    )
}

unsafe fn draw_progress(pixels: *mut u32, width: i32, height: i32, progress: f32) {
    let inset = 16;
    let line_top = height - 7;
    let line_width = ((width - inset * 2) as f32 * progress.clamp(0.0, 1.0)) as i32;
    for y in line_top..(line_top + 3).min(height - 4) {
        for x in inset..inset + line_width {
            let pixel = (y * width + x) as isize;
            let edge = (x - inset).min(inset + line_width - x).min(7) as f32 / 7.0;
            let alpha = (200.0 * edge) as u8;
            *pixels.offset(pixel) = blend_pixel(*pixels.offset(pixel), (210, 218, 230, alpha));
        }
    }
}

/// Bands near the bottom edge that the progress line can touch. Repainting
/// only these while the OSD fades keeps the fade cheap: the glass, the flag
/// and the text never change during it.
const PROGRESS_BAND: i32 = 9;

/// Rebuilds the bottom rows the progress line occupies from the glass itself,
/// so the previous line is erased without cutting a hole in the panel. Only
/// these few rows are recomputed while the OSD fades.
unsafe fn clear_progress_band(pixels: *mut u32, width: i32, height: i32) {
    let top = (height - PROGRESS_BAND).max(0);
    for y in top..height {
        for x in 0..width {
            *pixels.offset((y * width + x) as isize) = osd_glass_pixel(x, y, width, height);
        }
    }
}

/// The expensive part of a frame: glass, flag and text. Cached per window so
/// the fade only repaints the progress line over the most recent frame.
struct CachedFrame {
    language: u16,
    mode: u16,
    size_percent: u16,
    uk: bool,
    pixels: Vec<u32>,
    width: i32,
    height: i32,
}

static CACHED_FRAMES: [Mutex<Option<CachedFrame>>; MAX_OSD_WINDOWS] =
    [const { Mutex::new(None) }; MAX_OSD_WINDOWS];

/// Rebuilds the cached content when a setting or the language changed.
unsafe fn update_cached_frame(index: usize, width: i32, height: i32, language: u16) {
    let mode = OSD_MODE.load(Ordering::Relaxed);
    let size_percent = OSD_SIZE_PERCENT.load(Ordering::Relaxed);
    let uk = is_uk_flag(ENGLISH_FLAG_SETTING.load(Ordering::Relaxed) as u32);
    let mut slot = CACHED_FRAMES[index]
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let reuse = slot
        .as_ref()
        .is_some_and(|frame| frame.width == width && frame.height == height);
    if reuse {
        let frame = slot.as_ref().unwrap();
        if frame.language == language
            && frame.mode == mode
            && frame.size_percent == size_percent
            && frame.uk == uk
        {
            return;
        }
        let mut pixels = slot.take().unwrap().pixels;
        pixels.resize((width * height) as usize, 0);
        paint_content(&mut pixels, width, height, language, mode, uk);
        *slot = Some(CachedFrame {
            language,
            mode,
            size_percent,
            uk,
            pixels,
            width,
            height,
        });
    } else {
        let mut pixels = vec![0u32; (width * height) as usize];
        paint_content(&mut pixels, width, height, language, mode, uk);
        *slot = Some(CachedFrame {
            language,
            mode,
            size_percent,
            uk,
            pixels,
            width,
            height,
        });
    }
}

/// Glass, flag and text - everything that stays still while the OSD fades.
fn paint_content(pixels: &mut [u32], width: i32, height: i32, language: u16, mode: u16, uk: bool) {
    let pointer = pixels.as_mut_ptr();
    unsafe {
        paint_glass(pointer, width, height);
        draw_osd_content(null_mut(), pointer, width, height, language, mode, uk);
        clear_progress_band(pointer, width, height);
    }
}

unsafe fn draw_osd_content(
    _dc: HDC,
    pixels: *mut u32,
    width: i32,
    height: i32,
    language: u16,
    mode: u16,
    _uk: bool,
) {
    let padding = height / 5;
    let flag_height = height - padding * 2;
    let flag_width = (flag_height * 3) / 2;
    let text_width = (height * 3 / 2).min(width / 2);
    let group = if osd_shows_flags(mode) && osd_shows_text(mode) {
        flag_width + padding + text_width
    } else if osd_shows_flags(mode) {
        flag_width
    } else {
        text_width
    };
    let left = (width - group) / 2;

    if osd_shows_flags(mode) {
        let flag = RECT {
            left,
            top: padding,
            right: left + flag_width,
            bottom: height - padding,
        };
        draw_flag(pixels, width, height, flag, language);
    }

    if osd_shows_text(mode) {
        let text_left = if osd_shows_flags(mode) {
            left + flag_width + padding
        } else {
            left
        };
        let primary_height = (height * 46 / 100).max(8);
        let secondary_height = (height * 22 / 100).max(6);
        let half = height / 2;
        let primary_font = osd_font(primary_height, 600);
        let secondary_font = osd_font(secondary_height, 400);
        paint_centered_label(
            pixels,
            width,
            height,
            osd_short_name(language),
            RECT {
                left: text_left,
                top: 0,
                right: text_left + text_width,
                bottom: half,
            },
            primary_font,
            (255, 255, 255),
        );
        paint_centered_label(
            pixels,
            width,
            height,
            osd_full_name(language),
            RECT {
                left: text_left,
                top: half,
                right: text_left + text_width,
                bottom: height,
            },
            secondary_font,
            (198, 202, 208),
        );
        DeleteObject(primary_font);
        DeleteObject(secondary_font);
    }
}

unsafe fn apply_osd_rounding(window: HWND) {
    let preference: DWM_WINDOW_CORNER_PREFERENCE = DWMWCP_ROUND;
    DwmSetWindowAttribute(
        window,
        DWMWA_WINDOW_CORNER_PREFERENCE as u32,
        &preference as *const _ as *const c_void,
        size_of::<DWM_WINDOW_CORNER_PREFERENCE>() as u32,
    );
}

unsafe fn osd_window(index: usize) -> HWND {
    let window = OSD_WINDOWS[index].load(Ordering::Relaxed);
    if !window.is_null() {
        return window;
    }
    let window = CreateWindowExW(
        WS_EX_LAYERED | WS_EX_TOPMOST | WS_EX_TOOLWINDOW | WS_EX_NOACTIVATE,
        wide("CapsLockLayoutOsdWindow").as_ptr(),
        wide("CapsLock Layout").as_ptr(),
        WS_POPUP,
        0,
        0,
        0,
        0,
        null_mut(),
        null_mut(),
        GetModuleHandleW(null()),
        null(),
    );
    if !window.is_null() {
        // Keep the OSD out of screen captures and sharing, like a system overlay.
        SetWindowDisplayAffinity(window, WDA_EXCLUDEFROMCAPTURE);
        apply_osd_rounding(window);
        OSD_WINDOWS[index].store(window, Ordering::Relaxed);
    }
    window
}

unsafe fn place_osd(window: HWND, x: i32, y: i32, width: i32, height: i32, radius: i32) {
    let index = OSD_WINDOWS
        .iter()
        .position(|slot| slot.load(Ordering::Relaxed) == window);
    SetWindowPos(window, HWND_TOPMOST, x, y, width, height, SWP_NOACTIVATE);
    let screen = GetDC(null_mut());
    let region = rounded_region(
        screen,
        RECT {
            left: 0,
            top: 0,
            right: width,
            bottom: height,
        },
        radius,
    );
    ReleaseDC(null_mut(), screen);
    // Replace the tracked region and delete the old one: Win32 takes ownership
    // of the region passed to SetWindowRgn but never frees the previous one.
    if let Some(idx) = index {
        let old = OSD_REGIONS[idx].swap(region, Ordering::Relaxed);
        if !old.is_null() {
            DeleteObject(old);
        }
    }
    SetWindowRgn(window, region, 0);
}

/// Shows the OSD on the monitors selected in the menu. The monitor is chosen
/// by the setting: the one under the cursor (default), the one holding the
/// foreground window - the monitor the user is typing on - or all of them.
/// Windows that are not chosen stay hidden, so a keystroke on the second
/// display never flashes the OSD on the first one.
/// Shows the OSD in `language` on the chosen monitors. The language is passed
/// in, never re-read from the foreground window: right after a switch that
/// window may still report the old layout, which made the OSD contradict the
/// tray icon.
unsafe fn show_osd(language: u16) {
    if !OSD_ENABLED.load(Ordering::Relaxed) {
        return;
    }
    let language = flag_language(language);
    OSD_LANGUAGE.store(language, Ordering::Relaxed);
    OSD_VISIBLE.store(true, Ordering::Relaxed);
    OSD_FADING.store(false, Ordering::Relaxed);
    OSD_OPACITY.store(255, Ordering::Relaxed);
    let main_window = WINDOW.load(Ordering::Relaxed);
    KillTimer(main_window, OSD_FADE_TIMER);

    let mode = OSD_MODE.load(Ordering::Relaxed);
    let corner = OSD_CORNER.load(Ordering::Relaxed);
    let size_percent = OSD_SIZE_PERCENT.load(Ordering::Relaxed);
    let base_width = osd_width_for(mode, size_percent);
    let base_height = base_width / 3;
    let choice = OSD_MONITOR.load(Ordering::Relaxed);
    let target = osd_monitor();

    let mut state = OsdEnumState {
        monitors: [null_mut(); MAX_OSD_WINDOWS],
        count: 0,
    };
    EnumDisplayMonitors(
        null_mut(),
        null(),
        Some(collect_osd_monitors),
        &mut state as *mut _ as isize,
    );

    let geometry_key = ((mode as u64) << 48) | ((size_percent as u64) << 32);
    let mut visible_count = 0;
    for (index, monitor) in state.monitors[..state.count].iter().copied().enumerate() {
        let selected = choice == OSD_MONITOR_ALL || monitor == target;
        let window = osd_window(index);
        if window.is_null() {
            continue;
        }
        if !selected {
            ShowWindow(window, SW_HIDE);
            continue;
        }
        let Some(work) = osd_work_area(monitor) else {
            continue;
        };
        let scale = monitor_dpi(monitor) as f32 / 96.0;
        let width = (base_width as f32 * scale).round() as i32;
        let height = (base_height as f32 * scale).round() as i32;
        let key = geometry_key | ((monitor as u64) & 0xffff_ffff) | ((corner as u64) << 24);
        if OSD_WINDOW_GEOMETRY[index].swap(key, Ordering::Relaxed) != key {
            // A manually dragged position wins over the corner setting, but
            // only on the monitor it was set on and only while the window still
            // fits on that monitor's work area.
            let custom = custom_position(
                OSD_CUSTOM_ENABLED.load(Ordering::Relaxed),
                OSD_CUSTOM_X[index].load(Ordering::Relaxed),
                OSD_CUSTOM_Y[index].load(Ordering::Relaxed),
                width,
                height,
                work,
            );
            let (x, y) = custom.unwrap_or_else(|| osd_position(corner, work, width, height));
            place_osd(
                window,
                x,
                y,
                width,
                height,
                (OSD_RADIUS as f32 * scale) as i32,
            );
        }
        OSD_WINDOW_LANGUAGE[index].store(language, Ordering::Relaxed);
        paint_osd_window(window, 1000);
        ShowWindow(window, SW_SHOWNOACTIVATE);
        SetWindowPos(
            window,
            HWND_TOPMOST,
            0,
            0,
            0,
            0,
            SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE,
        );
        visible_count += 1;
    }

    for index in state.count..MAX_OSD_WINDOWS {
        let window = OSD_WINDOWS[index].load(Ordering::Relaxed);
        if !window.is_null() {
            ShowWindow(window, SW_HIDE);
            OSD_WINDOW_GEOMETRY[index].store(0, Ordering::Relaxed);
        }
    }

    if visible_count == 0 {
        OSD_VISIBLE.store(false, Ordering::Relaxed);
        return;
    }
    SetTimer(
        main_window,
        OSD_HIDE_TIMER,
        osd_display_ms(OSD_DURATION.load(Ordering::Relaxed)),
        None,
    );
}

/// Paints one window on its own, using the language that window shows. The
/// windows can sit on monitors with different foreground windows, so the
/// shared language is only a fallback for the very first frame.
unsafe fn paint_osd_window(window: HWND, progress: i32) {
    let Some(index) = osd_window_index(window) else {
        return;
    };
    let mut client: RECT = zeroed();
    GetClientRect(window, &mut client);
    let (width, height) = (client.right - client.left, client.bottom - client.top);
    if width <= 0 || height <= 0 {
        return;
    }
    let language = window_language(index);
    let Some((frame, frame_dc)) = prepare_frame(index, width, height, language, progress) else {
        return;
    };
    let opacity = OSD_OPACITY.load(Ordering::Relaxed) as u8;
    flush_layered_window(window, frame_dc, width, height, opacity);
    release_frame(frame, frame_dc);
}

unsafe fn osd_window_index(window: HWND) -> Option<usize> {
    OSD_WINDOWS
        .iter()
        .position(|slot| slot.load(Ordering::Relaxed) == window)
}

unsafe fn window_language(index: usize) -> u16 {
    let language = OSD_WINDOW_LANGUAGE[index].load(Ordering::Relaxed);
    if language != 0 {
        language
    } else {
        OSD_LANGUAGE.load(Ordering::Relaxed)
    }
}

/// Repaints every visible OSD window with its own language. A progress below
/// zero means "finished": the line has fully shrunk, as the OSD fades out.
unsafe fn repaint_osd_windows(progress: i32) {
    for slot in &OSD_WINDOWS {
        let window = slot.load(Ordering::Relaxed);
        if !window.is_null() && IsWindowVisible(window) != 0 {
            paint_osd_window(window, progress);
        }
    }
}

/// Builds the frame for one OSD window: cached glass/flag/text, a fresh copy
/// of it, then only the progress band is redrawn. Returns the DIB and its DC;
/// the caller flushes it and must release it with `release_frame`.
unsafe fn prepare_frame(
    index: usize,
    width: i32,
    height: i32,
    language: u16,
    progress: i32,
) -> Option<(HBITMAP, HDC)> {
    update_cached_frame(index, width, height, language);

    let mut info: BITMAPINFO = zeroed();
    info.bmiHeader.biSize = size_of::<BITMAPINFOHEADER>() as u32;
    info.bmiHeader.biWidth = width;
    info.bmiHeader.biHeight = -height;
    info.bmiHeader.biPlanes = 1;
    info.bmiHeader.biBitCount = 32;
    info.bmiHeader.biCompression = BI_RGB;
    let mut bits: *mut c_void = null_mut();
    let frame = CreateDIBSection(null_mut(), &info, DIB_RGB_COLORS, &mut bits, null_mut(), 0);
    if frame.is_null() {
        return None;
    }
    let frame_dc = CreateCompatibleDC(null_mut());
    if frame_dc.is_null() {
        DeleteObject(frame);
        return None;
    }
    SelectObject(frame_dc, frame);

    let pixels = bits as *mut u32;
    {
        let slot = CACHED_FRAMES[index]
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        if let Some(cached) = slot.as_ref() {
            std::ptr::copy_nonoverlapping(cached.pixels.as_ptr(), pixels, cached.pixels.len());
        }
    }
    let progress = if progress < 0 {
        0.0
    } else {
        progress as f32 / 1000.0
    };
    draw_progress(pixels, width, height, progress);
    GdiFlush();
    Some((frame, frame_dc))
}

unsafe fn release_frame(frame: HBITMAP, frame_dc: HDC) {
    DeleteDC(frame_dc);
    DeleteObject(frame);
}

/// Pushes the composited frame straight to the layered window. Straight
/// alpha (AC_SRC_ALPHA): the window shows exactly the frame, blended over
/// the desktop, so nothing depends on how the window would paint itself.
unsafe fn flush_layered_window(window: HWND, frame_dc: HDC, width: i32, height: i32, opacity: u8) {
    let mut rect: RECT = zeroed();
    GetWindowRect(window, &mut rect);
    let origin = POINT {
        x: rect.left,
        y: rect.top,
    };
    let size = SIZE {
        cx: width,
        cy: height,
    };
    let source: POINT = zeroed();
    let blend = BLENDFUNCTION {
        BlendOp: AC_SRC_OVER as u8,
        BlendFlags: 0,
        SourceConstantAlpha: opacity,
        AlphaFormat: AC_SRC_ALPHA as u8,
    };
    let updated = UpdateLayeredWindow(
        window,
        null_mut(),
        &origin,
        &size,
        frame_dc,
        &source,
        0,
        &blend,
        ULW_ALPHA,
    );
    if updated == 0 {
        // Fall back to the window DC, which works everywhere Windows does.
        let screen = GetDC(null_mut());
        let window_dc = GetDC(window);
        GdiAlphaBlend(
            window_dc, 0, 0, width, height, frame_dc, 0, 0, width, height, blend,
        );
        ReleaseDC(window, window_dc);
        ReleaseDC(null_mut(), screen);
    }
}

fn now_ms() -> u32 {
    unsafe { GetTickCount() }
}

unsafe fn hide_osd() {
    let main_window = WINDOW.load(Ordering::Relaxed);
    KillTimer(main_window, OSD_FADE_TIMER);
    KillTimer(main_window, OSD_HIDE_TIMER);
    OSD_VISIBLE.store(false, Ordering::Relaxed);
    OSD_FADING.store(false, Ordering::Relaxed);
    OSD_OPACITY.store(255, Ordering::Relaxed);
    for slot in &OSD_WINDOWS {
        let window = slot.load(Ordering::Relaxed);
        if !window.is_null() {
            ShowWindow(window, SW_HIDE);
        }
    }
}

unsafe fn osd_work_area(monitor: HMONITOR) -> Option<RECT> {
    let mut info: MONITORINFO = zeroed();
    info.cbSize = size_of::<MONITORINFO>() as u32;
    if GetMonitorInfoW(monitor, &mut info) == 0 {
        return None;
    }
    Some(info.rcWork)
}

unsafe extern "system" fn collect_osd_monitors(
    monitor: HMONITOR,
    _dc: HDC,
    _rect: *mut RECT,
    data: LPARAM,
) -> BOOL {
    let state = &mut *(data as *mut OsdEnumState);
    if state.count < MAX_OSD_WINDOWS {
        state.monitors[state.count] = monitor;
        state.count += 1;
    }
    1
}

struct OsdEnumState {
    monitors: [HMONITOR; MAX_OSD_WINDOWS],
    count: usize,
}

/// OSD windows never take focus and never paint through WM_PAINT: every frame
/// is pushed with per-pixel alpha in `repaint_osd_windows`, the fade included.
/// Dragging is supported only in custom-position mode; the hit test stays
/// transparent otherwise so the panel never blocks the desktop.
unsafe extern "system" fn osd_proc(
    window: HWND,
    message: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    let index = osd_window_index(window);
    match message {
        WM_NCHITTEST if OSD_CUSTOM_ENABLED.load(Ordering::Relaxed) => HTCAPTION as LRESULT,
        WM_NCHITTEST => HTTRANSPARENT as LRESULT,
        WM_MOUSEACTIVATE => MA_NOACTIVATE as LRESULT,
        WM_LBUTTONDOWN if OSD_CUSTOM_ENABLED.load(Ordering::Relaxed) => {
            if let Some(index) = index {
                OSD_DRAG_OFFSET_X[index].store(
                    (lparam as u32 & 0xffff) as u16 as i16 as i32,
                    Ordering::Relaxed,
                );
                OSD_DRAG_OFFSET_Y[index].store(
                    ((lparam as u32 >> 16) & 0xffff) as u16 as i16 as i32,
                    Ordering::Relaxed,
                );
                OSD_DRAGGING[index].store(true, Ordering::Relaxed);
                SetCapture(window);
            }
            0
        }
        WM_MOUSEMOVE if index.is_some_and(|i| OSD_DRAGGING[i].load(Ordering::Relaxed)) => {
            let index = index.unwrap();
            let mut point: POINT = zeroed();
            GetCursorPos(&mut point);
            SetWindowPos(
                window,
                null_mut(),
                point.x - OSD_DRAG_OFFSET_X[index].load(Ordering::Relaxed),
                point.y - OSD_DRAG_OFFSET_Y[index].load(Ordering::Relaxed),
                0,
                0,
                SWP_NOSIZE | SWP_NOZORDER | SWP_NOACTIVATE,
            );
            0
        }
        WM_LBUTTONUP if index.is_some_and(|i| OSD_DRAGGING[i].load(Ordering::Relaxed)) => {
            let index = index.unwrap();
            OSD_DRAGGING[index].store(false, Ordering::Relaxed);
            ReleaseCapture();
            let mut rect: RECT = zeroed();
            GetWindowRect(window, &mut rect);
            OSD_CUSTOM_X[index].store(rect.left, Ordering::Relaxed);
            OSD_CUSTOM_Y[index].store(rect.top, Ordering::Relaxed);
            save_osd_custom_positions();
            0
        }
        WM_DPICHANGED => {
            if OSD_VISIBLE.load(Ordering::Relaxed) {
                PostMessageW(WINDOW.load(Ordering::Relaxed), WM_DISPLAYCHANGE, 0, 0);
            }
            0
        }
        _ => DefWindowProcW(window, message, wparam, lparam),
    }
}

unsafe fn show_tray(window: HWND) {
    let language = foreground_language();
    set_flag(window, language);
    let icon = CURRENT_ICON.load(Ordering::Relaxed);
    if Shell_NotifyIconW(NIM_ADD, &tray_icon(window, icon)) != 0 {
        TRAY_VISIBLE.store(true, Ordering::Relaxed);
        set_flag(window, language);
    }
}

unsafe fn hide_tray(window: HWND) {
    if TRAY_VISIBLE.swap(false, Ordering::Relaxed) {
        let icon = CURRENT_ICON.load(Ordering::Relaxed);
        Shell_NotifyIconW(NIM_DELETE, &tray_icon(window, icon));
    }
}

unsafe fn build_menu() -> HMENU {
    let menu = CreatePopupMenu();
    let behavior_menu = CreatePopupMenu();
    let flag_menu = CreatePopupMenu();
    let size_menu = CreatePopupMenu();
    let osd_menu = CreatePopupMenu();
    let osd_monitor_menu = CreatePopupMenu();
    let osd_corner_menu = CreatePopupMenu();
    let osd_mode_menu = CreatePopupMenu();
    let osd_size_menu = CreatePopupMenu();
    let osd_duration_menu = CreatePopupMenu();
    if menu.is_null()
        || behavior_menu.is_null()
        || flag_menu.is_null()
        || size_menu.is_null()
        || osd_menu.is_null()
        || osd_monitor_menu.is_null()
        || osd_corner_menu.is_null()
        || osd_mode_menu.is_null()
        || osd_size_menu.is_null()
        || osd_duration_menu.is_null()
    {
        return menu;
    }

    let enabled = ENABLED.load(Ordering::Relaxed);
    AppendMenuW(
        behavior_menu,
        MF_STRING | if enabled { 0 } else { MF_CHECKED },
        CAPS_DEFAULT_ID,
        wide("CapsLock по умолчанию").as_ptr(),
    );
    AppendMenuW(
        behavior_menu,
        MF_STRING | if enabled { MF_CHECKED } else { 0 },
        CAPS_SWITCH_ID,
        wide("Изменение раскладки").as_ptr(),
    );
    AppendMenuW(
        menu,
        MF_POPUP,
        behavior_menu as usize,
        wide("Поведение кнопки CapsLock").as_ptr(),
    );

    let uk = is_uk_flag(ENGLISH_FLAG_SETTING.load(Ordering::Relaxed) as u32);
    AppendMenuW(
        flag_menu,
        MF_STRING | if uk { 0 } else { MF_CHECKED },
        FLAG_US_ID,
        wide("США").as_ptr(),
    );
    AppendMenuW(
        flag_menu,
        MF_STRING | if uk { MF_CHECKED } else { 0 },
        FLAG_UK_ID,
        wide("Великобритания").as_ptr(),
    );
    AppendMenuW(
        menu,
        MF_POPUP,
        flag_menu as usize,
        wide("Флаг английской раскладки").as_ptr(),
    );

    let size = FLOATING_SIZE_PERCENT.load(Ordering::Relaxed);
    for (id, percent, label) in [
        (FLOATING_SIZE_75_ID, 75, "75%"),
        (FLOATING_SIZE_100_ID, 100, "100%"),
        (FLOATING_SIZE_150_ID, 150, "150%"),
        (FLOATING_SIZE_200_ID, 200, "200%"),
    ] {
        AppendMenuW(
            size_menu,
            MF_STRING | if size == percent { MF_CHECKED } else { 0 },
            id,
            wide(label).as_ptr(),
        );
    }
    AppendMenuW(
        menu,
        MF_POPUP,
        size_menu as usize,
        wide("Размер плавающего окна").as_ptr(),
    );
    AppendMenuW(
        menu,
        MF_STRING,
        FLOATING_ID,
        wide(floating_label(FLOATING_VISIBLE.load(Ordering::Relaxed))).as_ptr(),
    );
    AppendMenuW(
        menu,
        MF_STRING,
        FLOATING_RESET_ID,
        wide("Сбросить плавающее окно").as_ptr(),
    );
    AppendMenuW(menu, MF_STRING, HIDE_ID, wide("Скрыть из трея").as_ptr());
    AppendMenuW(menu, MF_SEPARATOR, 0, null());

    let osd_enabled = OSD_ENABLED.load(Ordering::Relaxed);
    let monitor_mode = OSD_MONITOR.load(Ordering::Relaxed);
    for (label, mode) in osd_monitor_options() {
        AppendMenuW(
            osd_monitor_menu,
            MF_STRING | if monitor_mode == mode { MF_CHECKED } else { 0 },
            osd_monitor_id(mode),
            wide(label).as_ptr(),
        );
    }
    AppendMenuW(
        osd_menu,
        MF_POPUP,
        osd_monitor_menu as usize,
        wide("Монитор").as_ptr(),
    );

    let corner = OSD_CORNER.load(Ordering::Relaxed);
    for (label, value) in osd_corner_options() {
        AppendMenuW(
            osd_corner_menu,
            MF_STRING | if corner == value { MF_CHECKED } else { 0 },
            osd_corner_id(value),
            wide(label).as_ptr(),
        );
    }
    AppendMenuW(
        osd_menu,
        MF_POPUP,
        osd_corner_menu as usize,
        wide("Положение").as_ptr(),
    );

    let mode = OSD_MODE.load(Ordering::Relaxed);
    for (label, value) in osd_mode_options() {
        AppendMenuW(
            osd_mode_menu,
            MF_STRING | if mode == value { MF_CHECKED } else { 0 },
            osd_mode_id(value),
            wide(label).as_ptr(),
        );
    }
    AppendMenuW(
        osd_menu,
        MF_POPUP,
        osd_mode_menu as usize,
        wide("Отображение").as_ptr(),
    );

    let osd_size = OSD_SIZE_PERCENT.load(Ordering::Relaxed);
    for (label, value) in osd_size_options() {
        AppendMenuW(
            osd_size_menu,
            MF_STRING | if osd_size == value { MF_CHECKED } else { 0 },
            osd_size_id(value),
            wide(label).as_ptr(),
        );
    }
    AppendMenuW(
        osd_menu,
        MF_POPUP,
        osd_size_menu as usize,
        wide("Размер").as_ptr(),
    );

    let duration = OSD_DURATION.load(Ordering::Relaxed);
    for (label, value) in osd_duration_options() {
        AppendMenuW(
            osd_duration_menu,
            MF_STRING | if duration == value { MF_CHECKED } else { 0 },
            osd_duration_id(value),
            wide(label).as_ptr(),
        );
    }
    AppendMenuW(
        osd_menu,
        MF_POPUP,
        osd_duration_menu as usize,
        wide("Скорость исчезновения").as_ptr(),
    );
    AppendMenuW(
        osd_menu,
        MF_STRING | if OSD_CUSTOM_ENABLED.load(Ordering::Relaxed) { MF_CHECKED } else { 0 },
        OSD_CUSTOM_POS_ID,
        wide("Перетаскивание (своя позиция)").as_ptr(),
    );
    AppendMenuW(
        osd_menu,
        MF_STRING,
        OSD_RESET_POS_ID,
        wide("Сбросить позицию").as_ptr(),
    );

    AppendMenuW(
        menu,
        MF_STRING | if OSD_ANY_SWITCH.load(Ordering::Relaxed) { MF_CHECKED } else { 0 },
        OSD_ANY_SWITCH_ID,
        wide("OSD при любом переключении").as_ptr(),
    );
    AppendMenuW(
        menu,
        MF_STRING,
        OSD_ID,
        wide(osd_label(osd_enabled)).as_ptr(),
    );
    AppendMenuW(
        menu,
        MF_POPUP,
        osd_menu as usize,
        wide("OSD настройки").as_ptr(),
    );
    AppendMenuW(menu, MF_SEPARATOR, 0, null());
    AppendMenuW(
        menu,
        MF_STRING,
        AUTOSTART_ID,
        wide(autostart_label(autostart_enabled())).as_ptr(),
    );
    AppendMenuW(menu, MF_SEPARATOR, 0, null());
    AppendMenuW(menu, MF_STRING, EXIT_ID, wide("Выход").as_ptr());
    menu
}

unsafe fn show_menu(window: HWND) {
    let mut point: POINT = zeroed();
    GetCursorPos(&mut point);
    let mut selected = 0;
    loop {
        let menu = build_menu();
        if menu.is_null() {
            return;
        }
        SetForegroundWindow(window);
        let command = TrackPopupMenu(
            menu,
            TPM_RIGHTBUTTON | TPM_RETURNCMD,
            point.x,
            point.y,
            0,
            window,
            null(),
        ) as usize;
        DestroyMenu(menu);
        if command == 0 {
            return;
        }
        handle_command(window, command);
        if !rebuilds_menu(command) {
            return;
        }
        selected += 1;
        if selected > 8 {
            return;
        }
    }
}

/// Toggles that change the menu label must be opened again so the new state
/// is visible; the screenshot harness relies on this too.
fn rebuilds_menu(command: usize) -> bool {
    command == OSD_ID || command == FLOATING_ID
}

unsafe extern "system" fn window_proc(
    window: HWND,
    message: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    match message {
        WM_CREATE => {
            show_tray(window);
            if !TRAY_VISIBLE.load(Ordering::Relaxed) {
                return -1;
            }
            SetTimer(window, 1, 250, None);
            RegisterHotKey(window, OSD_PREVIEW_HOTKEY_ID, MOD_SHIFT, VK_F11 as u32);
            0
        }
        SHOW_TRAY_MESSAGE => {
            show_tray(window);
            0
        }
        SWITCH_LAYOUT_MESSAGE => {
            switch_layout();
            0
        }
        OSD_SWITCHED_MESSAGE => {
            // Caps Lock already told us the new language: show it directly
            // instead of asking a window that may still report the old one.
            show_osd(wparam as u16);
            0
        }
        OSD_SHOW_MESSAGE => {
            // Any other caller (preview, settings change): use the layout the
            // foreground window really has, unless a Caps Lock switch is still
            // landing, in which case the pending target is the truth.
            show_osd(display_language(now_ms(), foreground_language()));
            0
        }
        WM_TIMER if wparam == OSD_HIDE_TIMER => {
            let main_window = WINDOW.load(Ordering::Relaxed);
            KillTimer(main_window, OSD_HIDE_TIMER);
            OSD_FADING.store(true, Ordering::Relaxed);
            OSD_FADE_STEP.store(0, Ordering::Relaxed);
            SetTimer(main_window, OSD_FADE_TIMER, OSD_FADE_FRAME_MS, None);
            0
        }
        WM_TIMER if wparam == OSD_FADE_TIMER => {
            let step = OSD_FADE_STEP.fetch_add(1, Ordering::Relaxed);
            let main_window = WINDOW.load(Ordering::Relaxed);
            if step >= OSD_FADE_STEPS {
                KillTimer(main_window, OSD_FADE_TIMER);
                hide_osd();
            } else {
                OSD_OPACITY.store(fade_opacity(step), Ordering::Relaxed);
                repaint_osd_windows(i32::MAX);
            }
            0
        }
        WM_TIMER => {
            let now = now_ms();
            // While a Caps Lock switch is still landing, the foreground window
            // reports the old layout. Use the pending target so the tray flag
            // and the OSD do not flip back to it.
            let current = display_language(now, foreground_language());
            if TRAY_VISIBLE.load(Ordering::Relaxed) {
                set_flag(window, current);
            }
            // WM_INPUTLANGCHANGE is only delivered to the active window's
            // thread, so a switch made in another application never reaches
            // this process through it. Poll the foreground layout here instead,
            // but only when no Caps Lock switch is pending: otherwise the stale
            // foreground layout would restart the OSD with the previous flag.
            // The OSD follows only when the user enabled "show on any switch".
            let previous = LAST_SEEN_LANGUAGE.load(Ordering::Relaxed);
            LAST_SEEN_LANGUAGE.store(flag_language(current), Ordering::Relaxed);
            if !switch_pending(now)
                && previous != 0
                && external_switch_detected(
                    OSD_ANY_SWITCH.load(Ordering::Relaxed),
                    current,
                    previous,
                )
                && is_new_layout_report(now)
            {
                OSD_LAST_MESSAGE_MS.store(now, Ordering::Relaxed);
                show_osd(current);
            }
            refresh_floating();
            0
        }
        WM_INPUTLANGCHANGE => {
            // Covers switching the layout with the Windows hotkey, not only
            // with Caps Lock. Windows also echoes the Caps Lock switch here;
            // that echo must not restart the OSD with a stale language. When
            // "show on any switch" is off, the Caps Lock path already showed
            // the OSD, so this report only updates the tray flag.
            let language = (wparam & 0xffff) as u16;
            let now = now_ms();
            // Prefer the pending target while a Caps Lock switch is landing:
            // this report carries the old language until Windows applies it.
            let shown = display_language(now, language);
            set_flag(window, shown);
            if !switch_pending(now) && is_new_layout_report(now) {
                OSD_LAST_MESSAGE_MS.store(now, Ordering::Relaxed);
                if shows_osd_for_external_switch(OSD_ANY_SWITCH.load(Ordering::Relaxed)) {
                    show_osd(shown);
                }
            }
            0
        }
        WM_DISPLAYCHANGE => {
            if OSD_VISIBLE.load(Ordering::Relaxed) {
                show_osd(OSD_LANGUAGE.load(Ordering::Relaxed));
            }
            0
        }
        TRAY_MESSAGE if lparam == WM_RBUTTONUP as isize => {
            show_menu(window);
            0
        }
        WM_HOTKEY if wparam as i32 == OSD_PREVIEW_HOTKEY_ID => {
            // Shift+F11 previews the OSD with the current settings.
            show_osd(foreground_language());
            0
        }
        WM_COMMAND => {
            handle_command(window, wparam & 0xffff);
            0
        }
        WM_DESTROY => {
            hide_tray(window);
            hide_osd();
            DestroyWindow(FLOAT_WINDOW.load(Ordering::Relaxed));
            for slot in &OSD_WINDOWS {
                let window = slot.load(Ordering::Relaxed);
                if !window.is_null() {
                    // Free the tracked region before destroying the window.
                    if let Some(idx) = OSD_WINDOWS
                        .iter()
                        .position(|s| s.load(Ordering::Relaxed) == window)
                    {
                        let old = OSD_REGIONS[idx].swap(null_mut(), Ordering::Relaxed);
                        if !old.is_null() {
                            DeleteObject(old);
                        }
                    }
                    DestroyWindow(window);
                }
            }
            drop_cached_flag_icons();
            KillTimer(window, 1);
            UnregisterHotKey(window, OSD_PREVIEW_HOTKEY_ID);
            WINDOW.store(null_mut(), Ordering::Relaxed);
            let icon = CURRENT_ICON.swap(null_mut(), Ordering::Relaxed);
            if !icon.is_null() {
                DestroyIcon(icon);
            }
            PostQuitMessage(0);
            0
        }
        _ => DefWindowProcW(window, message, wparam, lparam),
    }
}

/// Shows the OSD once after a settings change, so the result is visible right
/// away. Does nothing when the OSD is off.
unsafe fn preview_osd() {
    if OSD_ENABLED.load(Ordering::Relaxed) {
        // Keep showing the current language while the OSD is already up, so a
        // preview never disagrees with the tray icon.
        let language = if OSD_VISIBLE.load(Ordering::Relaxed) {
            OSD_LANGUAGE.load(Ordering::Relaxed)
        } else {
            foreground_language()
        };
        show_osd(language);
    }
}

unsafe fn handle_command(window: HWND, command: usize) {
    match command {
        CAPS_DEFAULT_ID | CAPS_SWITCH_ID => {
            let enabled = command == CAPS_SWITCH_ID;
            ENABLED.store(enabled, Ordering::Relaxed);
            CAPS_DOWN.store(false, Ordering::Relaxed);
            write_setting("CapsLockEnabled", enabled as u32);
        }
        FLAG_US_ID | FLAG_UK_ID => {
            let setting = (command == FLAG_UK_ID) as u16;
            ENGLISH_FLAG_SETTING.store(setting, Ordering::Relaxed);
            write_setting("EnglishFlag", setting as u32);
            // The UK/US cached icons depend on this setting, so drop them.
            drop_cached_flag_icons();
            CURRENT_LANGUAGE.store(0, Ordering::Relaxed);
            set_flag(window, foreground_language());
            if OSD_ENABLED.load(Ordering::Relaxed) {
                PostMessageW(window, OSD_SHOW_MESSAGE, 0, 0);
            }
        }
        FLOATING_SIZE_75_ID | FLOATING_SIZE_100_ID | FLOATING_SIZE_150_ID
        | FLOATING_SIZE_200_ID => {
            let percent = match command {
                FLOATING_SIZE_75_ID => 75,
                FLOATING_SIZE_100_ID => 100,
                FLOATING_SIZE_150_ID => 150,
                _ => 200,
            };
            set_floating_size(percent);
        }
        FLOATING_ID => {
            set_floating(!FLOATING_VISIBLE.load(Ordering::Relaxed));
        }
        FLOATING_RESET_ID => {
            reset_floating();
        }
        OSD_ID => {
            let enabled = !OSD_ENABLED.load(Ordering::Relaxed);
            OSD_ENABLED.store(enabled, Ordering::Relaxed);
            write_setting("OsdEnabled", enabled as u32);
            if enabled {
                // Immediate feedback that the OSD is back.
                show_osd(foreground_language());
            } else {
                hide_osd();
            }
        }
        _ if osd_monitor_from_id(command).is_some() => {
            if let Some(mode) = osd_monitor_from_id(command) {
                OSD_MONITOR.store(mode, Ordering::Relaxed);
                write_setting("OsdMonitor", mode as u32);
                preview_osd();
            }
        }
        _ if osd_corner_from_id(command).is_some() => {
            if let Some(corner) = osd_corner_from_id(command) {
                OSD_CORNER.store(corner, Ordering::Relaxed);
                write_setting("OsdCorner", corner as u32);
                preview_osd();
            }
        }
        _ if osd_mode_from_id(command).is_some() => {
            if let Some(mode) = osd_mode_from_id(command) {
                OSD_MODE.store(mode, Ordering::Relaxed);
                write_setting("OsdMode", mode as u32);
                preview_osd();
            }
        }
        _ if osd_duration_from_id(command).is_some() => {
            if let Some(duration) = osd_duration_from_id(command) {
                OSD_DURATION.store(duration, Ordering::Relaxed);
                write_setting("OsdDuration", duration as u32);
                preview_osd();
            }
        }
        _ if osd_size_from_id(command).is_some() => {
            if let Some(percent) = osd_size_from_id(command) {
                OSD_SIZE_PERCENT.store(percent, Ordering::Relaxed);
                write_setting("OsdSize", percent as u32);
            }
        }
        OSD_CUSTOM_POS_ID => {
            let enabled = !OSD_CUSTOM_ENABLED.load(Ordering::Relaxed);
            OSD_CUSTOM_ENABLED.store(enabled, Ordering::Relaxed);
            write_setting("OsdCustomEnabled", enabled as u32);
            if !enabled {
                // Leaving drag mode forgets the manual spot: back to the corner.
                reset_osd_custom_position();
            }
            preview_osd();
        }
        OSD_RESET_POS_ID => {
            reset_osd_custom_position();
            preview_osd();
        }
        OSD_ANY_SWITCH_ID => {
            let enabled = !OSD_ANY_SWITCH.load(Ordering::Relaxed);
            OSD_ANY_SWITCH.store(enabled, Ordering::Relaxed);
            write_setting("OsdAnySwitch", enabled as u32);
        }
        HIDE_ID => {
            hide_tray(window);
        }
        AUTOSTART_ID => {
            let enabled = !autostart_enabled();
            if set_autostart(enabled) {
                write_setting("AutostartEnabled", enabled as u32);
            } else {
                show_error("Не удалось изменить настройку автозагрузки.");
            }
        }
        EXIT_ID => {
            DestroyWindow(window);
        }
        _ => {}
    }
}

fn show_error(text: &str) {
    unsafe {
        MessageBoxW(
            null_mut(),
            wide(text).as_ptr(),
            wide("CapsLock Layout").as_ptr(),
            MB_ICONERROR,
        );
    }
}

fn main() {
    unsafe {
        if !is_elevated() {
            if autostart_enabled() && run_schtasks("/Run /TN CapsLockLayout") {
                return;
            }
            show_error("Не удалось перезапустить приложение с правами администратора.");
            return;
        }

        ENABLED.store(read_setting("CapsLockEnabled", 1) != 0, Ordering::Relaxed);
        ENGLISH_FLAG_SETTING.store(read_setting("EnglishFlag", 0) as u16, Ordering::Relaxed);
        let floating_size = read_setting("FloatingSize", 100) as u16;
        FLOATING_SIZE_PERCENT.store(
            if matches!(floating_size, 75 | 100 | 150 | 200) {
                floating_size
            } else {
                100
            },
            Ordering::Relaxed,
        );
        OSD_ENABLED.store(read_setting("OsdEnabled", 1) != 0, Ordering::Relaxed);
        OSD_MONITOR.store(
            valid_osd_monitor_mode(read_setting("OsdMonitor", OSD_MONITOR_CURSOR as u32) as u16),
            Ordering::Relaxed,
        );
        OSD_CORNER.store(
            valid_osd_corner(read_setting("OsdCorner", OSD_CORNER_BELOW_CENTER as u32) as u16),
            Ordering::Relaxed,
        );
        OSD_MODE.store(
            valid_osd_mode(read_setting("OsdMode", OSD_MODE_BOTH as u32) as u16),
            Ordering::Relaxed,
        );
        OSD_SIZE_PERCENT.store(
            valid_osd_size(read_setting("OsdSize", 100) as u16),
            Ordering::Relaxed,
        );
        OSD_DURATION.store(
            valid_osd_duration(read_setting("OsdDuration", OSD_DURATION_NORMAL as u32) as u16),
            Ordering::Relaxed,
        );
        load_osd_custom();
        LAST_SEEN_LANGUAGE.store(flag_language(foreground_language()), Ordering::Relaxed);

        let mutex_name = wide("Local\\CapsLockLayoutSingleInstance");
        let mutex = CreateMutexW(null(), 0, mutex_name.as_ptr());
        if mutex.is_null() {
            show_error("Не удалось запустить приложение.");
            return;
        }
        if GetLastError() == ERROR_ALREADY_EXISTS {
            PostMessageW(HWND_BROADCAST, SHOW_TRAY_MESSAGE, 0, 0);
            CloseHandle(mutex);
            return;
        }

        sync_autostart();

        let instance: HINSTANCE = GetModuleHandleW(null());
        let class_name = wide("CapsLockLayoutWindow");
        let class = WNDCLASSW {
            lpfnWndProc: Some(window_proc),
            hInstance: instance,
            lpszClassName: class_name.as_ptr(),
            ..zeroed()
        };
        if RegisterClassW(&class) == 0 {
            show_error("Не удалось зарегистрировать окно приложения.");
            CloseHandle(mutex);
            return;
        }
        let floating_class_name = wide("CapsLockLayoutFloatingWindow");
        let floating_class = WNDCLASSW {
            lpfnWndProc: Some(floating_proc),
            hInstance: instance,
            lpszClassName: floating_class_name.as_ptr(),
            ..zeroed()
        };
        if RegisterClassW(&floating_class) == 0 {
            show_error("Не удалось зарегистрировать плавающее окно.");
            CloseHandle(mutex);
            return;
        }
        let osd_class_name = wide("CapsLockLayoutOsdWindow");
        let osd_class = WNDCLASSW {
            lpfnWndProc: Some(osd_proc),
            hInstance: instance,
            lpszClassName: osd_class_name.as_ptr(),
            ..zeroed()
        };
        if RegisterClassW(&osd_class) == 0 {
            show_error("Не удалось зарегистрировать OSD языка.");
            CloseHandle(mutex);
            return;
        }

        let window = CreateWindowExW(
            0,
            class_name.as_ptr(),
            wide("CapsLock Layout").as_ptr(),
            0,
            0,
            0,
            0,
            0,
            null_mut(),
            null_mut(),
            instance,
            null(),
        );
        if window.is_null() {
            show_error("Не удалось создать иконку в области уведомлений.");
            CloseHandle(mutex);
            return;
        }
        WINDOW.store(window, Ordering::Relaxed);
        if read_setting("FloatingWindow", 0) != 0 {
            set_floating(true);
        }

        let hook = SetWindowsHookExW(WH_KEYBOARD_LL, Some(keyboard_hook), instance, 0);
        if hook.is_null() {
            show_error("Не удалось установить обработчик Caps Lock.");
            DestroyWindow(window);
            CloseHandle(mutex);
            return;
        }

        let mut message: MSG = zeroed();
        while GetMessageW(&mut message, null_mut(), 0, 0) > 0 {
            TranslateMessage(&message);
            DispatchMessageW(&message);
        }

        UnhookWindowsHookEx(hook);
        CloseHandle(mutex);
    }
}

fn flag_width(base: i32) -> i32 {
    base + base / 4
}

fn autostart_label(enabled: bool) -> &'static str {
    if enabled {
        "Убрать из автозагрузки"
    } else {
        "Добавить в автозагрузку"
    }
}

unsafe fn is_elevated() -> bool {
    let mut token = null_mut();
    if OpenProcessToken(GetCurrentProcess(), TOKEN_QUERY, &mut token) == 0 {
        return false;
    }
    let mut elevation: TOKEN_ELEVATION = zeroed();
    let mut returned = 0;
    let success = GetTokenInformation(
        token,
        TokenElevation,
        &mut elevation as *mut _ as *mut c_void,
        size_of::<TOKEN_ELEVATION>() as u32,
        &mut returned,
    );
    let mut elevation_type: TOKEN_ELEVATION_TYPE = 0;
    let type_success = GetTokenInformation(
        token,
        TokenElevationType,
        &mut elevation_type as *mut _ as *mut c_void,
        size_of::<TOKEN_ELEVATION_TYPE>() as u32,
        &mut returned,
    );
    CloseHandle(token);
    success != 0
        && type_success != 0
        && (elevation_type == TokenElevationTypeFull
            || (elevation_type == TokenElevationTypeDefault && elevation.TokenIsElevated != 0))
}

unsafe fn run_schtasks(arguments: &str) -> bool {
    let mut command = wide(&format!("C:\\Windows\\System32\\schtasks.exe {arguments}"));
    let mut startup: STARTUPINFOW = zeroed();
    startup.cb = size_of::<STARTUPINFOW>() as u32;
    let mut process: PROCESS_INFORMATION = zeroed();
    if CreateProcessW(
        null(),
        command.as_mut_ptr(),
        null(),
        null(),
        0,
        CREATE_NO_WINDOW,
        null(),
        null(),
        &startup,
        &mut process,
    ) == 0
    {
        return false;
    }
    WaitForSingleObject(process.hProcess, 10_000);
    let mut exit_code = 1;
    GetExitCodeProcess(process.hProcess, &mut exit_code);
    CloseHandle(process.hThread);
    CloseHandle(process.hProcess);
    exit_code == 0
}

unsafe fn autostart_enabled() -> bool {
    run_schtasks("/Query /TN CapsLockLayout")
}

unsafe fn set_autostart(enabled: bool) -> bool {
    if enabled {
        register_autostart()
    } else {
        run_schtasks("/Delete /TN CapsLockLayout /F")
    }
}

/// Applies the saved autostart preference on every start. The task is
/// re-registered each time so a moved or rebuilt executable keeps working and
/// stale task settings (for example "start only on AC power") are repaired.
unsafe fn sync_autostart() {
    if read_setting("AutostartEnabled", 1) != 0 {
        register_autostart();
    } else if autostart_enabled() {
        run_schtasks("/Delete /TN CapsLockLayout /F");
    }
}

unsafe fn register_autostart() -> bool {
    let Some(path) = current_exe_path() else {
        return false;
    };
    let Ok(user) = std::env::var("USERNAME") else {
        return false;
    };
    if user.is_empty() {
        return false;
    }

    let xml = task_xml(&path, &user);
    let file = std::env::temp_dir().join("capslock-layout-task.xml");
    let mut bytes = vec![0xff, 0xfe];
    for unit in xml.encode_utf16() {
        bytes.extend_from_slice(&unit.to_le_bytes());
    }
    if std::fs::write(&file, bytes).is_err() {
        return false;
    }
    let created = run_schtasks(&format!(
        "/Create /TN CapsLockLayout /XML \"{}\" /F",
        file.display()
    ));
    let _ = std::fs::remove_file(&file);
    created
}

unsafe fn current_exe_path() -> Option<String> {
    let mut path = [0u16; 32768];
    let length = GetModuleFileNameW(null_mut(), path.as_mut_ptr(), path.len() as u32);
    if length == 0 || length as usize >= path.len() {
        return None;
    }
    Some(String::from_utf16_lossy(&path[..length as usize]))
}

fn xml_escape(text: &str) -> String {
    let mut escaped = String::with_capacity(text.len());
    for character in text.chars() {
        match character {
            '&' => escaped.push_str("&amp;"),
            '<' => escaped.push_str("&lt;"),
            '>' => escaped.push_str("&gt;"),
            _ => escaped.push(character),
        }
    }
    escaped
}

fn task_xml(path: &str, user: &str) -> String {
    let command = xml_escape(&format!("\"{path}\""));
    let user = xml_escape(user);
    format!(
        "<?xml version=\"1.0\" encoding=\"UTF-16\"?>\n\
<Task version=\"1.2\" xmlns=\"http://schemas.microsoft.com/windows/2004/02/mit/task\">\n\
  <RegistrationInfo>\n\
    <Description>CapsLock Layout</Description>\n\
  </RegistrationInfo>\n\
  <Triggers>\n\
    <LogonTrigger>\n\
      <Enabled>true</Enabled>\n\
      <StartBoundary>2000-01-01T00:00:00</StartBoundary>\n\
      <Delay>PT30S</Delay>\n\
      <UserId>{user}</UserId>\n\
    </LogonTrigger>\n\
  </Triggers>\n\
  <Principals>\n\
    <Principal id=\"Author\">\n\
      <UserId>{user}</UserId>\n\
      <LogonType>InteractiveToken</LogonType>\n\
      <RunLevel>HighestAvailable</RunLevel>\n\
    </Principal>\n\
  </Principals>\n\
  <Settings>\n\
    <MultipleInstancesPolicy>IgnoreNew</MultipleInstancesPolicy>\n\
    <DisallowStartIfOnBatteries>false</DisallowStartIfOnBatteries>\n\
    <StopIfGoingOnBatteries>false</StopIfGoingOnBatteries>\n\
    <AllowHardTerminate>true</AllowHardTerminate>\n\
    <StartWhenAvailable>true</StartWhenAvailable>\n\
    <RunOnlyIfNetworkAvailable>false</RunOnlyIfNetworkAvailable>\n\
    <AllowStartOnDemand>true</AllowStartOnDemand>\n\
    <Enabled>true</Enabled>\n\
    <Hidden>false</Hidden>\n\
    <RunOnlyIfIdle>false</RunOnlyIfIdle>\n\
    <WakeToRun>false</WakeToRun>\n\
    <ExecutionTimeLimit>PT0S</ExecutionTimeLimit>\n\
    <Priority>7</Priority>\n\
  </Settings>\n\
  <Actions Context=\"Author\">\n\
    <Exec>\n\
      <Command>{command}</Command>\n\
    </Exec>\n\
  </Actions>\n\
</Task>\n"
    )
}

unsafe fn read_setting(name: &str, default: u32) -> u32 {
    let mut value = default;
    let mut size = size_of::<u32>() as u32;
    let status = RegGetValueW(
        HKEY_CURRENT_USER,
        wide("Software\\CapsLockLayout").as_ptr(),
        wide(name).as_ptr(),
        RRF_RT_REG_DWORD,
        null_mut(),
        &mut value as *mut _ as *mut c_void,
        &mut size,
    );
    if status == ERROR_SUCCESS {
        value
    } else {
        default
    }
}

unsafe fn write_setting(name: &str, value: u32) {
    let mut key = null_mut();
    if RegCreateKeyExW(
        HKEY_CURRENT_USER,
        wide("Software\\CapsLockLayout").as_ptr(),
        0,
        null(),
        REG_OPTION_NON_VOLATILE,
        KEY_SET_VALUE,
        null(),
        &mut key,
        null_mut(),
    ) == ERROR_SUCCESS
    {
        RegSetValueExW(
            key,
            wide(name).as_ptr(),
            0,
            REG_DWORD,
            &value as *const _ as *const u8,
            size_of::<u32>() as u32,
        );
        RegCloseKey(key);
    }
}

fn is_uk_flag(setting: u32) -> bool {
    setting == 1
}

fn floating_label(visible: bool) -> &'static str {
    if visible {
        "Деактивировать плавающее окно"
    } else {
        "Активировать плавающее окно"
    }
}

fn floating_dimension(base: i32, percent: u16) -> i32 {
    base * percent as i32 / 100
}

/// Persists the manual OSD positions so a dragged panel returns to the same
/// spot after a restart. Only the primary OSD window (index 0) is stored; other
/// monitors follow the configured corner.
unsafe fn save_osd_custom_positions() {
    let x = OSD_CUSTOM_X[0].load(Ordering::Relaxed);
    let y = OSD_CUSTOM_Y[0].load(Ordering::Relaxed);
    if x != i32::MIN && y != i32::MIN {
        write_setting("OsdCustomX", x as u32);
        write_setting("OsdCustomY", y as u32);
        write_setting("OsdCustomEnabled", OSD_CUSTOM_ENABLED.load(Ordering::Relaxed) as u32);
    }
}

unsafe fn load_osd_custom() {
    let enabled = read_setting("OsdCustomEnabled", 0) != 0;
    OSD_CUSTOM_ENABLED.store(enabled, Ordering::Relaxed);
    OSD_ANY_SWITCH.store(read_setting("OsdAnySwitch", 1) != 0, Ordering::Relaxed);
    let default = i32::MIN as u32;
    let x = read_setting("OsdCustomX", default) as i32;
    let y = read_setting("OsdCustomY", default) as i32;
    if x != i32::MIN && y != i32::MIN {
        OSD_CUSTOM_X[0].store(x, Ordering::Relaxed);
        OSD_CUSTOM_Y[0].store(y, Ordering::Relaxed);
    }
}

/// Forgets the manual OSD position and returns to the corner placement.
unsafe fn reset_osd_custom_position() {
    for index in 0..MAX_OSD_WINDOWS {
        OSD_CUSTOM_X[index].store(i32::MIN, Ordering::Relaxed);
        OSD_CUSTOM_Y[index].store(i32::MIN, Ordering::Relaxed);
        // Force the next show to recompute geometry instead of reusing it.
        OSD_WINDOW_GEOMETRY[index].store(0, Ordering::Relaxed);
    }
    write_setting("OsdCustomX", i32::MIN as u32);
    write_setting("OsdCustomY", i32::MIN as u32);
}

/// A manually dragged position wins over the corner setting, but only while it
/// still lies on the monitor it was set on. Kept free of Win32 so it can be
/// tested: `None` means "use the corner".
fn custom_position(
    enabled: bool,
    x: i32,
    y: i32,
    width: i32,
    height: i32,
    work: RECT,
) -> Option<(i32, i32)> {
    if !enabled || x == i32::MIN || y == i32::MIN {
        return None;
    }
    let visible = x + width > work.left && x < work.right && y + height > work.top && y < work.bottom;
    visible.then_some((x, y))
}

/// `WM_INPUTLANGCHANGE` shows the OSD only when the user asked for it: with
/// "show on any switch" off, the Caps Lock path alone owns the OSD.
fn shows_osd_for_external_switch(any_switch: bool) -> bool {
    any_switch
}

/// The polled detector used by the timer: an external switch is a language
/// that differs from the one the OSD last showed (compared by flag - a UK
/// English switch does not count as RU/EN), only when the user enabled it.
fn external_switch_detected(any_switch: bool, current: u16, shown: u16) -> bool {
    shows_osd_for_external_switch(any_switch) && flag_language(current) != flag_language(shown)
}

#[cfg(test)]
mod tests;
