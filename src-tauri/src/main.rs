// Prevents additional console window on Windows in release, DO NOT REMOVE!!
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use brew::Package;
use chrono::prelude::{DateTime, Utc};
use dialog::DialogBox;
use flate2::read::GzDecoder;
use icns::{IconFamily, IconType};
// use rust_search::{similarity_sort, SearchBuilder};
use rusty_ytdl::{Video, VideoOptions, VideoQuality, VideoSearchOptions};
use serde_json::Value;
use std::fs::{self, read_dir, ReadDir};
use std::io::{BufReader, BufWriter, Read, Write};
use std::process::{Command, Stdio};
use std::{
    env::{current_dir, set_current_dir},
    fs::{copy, create_dir, remove_dir_all, remove_file, File},
    path::PathBuf,
};
use stopwatch::Stopwatch;
use tauri::{
    api::path::{
        app_config_dir, audio_dir, config_dir, desktop_dir, document_dir, download_dir, home_dir,
        picture_dir, video_dir,
    },
    Config,
};
use tauri::{Manager, Window, WindowEvent};
use unrar::Archive;
use zip::write::FileOptions;
use zip_extensions::*;
mod utils;
use rayon::prelude::*;
use sysinfo::Disks;
use utils::{
    calc_transfer_speed, copy_to, count_entries, dbg_log, err_log, format_bytes, unpack_tar,
    update_progressbar, update_progressbar_2, wng_log, DirWalker, DirWalkerEntry, COPY_COUNTER,
    TO_COPY_COUNTER,
};
#[cfg(target_os = "macos")]
mod window_tauri_ext;
#[cfg(target_os = "macos")]
use window_tauri_ext::WindowExt;
mod applications;
use applications::{get_apps, open_file_with};
use archiver_rs::Compressed;
mod rdpfs;
use substring::Substring;

static mut ISCANCELED: bool = false;

static mut PATH_HISTORY: Vec<String> = vec![];

// #[cfg(target_os = "windows")]
// const SLASH: &str = "\\";
#[cfg(target_os = "windows")]
const ASSET_LOCATION: &str = "https://asset.localhost/";

// #[cfg(not(target_os = "windows"))]
// const SLASH: &str = "/";
#[cfg(not(target_os = "windows"))]
const ASSET_LOCATION: &str = "asset://localhost/";

