use super::*;

#[test]
fn switches_from_russian_to_english() {
    assert_eq!(next_language(RUSSIAN, &[ENGLISH, RUSSIAN]), Some(ENGLISH));
}

#[test]
fn switches_from_english_to_russian() {
    assert_eq!(next_language(ENGLISH, &[ENGLISH, RUSSIAN]), Some(RUSSIAN));
}

#[test]
fn does_nothing_when_target_layout_is_not_installed() {
    assert_eq!(next_language(RUSSIAN, &[RUSSIAN]), None);
}

#[test]
fn switches_only_on_the_first_key_down() {
    assert!(should_switch(true, false));
    assert!(!should_switch(true, true));
    assert!(!should_switch(false, false));
}

#[test]
fn falls_back_to_the_us_flag_for_an_unknown_layout() {
    assert_eq!(flag_language(RUSSIAN), RUSSIAN);
    assert_eq!(flag_language(ENGLISH), ENGLISH);
    assert_eq!(flag_language(0x0407), ENGLISH);
}

#[test]
fn ignores_the_layout_change_windows_echoes_after_caps_lock() {
    // Caps Lock switched at 10_000; the system reports the change 5 ms later.
    // That echo must not restart the OSD with a stale language.
    assert!(!should_show_for_report(10_005, 10_000, 0));
    assert!(!should_show_for_report(10_000 + OSD_DUPLICATE_MS - 1, 10_000, 0));
    // Clearly later, it is a real, separate change (for example the Win+Space
    // hotkey), so it does get its own OSD.
    assert!(should_show_for_report(10_000 + OSD_DUPLICATE_MS, 10_000, 0));
}

#[test]
fn ignores_a_duplicate_layout_message() {
    // No Caps Lock involved: Windows sent the same change twice.
    assert!(!should_show_for_report(20_030, 0, 20_000));
    assert!(should_show_for_report(20_000 + OSD_DUPLICATE_MS, 0, 20_000));
}

#[test]
fn ignores_reports_while_the_switch_is_still_running() {
    // `complete_layout_switch` can put the reference in the future while a
    // slow application applies the layout. Such a report must be ignored, and
    // the check must not be tricked by unsigned wrap-around.
    assert!(reference_is_in_future(25_000, 25_500));
    assert!(reference_is_in_future(0, 200));
    assert!(!reference_is_in_future(26_000, 25_500));
}

#[test]
fn measures_time_across_the_tick_counter_wrap() {
    // GetTickCount wraps every ~49 days; the gap must stay a small number
    // instead of a huge one. 11 ms after the switch is still the echo, while
    // the same report 1000 ms after it is a separate change.
    let wrap = 0u32;
    let before = u32::MAX - 10;
    assert_eq!(elapsed_ms(wrap, before), 11);
    let long_ago = u32::MAX - 1000;
    assert!(should_show_for_report(wrap, long_ago, long_ago.wrapping_sub(5)));
    assert!(!should_show_for_report(wrap, before, long_ago));
}

#[test]
fn makes_flags_twenty_five_percent_wider() {
    assert_eq!(flag_width(32), 40);
    assert_eq!(flag_width(16), 20);
}

#[test]
fn reflects_the_current_autostart_state_in_the_menu() {
    assert_eq!(autostart_label(false), "Добавить в автозагрузку");
    assert_eq!(autostart_label(true), "Убрать из автозагрузки");
}

#[test]
fn escapes_xml_special_characters_in_task_paths() {
    assert_eq!(
        xml_escape("C:\\A&B\\<app>.exe"),
        "C:\\A&amp;B\\&lt;app&gt;.exe"
    );
}

#[test]
fn registers_the_task_without_power_restrictions() {
    let xml = task_xml("C:\\Apps\\capslock-layout.exe", "vitac");
    assert!(xml.contains("<Command>\"C:\\Apps\\capslock-layout.exe\"</Command>"));
    assert!(xml.contains("<DisallowStartIfOnBatteries>false</DisallowStartIfOnBatteries>"));
    assert!(xml.contains("<StopIfGoingOnBatteries>false</StopIfGoingOnBatteries>"));
    assert!(xml.contains("<StartWhenAvailable>true</StartWhenAvailable>"));
    assert!(xml.contains("<RunLevel>HighestAvailable</RunLevel>"));
    assert!(xml.contains("<UserId>vitac</UserId>"));
}

