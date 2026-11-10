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