fn main() {
    tauri::Builder::default()
        .setup(|app| {
            #[cfg(target_os = "macos")]
            let win = app.get_window("main").unwrap();
            #[cfg(target_os = "macos")]
            win.set_transparent_titlebar(true);
            #[cfg(target_os = "macos")]
            win.position_traffic_lights(20.0, 25.0);
            Ok(())
        })
        .on_window_event(|e| {
            #[cfg(target_os = "macos")]
            if let WindowEvent::Resized(..) = e.event() {
                let win = e.window();
                win.position_traffic_lights(20.0, 25.0);
            }
        })
        .invoke_handler(tauri::generate_handler![
            list_dirs,
            open_dir,
            open_item,
            go_back,
            go_home,
            search_for,
            go_to_dir,
            copy_paste,
            delete_item,
            extract_item,
            compress_item,
            create_folder,
            switch_view,
            check_app_config,
            create_file,
            get_current_dir,
            set_dir,
            list_disks,
            open_in_terminal,
            rename_element,
            save_config,
            switch_to_directory,
            mount_sshfs,
            rename_elements_with_format,
            add_favorite,
            arr_copy_paste,
            arr_delete_items,
            arr_compress_items,
            get_installed_apps,
            open_with,
            find_duplicates,
            cancel_operation,
            get_df_dir,
            download_yt_video,
            // get_llm_response,
            get_app_icns,
            get_thumbnail,
            install_dep,
            get_dir_size,
            get_themes,
            stop_searching,
            get_file_content,
            open_config_location
        ])
        .plugin(tauri_plugin_drag::init())
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

#[derive(serde::Serialize, serde::Deserialize, Clone, Debug)]
struct FDir {
    name: String,
    is_dir: i8,
    path: String,
    extension: String,
    size: String,
    last_modified: String,
}

#[derive(serde::Serialize, serde::Deserialize, Clone, Debug)]
struct Theme {
    name: String,
    primary_color: String,
    secondary_color: String,
    tertiary_color: String,
    text_color: String,
    text_color2: String,
    text_color3: String,
    transparent_color: String,
    transparent_color_active: String,
    site_bar_color: String,
    nav_bar_color: String,
}

#[derive(serde::Serialize)]
struct AppConfig {
    view_mode: String,
    last_modified: String,
    configured_path_one: String,
    configured_path_two: String,
    configured_path_three: String,
    is_open_in_terminal: String,
    is_dual_pane_enabled: String,
    launch_path: String,
    is_dual_pane_active: String,
    search_depth: i32,
    max_items: i32,
    is_image_preview: String,
    is_select_mode: String,
    arr_favorites: Vec<String>,
    current_theme: String,
}

#[tauri::command]
async fn check_app_config() -> AppConfig {
    // Create general config directory for the app
    create_folder(
        config_dir()
            .unwrap()
            .join("com.rdpFX.dev")
            .to_str()
            .unwrap()
            .to_string(),
    )
    .await;
    // Create directory for saving app thumnails on a mac
    create_folder(
        config_dir()
            .unwrap()
            .join("com.rdpFX.dev")
            .join("App-Thumbnails")
            .to_str()
            .unwrap()
            .to_string(),
    )
    .await;
    // Create a directory for themes
    create_folder(
        config_dir()
            .unwrap()
            .join("com.rdpFX.dev")
            .join("Themes")
            .to_str()
            .unwrap()
            .to_string(),
    )
    .await;

    // If config doesn't exist, create it
    if fs::metadata(config_dir().unwrap().join("com.rdpFX.dev/app_config.json")).is_err() {
        let _ = File::create(config_dir().unwrap().join("com.rdpFX.dev/app_config.json"));
        let app_config_json = AppConfig {
            view_mode: "".to_string(),
            last_modified: chrono::offset::Local::now().to_string(),
            configured_path_one: "".to_string(),
            configured_path_two: "".to_string(),
            configured_path_three: "".to_string(),
            is_open_in_terminal: "0".to_string(),
            is_dual_pane_enabled: "0".to_string(),
            launch_path: "".to_string(),
            is_dual_pane_active: "0".to_string(),
            search_depth: 10,
            max_items: 1000,
            is_image_preview: "0".to_string(),
            is_select_mode: "1".to_string(),
            arr_favorites: vec![],
            current_theme: "0".to_string(),
        };
        let _ = serde_json::to_writer_pretty(
            File::create(
                config_dir()
                    .unwrap()
                    .join("com.rdpFX.dev/app_config.json")
                    .to_str()
                    .unwrap()
                    .to_string(),
            )
            .unwrap(),
            &app_config_json,
        );
    }

    let app_config_file =
        File::open(config_dir().unwrap().join("com.rdpFX.dev/app_config.json")).unwrap();
    let app_config_reader = BufReader::new(app_config_file);
    let app_config: Value = serde_json::from_reader(app_config_reader).unwrap();

    let default_vec: Vec<Value> = vec![];
    return AppConfig {
        view_mode: app_config["view_mode"].to_string().replace('"', ""),
        last_modified: app_config["last_modified"].to_string().replace('"', ""),
        configured_path_one: app_config["configured_path_one"]
            .to_string()
            .replace('"', ""),
        configured_path_two: app_config["configured_path_two"]
            .to_string()
            .replace('"', ""),
        configured_path_three: app_config["configured_path_three"]
            .to_string()
            .replace('"', ""),
        is_open_in_terminal: app_config["is_open_in_terminal"]
            .to_string()
            .replace('"', ""),
        is_dual_pane_enabled: app_config["is_dual_pane_enabled"]
            .to_string()
            .replace('"', ""),
        launch_path: app_config["launch_path"].to_string().replace('"', ""),
        is_dual_pane_active: app_config["is_dual_pane_active"]
            .to_string()
            .replace('"', ""),
        search_depth: app_config["search_depth"]
            .to_string()
            .parse::<i32>()
            .unwrap(),
        max_items: app_config["max_items"].to_string().parse::<i32>().unwrap(),
        is_image_preview: app_config["is_image_preview"].to_string().replace('"', ""),
        is_select_mode: app_config["is_select_mode"].to_string().replace('"', ""),
        arr_favorites: app_config["arr_favorites"]
            .as_array()
            .unwrap_or_else(|| &default_vec)
            .iter()
            .map(|x| x.to_string().replace('"', ""))
            .collect(),
        current_theme: app_config["current_theme"].to_string().replace('"', ""),
    };
}

#[tauri::command]
async fn get_themes() -> Vec<Theme> {
    let mut vec_themes: Vec<Theme> = vec![];
    let themes = read_dir(config_dir().unwrap().join("com.rdpFX.dev").join("Themes"));
    for theme_entry in themes.unwrap() {
        let app_config_file = File::open(theme_entry.unwrap().path()).unwrap();
        let app_config_reader = BufReader::new(app_config_file);
        let app_config: Value = serde_json::from_reader(app_config_reader).unwrap();
        vec_themes.push(Theme {
            name: app_config["name"].to_string().replace('"', ""),
            primary_color: app_config["primary_color"].to_string().replace('"', ""),
            secondary_color: app_config["secondary_color"].to_string().replace('"', ""),
            tertiary_color: app_config["tertiary_color"].to_string().replace('"', ""),
            text_color: app_config["text_color"].to_string().replace('"', ""),
            text_color2: app_config["text_color2"].to_string().replace('"', ""),
            text_color3: app_config["text_color3"].to_string().replace('"', ""),
            transparent_color: app_config["transparent_color"].to_string().replace('"', ""),
            transparent_color_active: app_config["transparent_color_active"]
                .to_string()
                .replace('"', ""),
            site_bar_color: app_config["site_bar_color"].to_string().replace('"', ""),
            nav_bar_color: app_config["nav_bar_color"].to_string().replace('"', ""),
        })
    }
    vec_themes
}

#[derive(serde::Serialize)]
struct DisksInfo {
    name: String,
    dev: String,
    format: String,
    path: String,
    avail: String,
    capacity: String,
    is_removable: bool,
}

#[tauri::command]
async fn list_disks() -> Vec<DisksInfo> {
    let mut ls_disks: Vec<DisksInfo> = vec![];
    let disks = Disks::new_with_refreshed_list();
    for disk in &disks {
        dbg_log(format!("{:?}", &disk));
        ls_disks.push(DisksInfo {
            name: format!("{:?}", disk.mount_point())
                .split("/")
                .last()
                .unwrap_or("/")
                .to_string()
                .replace("\"", ""),
            dev: format!("{:?}", disk.name()),
            format: format!("{:?}", disk.file_system().to_string_lossy()),
            path: format!("{:?}", disk.mount_point()),
            avail: format!("{:?}", disk.available_space()),
            capacity: format!("{:?}", disk.total_space()),
            is_removable: disk.is_removable(),
        });
    }

    let ls_sshfs_mounts = fs::read_dir("/tmp/rdpFX-sshfs-mount");
    if ls_sshfs_mounts.is_ok() {
        let ls_sshfs_mounts = ls_sshfs_mounts.unwrap();
        for mount in ls_sshfs_mounts {
            let mount = mount.unwrap();
            dbg_log(format!("{:?} || {:?}", mount.file_name(), mount.path()));
            ls_disks.push(DisksInfo {
                name: format!("{:?}", mount.file_name())
                    .split("/")
                    .last()
                    .unwrap_or("/")
                    .to_string()
                    .replace("\"", ""),
                dev: format!("{:?}", mount.file_name()),
                format: format!("{:?}", mount.file_type()),
                path: format!("{:?}", mount.path()),
                avail: format!("{:?}", mount.metadata().unwrap().len()),
                capacity: format!("{:?}", mount.metadata().unwrap().len()),
                is_removable: true,
            });
        }
    }
    return ls_disks;
}

#[tauri::command]
async fn switch_to_directory(current_dir: String) {
    dbg_log(format!("Switching to directory: {}", &current_dir));
    set_current_dir(current_dir).unwrap();
}
#[tauri::command]
async fn switch_view(view_mode: String) -> Vec<FDir> {
    let app_config_file = File::open(
        app_config_dir(&Config::default())
            .unwrap()
            .join("com.rdpFX.dev/app_config.json"),
    )
    .unwrap();
    let app_config_reader = BufReader::new(app_config_file);
    let mut app_config: Value = serde_json::from_reader(app_config_reader).unwrap();
    app_config["view_mode"] = Value::from(String::from(&view_mode));
    let _ = serde_json::to_writer_pretty(
        File::create(
            app_config_dir(&Config::default())
                .unwrap()
                .join("com.rdpFX.dev/app_config.json")
                .to_str()
                .unwrap()
                .to_string(),
        )
        .unwrap(),
        &app_config,
    );
    dbg_log(format!("View-style switched to: {}", view_mode));
    return list_dirs().await;
}

#[tauri::command]
async fn get_current_dir() -> String {
    return current_dir()
        .unwrap()
        .as_path()
        .to_str()
        .unwrap()
        .to_string()
        .replace("\\", "/");
}

#[tauri::command]
async fn set_dir(current_dir: String) {
    dbg_log(format!("Current dir: {}", &current_dir));
    let _ = set_current_dir(current_dir);
}

#[tauri::command]
async fn list_dirs() -> Vec<FDir> {
    let mut dir_list: Vec<FDir> = Vec::new();
    let current_dir = fs::read_dir(current_dir().unwrap()).unwrap();
    for item in current_dir {
        let temp_item = item.unwrap();
        let name = &temp_item.file_name().into_string().unwrap();
        let path = &temp_item
            .path()
            .to_str()
            .unwrap()
            .to_string()
            .replace("\\", "/");
        let file_ext = ".".to_string().to_owned()
            + &path
                .split(".")
                .nth(&path.split(".").count() - 1)
                .unwrap_or("");
        let file_data = fs::metadata(&temp_item.path());
        let file_date: DateTime<Utc>;
        let size = temp_item.metadata().unwrap().len();
        if file_data.is_ok() {
            file_date = file_data.unwrap().modified().unwrap().clone().into();
        } else {
            file_date = Utc::now();
        }
        let is_dir_int = match temp_item.path().is_dir() {
            true => 1,
            false => 0,
        };
        dir_list.push(FDir {
            name: String::from(name),
            is_dir: is_dir_int,
            path: String::from(path),
            extension: file_ext,
            size: size.to_string(),
            last_modified: String::from(file_date.to_string().split(".").nth(0).unwrap()),
        });
    }
    dir_list.sort_by_key(|a| a.name.to_lowercase());
    return dir_list;
}

#[allow(dead_code)]
fn alert_not_found_dir(_x: std::io::Error) -> ReadDir {
    dialog::Message::new("No directory found or unable to open due to missing permissions")
        .title("No directory found")
        .show()
        .expect("Error opening dialog");
    return fs::read_dir(current_dir().unwrap()).unwrap();
}

#[tauri::command]
async fn open_dir(path: String) -> Vec<FDir> {
    let _ = set_current_dir(&path);
    unsafe {
        PATH_HISTORY.push(path);
        dbg_log(format!("Path history: {:?}", PATH_HISTORY));
    }
    return list_dirs().await;
}

#[tauri::command]
async fn go_back() -> Vec<FDir> {
    unsafe {
        if PATH_HISTORY.len() > 1 {
            let last_path = &PATH_HISTORY[PATH_HISTORY.len() - 2];
            dbg_log(format!("Went back to: {}", last_path));
            let _ = set_current_dir(last_path);
            PATH_HISTORY.pop();
        } else {
            let _ = set_current_dir("./../");
        }
    }
    return list_dirs().await;
}

#[tauri::command]
async fn go_to_dir(directory: u8) -> Vec<FDir> {
    let wanted_directory = match directory {
        0 => set_current_dir(desktop_dir().unwrap_or_default()),
        1 => set_current_dir(download_dir().unwrap_or_default()),
        2 => set_current_dir(document_dir().unwrap_or_default()),
        3 => set_current_dir(picture_dir().unwrap_or_default()),
        4 => set_current_dir(video_dir().unwrap_or_default()),
        5 => set_current_dir(audio_dir().unwrap_or_default()),
        _ => set_current_dir(current_dir().unwrap()),
    };
    if wanted_directory.is_err() {
        err_log("Not a valid directory".into());
        dialog::Message::new("Not a valid directory")
            .show()
            .unwrap();
    } else {
        unsafe {
            PATH_HISTORY.push(current_dir().unwrap().to_string_lossy().to_string());
        }
    }
    return list_dirs().await;
}

// :ftp
#[tauri::command]
async fn mount_sshfs(
    hostname: String,
    username: String,
    password: String,
    remote_path: String,
) -> Vec<FDir> {
    let remote_address = format!("{}@{}:{}", username, hostname, remote_path);

    let mount_point = "/tmp/rdpFX-sshfs-mount/".to_owned() + &hostname;

    // Ensure the local mount point exists
    std::fs::create_dir_all(&mount_point).expect("Failed to create mount point directory");

    // Start sshfs process
    let mut child = Command::new("sshfs")
        .arg(&remote_address)
        .arg(&mount_point)
        .arg("-o")
        .arg("password_stdin")
        .stdin(Stdio::piped())
        .spawn()
        .expect("Failed to start sshfs process");

    // Write the password to stdin of the sshfs process
    dbg_log(format!("Connecting to {}", remote_address));
    let stdin = child.stdin.as_mut().expect("Failed to open stdin");
    stdin
        .write_all(password.as_bytes())
        .expect("Failed to write to stdin");

    let output = child
        .wait_with_output()
        .expect("Failed to read sshfs output");

    if output.status.success() {
        dbg_log(format!("Mounted {} to {}", remote_address, mount_point));
    } else {
        wng_log(format!(
            "Failed to mount: {}",
            String::from_utf8_lossy(&output.stderr),
        ));
    }

    return open_dir(mount_point).await;
}

#[tauri::command]
async fn open_in_terminal() {
    // TODO: implement
    // open terminal on macOS
    // open terminal on linux
    // open terminal on windows
}

#[tauri::command]
async fn go_home() -> Vec<FDir> {
    let _ = set_current_dir(home_dir().unwrap());
    unsafe {
        PATH_HISTORY.push(home_dir().unwrap().to_string_lossy().to_string());
    }
    return list_dirs().await;
}

#[tauri::command]
async fn stop_searching() {
    dbg_log(format!("Stopped searching: {}", unsafe { IS_SEARCHING }));
    unsafe {
        IS_SEARCHING = false;
        COUNT_CALLED_BACK = 0;
    }
}

static mut IS_SEARCHING: bool = false;
static mut COUNT_CALLED_BACK: i32 = 0;
#[tauri::command]
async fn search_for(
    mut file_name: String,
    max_items: i32,
    search_depth: i32,
    file_content: String,
    app_window: Window,
    is_quick_search: bool,
) {
    unsafe {
        IS_SEARCHING = true;
        COUNT_CALLED_BACK = 0;
    }
    let _ = app_window.eval("$('.file-searching-file-count').css('display', 'block')");
    let _ = app_window.eval(&format!(
        "$('.file-searching-file-count').html('{} items found')",
        unsafe { COUNT_CALLED_BACK }
    ));
    dbg_log(format!(
        "Start searching for: {} with depth: {}, max items: {}, content: {}, threads: {}",
        &file_name,
        search_depth,
        max_items,
        &file_content,
        num_cpus::get()
    ));
    let temp_file_name = String::from(&file_name);
    if temp_file_name.split(".").nth(0).unwrap().contains("*") {
        file_name = temp_file_name.trim().replace("*", "");
    }

    let file_ext = ".".to_string().to_owned()
        + file_name
            .split(".")
            .nth(file_name.split(".").count() - 1)
            .unwrap_or("");

    let sw = Stopwatch::start_new();

    if file_ext != ".".to_string().to_owned() + &file_name {
        let _ = DirWalker::new()
            .set_ext(vec![file_ext.to_lowercase()])
            .search(
                current_dir().unwrap().to_str().unwrap(),
                search_depth as u32,
                file_name,
                max_items,
                is_quick_search,
                &|item: DirWalkerEntry| {
                    let _ = app_window
                        .emit_all(
                            "addSingleItem",
                            serde_json::to_string(&item).unwrap().to_string(),
                        )
                        .expect("Failed to emit");
                    unsafe {
                        COUNT_CALLED_BACK += 1;
                    }
                    let _ = app_window.eval(&format!(
                        "$('.file-searching-file-count').html('{} items found')",
                        unsafe { COUNT_CALLED_BACK }
                    ));
                },
            );
    } else {
        unsafe {
            COUNT_CALLED_BACK = 0;
        }
        let _ = DirWalker::new().search(
            current_dir().unwrap().to_str().unwrap(),
            search_depth as u32,
            file_name,
            max_items,
            is_quick_search,
            &|item| {
                let _ = app_window
                    .emit_all(
                        "addSingleItem",
                        serde_json::to_string(&item).unwrap().to_string(),
                    )
                    .expect("Failed to emit");
                unsafe {
                    COUNT_CALLED_BACK += 1;
                }
                let _ = app_window.eval(&format!(
                    "$('.file-searching-file-count').html('{} items found')",
                    unsafe { COUNT_CALLED_BACK }
                ));
            },
        );
    }
    unsafe {
        IS_SEARCHING = false;
    }
    let _ = app_window.eval("$('.file-searching-done').css('display', 'block')");
    let _ = app_window.eval("$('.is-file-searching').css('display', 'none')");
    let _ = app_window.eval(&format!(
        "$('.file-searching-done').html('Searching done in: {:.2} sec.!')",
        sw.elapsed().as_millis() as f64 / 1000.0
    ));
    let _ =
        app_window.eval("setTimeout(() => $('.file-searching-done').css('display', 'none'), 1500)");
    let _ = app_window.eval("setTimeout(() => $('.file-searching-done').html(''), 1500)");
    let _ = app_window
        .eval("setTimeout(() => $('.file-searching-file-count').css('display', 'none'), 1500)");
    dbg_log(format!("Search took: {:?}", sw.elapsed()));
}

#[tauri::command]
async fn copy_paste(
    app_window: Window,
    act_file_name: String,
    from_path: String,
    is_for_dual_pane: String,
    mut copy_to_path: String,
) {
    if &copy_to_path.len() == &0 {
        wng_log("No destination path provided. Defaulting to current dir".into());
        copy_to_path = current_dir().unwrap().to_string_lossy().to_string();
    }
    unsafe {
        COPY_COUNTER = 0.0;
    }
    let _ = &app_window
        .eval("document.querySelector('.progress-bar-container-popup').style.display = 'flex'");
    dbg_log(format!("Copying: {} ...", &act_file_name));
    let final_filename = get_final_filename(
        act_file_name,
        from_path.clone(),
        is_for_dual_pane,
        copy_to_path,
    )
    .await;

    unsafe {
        TO_COPY_COUNTER = count_entries(&from_path).unwrap();
        if TO_COPY_COUNTER == 1.0 {
            let _ =
                app_window.eval("document.querySelector('.progress-bar-2').style.display = 'none'");
        } else {
            let _ = app_window
                .eval("document.querySelector('.progress-bar-2').style.display = 'block'");
        }
    }
    let sw = Stopwatch::start_new();
    // Execute the copy process for either a dir or file
    copy_to(&app_window, final_filename, from_path);
    dbg_log(format!("Copy-Paste time: {:?}", sw.elapsed()));
    app_window.eval("resetProgressBar()").unwrap();
}

#[tauri::command]
async fn arr_copy_paste(
    app_window: Window,
    arr_items: Vec<FDir>,
    is_for_dual_pane: String,
    mut copy_to_path: String,
) {
    if &copy_to_path.len() == &0 {
        wng_log("No destination path provided. Defaulting to current dir".into());
        copy_to_path = current_dir().unwrap().to_string_lossy().to_string();
    }
    unsafe {
        COPY_COUNTER = 0.0;
        TO_COPY_COUNTER = 0.0
    }
    let _ = &app_window
        .eval("document.querySelector('.progress-bar-container-popup').style.display = 'flex'");
    let _ = app_window.eval("document.querySelector('.progress-bar-2').style.display = 'block'");
    let mut filename: String;
    for item in arr_items.clone() {
        unsafe {
            TO_COPY_COUNTER += count_entries(&item.path).unwrap();
        }
    }
    let sw = Stopwatch::start_new();
    for item in arr_items {
        let item_path = item.path;
        filename = item_path
            .replace("\\", "/")
            .split("/")
            .last()
            .unwrap()
            .to_string();
        let final_filename = get_final_filename(
            filename,
            item_path.clone(),
            is_for_dual_pane.clone(),
            copy_to_path.clone(),
        )
        .await;
        // Execute the copy process for either a dir or file
        // #[cfg(target_os = "macos")]
        // let _ = copy(item_path, final_filename); // Copying of files is different on macOS
        // #[cfg(not(target_os = "macos"))]
        copy_to(&app_window, final_filename, item_path);
    }
    dbg_log(format!("Copy-Paste time: {:?}", sw.elapsed()));
    app_window.eval("resetProgressBar()").unwrap();
    app_window.eval("listDirectories(true)").unwrap();
}

#[tauri::command]
async fn get_final_filename(
    act_file_name: String,
    from_path: String,
    is_for_dual_pane: String,
    copy_to_path: String,
) -> String {
    let file = fs::metadata(&from_path);
    if &file.is_ok() != &true {
        err_log("File could not be copied".into());
        return "".into();
    }
    let file_name: String;
    if is_for_dual_pane == "1" {
        file_name = act_file_name;
    } else {
        file_name = PathBuf::from(copy_to_path)
            .join(&act_file_name)
            .to_str()
            .unwrap()
            .to_string();
    }
    let temp_file_ext: String;
    let mut file_ext: String;
    let mut temp_filename: String = String::new();

    for i in 0..file_name.split(".").count() - 1 {
        temp_filename += file_name.split(".").nth(i).unwrap();
    }

    temp_file_ext = file_name
        .split(".")
        .nth(file_name.split(".").count() - 1)
        .unwrap()
        .to_string();
    file_ext = ".".to_string().to_owned() + &temp_file_ext.as_str();

    if temp_file_ext == file_name {
        file_ext = "".to_string();
    }

    temp_filename = file_name
        .strip_suffix(&file_ext)
        .unwrap_or(&file_name)
        .to_string();
    let mut is_file_existing = fs::metadata(&file_name).is_ok();
    let mut counter = 1;
    let mut final_filename: String = format!("{}{}", &temp_filename, file_ext);

    while is_file_existing {
        final_filename = format!("{}_{}{}", &temp_filename, counter, file_ext);
        is_file_existing = fs::metadata(&final_filename).is_ok();
        counter += 1;
    }
    final_filename = final_filename.replace("\\", "/");
    return final_filename;
}

#[tauri::command]
async fn delete_item(act_file_name: String) {
    let file = File::open(&act_file_name);
    let is_dir: bool;
    if file.is_ok() {
        is_dir = file.unwrap().metadata().unwrap().is_dir();
    } else {
        return;
    }
    dbg_log(format!("Deleting: {}", String::from(&act_file_name)));
    if is_dir {
        let _ = remove_dir_all(act_file_name.replace("\\", "/")).expect("Failed to delete dir");
    } else {
        let _ = remove_file(act_file_name.replace("\\", "/")).expect("Failed to delete file");
    }
}

#[tauri::command]
async fn arr_delete_items(arr_items: Vec<String>) {
    for path in arr_items {
        delete_item(path).await;
    }
}

#[tauri::command]
async fn extract_item(from_path: String) -> Vec<FDir> {
    // Check file extension
    let file_ext = ".".to_string().to_owned()
        + from_path
            .split(".")
            .nth(from_path.split(".").count() - 1)
            .unwrap_or("");

    dbg_log(format!("Start unpacking {} - {}", &file_ext, &from_path));

    // zip, 7z or rar unpack
    let sw = Stopwatch::start_new();
    if file_ext == ".zip" {
        let file = PathBuf::from(&from_path);
        let _ = create_dir(&from_path.strip_suffix(&file_ext).unwrap());
        let new_dir = PathBuf::from(&from_path.strip_suffix(&file_ext).unwrap());
        zip_extract(&file, &new_dir).unwrap();
    } else if file_ext == ".rar" {
        let mut archive = Archive::new(&from_path).open_for_processing().unwrap();
        while let Some(header) = archive.read_header().unwrap() {
            dbg_log(format!(
                "{} bytes: {}",
                header.entry().unpacked_size,
                header.entry().filename.to_string_lossy()
            ));
            archive = if header.entry().is_file() {
                header.extract().unwrap()
            } else {
                header.skip().unwrap()
            }
        }
    } else if file_ext == ".7z" {
        let _ =
            sevenz_rust::decompress_file(&from_path, &from_path.strip_suffix(&file_ext).unwrap());
    } else if file_ext == ".tar" {
        unpack_tar(File::open(&from_path).unwrap());
    } else if file_ext == ".gz" {
        let file = File::open(&from_path).unwrap();
        let mut archive = GzDecoder::new(file);
        let mut buffer = Vec::new();
        let _ = archive.read_to_end(&mut buffer).unwrap();
        let _ = File::create(&from_path.strip_suffix(&file_ext).unwrap())
            .unwrap()
            .write_all(&buffer);
        unpack_tar(File::open(&from_path.strip_suffix(&file_ext).unwrap()).unwrap());
        let _ = remove_file(&from_path.strip_suffix(&file_ext).unwrap());
    } else if file_ext == ".bz2" {
        let mut file = archiver_rs::Bzip2::open(&PathBuf::from(&from_path)).unwrap();
        file.decompress(&PathBuf::from(&from_path.strip_suffix(&file_ext).unwrap()))
            .unwrap();
        unpack_tar(File::open(&from_path.strip_suffix(&file_ext).unwrap()).unwrap());
        let _ = remove_file(&from_path.strip_suffix(&file_ext).unwrap());
    } else {
        err_log("Unsupported file type".into());
        return vec![];
    }

    dbg_log(format!("Unpack time: {:?}", sw.elapsed()));
    return list_dirs().await;
}

#[tauri::command]
async fn open_item(path: String) {
    dbg_log(format!("Opening: {}", &path));
    let _ = open::that_detached(path);
}

#[tauri::command]
async fn compress_item(from_path: String, compression_level: i32) {
    let sw = Stopwatch::start_new();

    dbg_log(format!(
        "Compression of '{}' started with compression level: {}",
        &from_path.split("/").last().unwrap(),
        &compression_level
    ));
    let file_ext = ".".to_string().to_owned() + from_path.split(".").last().unwrap_or("");
    let _ = File::create(
        from_path
            .strip_suffix(&file_ext)
            .unwrap_or(&from_path)
            .to_owned()
            + ".zip",
    )
    .unwrap();
    let source: PathBuf;
    let archive = PathBuf::from(
        from_path
            .strip_suffix(&file_ext)
            .unwrap_or(&from_path)
            .to_owned()
            + ".zip",
    );
    if fs::metadata(&from_path).unwrap().is_dir() {
        source = PathBuf::from(&from_path);
    } else {
        let file_name = &from_path.split("/").last().unwrap();
        let _ = create_dir("__compressed_dir");
        let _ = copy(
            &from_path,
            "__compressed_dir/".to_string().to_owned() + file_name,
        );
        source = PathBuf::from("__compressed_dir");
    }
    let options = FileOptions::default()
        .compression_method(zip::CompressionMethod::Deflated)
        .compression_level(Option::from(compression_level));
    let _ = zip_create_from_directory_with_options(&archive, &source, options);
    let _ = remove_dir_all("__compressed_dir");
    dbg_log(format!("Compression time: {:?}", sw.elapsed()));
}

#[tauri::command]
async fn arr_compress_items(arr_items: Vec<String>, compression_level: i32) {
    let _ = create_dir("compressed_items_archive");
    for item_path in arr_items {
        let file_name = &item_path.split("/").last().unwrap();
        let _ = copy(
            &item_path,
            "compressed_items_archive/".to_string().to_owned() + file_name,
        );
    }
    compress_item(
        current_dir()
            .unwrap()
            .to_owned()
            .join("compressed_items_archive")
            .to_string_lossy()
            .to_string(),
        compression_level,
    )
    .await;
    let _ = remove_dir_all("compressed_items_archive");
}

#[tauri::command]
async fn create_folder(folder_name: String) {
    let new_folder_path = PathBuf::from(&folder_name);
    let _ = fs::create_dir(current_dir().unwrap().join(new_folder_path));
}

#[tauri::command]
async fn create_file(file_name: String) {
    let new_file_path = PathBuf::from(&file_name);
    let _ = File::create(current_dir().unwrap().join(new_file_path));
}

#[tauri::command]
async fn rename_element(path: String, new_name: String) -> Vec<FDir> {
    let _ = fs::rename(
        current_dir().unwrap().join(&path.replace("\\", "/")),
        current_dir().unwrap().join(&new_name.replace("\\", "/")),
    );
    dbg_log(format!("Renamed from {} to {}", path, new_name));
    return list_dirs().await;
}

#[tauri::command]
async fn save_config(
    configured_path_one: String,
    configured_path_two: String,
    configured_path_three: String,
    is_open_in_terminal: String,
    is_dual_pane_enabled: String,
    launch_path: String,
    is_dual_pane_active: String,
    search_depth: i32,
    max_items: i32,
    is_image_preview: String,
    is_select_mode: String,
    arr_favorites: Vec<String>,
    current_theme: String,
) {
    let app_config_file = File::open(
        app_config_dir(&Config::default())
            .unwrap()
            .join("com.rdpFX.dev/app_config.json"),
    )
    .unwrap();
    let app_config_reader = BufReader::new(app_config_file);
    let app_config: Value = serde_json::from_reader(app_config_reader).unwrap();
    let app_config_json = AppConfig {
        view_mode: app_config["view_mode"].to_string().replace('"', ""),
        last_modified: chrono::offset::Local::now().to_string(),
        configured_path_one: configured_path_one.replace("\\", "/"),
        configured_path_two: configured_path_two.replace("\\", "/"),
        configured_path_three: configured_path_three.replace("\\", "/"),
        is_open_in_terminal: is_open_in_terminal.replace("\\", ""),
        is_dual_pane_enabled: is_dual_pane_enabled.replace("\\", ""),
        launch_path: launch_path.replace("\\", "/"),
        is_dual_pane_active: is_dual_pane_active.replace("\\", ""),
        search_depth,
        max_items,
        is_image_preview: is_image_preview.replace("\\", "/"),
        is_select_mode: is_select_mode.replace("\\", "/"),
        arr_favorites,
        current_theme: current_theme.replace("\\", "/"),
    };
    let config_dir = app_config_dir(&Config::default())
        .unwrap()
        .join("com.rdpFX.dev/app_config.json")
        .to_str()
        .unwrap()
        .to_string();
    let _ = serde_json::to_writer_pretty(File::create(&config_dir).unwrap(), &app_config_json);
    dbg_log(format!("app_config was saved to {}", config_dir));
}

#[tauri::command]
async fn rename_elements_with_format(
    arr_elements: Vec<String>,
    new_name: String,
    start_at: i32,
    step_by: i32,
    n_digits: usize,
    ext: String,
) {
    let mut counter = start_at;
    for element in arr_elements {
        let mut item_ext: String = ext.to_string();
        if element.split(".").last().unwrap().len() > 0 && ext.len() == 0 {
            item_ext = format!("{}", ".".to_string() + element.split(".").last().unwrap());
        }
        let _ = fs::rename(
            &element,
            format!("{}{:0>n_digits$}{}", new_name, counter, item_ext),
        );
        dbg_log(format!(
            "Renamed from {} to {}",
            element,
            format!("{}{:0>n_digits$}{}", new_name, counter, item_ext)
        ));
        counter += step_by;
    }
}

// TODO: impl this stuff
#[tauri::command]
async fn add_favorite(arr_favorites: Vec<String>) {
    let app_config_file = File::open(
        app_config_dir(&Config::default())
            .unwrap()
            .join("com.rdpFX.dev/app_config.json"),
    )
    .unwrap();
    let app_config_reader = BufReader::new(app_config_file);
    let mut app_config: Value = serde_json::from_reader(app_config_reader).unwrap();
    app_config["arr_favorites"] = arr_favorites
        .clone()
        .into_iter()
        .map(|x| Value::String(x))
        .collect();
    let _ = serde_json::to_writer_pretty(
        File::create(
            app_config_dir(&Config::default())
                .unwrap()
                .join("com.rdpFX.dev/app_config.json")
                .to_str()
                .unwrap()
                .to_string(),
        )
        .unwrap(),
        &app_config,
    );
    dbg_log(format!("Saved favorites: {:?}", arr_favorites));
}

#[tauri::command]
async fn get_installed_apps(extension: String) -> Vec<(String, String)> {
    let list_apps = get_apps(extension);
    let mut arr_apps: Vec<(String, String)> = vec![];
    for app in list_apps {
        arr_apps.push((app.name.clone(), app.app_path_exe));
    }
    return arr_apps;
}

#[tauri::command]
async fn open_with(file_path: String, app_path: String) {
    #[cfg(not(target_os = "linux"))]
    open_file_with(file_path, app_path);
}

#[tauri::command]
async fn find_duplicates(app_window: Window, path: String, depth: u32) -> Vec<Vec<DirWalkerEntry>> {
    let files = DirWalker::new()
        .depth(depth)
        .run(&path)
        .ext(vec![
            "png", "jpg", "jpeg", "txt", "svg", "gif", "mp4", "mp3", "wav", "pdf", "docx", "xlsx",
            "doc", "zip", "rar", "7z", "dmg", "iso", "exe", "msi", "jar", "deb", "sh", "py", "htm",
            "html",
        ])
        .get_items();
    let mut seen_items: Vec<DirWalkerEntry> = Vec::new();
    let mut duplicates: Vec<Vec<DirWalkerEntry>> = Vec::new();
    for item in files.into_par_iter().collect::<Vec<DirWalkerEntry>>() {
        let seen_item = seen_items.par_iter().find_any(|x| {
            x.is_file == true
                && x.size == item.size
                && x.size > 0
                && x.name
                    .contains(&item.name.substring(0, item.name.len() - 3))
        });
        if *&seen_item.is_some() {
            if duplicates.len() == 0 {
                duplicates.push(vec![seen_item.unwrap().clone(), item.clone()]);
            } else {
                let collection = duplicates.par_iter_mut().find_any(|x| {
                    x[0].size == seen_item.unwrap().size
                        && x[0].size > 0
                        && x[0]
                            .name
                            .contains(&item.name.substring(0, item.name.len() - 3))
                });
                if *&collection.is_some() {
                    collection.unwrap().push(item.clone());
                } else {
                    duplicates.push(vec![item.clone(), seen_item.unwrap().clone()]);
                }
            }
        } else {
            seen_items.push(item);
        }
    }
    for (idx, arr_duplicate) in duplicates.clone().iter().enumerate() {
        let var_idx = &idx.clone().to_string();
        let mut inner_html = String::new();
        let mut js_query = String::new()
            + "
            var duplicate"
            + var_idx
            + " = document.createElement('div');
            duplicate"
            + var_idx
            + ".setAttribute('itempaneside', '');
            duplicate"
            + var_idx
            + ".setAttribute('itemisdir', '0');
            duplicate"
            + var_idx
            + ".setAttribute('itemext', '');
            duplicate"
            + var_idx
            + ".setAttribute('isftp', '0');
            duplicate"
            + var_idx
            + ".className = 'list-item duplicate-item';
        ";
        for (idx, item) in arr_duplicate.clone().iter().enumerate() {
            inner_html.push_str(
                &(String::new()
                    + "
                <div style='display: flex; align-items: center; justify-content: space-between;'>
                    <div>
                        <h4>"
                    + &item.name
                    + "</h3>
                        <h4 class='text-2'>"
                    + &item.path
                    + "</h4>
                        <h4 class='text-2'>"
                    + &format_bytes(item.size)
                    + "</h4>
                    </div>
            "),
            );
            if item.name.ends_with("jpg")
                || item.name.ends_with("jpeg")
                || item.name.ends_with("png")
                || item.name.ends_with("gif")
                || item.name.ends_with("svg")
                || item.name.ends_with("webp")
                || item.name.ends_with("jfif")
                || item.name.ends_with("tiff")
            {
                inner_html.push_str(&(String::new()+"
                    <img style='box-shadow: 0px 0px 10px 1px var(--transparentColorActive); border-radius: 5px;' width='64px' height='auto' src='"+ASSET_LOCATION+""+&item.path+"'>
                </div>
                "));
            } else {
                inner_html.push_str(
                    &(String::new()
                        + "
                    </div>
                "),
                );
            }
            js_query.push_str(
                &(String::new()
                    + "
                duplicate"
                    + var_idx
                    + ".setAttribute('"
                    + &format!("itempath-{}", idx)
                    + "', '"
                    + &item.path
                    + "');
            "),
            );
        }
        js_query.push_str(
            &(String::new()
                + "
            duplicate"
                + var_idx
                + ".innerHTML = `"
                + &inner_html
                + "`;
            duplicate"
                + var_idx
                + ".oncontextmenu = (e) => showExtraContextMenu(e, duplicate"
                + var_idx
                + ");
            document.querySelector('.duplicates-list').append(duplicate"
                + var_idx
                + ");
        "),
        );
        let _ = app_window.eval(&js_query);
    }
    duplicates
}

#[tauri::command]
async fn cancel_operation() {
    unsafe {
        ISCANCELED = true;
    }
}

#[tauri::command]
async fn get_df_dir(number: u8) -> String {
    return match number {
        0 => desktop_dir()
            .unwrap_or_default()
            .to_string_lossy()
            .to_string(),
        1 => download_dir()
            .unwrap_or_default()
            .to_string_lossy()
            .to_string(),
        2 => document_dir()
            .unwrap_or_default()
            .to_string_lossy()
            .to_string(),
        3 => picture_dir()
            .unwrap_or_default()
            .to_string_lossy()
            .to_string(),
        4 => video_dir()
            .unwrap_or_default()
            .to_string_lossy()
            .to_string(),
        5 => audio_dir()
            .unwrap_or_default()
            .to_string_lossy()
            .to_string(),
        _ => current_dir().unwrap().to_string_lossy().to_string(),
    };
}

#[tauri::command]
async fn download_yt_video(app_window: Window, url: String, quality: String) {
    dbg_log(format!("Downloading {} as {}", url, quality));
    let chosen_quality = match quality.as_str() {
        "lowestvideo" => VideoQuality::LowestVideo,
        "lowestaudio" => VideoQuality::LowestAudio,
        "highestvideo" => VideoQuality::HighestVideo,
        "highestaudio" => VideoQuality::HighestAudio,
        _ => VideoQuality::HighestVideo,
    };

    dbg_log(format!("Chosen quality: {:?}", chosen_quality));

    let video_options = VideoOptions {
        quality: chosen_quality,
        filter: VideoSearchOptions::Video,
        ..Default::default()
    };

    let video = Video::new_with_options(url, video_options).unwrap();

    let stream = video.stream().await;
    if stream.is_err() {
        let _ = &app_window.eval("closeLoadingPopup()");
        let _ = &app_window.eval("alert('Failed to retrieve source')");
        return;
    }
    let stream = stream.unwrap();
    let video_info = video.get_basic_info().await.unwrap();
    let mut file = File::create(video_info.video_details.title.to_owned() + ".mp4").unwrap();
    let total_size = stream.content_length() as f32;
    let mut downloaded: f64 = 0.0;
    let sw = Stopwatch::start_new();

    while let Some(chunk) = stream.chunk().await.unwrap() {
        file.write_all(&chunk).unwrap();
        downloaded += chunk.len() as f64;
        let speed = calc_transfer_speed(downloaded, sw.elapsed_ms() as f64 / 1000.0);
        update_progressbar_2(&app_window, 0.0, &video_info.video_details.title);
        update_progressbar(
            &app_window,
            100.0 / total_size * downloaded as f32,
            &format_bytes(downloaded as u64),
            speed,
        );
    }

    app_window.eval("resetProgressBar()").unwrap();
    app_window.eval("listDirectories(true)").unwrap();
}

// #[tauri::command]
// async fn get_llm_response(app_window: Window, prompt: String) {
//     let model_architecture = ModelArchitecture::GptNeoX;
//     let model_path = config_dir()
//         .unwrap()
//         .join("com.rdpFX.dev")
//         .join("models")
//         .join("ggml-model.bin");
//     let prompt = format!("{}\n", prompt);

//     let now = std::time::Instant::now();

//     let model = llm::load_dynamic(
//         Some(model_architecture),
//         &model_path,
//         llm::TokenizerSource::Embedded,
//         Default::default(),
//         llm::load_progress_callback_stdout,
//     )
//     .unwrap_or_else(|err| {
//         panic!("Failed to load the {model_architecture} model from {model_path:?}: {err}")
//     });

//     println!(
//         "Model fully loaded! Elapsed: {}ms",
//         now.elapsed().as_millis()
//     );

//     app_window.eval("closeLoadingPopup()").unwrap();
//     app_window
//         .eval("showLoadingPopup('Loading files ...')")
//         .unwrap();

//     let mut context: String = String::new();

//     let items: Vec<String> = rust_search::SearchBuilder::default()
//         .location(current_dir().unwrap())
//         .search_input("")
//         .ignore_case()
//         .hidden()
//         .depth(1)
//         .build()
//         .collect();

//     for item in items {
//         let item: String = item;
//         println!("Item: {:?}", item);
//         let path = item;
//         let file_ext = ".".to_string().to_owned() + &path.split(".").last().unwrap_or("");
//         if file_ext == ".txt" {
//             // let file_content = fs::read_to_string(&path).unwrap_or_else(|x| {
//             //     err_log(format!("Error reading: {}", x));
//             //     String::from("")
//             // });
//             // let mut found_keywords: String = String::new();
//             // for keyword in prompt.split(" ") {
//             //     println!("Searching for keyword: {}", keyword);
//             //     if file_content
//             //         .to_lowercase()
//             //         .contains(keyword.to_lowercase().as_str())
//             //     {
//             //         println!("Found keyword: {}", keyword);
//             //         found_keywords += &format!("{} ", keyword);
//             //     }
//             // }
//             // context += &format!(
//             //     "\n<start_of_file>\nFile: {}\nFound keywords: {}\n<end_of_file>",
//             //     path, found_keywords
//             // );
//             context += &format!(
//                 "\nFile: {}\nContent: {}\n",
//                 path,
//                 fs::read_to_string(&path).unwrap_or_else(|x| {
//                     err_log(format!("Error reading: {}", x));
//                     String::from("")
//                 })
//             );
//         }
//     }

//     let general_information = "Provide a list of files which match the meaning of the prompt. If no file matches the prompt, respond with: 'No files found'. The response needs to be a reference to the files. There musn't be any other detail.";

//     app_window.eval("closeLoadingPopup()").unwrap();
//     app_window
//         .eval("showLoadingPopup('Analyzing ...')")
//         .unwrap();

//     let mut session = model.start_session(Default::default());
//     session
//         .feed_prompt(
//             model.as_ref(),
//             format!(
//                 "General information: {}\nCurrent directory: {}\nFiles: \n{}\nPrompt: {}",
//                 general_information,
//                 current_dir().unwrap().to_str().unwrap(),
//                 context,
//                 prompt
//             )
//             .as_str(),
//             &mut Default::default(),
//             llm::feed_prompt_callback(|resp| match resp {
//                 llm::InferenceResponse::PromptToken(t)
//                 | llm::InferenceResponse::InferredToken(t) => {
//                     unsafe {
//                         if ISCANCELED {
//                             ISCANCELED = false;
//                             return Ok(llm::InferenceFeedback::Halt);
//                         }
//                     }
//                     print!("{t}");
//                     std::io::stdout().flush().unwrap();
//                     Ok::<llm::InferenceFeedback, Infallible>(llm::InferenceFeedback::Continue)
//                 }
//                 _ => Ok(llm::InferenceFeedback::Continue),
//             }),
//         )
//         .expect("Failed to ingest initial prompt.");

//     let mut is_first_run = true;

//     app_window.eval("closeLoadingPopup()").unwrap();
//     app_window
//         .eval("showLoadingPopup('Generating response ...')")
//         .unwrap();

//     let res = session.infer::<Infallible>(
//         model.as_ref(),
//         &mut rand::thread_rng(),
//         &llm::InferenceRequest {
//             prompt: format!("Response: ").as_str().into(),
//             parameters: &llm::InferenceParameters::default(),
//             play_back_previous_tokens: false,
//             maximum_token_count: None,
//         },
//         // OutputRequest
//         &mut Default::default(),
//         |r| match r {
//             llm::InferenceResponse::PromptToken(t) | llm::InferenceResponse::InferredToken(t) => {
//                 if is_first_run {
//                     let _ = app_window
//                         .eval("document.querySelector('.llm-prompt-response').value = ''")
//                         .expect("Failed to set llm-prompt-response value");
//                     app_window.eval("closeLoadingPopup();").unwrap();
//                     is_first_run = false;
//                 }
//                 unsafe {
//                     if ISCANCELED {
//                         ISCANCELED = false;
//                         return Ok(llm::InferenceFeedback::Halt);
//                     }
//                 }
//                 let _ = app_window
//                     .eval(&format!(
//                         "document.querySelector('.llm-prompt-response').value += `{t}`"
//                     ))
//                     .unwrap();
//                 if prompt.starts_with(t.to_string().as_str()) {
//                     return Ok(llm::InferenceFeedback::Continue);
//                 }
//                 Ok(llm::InferenceFeedback::Continue)
//             }
//             _ => Ok(llm::InferenceFeedback::Continue),
//         },
//     );

//     // Reset input and run button
//     app_window
//         .eval("document.querySelector('.llm-prompt-input').disabled = false")
//         .unwrap();
//     app_window
//         .eval("document.querySelector('.llm-prompt-input').style.opacity = 1")
//         .unwrap();
//     app_window
//         .eval("document.querySelector('.llm-prompt-run').disabled = false")
//         .unwrap();
//     app_window
//         .eval("document.querySelector('.llm-prompt-run').style.opacity = 1")
//         .unwrap();

//     match res {
//         Ok(result) => println!("\n\nInference stats:\n{result}"),
//         Err(err) => println!("\n{err}"),
//     }
// }

#[tauri::command]
async fn get_app_icns(path: String) -> String {
    #[cfg(target_os = "linux")]
    return "".into();

    #[cfg(target_os = "windows")]
    return "".into();

    #[cfg(target_os = "macos")]
    {
        let icns = applications::find_app_icns(path.clone().into());
        if icns.is_some() {
            let icns = icns.unwrap();

            let icns_path = config_dir()
                .unwrap()
                .join("com.rdpFX.dev")
                .join("App-Thumbnails");
            let new_img_path = icns_path.to_string_lossy().to_string()
                + "/"
                + path.split("/").last().unwrap()
                + icns.file_name().unwrap().to_str().unwrap()
                + ".png";

            if PathBuf::from(new_img_path.clone()).exists() {
                return new_img_path;
            }

            let file = BufReader::new(File::open(icns.to_string_lossy().to_string()).unwrap());
            let icon_family = IconFamily::read(file);
            if icon_family.is_err() {
                return icns.to_string_lossy().to_string();
            }
            let icon_family = icon_family.unwrap();

            let mut image = icon_family.get_icon_with_type(IconType::RGBA32_512x512_2x);
            if image.is_err() {
                image = icon_family.get_icon_with_type(IconType::RGBA32_512x512);
                if image.is_err() {
                    image = icon_family.get_icon_with_type(IconType::RGBA32_256x256_2x);
                    if image.is_err() {
                        image = icon_family.get_icon_with_type(IconType::RGBA32_256x256);
                        if image.is_err() {
                            image = icon_family.get_icon_with_type(IconType::RGBA32_128x128_2x);
                            if image.is_err() {
                                image = icon_family.get_icon_with_type(IconType::RGBA32_128x128);
                                if image.is_err() {
                                    image = icon_family.get_icon_with_type(IconType::RGBA32_64x64);
                                    if image.is_err() {
                                        image = icon_family
                                            .get_icon_with_type(IconType::RGBA32_32x32_2x);
                                        if image.is_err() {
                                            image = icon_family
                                                .get_icon_with_type(IconType::RGBA32_32x32);
                                            if image.is_err() {
                                                image = icon_family
                                                    .get_icon_with_type(IconType::RGBA32_16x16_2x);
                                                if image.is_err() {
                                                    image = icon_family
                                                        .get_icon_with_type(IconType::RGBA32_16x16);
                                                    if image.is_err() {
                                                        image = icon_family.get_icon_with_type(
                                                            IconType::RGB24_128x128,
                                                        );
                                                        if image.is_err() {
                                                            image = icon_family.get_icon_with_type(
                                                                IconType::RGB24_48x48,
                                                            );
                                                            if image.is_err() {
                                                                image = icon_family
                                                                    .get_icon_with_type(
                                                                        IconType::RGB24_32x32,
                                                                    );
                                                                if image.is_err() {
                                                                    image = icon_family
                                                                        .get_icon_with_type(
                                                                            IconType::RGB24_16x16,
                                                                        );
                                                                    if image.is_err() {
                                                                        image = icon_family
                                                                            .get_icon_with_type(
                                                                            IconType::Mask8_128x128,
                                                                        );
                                                                        if image.is_err() {
                                                                            image = icon_family
                                                                                .get_icon_with_type(
                                                                                    IconType::Mask8_48x48,
                                                                                );
                                                                            if image.is_err() {
                                                                                image = icon_family.get_icon_with_type(IconType::Mask8_32x32);
                                                                                if image.is_err() {
                                                                                    image = icon_family.get_icon_with_type(IconType::Mask8_16x16);
                                                                                    if image
                                                                                        .is_err()
                                                                                    {
                                                                                        return icns.to_string_lossy().to_string();
                                                                                    }
                                                                                }
                                                                            }
                                                                        }
                                                                    }
                                                                }
                                                            }
                                                        }
                                                    }
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }

            // Save additional icon to read from rdpFX
            let image = image.unwrap();
            if !PathBuf::from(&new_img_path).exists() {
                let file = File::create(&new_img_path);
                if file.is_err() {
                    return icns.to_string_lossy().to_string();
                }
                let file = file.unwrap();
                BufWriter::new(&file);
                image.write_png(file).unwrap();
                dbg_log(format!("Writing image to: {}", new_img_path));
            }

            return new_img_path;
        } else {
            return String::from("");
        }
    }
}

#[tauri::command]
async fn get_thumbnail(image_path: String) -> String {
    let item = image::open(&image_path);

    if item.is_err() {
        dbg_log(format!(
            "Couldn't load image for thumbnail: {}",
            &image_path.split("/").last().unwrap()
        ));
        return image_path;
    }
    let item = item.unwrap();
    let thumbnails_dir = config_dir()
        .unwrap()
        .join("com.rdpFX.dev")
        .join("Thumbnails");

    if !PathBuf::from(&thumbnails_dir).exists() {
        let _ = create_dir(&thumbnails_dir);
    }

    let thumbnail = item.thumbnail(100, 50);
    let new_thumbnail_path = thumbnails_dir.join(image_path.clone().split("/").last().unwrap());

    if PathBuf::from(&new_thumbnail_path).exists() {
        dbg_log(format!("Getting thumbnail for: {}", image_path));
        return new_thumbnail_path.to_string_lossy().to_string();
    }

    dbg_log(format!(
        "Saving thumbnail for: {}",
        image_path.split("/").last().unwrap()
    ));

    let _ = thumbnail
        .save_with_format(
            &new_thumbnail_path,
            image::ImageFormat::from_extension(image_path.split(".").last().unwrap())
                .expect("Couldn't get format by extension"),
        )
        .expect("Couldn't save thumbnail");
    new_thumbnail_path.to_string_lossy().to_string()
}

#[tauri::command]
async fn install_dep(dep_name: String) {
    // Check if package is already installed
    let package = Package::new(&dep_name);
    if package.is_err() {
        let err = package.err();
        dbg_log(format!(
            "Installation of dependency not possible: {:?}",
            err.unwrap()
        ));
        return;
    }
    let package = package.unwrap();
    if !package.is_installed() {
        let package_result = package.install(&brew::Options::new().head().force().env_std());
        dbg_log(format!(
            "{} = {}",
            dep_name,
            package_result.unwrap().is_installed()
        ));
    }
}

#[tauri::command]
async fn get_dir_size(path: String, app_window: Window, class_to_fill: String) -> u64 {
    unsafe {
        CALCED_SIZE = 0;
    }
    dir_size(path, &app_window, class_to_fill)
}

static mut CALCED_SIZE: u64 = 0; // Currently unused -> Coming implementation for showing progress
fn dir_size(path: String, app_window: &Window, class_to_fill: String) -> u64 {
    if PathBuf::from(&path).is_file() {
        return PathBuf::from(&path).metadata().unwrap().len();
    }

    let entry = match fs::read_dir(path) {
        Ok(entry) => entry,
        Err(_) => return 0,
    };
    let mut size = 0;

    for entry in entry {
        if let Ok(entry) = entry {
            if entry.file_type().unwrap().is_file() {
                let file_size = match entry.metadata() {
                    Ok(s) => s.len(),
                    Err(_) => continue,
                };
                size += file_size;
            } else if entry.file_type().unwrap().is_dir() {
                let dir_size = dir_size(
                    entry.path().to_string_lossy().to_string(),
                    app_window,
                    class_to_fill.clone(),
                );
                size += dir_size;
                // unsafe {
                //     CALCED_SIZE += dir_size;
                //     if CALCED_SIZE % 1000 == 0 {
                //         let _ = app_window.eval(&format!(
                //             "document.querySelector('{}').innerHTML = formatBytes({}) + ' ' + formatBytes({})",
                //             class_to_fill, CALCED_SIZE, dir_size
                //         ));
                //     }
                // }
            }
        }
    }
    size
}

#[tauri::command]
async fn get_file_content(path: String) -> String {
    let content = fs::read_to_string(&path).unwrap();
    if path.ends_with(".json") {
        let json: Value = serde_json::from_str(&content).unwrap();
        let json_string_pretty = serde_json::to_string_pretty(&json).unwrap();
        return json_string_pretty;
    }
    content
}

#[tauri::command]
async fn open_config_location() {
    let _ = open::that(config_dir().unwrap().join("com.rdpFX.dev"));
}