#[test]
fn delays_logon_start_until_the_desktop_is_ready() {
    let xml = task_xml("C:\\Apps\\capslock-layout.exe", "vitac");
    assert!(xml.contains("<Delay>PT30S</Delay>"));
}

#[test]
fn maps_only_the_uk_setting_to_the_uk_flag() {
    assert!(!is_uk_flag(0));
    assert!(is_uk_flag(1));
    assert!(!is_uk_flag(2));
}

#[test]
fn reflects_the_floating_window_state_in_the_menu() {
    assert_eq!(floating_label(false), "Активировать плавающее окно");
    assert_eq!(floating_label(true), "Деактивировать плавающее окно");
}

#[test]
fn scales_the_floating_window_without_changing_the_tray_icon() {
    assert_eq!(floating_dimension(40, 75), 30);
    assert_eq!(floating_dimension(40, 100), 40);
    assert_eq!(floating_dimension(40, 150), 60);
    assert_eq!(floating_dimension(40, 200), 80);
    assert_eq!(floating_dimension(32, 200), 64);
}

#[test]
fn names_the_layout_like_the_kde_osd() {
    assert_eq!(osd_short_name(RUSSIAN), "\u{0420}\u{0423}\u{0421}");
    assert_eq!(osd_short_name(ENGLISH), "ENG");
    assert_eq!(osd_full_name(RUSSIAN), "\u{0420}\u{0443}\u{0441}\u{0441}\u{043a}\u{0438}\u{0439}");
    assert_eq!(osd_full_name(ENGLISH), "English");
}

#[test]
fn keeps_only_known_osd_settings() {
    assert_eq!(valid_osd_monitor_mode(OSD_MONITOR_ALL), OSD_MONITOR_ALL);
    assert_eq!(valid_osd_monitor_mode(OSD_MONITOR_ACTIVE), OSD_MONITOR_ACTIVE);
    assert_eq!(valid_osd_monitor_mode(9), OSD_MONITOR_CURSOR);
    for (_, corner) in osd_corner_options() {
        assert_eq!(valid_osd_corner(corner), corner);
    }
    assert_eq!(valid_osd_corner(42), OSD_CORNER_BELOW_CENTER);
    assert_eq!(valid_osd_mode(OSD_MODE_TEXT), OSD_MODE_TEXT);
    assert_eq!(valid_osd_mode(7), OSD_MODE_BOTH);
    for (_, duration) in osd_duration_options() {
        assert_eq!(valid_osd_duration(duration), duration);
    }
    assert_eq!(valid_osd_duration(9), OSD_DURATION_NORMAL);
    for (_, size) in osd_size_options() {
        assert_eq!(valid_osd_size(size), size);
    }
    assert_eq!(valid_osd_size(37), 100);
}

#[test]
fn maps_every_osd_menu_entry_to_its_setting() {
    for (label, corner) in osd_corner_options() {
        assert!(!label.is_empty());
        assert_eq!(osd_corner_id(corner), OSD_CORNER_TOP_LEFT_ID + corner as usize);
        assert_eq!(osd_corner_from_id(osd_corner_id(corner)), Some(corner));
    }
    assert_eq!(osd_corner_from_id(OSD_MODE_FLAGS_ID), None);

    for (label, mode) in osd_mode_options() {
        assert!(!label.is_empty());
        assert_eq!(osd_mode_from_id(osd_mode_id(mode)), Some(mode));
    }

    for (label, monitor) in osd_monitor_options() {
        assert!(!label.is_empty());
        assert_eq!(osd_monitor_from_id(osd_monitor_id(monitor)), Some(monitor));
    }
    assert_eq!(osd_monitor_from_id(EXIT_ID), None);
}

