#![windows_subsystem = "windows"]
#![allow(unsafe_op_in_unsafe_fn)]

use std::ffi::c_void;
use std::mem::{size_of, zeroed};
use std::ptr::{null, null_mut};
use std::sync::atomic::{AtomicBool, AtomicPtr, AtomicU16, Ordering};
use windows_sys::Win32::Foundation::{
    COLORREF, CloseHandle, ERROR_ALREADY_EXISTS, ERROR_SUCCESS, GetLastError, HINSTANCE, HWND,
    LPARAM, LRESULT, POINT, RECT, WPARAM,
};
use windows_sys::Win32::Graphics::Gdi::*;
use windows_sys::Win32::Security::{
    GetTokenInformation, TOKEN_ELEVATION, TOKEN_ELEVATION_TYPE, TOKEN_QUERY, TokenElevation,
    TokenElevationType, TokenElevationTypeDefault, TokenElevationTypeFull,
};
use windows_sys::Win32::System::LibraryLoader::{GetModuleFileNameW, GetModuleHandleW};
use windows_sys::Win32::System::Registry::{
    HKEY_CURRENT_USER, KEY_SET_VALUE, REG_DWORD, REG_OPTION_NON_VOLATILE, RRF_RT_REG_DWORD,
    RegCloseKey, RegCreateKeyExW, RegGetValueW, RegSetValueExW,
};
use windows_sys::Win32::System::Threading::{
    CREATE_NO_WINDOW, CreateMutexW, CreateProcessW, GetCurrentProcess, GetExitCodeProcess,
    OpenProcessToken, PROCESS_INFORMATION, STARTUPINFOW, WaitForSingleObject,
};
use windows_sys::Win32::UI::Input::KeyboardAndMouse::{
    GetKeyboardLayout, GetKeyboardLayoutList, HKL, VK_CAPITAL,
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
const MAX_LAYOUTS: usize = 32;

static ENABLED: AtomicBool = AtomicBool::new(true);
static CAPS_DOWN: AtomicBool = AtomicBool::new(false);
static TRAY_VISIBLE: AtomicBool = AtomicBool::new(false);
static WINDOW: AtomicPtr<c_void> = AtomicPtr::new(null_mut());
static CURRENT_ICON: AtomicPtr<c_void> = AtomicPtr::new(null_mut());
static CURRENT_LANGUAGE: AtomicU16 = AtomicU16::new(0);
static ENGLISH_FLAG_SETTING: AtomicU16 = AtomicU16::new(0);

fn next_language(current: u16, available: &[u16]) -> Option<u16> {
    let target = if current == RUSSIAN { ENGLISH } else { RUSSIAN };
    available.contains(&target).then_some(target)
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

unsafe fn foreground_language() -> u16 {
    let foreground = GetForegroundWindow();
    if foreground.is_null() {
        return ENGLISH;
    }
    let thread = GetWindowThreadProcessId(foreground, null_mut());
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
    let current = (GetKeyboardLayout(thread) as usize & 0xffff) as u16;
    if let Some(target) = next_language(current, &languages[..count]) {
        let handle = layouts[..count]
            .iter()
            .copied()
            .find(|layout| (*layout as usize & 0xffff) as u16 == target);
        if let Some(handle) = handle {
            let owner = GetWindow(foreground, GW_OWNER);
            let destination = if owner.is_null() { foreground } else { owner };
            SendMessageTimeoutW(
                destination,
                WM_INPUTLANGCHANGEREQUEST,
                0,
                handle as isize,
                SMTO_ABORTIFHUNG,
                1000,
                null_mut(),
            );
            set_flag(WINDOW.load(Ordering::Relaxed), target);
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
    let dc = CreateCompatibleDC(screen);
    let color = CreateCompatibleBitmap(screen, width, height);
    let mask = CreateBitmap(width, height, 1, 1, null());
    ReleaseDC(null_mut(), screen);
    SelectObject(dc, color);

    if flag_language(language) == RUSSIAN {
        fill_rect(dc, 0, 0, width, height / 3, 0x00ff_ffff);
        fill_rect(dc, 0, height / 3, width, height * 2 / 3, 0x00a6_3900);
        fill_rect(dc, 0, height * 2 / 3, width, height, 0x001e_2bd5);
    } else if is_uk_flag(ENGLISH_FLAG_SETTING.load(Ordering::Relaxed) as u32) {
        fill_rect(dc, 0, 0, width, height, 0x0069_2101);
        let diagonal_white = (width / 6).max(2);
        let diagonal_red = (width / 12).max(1);
        for (x1, y1, x2, y2) in [(0, 0, width, height), (width, 0, 0, height)] {
            draw_line(dc, x1, y1, x2, y2, diagonal_white, 0x00ff_ffff);
            draw_line(dc, x1, y1, x2, y2, diagonal_red, 0x002e_10c8);
        }
        fill_rect(dc, width * 3 / 8, 0, width * 5 / 8, height, 0x00ff_ffff);
        fill_rect(dc, 0, height * 3 / 8, width, height * 5 / 8, 0x00ff_ffff);
        fill_rect(dc, width * 7 / 16, 0, width * 9 / 16, height, 0x002e_10c8);
        fill_rect(dc, 0, height * 7 / 16, width, height * 9 / 16, 0x002e_10c8);
    } else {
        let stripe_height = (height + 12) / 13;
        for stripe in 0..13 {
            let color = if stripe % 2 == 0 {
                0x0034_22b2
            } else {
                0x00ff_ffff
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
        fill_rect(dc, 0, 0, canton_width, canton_height, 0x006e_3b3c);
        for row in 0..4 {
            for column in 0..5 {
                let x = 2 + column * (canton_width - 4) / 5;
                let y = 2 + row * (canton_height - 4) / 4;
                fill_rect(dc, x, y, x + 1, y + 1, 0x00ff_ffff);
            }
        }
    }

    let mut info: ICONINFO = zeroed();
    info.fIcon = 1;
    info.hbmMask = mask;
    info.hbmColor = color;
    let icon = CreateIconIndirect(&info);
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

unsafe fn show_menu(window: HWND) {
    let menu = CreatePopupMenu();
    let behavior_menu = CreatePopupMenu();
    let flag_menu = CreatePopupMenu();
    if menu.is_null() || behavior_menu.is_null() || flag_menu.is_null() {
        return;
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

    AppendMenuW(menu, MF_STRING, HIDE_ID, wide("Скрыть из трея").as_ptr());
    AppendMenuW(
        menu,
        MF_STRING,
        AUTOSTART_ID,
        wide(autostart_label(autostart_enabled())).as_ptr(),
    );
    AppendMenuW(menu, MF_SEPARATOR, 0, null());
    AppendMenuW(menu, MF_STRING, EXIT_ID, wide("Выход").as_ptr());

    let mut point: POINT = zeroed();
    GetCursorPos(&mut point);
    SetForegroundWindow(window);
    TrackPopupMenu(menu, TPM_RIGHTBUTTON, point.x, point.y, 0, window, null());
    DestroyMenu(menu);
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
            SetTimer(window, 1, 1000, None);
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
        WM_TIMER => {
            if TRAY_VISIBLE.load(Ordering::Relaxed) {
                set_flag(window, foreground_language());
            }
            0
        }
        TRAY_MESSAGE if lparam == WM_RBUTTONUP as isize => {
            show_menu(window);
            0
        }
        WM_COMMAND => match wparam & 0xffff {
            CAPS_DEFAULT_ID | CAPS_SWITCH_ID => {
                let enabled = wparam & 0xffff == CAPS_SWITCH_ID;
                ENABLED.store(enabled, Ordering::Relaxed);
                CAPS_DOWN.store(false, Ordering::Relaxed);
                write_setting("CapsLockEnabled", enabled as u32);
                0
            }
            FLAG_US_ID | FLAG_UK_ID => {
                let setting = (wparam & 0xffff == FLAG_UK_ID) as u16;
                ENGLISH_FLAG_SETTING.store(setting, Ordering::Relaxed);
                write_setting("EnglishFlag", setting as u32);
                CURRENT_LANGUAGE.store(0, Ordering::Relaxed);
                set_flag(window, foreground_language());
                0
            }
            HIDE_ID => {
                hide_tray(window);
                0
            }
            AUTOSTART_ID => {
                if !set_autostart(!autostart_enabled()) {
                    show_error("Не удалось изменить настройку автозагрузки.");
                }
                0
            }
            EXIT_ID => {
                DestroyWindow(window);
                0
            }
            _ => 0,
        },
        WM_DESTROY => {
            hide_tray(window);
            KillTimer(window, 1);
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
            let task_ready = autostart_enabled() || set_autostart(true);
            if task_ready && run_schtasks("/Run /TN CapsLockLayout") {
                return;
            }
            show_error("Не удалось перезапустить приложение с правами администратора.");
            return;
        }

        ENABLED.store(read_setting("CapsLockEnabled", 1) != 0, Ordering::Relaxed);
        ENGLISH_FLAG_SETTING.store(read_setting("EnglishFlag", 0) as u16, Ordering::Relaxed);

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
    if !enabled {
        return run_schtasks("/Delete /TN CapsLockLayout /F");
    }
    let mut path = [0u16; 260];
    let length = GetModuleFileNameW(null_mut(), path.as_mut_ptr(), path.len() as u32);
    if length == 0 || length as usize >= path.len() {
        return false;
    }
    let path = String::from_utf16_lossy(&path[..length as usize]);
    run_schtasks(&format!(
        "/Create /TN CapsLockLayout /TR \"\\\"{path}\\\"\" /SC ONLOGON /RL HIGHEST /F"
    ))
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

#[cfg(test)]
mod tests;
