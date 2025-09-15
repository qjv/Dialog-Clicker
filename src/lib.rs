use nexus::{
    gui::{register_render, render, RenderType},
    imgui::{TableFlags, InputInt, InputText},
    keybind::{keybind_handler, register_keybind_with_string, unregister_keybind},
    log::LogLevel,
    paths::get_addon_dir,
    AddonFlags, UpdateProvider,
};
use once_cell::sync::Lazy;
use parking_lot::Mutex;
use serde::{Deserialize, Serialize};
use std::{collections::HashSet, fs, path::PathBuf};
use winapi::shared::windef::POINT;
use winapi::um::winuser::{
    GetCursorPos, GetSystemMetrics, SendInput, INPUT, INPUT_MOUSE, MOUSEEVENTF_ABSOLUTE,
    MOUSEEVENTF_LEFTDOWN, MOUSEEVENTF_LEFTUP, MOUSEEVENTF_MOVE, MOUSEEVENTF_RIGHTDOWN,
    MOUSEEVENTF_RIGHTUP, MOUSEINPUT, SM_CXSCREEN, SM_CYSCREEN,
};

// --- Configuration & State Management ---

const CONFIG_FILENAME: &str = "dialog_clicker_config.json";
const LOG_CHANNEL: &str = "Dialog Clicker";

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
enum ClickType {
    Left,
    Right,
}

// FIX: Manually implement `Default` for the ClickType enum.
impl Default for ClickType {
    fn default() -> Self {
        Self::Left
    }
}