#[test]
fn keeps_the_menu_ids_aligned_with_the_menu_order() {
    // osd_corner_options() and osd_corner_id() must agree on the order: the
    // constant IDs are what the command handler receives back from Win32.
    let corners: Vec<usize> = osd_corner_options()
        .iter()
        .map(|(_, corner)| osd_corner_id(*corner))
        .collect();
    assert_eq!(corners.len(), osd_corner_options().len());
    for (index, id) in corners.iter().enumerate() {
        let expected = OSD_CORNER_TOP_LEFT_ID + osd_corner_options()[index].1 as usize;
        assert_eq!(*id, expected);
        assert!(id <= &OSD_CORNER_LAST_ID);
    }
    let modes: Vec<usize> = osd_mode_options()
        .iter()
        .map(|(_, mode)| osd_mode_id(*mode))
        .collect();
    assert_eq!(
        modes,
        (OSD_MODE_FLAGS_ID..=OSD_MODE_BOTH_ID).collect::<Vec<usize>>()
    );
    let monitors: Vec<usize> = osd_monitor_options()
        .iter()
        .map(|(_, monitor)| osd_monitor_id(*monitor))
        .collect();
    assert_eq!(monitors.first().copied(), Some(OSD_MONITOR_ALL_ID));
    assert_eq!(monitors.last().copied(), Some(OSD_MONITOR_ACTIVE_ID));
}

#[test]
fn shows_flags_or_text_according_to_the_osd_content_setting() {
    assert!(osd_shows_flags(OSD_MODE_FLAGS));
    assert!(!osd_shows_text(OSD_MODE_FLAGS));
    assert!(osd_shows_text(OSD_MODE_TEXT));
    assert!(!osd_shows_flags(OSD_MODE_TEXT));
    assert!(osd_shows_flags(OSD_MODE_BOTH));
    assert!(osd_shows_text(OSD_MODE_BOTH));
}

#[test]
fn fades_from_fully_opaque_to_invisible() {
    assert_eq!(fade_opacity(0), 255);
    assert_eq!(fade_opacity(OSD_FADE_STEPS), 0);
    assert_eq!(fade_opacity(OSD_FADE_STEPS + 5), 0);
    let mut previous = 256;
    for step in 0..=OSD_FADE_STEPS {
        let opacity = fade_opacity(step);
        assert!(opacity < previous);
        previous = opacity;
    }
    assert_eq!(previous, 0);
}

#[test]
fn places_the_osd_below_the_center_by_default() {
    let work = RECT {
        left: 0,
        top: 0,
        right: 1920,
        bottom: 1040,
    };
    let (width, height) = (240, 80);
    let (x, y) = osd_position(OSD_CORNER_BELOW_CENTER, work, width, height);
    assert_eq!(x, (1920 - width) / 2);
    // The OSD sits under the middle of the screen, as KDE shows it: its own
    // centre is below the screen centre by half its height plus the gap.
    let middle = (work.bottom - work.top) / 2;
    assert!(y + height / 2 > middle);
    assert!(y > middle);
    assert!(y + height < work.bottom);

    let (x, y) = osd_position(OSD_CORNER_ABOVE_CENTER, work, width, height);
    assert_eq!(x, (1920 - width) / 2);
    assert!(y + height < middle);

    let (x, y) = osd_position(OSD_CORNER_TOP_LEFT, work, width, height);
    let margin = (height as f32 * 0.3) as i32;
    assert_eq!(x, margin);
    assert_eq!(y, margin);
}

#[test]
fn sizes_the_osd_per_content() {
    assert!(osd_window_size(OSD_MODE_BOTH) > osd_window_size(OSD_MODE_TEXT));
    assert!(osd_window_size(OSD_MODE_TEXT) > osd_window_size(OSD_MODE_FLAGS));
    for mode in [OSD_MODE_FLAGS, OSD_MODE_TEXT, OSD_MODE_BOTH] {
        assert!(osd_window_size(mode) >= 96);
        assert_eq!(osd_window_size(mode) % 4, 0);
    }
}

#[test]
fn draws_a_readable_glass_panel() {
    // The KDE panel: opaque dark fill, feathered at the edge, and an
    // anti-aliased arc in the corner.
    let fill = osd_glass_pixel(120, 40, 240, 80);
    let edge = osd_glass_pixel(40, 0, 240, 80);
    assert!(edge >> 24 < fill >> 24, "the outer row blends into the desktop");
    assert_eq!(osd_glass_pixel(120, -1, 240, 80) & 0xff00_0000, 0);
    let feathered = osd_glass_pixel(0, 40, 240, 80);
    assert!(feathered >> 24 > 0 && feathered >> 24 < fill >> 24);
    assert!(fill >> 24 >= 200);
    let luminance = |p: u32| (p >> 16 & 0xff) + (p >> 8 & 0xff) + (p & 0xff);
    assert!(luminance(fill) < 150, "dark glass");
}

#[test]
fn shapes_the_panel_like_kde() {
    // The bottom right corner is the broad arc; it is cut away while the rest
    // of the panel is solid.
    let (top_left, bottom_right) = osd_corners(240, 80);
    assert!(bottom_right > top_left);
    let cut = osd_glass_pixel(239, 79, 240, 80);
    let square = osd_glass_pixel(239, 2, 240, 80);
    assert_eq!(cut & 0xff00_0000, 0, "bottom-right corner is transparent");
    assert!(square >> 24 > 0, "top right stays filled");
}

#[test]
fn blends_a_source_over_the_panel() {
    let panel = 200u32 << 24 | 0x10 << 16 | 0x10 << 8 | 0x10;
    let white = blend_pixel(panel, (255, 255, 255, 255));
    // The source pixels are premultiplied by their alpha.
    assert_eq!(white, 0xffff_ffff);
    let transparent = blend_pixel(panel, (255, 255, 255, 0));
    assert_eq!(transparent & 0xff00_0000, 200u32 << 24);
    let half = blend_pixel(panel, (255, 255, 255, 128));
    let red = (half >> 16) & 0xff;
    assert!(red > 0x10 && red < 255);
}

#[test]
fn reflects_the_osd_state_in_the_menu() {
    assert_eq!(osd_label(true), "Отключить OSD языка");
    assert_eq!(osd_label(false), "Включить OSD языка");
}

#[test]
fn uses_a_dragged_position_while_it_is_on_screen() {
    let work = RECT {
        left: 0,
        top: 0,
        right: 1920,
        bottom: 1040,
    };
    // Inside the work area: the manual spot wins.
    assert_eq!(custom_position(true, 500, 300, 240, 80, work), Some((500, 300)));
    // Disabled: fall back to the corner.
    assert_eq!(custom_position(false, 500, 300, 240, 80, work), None);
    // Never dragged: fall back to the corner.
    assert_eq!(custom_position(true, i32::MIN, i32::MIN, 240, 80, work), None);
    // Dragged off the monitor (e.g. a display that was unplugged): fall back.
    assert_eq!(custom_position(true, -500, 300, 240, 80, work), None);
    assert_eq!(custom_position(true, 2000, 300, 240, 80, work), None);
    assert_eq!(custom_position(true, 500, 1200, 240, 80, work), None);
    // Partly on screen is still usable.
    assert_eq!(custom_position(true, -100, 300, 240, 80, work), Some((-100, 300)));
}

#[test]
fn shows_the_osd_on_any_switch_only_when_enabled() {
    assert!(shows_osd_for_external_switch(true));
    assert!(!shows_osd_for_external_switch(false));
}

#[test]
fn detects_an_external_switch_by_the_visible_flag() {
    // Enabled: a real RU/EN change is detected, a UK/US change of the same
    // language is not.
    assert!(external_switch_detected(true, ENGLISH, RUSSIAN));
    assert!(external_switch_detected(true, RUSSIAN, ENGLISH));
    assert!(!external_switch_detected(true, RUSSIAN, RUSSIAN));
    assert!(!external_switch_detected(true, ENGLISH, 0x0809));
    // Disabled: never shown from the poller, the Caps Lock path alone owns it.
    assert!(!external_switch_detected(false, ENGLISH, RUSSIAN));
}