#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Default)]
struct Binding {
    coords: [i32; 2],
    click_type: ClickType,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
struct CustomBinding {
    name: String,
    binding: Binding,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
struct Config {
    dialogs: [Binding; 9],
    yes: Binding,
    no: Binding,
    custom: Vec<CustomBinding>,
    next_custom_id: u32,
}

impl Default for Config {
    fn default() -> Self {
        let mut dialogs: [Binding; 9] = Default::default();
        let mut y = -280;
        for i in 0..9 {
            dialogs[i] = Binding { coords: [-260, y], click_type: ClickType::Left };
            y += 48;
        }

        Self {
            dialogs,
            yes: Binding { coords: [0, 54], click_type: ClickType::Left },
            no: Binding { coords: [130, 54], click_type: ClickType::Left },
            custom: Vec::new(),
            next_custom_id: 1,
        }
    }
}


static CONFIG: Lazy<Mutex<Config>> = Lazy::new(Mutex::default);

// --- Nexus Addon Definition ---

nexus::export! {
    name: "Dialog Clicker",
    signature: -98765433,
    load,
    unload,
    flags: AddonFlags::None,
    provider: UpdateProvider::None,
    update_link: "",
}

fn load() {
    load_config_from_file();
    register_all_keybinds();

    register_render(
        RenderType::OptionsRender,
        render!(|ui| {
            let mut config_changed = false;
            let mut custom_to_remove = None;
            let mut custom_to_add = false;
            
            let mut config = CONFIG.lock();

            if let Some(_t) = ui.begin_table_with_flags("Master Table", 4, TableFlags::SIZING_STRETCH_SAME) {
                ui.table_setup_column("Action");
                ui.table_setup_column("X");
                ui.table_setup_column("Y");
                ui.table_setup_column("Details");

                ui.table_next_row();
                ui.table_next_column(); ui.text_disabled("Label");
                ui.table_next_column(); ui.text_disabled("X Coordinate");
                ui.table_next_column(); ui.text_disabled("Y Coordinate");
                ui.table_next_column(); ui.text_disabled("Click Type / Remove");

                ui.table_next_row();
                ui.table_next_column(); ui.text("Current Position");
                ui.table_next_column();
                let (center_x, center_y) = unsafe { (GetSystemMetrics(SM_CXSCREEN) / 2, GetSystemMetrics(SM_CYSCREEN) / 2) };
                let mouse_pos = ui.io().mouse_pos;
                ui.text(format!("{:.0}", mouse_pos[0] - center_x as f32));
                ui.table_next_column();
                ui.text(format!("{:.0}", mouse_pos[1] - center_y as f32));
                ui.table_next_column(); ui.text_disabled("(Relative to screen center)");

                ui.separator();

                for i in 0..9 {
                    ui.table_next_row();
                    ui.table_next_column(); ui.text(format!("Dialog {}", i + 1));
                    ui.table_next_column(); if InputInt::new(ui, &format!("##diag_x_{}", i), &mut config.dialogs[i].coords[0]).build() { config_changed = true; }
                    ui.table_next_column(); if InputInt::new(ui, &format!("##diag_y_{}", i), &mut config.dialogs[i].coords[1]).build() { config_changed = true; }
                }

                ui.table_next_row();
                ui.table_next_column(); ui.text("Yes");
                ui.table_next_column(); if InputInt::new(ui, "##yes_x", &mut config.yes.coords[0]).build() { config_changed = true; }
                ui.table_next_column(); if InputInt::new(ui, "##yes_y", &mut config.yes.coords[1]).build() { config_changed = true; }

                ui.table_next_row();
                ui.table_next_column(); ui.text("No");
                ui.table_next_column(); if InputInt::new(ui, "##no_x", &mut config.no.coords[0]).build() { config_changed = true; }
                ui.table_next_column(); if InputInt::new(ui, "##no_y", &mut config.no.coords[1]).build() { config_changed = true; }

                ui.separator();
                
                let mut name_error: Option<usize> = None;
                for i in 0..config.custom.len() {
                    ui.table_next_row();

                    let (before, remaining) = config.custom.split_at_mut(i);
                    let (current, after) = remaining.split_at_mut(1);
                    let custom = &mut current[0];

                    ui.table_next_column();
                    let old_name = custom.name.clone();
                    if InputText::new(ui, &format!("##custom_name_{}", i), &mut custom.name).build() {
                        let new_name = &custom.name;
                        let is_duplicate = before.iter().any(|c| c.name == *new_name) || after.iter().any(|c| c.name == *new_name);

                        if is_duplicate || new_name.is_empty() {
                            custom.name = old_name; 
                            name_error = Some(i);
                        } else {
                            unregister_keybind(&format!("Dialog Custom {}", old_name));
                            let handler = keybind_handler!(|id, is_release| keybind_handler_logic(Some(id), is_release));
                            let _ = register_keybind_with_string(&format!("Dialog Custom {}", new_name), handler, "");
                            config_changed = true;
                        }
                    }
                    if name_error == Some(i) {
                        ui.same_line();
                        ui.text_colored([1.0, 0.0, 0.0, 1.0], "Name must be unique and not empty!");
                    }
                    
                    ui.table_next_column(); if InputInt::new(ui, &format!("##custom_x_{}", i), &mut custom.binding.coords[0]).build() { config_changed = true; }
                    ui.table_next_column(); if InputInt::new(ui, &format!("##custom_y_{}", i), &mut custom.binding.coords[1]).build() { config_changed = true; }
                    
                    ui.table_next_column();
                    if ui.radio_button_bool(format!("L##{}", i), custom.binding.click_type == ClickType::Left) { custom.binding.click_type = ClickType::Left; config_changed = true; }
                    ui.same_line();
                    if ui.radio_button_bool(format!("R##{}", i), custom.binding.click_type == ClickType::Right) { custom.binding.click_type = ClickType::Right; config_changed = true; }
                    ui.same_line();
                    if ui.button(&format!("-##custom_remove_{}", i)) { custom_to_remove = Some(i); }
                }
            }
            
            if ui.button("[+] Add Custom Macro") { custom_to_add = true; }
            ui.text_disabled("Note: New custom macros must be assigned a key in the main Nexus keybinds menu.");
            ui.separator();
            ui.text_wrapped("Warning: To prevent exploitation, this addon can only simulate a single mouse click per keypress.");
            
            drop(config);

            if let Some(index) = custom_to_remove {
                let mut config = CONFIG.lock();
                let removed = config.custom.remove(index);
                unregister_keybind(&format!("Dialog Custom {}", removed.name));
                config_changed = true;
            }
            if custom_to_add {
                let mut config = CONFIG.lock();
                let mut next_id = 1;
                let existing_names: HashSet<_> = config.custom.iter().map(|c| c.name.as_str()).collect();
                while existing_names.contains(&*format!("New Macro {}", next_id)) {
                    next_id += 1;
                }
                
                let new_binding = CustomBinding {
                    name: format!("New Macro {}", next_id),
                    binding: Binding { coords: [0, 0], click_type: ClickType::Left },
                };
                let handler = keybind_handler!(|id, is_release| keybind_handler_logic(Some(id), is_release));
                let _ = register_keybind_with_string(&format!("Dialog Custom {}", new_binding.name), handler, "");
                config.custom.push(new_binding);
                config.next_custom_id = next_id + 1;
                config_changed = true;
            }

            if config_changed { save_config_to_file(); }
        }),
    ).revert_on_unload();

    nexus::log::log(LogLevel::Info, LOG_CHANNEL, "Dialog Clicker loaded!");
}

fn unload() {
    let config = CONFIG.lock();
    for i in 1..=9 { unregister_keybind(&format!("Dialog {}", i)); }
    unregister_keybind("Dialog Yes");
    unregister_keybind("Dialog No");
    for custom in &config.custom { unregister_keybind(&format!("Dialog Custom {}", custom.name)); }
    nexus::log::log(LogLevel::Info, LOG_CHANNEL, "Dialog Clicker unloaded!");
}

fn register_all_keybinds() {
    let config = CONFIG.lock();
    let handler = keybind_handler!(|id, is_release| keybind_handler_logic(Some(id), is_release));
    
    register_keybind_with_string("Dialog 1", handler, "ALT+1").revert_on_unload();
    register_keybind_with_string("Dialog 2", handler, "ALT+2").revert_on_unload();
    register_keybind_with_string("Dialog 3", handler, "ALT+3").revert_on_unload();
    register_keybind_with_string("Dialog 4", handler, "ALT+4").revert_on_unload();
    register_keybind_with_string("Dialog 5", handler, "ALT+5").revert_on_unload();
    register_keybind_with_string("Dialog 6", handler, "ALT+6").revert_on_unload();
    register_keybind_with_string("Dialog 7", handler, "ALT+7").revert_on_unload();
    register_keybind_with_string("Dialog 8", handler, "ALT+8").revert_on_unload();
    register_keybind_with_string("Dialog 9", handler, "ALT+9").revert_on_unload();
    register_keybind_with_string("Dialog Yes", handler, "ALT+S").revert_on_unload();
    register_keybind_with_string("Dialog No", handler, "ALT+F").revert_on_unload();

    for custom in &config.custom {
        let _ = register_keybind_with_string(&format!("Dialog Custom {}", custom.name), handler, "");
    }
}

fn keybind_handler_logic(id: Option<&str>, is_release: bool) {
    if is_release { return; }
    if let Some(id) = id {
        let config = CONFIG.lock();
        let binding_opt = if id.starts_with("Dialog Custom ") {
            id.strip_prefix("Dialog Custom ").and_then(|name| {
                config.custom.iter().find(|c| c.name == name).map(|c| &c.binding)
            })
        } else {
            match id {
                "Dialog 1" => Some(&config.dialogs[0]), "Dialog 2" => Some(&config.dialogs[1]),
                "Dialog 3" => Some(&config.dialogs[2]), "Dialog 4" => Some(&config.dialogs[3]),
                "Dialog 5" => Some(&config.dialogs[4]), "Dialog 6" => Some(&config.dialogs[5]),
                "Dialog 7" => Some(&config.dialogs[6]), "Dialog 8" => Some(&config.dialogs[7]),
                "Dialog 9" => Some(&config.dialogs[8]), "Dialog Yes" => Some(&config.yes),
                "Dialog No" => Some(&config.no),
                _ => None,
            }
        };

        if let Some(binding) = binding_opt {
            let [x, y] = binding.coords;
            let message = format!("Simulating {} click for keybind '{}' at ({}, {})",
                if binding.click_type == ClickType::Left { "Left" } else { "Right" }, id, x, y);
            nexus::log::log(LogLevel::Info, LOG_CHANNEL, message);
            simulate_click(x, y, &binding.click_type);
        }
    }
}

fn simulate_click(x: i32, y: i32, click_type: &ClickType) {
    unsafe {
        let mut original_pos: POINT = std::mem::zeroed();
        GetCursorPos(&mut original_pos);

        let screen_width = GetSystemMetrics(SM_CXSCREEN);
        let screen_height = GetSystemMetrics(SM_CYSCREEN);
        if screen_width == 0 || screen_height == 0 { return; }

        let absolute_x = (screen_width / 2) + x;
        let absolute_y = (screen_height / 2) + y;

        let nx = (absolute_x as f64 * 65535.0 / screen_width as f64) as i32;
        let ny = (absolute_y as f64 * 65535.0 / screen_height as f64) as i32;
        
        let (down_flag, up_flag) = match click_type {
            ClickType::Left => (MOUSEEVENTF_LEFTDOWN, MOUSEEVENTF_LEFTUP),
            ClickType::Right => (MOUSEEVENTF_RIGHTDOWN, MOUSEEVENTF_RIGHTUP),
        };

        let original_nx = (original_pos.x as f64 * 65535.0 / screen_width as f64 + 1.0) as i32;
        let original_ny = (original_pos.y as f64 * 65535.0 / screen_height as f64 + 1.0) as i32;

        let mut input = [
            INPUT { type_: INPUT_MOUSE, u: std::mem::transmute_copy(&MOUSEINPUT {
                dx: nx, dy: ny, mouseData: 0, dwFlags: MOUSEEVENTF_MOVE | MOUSEEVENTF_ABSOLUTE, time: 0, dwExtraInfo: 0,
            })},
            INPUT { type_: INPUT_MOUSE, u: std::mem::transmute_copy(&MOUSEINPUT {
                dx: 0, dy: 0, mouseData: 0, dwFlags: down_flag, time: 0, dwExtraInfo: 0,
            })},
            INPUT { type_: INPUT_MOUSE, u: std::mem::transmute_copy(&MOUSEINPUT {
                dx: 0, dy: 0, mouseData: 0, dwFlags: up_flag, time: 0, dwExtraInfo: 0,
            })},
            INPUT { type_: INPUT_MOUSE, u: std::mem::transmute_copy(&MOUSEINPUT {
                dx: original_nx, dy: original_ny, mouseData: 0, dwFlags: MOUSEEVENTF_MOVE | MOUSEEVENTF_ABSOLUTE, time: 0, dwExtraInfo: 0,
            })},
        ];
        
        SendInput(input.len() as u32, input.as_mut_ptr(), std::mem::size_of::<INPUT>() as i32);
    }
}

fn get_config_path() -> Option<PathBuf> {
    get_addon_dir("dialog_clicker").map(|p| p.join(CONFIG_FILENAME))
}

fn load_config_from_file() {
    if let Some(path) = get_config_path() {
        if path.exists() {
            if let Ok(json_str) = std::fs::read_to_string(&path) {
                if let Ok(loaded_config) = serde_json::from_str::<Config>(&json_str) {
                    *CONFIG.lock() = loaded_config;
                    return;
                }
            }
        }
    }
    save_config_to_file();
}

fn save_config_to_file() {
    let config = CONFIG.lock();
    if let Some(path) = get_config_path() {
        if let Some(dir) = path.parent() { fs::create_dir_all(dir).ok(); }
        if let Ok(json_str) = serde_json::to_string_pretty(&*config) {
            fs::write(&path, json_str).ok();
        }
    }
}