#[test]
fn bases_a_switch_on_the_pending_target_while_it_is_in_flight() {
    // First switch: nothing pending, start from the foreground layout.
    assert_eq!(
        switch_start_language(1_000, ENGLISH, 0, 0),
        (ENGLISH, false)
    );
    // A switch to RUSSIAN is in flight: a second press must start from it,
    // otherwise it would compute the same target and undo the switch.
    assert_eq!(
        switch_start_language(1_000, ENGLISH, RUSSIAN, 2_000),
        (RUSSIAN, true)
    );
    // The pending window expired: trust the foreground layout again.
    assert_eq!(
        switch_start_language(3_000, RUSSIAN, RUSSIAN, 2_000),
        (RUSSIAN, false)
    );
}

#[test]
fn shows_the_pending_target_while_a_switch_is_in_flight() {
    // Nothing pending: the foreground layout is shown.
    assert_eq!(display_language(1_000, RUSSIAN), RUSSIAN);
    // A switch to ENGLISH is landing but the window still reports RUSSIAN.
    // The OSD must show ENGLISH, not flip back to the stale layout.
    mark_switch_pending(ENGLISH, 2_000);
    assert_eq!(display_language(1_000, RUSSIAN), ENGLISH);
    // Past the deadline the foreground layout is trusted again.
    assert_eq!(display_language(3_000, ENGLISH), ENGLISH);
    // Reset the global for other tests.
    mark_switch_pending(0, 0);
}

#[test]
fn does_not_block_a_press_once_the_switch_landed() {
    // A switch to RUSSIAN is in flight and the window still reports ENGLISH:
    // such a press is ignored so the two requests cannot race.
    assert!(should_ignore_press(1_000, ENGLISH, RUSSIAN, 2_000));
    // The very same pending window, but the window already reports RUSSIAN:
    // the request landed, so the next press must switch normally.
    assert!(!should_ignore_press(1_000, RUSSIAN, RUSSIAN, 2_000));
    // Nothing pending: never blocked.
    assert!(!should_ignore_press(1_000, ENGLISH, 0, 0));
    // Past the deadline: never blocked.
    assert!(!should_ignore_press(3_000, ENGLISH, RUSSIAN, 2_000));
}

#[test]
fn draws_different_pixels_for_russian_and_english() {
    // The OSD frame is not cached across languages: switching RU/EN must
    // repaint the flag, so the two frames cannot be identical. Draw only the
    // flag area (no text) to isolate the flag from the glyph rendering.
    let (width, height) = (120, 40);
    let mut russian = vec![0u32; (width * height) as usize];
    let mut english = vec![0u32; (width * height) as usize];
    paint_content(&mut russian, width, height, RUSSIAN, OSD_MODE_FLAGS, false);
    paint_content(&mut english, width, height, ENGLISH, OSD_MODE_FLAGS, false);
    assert_ne!(russian, english, "the flag must change between RU and EN");
}

#[test]
fn offers_longer_fade_options() {
    // The default must be long enough to read comfortably, and every option
    // must grow with its index (fast < normal < slow < very slow).
    assert!(osd_display_ms(OSD_DURATION_NORMAL) >= 3_000);
    assert!(osd_display_ms(OSD_DURATION_FAST) < osd_display_ms(OSD_DURATION_NORMAL));
    assert!(osd_display_ms(OSD_DURATION_NORMAL) < osd_display_ms(OSD_DURATION_SLOW));
    assert!(osd_display_ms(OSD_DURATION_SLOW) < osd_display_ms(OSD_DURATION_VERY_SLOW));
    assert_eq!(osd_duration_options().len(), 4);
    for (_, value) in osd_duration_options() {
        assert_eq!(osd_duration_from_id(osd_duration_id(value)), Some(value));
    }
}
