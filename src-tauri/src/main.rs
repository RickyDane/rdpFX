// Prevents additional console window on Windows in release, DO NOT REMOVE!!
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use async_ftp::FtpStream;
use chrono::prelude::{DateTime, Utc};
use dialog::DialogBox;
use flate2::read::GzDecoder;
use rust_search::{similarity_sort, SearchBuilder};
use serde_json::Value;
use tauri::{Manager, Window};
use window_shadows::set_shadow;
use zip::write::FileOptions;
use std::fs::{self, ReadDir};
use std::io::{BufRead, BufReader, Read, Write};
use std::{
    env::{current_dir, set_current_dir},
    fs::{copy, create_dir, remove_dir_all, remove_file, File},
    path::PathBuf
};
use stopwatch::Stopwatch;
use tauri::{
    api::path::{
        app_config_dir, audio_dir, config_dir, desktop_dir, document_dir, download_dir, home_dir,
        picture_dir, video_dir,
    },
    Config,
};
use unrar::Archive;
use zip_extensions::*;
mod utils;
use sysinfo::Disks;
use utils::{copy_to, count_entries, dbg_log, err_log, format_bytes, unpack_tar, wng_log, DirWalker, DirWalkerEntry, COPY_COUNTER, TO_COPY_COUNTER};
#[allow(unused_imports)]
use rayon::prelude::*;
mod applications;
use applications::{open_file_with, get_apps};
use substring::Substring;
use archiver_rs::Compressed;

static mut HOSTNAME: String = String::new();
static mut USERNAME: String = String::new();
static mut PASSWORD: String = String::new();

static mut ISCANCELED: bool = false;

fn main() {
    tauri::Builder::default()
        .setup(|app| {
            let main_window = app.get_window("main").unwrap();
            #[cfg(any(windows, target_os = "macos"))]
            set_shadow(&main_window, true).unwrap();
            Ok(())
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
            open_fav_ftp,
            open_ftp_dir,
            ftp_go_back,
            copy_from_ftp,
            rename_elements_with_format,
            add_favorite,
            arr_copy_paste,
            arr_delete_items,
            arr_compress_items,
            get_installed_apps,
            open_with,
            find_duplicates,
            cancel_operation,
            get_df_dir
        ])
        .plugin(tauri_plugin_drag::init())
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

#[derive(serde::Serialize)]
struct FDir {
    name: String,
    is_dir: i8,
    path: String,
    extension: String,
    size: String,
    last_modified: String,
    is_ftp: i8,
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
    is_light_mode: String,
    is_image_preview: String,
    is_select_mode: String,
    arr_favorites: Vec<String>
}

#[tauri::command]
async fn check_app_config() -> AppConfig {
    create_folder(
        config_dir()
            .unwrap()
            .join("com.rdpFX.dev")
            .to_str()
            .unwrap()
            .to_string(),
    ).await;


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
            is_light_mode: "0".to_string(),
            is_image_preview: "0".to_string(),
            is_select_mode: "1".to_string(),
            arr_favorites: vec![]
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

    let app_config_file = File::open(config_dir().unwrap().join("com.rdpFX.dev/app_config.json")).unwrap();
    let app_config_reader = BufReader::new(app_config_file);
    let app_config: Value = serde_json::from_reader(app_config_reader).unwrap();

    let default_vec: Vec<Value> = vec![];
    return AppConfig {
        view_mode: app_config["view_mode"].to_string(),
        last_modified: app_config["last_modified"].to_string(),
        configured_path_one: app_config["configured_path_one"].to_string().replace('"', ""),
        configured_path_two: app_config["configured_path_two"].to_string().replace('"', ""),
        configured_path_three: app_config["configured_path_three"].to_string().replace('"', ""),
        is_open_in_terminal: app_config["is_open_in_terminal"].to_string(),
        is_dual_pane_enabled: app_config["is_dual_pane_enabled"].to_string(),
        launch_path: app_config["launch_path"].to_string().replace('"', ""),
        is_dual_pane_active: app_config["is_dual_pane_active"].to_string(),
        search_depth: app_config["search_depth"].to_string().parse::<i32>().unwrap(),
        max_items: app_config["max_items"].to_string().parse::<i32>().unwrap(),
        is_light_mode: app_config["is_light_mode"].to_string(),
        is_image_preview: app_config["is_image_preview"].to_string(),
        is_select_mode: app_config["is_select_mode"].to_string(),
        arr_favorites: app_config["arr_favorites"].as_array().unwrap_or_else(|| { &default_vec }).iter().map(|x| x.to_string().replace('"', "")).collect()
    };
}

#[derive(serde::Serialize)]
struct DisksInfo {
    name: String,
    dev: String,
    format: String,
    path: String,
    avail: String,
    capacity: String,
    is_removable: bool
}

#[tauri::command]
async fn list_disks() -> Vec<DisksInfo> {
    let mut ls_disks: Vec<DisksInfo> = vec![];
    let disks = Disks::new_with_refreshed_list();
    for disk in &disks {
        dbg_log(format!("{:?}", &disk));
        ls_disks.push(DisksInfo {
            name: format!("{:?}", disk.mount_point()).split("/").last().unwrap_or("/").to_string().replace("\"", ""),
            dev: format!("{:?}", disk.name()),
            format: format!("{:?}", disk.file_system().to_string_lossy()),
            path: format!("{:?}", disk.mount_point()),
            avail: format!("{:?}", disk.available_space()),
            capacity: format!("{:?}", disk.total_space()),
            is_removable: disk.is_removable()
        });
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
    let app_config_file = File::open(app_config_dir(&Config::default()).unwrap().join("com.rdpFX.dev/app_config.json")).unwrap();
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
        let path = &temp_item.path().to_str().unwrap().to_string().replace("\\", "/");
        let file_ext = ".".to_string().to_owned() + &path.split(".").nth(&path.split(".").count() - 1).unwrap_or("");
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
           false => 0
        };
        dir_list.push(FDir {
            name: String::from(name),
            is_dir: is_dir_int,
            path: String::from(path),
            extension: file_ext,
            size: size.to_string(),
            last_modified: String::from(file_date.to_string().split(".").nth(0).unwrap()),
            is_ftp: 0,
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
    return list_dirs().await;
}

#[tauri::command]
async fn go_back() -> Vec<FDir> {
    let _ = set_current_dir("./../");
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
    }
    return list_dirs().await;
}

#[tauri::command]
async fn open_fav_ftp(hostname: String, username: String, password: String) -> Vec<FDir> {
    // Initialize ftp arguments
    unsafe {
        USERNAME = username.clone();
        PASSWORD = password.clone();
        HOSTNAME = hostname.clone();
    }
    let mut ftp_stream = FtpStream::connect(hostname).await.unwrap();
    let _ = &ftp_stream
        .login(username.as_str(), password.as_str())
        .await
        .unwrap();
    let ftp_dir = ftp_stream.pwd().await.unwrap();
    let ftp_dir_list = &ftp_stream.nlst(Some(&ftp_dir)).await.unwrap();

    for item in ftp_dir_list {
        dbg_log(format!("{:?}", item.split("/").last().unwrap()));
    }

    // Get the current directory that the client will be reading from and writing to.
    dbg_log(format!("Current dir: {:?}", &ftp_dir));

    let mut dir_list: Vec<FDir> = Vec::new();

    for item in ftp_dir_list {
        let name = &item.split("/").last().unwrap().to_string();
        let mod_date = &ftp_stream.mdtm(&item).await.unwrap_or_default();
        let size = &ftp_stream.size(&item).await.unwrap_or_default();
        let is_dir = is_directory(&mut ftp_stream, item).await;
        dir_list.push(FDir {
            name: name.to_string(),
            is_dir: is_dir,
            path: item.to_string(),
            extension: "".to_string(),
            size: size.unwrap_or_default().to_string(),
            last_modified: mod_date.unwrap_or_default().to_string(),
            is_ftp: 1,
        });
    }
    dir_list.sort_by_key(|a| a.name.to_lowercase());
    dir_list
}

#[tauri::command]
async fn open_ftp_dir(path: String) -> Vec<FDir> {
    let mut ftp_stream = get_current_connection().await;
    let _ = ftp_stream.cwd(&path).await.unwrap();
    let ftp_dir = ftp_stream.pwd().await.unwrap();
    let ftp_dir_list = &ftp_stream.nlst(Some(&ftp_dir)).await.unwrap();

    for item in ftp_dir_list {
        dbg_log(format!("{:?}", item));
    }

    // Get the current directory that the client will be reading from and writing to.
    dbg_log(format!("Current dir: {:?}", &ftp_dir));

    let mut dir_list: Vec<FDir> = Vec::new();

    for item in ftp_dir_list {
        let name = &item.split("/").last().unwrap().to_string();
        let mod_date = &ftp_stream.mdtm(&item).await.unwrap_or_default();
        let size = &ftp_stream.size(&item).await.unwrap_or_default();
        let is_dir = is_directory(&mut ftp_stream, item).await;
        dir_list.push(FDir {
            name: name.to_string(),
            is_dir: is_dir,
            path: item.to_string(),
            extension: "".to_string(),
            size: size.unwrap_or_default().to_string(),
            last_modified: mod_date.unwrap_or_default().to_string(),
            is_ftp: 1,
        });
    }
    dir_list.sort_by_key(|a| a.name.to_lowercase());
    dir_list
}

#[tauri::command]
async fn copy_from_ftp(path: String) {
    let mut ftp_stream = get_current_connection().await;
    let file = ftp_stream.simple_retr(&path).await.unwrap();
    let mut data = String::new();
    for line in file.lines() {
        data.push_str(&line.unwrap());
    }
    dbg_log(format!("Data: {:?}", data));
}

async fn is_directory(ftp_stream: &mut FtpStream, path: &str) -> i8 {
    match ftp_stream.cwd(path).await {
        Ok(_) => {
            // Zurück zum ursprünglichen Verzeichnis wechseln
            if let Err(_) = ftp_stream.cdup().await {
                err_log("Fehler beim Wechseln zum ursprünglichen Verzeichnis".into());
            }
            1 // Erfolgreich gewechselt, also ist es ein Verzeichnis
        }
        Err(_) => 0, // Fehler beim Wechseln, also ist es kein Verzeichnis
    }
}

#[tauri::command]
async fn ftp_go_back(path: String) -> Vec<FDir> {
    let mut ftp_stream = get_current_connection().await;
    ftp_stream.cwd(&path).await.unwrap();
    ftp_stream.cdup().await.unwrap();
    let ftp_dir = ftp_stream.pwd().await.unwrap();
    let ftp_dir_list = &ftp_stream.nlst(Some(&ftp_dir)).await.unwrap();

    for item in *&ftp_dir_list {
        dbg_log(format!("{:?}", item.split("/").last().unwrap()));
    }

    // Get the current directory that the client will be reading from and writing to.
    dbg_log(format!("Current dir: {:?}", &ftp_dir));

    let mut dir_list: Vec<FDir> = Vec::new();

    for item in ftp_dir_list {
        let name = &item.split("/").last().unwrap().to_string();
        let mod_date = &ftp_stream.mdtm(&item).await.unwrap_or_default();
        let size = &ftp_stream.size(&item).await.unwrap_or_default();
        let is_dir = is_directory(&mut ftp_stream, item).await;
        dir_list.push(FDir {
            name: name.to_string(),
            is_dir: is_dir,
            path: item.to_string(),
            extension: "".to_string(),
            size: size.unwrap_or_default().to_string(),
            last_modified: mod_date.unwrap_or_default().to_string(),
            is_ftp: 1,
        });
    }
    dir_list.sort_by_key(|a| a.name.to_lowercase());
    dir_list
}

async fn get_current_connection() -> FtpStream {
    let host: String;
    let user: String;
    let password: String;
    unsafe {
        host = HOSTNAME.clone();
        user = USERNAME.clone();
        password = PASSWORD.clone();
    }
    let mut ftp_stream = FtpStream::connect(host).await.unwrap();
    let _ = ftp_stream
        .login(user.as_str(), password.as_str())
        .await
        .unwrap();
    ftp_stream
}

#[tauri::command]
async fn open_in_terminal() {
    // open terminal on macOS
    // open terminal on linux
    // open terminal on windows
}

#[tauri::command]
async fn go_home() -> Vec<FDir> {
    let _ = set_current_dir(home_dir().unwrap());
    return list_dirs().await;
}

#[tauri::command]
async fn search_for(mut file_name: String, max_items: i32, search_depth: i32, file_content: String) -> Vec<FDir> {
    dbg_log(format!("Start searching for {}", &file_name));
    let temp_file_name = String::from(&file_name);
    if temp_file_name.split(".").nth(0).unwrap().contains("*") {
        file_name = temp_file_name.trim().replace("*", "");
    }

    let mut file_ext = ".".to_string().to_owned() + file_name.split(".").nth(file_name.split(".").count() - 1).unwrap_or("");
    println!("");

    let sw = Stopwatch::start_new();
    let mut search: Vec<String>;
    if file_ext != ".".to_string().to_owned() + &file_name {
        search = SearchBuilder::default()
            .location(current_dir().unwrap())
            .search_input(file_name.strip_suffix(&file_ext).unwrap())
            .ignore_case()
            .hidden()
            .depth(search_depth as usize)
            .limit(max_items as usize)
            .ext(&file_ext)
            .build()
            .collect();
    }
    else {
        search = SearchBuilder::default()
            .location(current_dir().unwrap())
            .search_input(&file_name)
            .ignore_case()
            .hidden()
            .depth(search_depth as usize)
            .limit(max_items as usize)
            .build()
            .collect();
    }

    // Sorting search results by input
    let sw2 = Stopwatch::start_new();
    similarity_sort(&mut search, &file_name);
    dbg_log(format!("Sorting took: {:?}", sw2.elapsed()));

    let mut dir_list: Vec<FDir> = Vec::new();
    for item in search {
        file_ext = ".".to_string().to_owned() + item.split(".").nth(item.split(".").count() - 1).unwrap_or("");
        let item = item.replace("\\", "/");
        let temp_item = &item.split("/").collect::<Vec<&str>>();
        let name = &temp_item[*&temp_item.len() - 1];
        let path = &item.replace("\\", "/");
        let temp_file = fs::metadata(&item);
        let file_size: String;
        let file_date: DateTime<Utc>;

        if &temp_file.is_ok() == &true {
            file_size = String::from(fs::metadata(&item).unwrap().len().to_string());
            file_date = fs::metadata(&item).unwrap().modified().unwrap().clone().into();
        }
        else { continue; }

        // Check if the item is a directory
        let is_dir_int;
        if &temp_file.is_ok() == &true && *&temp_file.unwrap().is_dir() {
            is_dir_int = 1;
        }
        else {
            is_dir_int = 0;
        }

        // Don't include the directory searched in
        if path == current_dir().unwrap().to_str().unwrap() {
            continue;
        }

        // Search for file contents
        if *&file_content.as_str() != "" {
            let check_file = fs::File::open(&path);
            let mut file: File;
            if &check_file.is_ok() == &true {
                file = check_file.unwrap();
            }
            else {
                err_log("Couldn't access file. Probably due to insufficient permissions".into());
                continue;
            }
            let mut buffer = String::from("");
            // dbg_log(format!("Checking {}", &path));

            if &file.metadata().unwrap().is_dir() == &false {
                file.read_to_string(&mut buffer).unwrap_or_else(|x| {
                    err_log(format!("Error reading: {}", x));
                    0 as usize
                });
                for (_idx, line) in buffer.lines().enumerate() {
                    if line.contains(&file_content) {
                        dir_list.push(FDir {
                            name: name.to_string(),
                            is_dir: is_dir_int,
                            path: path.to_string(),
                            extension: String::from(&file_ext),
                            size: file_size.clone(),
                            last_modified: String::from(file_date.to_string().split(".").nth(0).unwrap()),
                            is_ftp: 0,
                        });
                        break;
                    }
                    else { continue; }
                }
            }
        }
        else {
            dir_list.push(FDir {
                name: name.to_string(),
                is_dir: is_dir_int,
                path: path.to_string(),
                extension: String::from(&file_ext),
                size: file_size,
                last_modified: String::from(file_date.to_string().split(".").nth(0).unwrap()),
                is_ftp: 0,
            });
        }
    }
    if dir_list.len() == 0 {
        wng_log("No item found ".into());
    }
    dbg_log(format!("Search took: {:?}", sw.elapsed()));
    dbg_log(format!("{} items found", dir_list.len()));
    dir_list.sort_by_key(|a| a.name.to_lowercase());
    return dir_list;
}

#[tauri::command]
async fn copy_paste(app_window: Window, act_file_name: String, from_path: String, is_for_dual_pane: String, mut copy_to_path: String) {
    if &copy_to_path.len() == &0 {
        wng_log("No destination path provided. Defaulting to current dir".into());
        copy_to_path = current_dir().unwrap().to_string_lossy().to_string();
    }
    unsafe {
        COPY_COUNTER = 0.0;
    }
    let _ = &app_window.eval("document.querySelector('.progress-bar-container-popup').style.display = 'flex'");
    dbg_log(format!("Copying: {} ...", &act_file_name));
    let final_filename = get_final_filename(act_file_name, from_path.clone(), is_for_dual_pane, copy_to_path).await;

    unsafe {
        TO_COPY_COUNTER = count_entries(&from_path);
        if TO_COPY_COUNTER == 1.0 {
            let _ = app_window.eval("document.querySelector('.progress-bar-2').style.display = 'none'");
        }
        else {
            let _ = app_window.eval("document.querySelector('.progress-bar-2').style.display = 'block'");
        }
    }
    let sw = Stopwatch::start_new();
    // Execute the copy process for either a dir or file
    copy_to(&app_window, final_filename, from_path);
    dbg_log(format!("Copy-Paste time: {:?}", sw.elapsed()));
}

#[tauri::command]
async fn arr_copy_paste(app_window: Window, arr_items: Vec<String>, is_for_dual_pane: String, mut copy_to_path: String) {
    if &copy_to_path.len() == &0 {
        wng_log("No destination path provided. Defaulting to current dir".into());
        copy_to_path = current_dir().unwrap().to_string_lossy().to_string();
    }
    unsafe {
        COPY_COUNTER = 0.0;
        TO_COPY_COUNTER = 0.0
    }
    let _ = &app_window.eval("document.querySelector('.progress-bar-container-popup').style.display = 'flex'");
    let _ = app_window.eval("document.querySelector('.progress-bar-2').style.display = 'block'");
    let mut filename: String;
    for item in arr_items.clone() {
        unsafe {
            TO_COPY_COUNTER += count_entries(&item);
        }
    }
    let sw = Stopwatch::start_new();
    for item_path in arr_items {
        filename = item_path.replace("\\", "/").split("/").last().unwrap().to_string();
        let final_filename = get_final_filename(filename, item_path.clone(), is_for_dual_pane.clone(), copy_to_path.clone()).await;
        // Execute the copy process for either a dir or file
        copy_to(&app_window, final_filename, item_path);
    }
    dbg_log(format!("Copy-Paste time: {:?}", sw.elapsed()));
}

#[tauri::command]
async fn get_final_filename(act_file_name: String, from_path: String, is_for_dual_pane: String, copy_to_path: String) -> String {
    let file = fs::metadata(&from_path);
    if &file.is_ok() != &true {
        err_log("File could not be copied".into());
        return "".into();
    }
    let file_name: String;
    if is_for_dual_pane == "1" {
        file_name = act_file_name;
    }
    else {
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
    }
    else {
        return;
    }
    dbg_log(format!("Deleting: {}", String::from(&act_file_name)));
    if is_dir {
        let _ = remove_dir_all(act_file_name.replace("\\", "/"));
    }
    else {
        let _ = remove_file(act_file_name.replace("\\", "/"));
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
    }
    else if file_ext == ".rar" {
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
    }
    else if file_ext == ".7z" {
        let _ = sevenz_rust::decompress_file(&from_path, &from_path.strip_suffix(&file_ext).unwrap());
    }
    else if file_ext == ".tar" {
        unpack_tar(File::open(&from_path).unwrap());
    }
    else if file_ext == ".gz" {
        let file = File::open(&from_path).unwrap();
        let mut archive = GzDecoder::new(file);
        let mut buffer = Vec::new();
        let _ = archive.read_to_end(&mut buffer).unwrap();
        let _ = File::create(&from_path.strip_suffix(&file_ext).unwrap()).unwrap().write_all(&buffer);
        unpack_tar(File::open(&from_path.strip_suffix(&file_ext).unwrap()).unwrap());
        let _ = remove_file(&from_path.strip_suffix(&file_ext).unwrap());
    }
    else if file_ext == ".bz2" {
        let mut file = archiver_rs::Bzip2::open(&PathBuf::from(&from_path)).unwrap();
        file.decompress(&PathBuf::from(&from_path.strip_suffix(&file_ext).unwrap())).unwrap();
        unpack_tar(File::open(&from_path.strip_suffix(&file_ext).unwrap()).unwrap());
        let _ = remove_file(&from_path.strip_suffix(&file_ext).unwrap());
    }
    else {
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

    dbg_log(format!("Compression of '{}' started with compression level: {}", &from_path.split("/").last().unwrap(), &compression_level));
    let file_ext = ".".to_string().to_owned() + from_path.split(".").last().unwrap_or("");
    let _ = File::create(from_path.strip_suffix(&file_ext).unwrap_or(&from_path).to_owned() + ".zip").unwrap();
    let source: PathBuf;
    let archive = PathBuf::from(
        from_path.strip_suffix(&file_ext)
            .unwrap_or(&from_path)
            .to_owned()
            + ".zip",
    );
    if fs::metadata(&from_path).unwrap().is_dir() {
        source = PathBuf::from(&from_path);
    }
    else {
        let file_name = &from_path.split("/").last().unwrap();
        let _ = create_dir("__compressed_dir");
        let _ = copy(&from_path, "__compressed_dir/".to_string().to_owned() + file_name);
        source = PathBuf::from("__compressed_dir");
    }
    let options = FileOptions::default()
        .compression_method(zip::CompressionMethod::Zstd)
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
        let _ = copy(&item_path, "compressed_items_archive/".to_string().to_owned() + file_name);
    }
    compress_item(current_dir().unwrap().to_owned().join("compressed_items_archive").to_string_lossy().to_string(), compression_level).await;
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
    is_light_mode: String,
    is_image_preview: String,
    is_select_mode: String,
    arr_favorites: Vec<String>
) {
    let app_config_file = File::open(app_config_dir(&Config::default()).unwrap().join("com.rdpFX.dev/app_config.json")).unwrap();
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
        is_light_mode: is_light_mode.replace("\\", "/"),
        is_image_preview: is_image_preview.replace("\\", "/"),
        is_select_mode: is_select_mode.replace("\\", "/"),
        arr_favorites
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
async fn rename_elements_with_format(arr_elements: Vec<String>, new_name: String, start_at: i32, step_by: i32, n_digits: usize, ext: String) {
    let mut counter = start_at;
    for element in arr_elements {
        let mut item_ext: String = ext.to_string();
        if element.split(".").last().unwrap().len() > 0 && ext.len() == 0 {
            item_ext = format!("{}", ".".to_string() + element.split(".").last().unwrap());
        }
        let _ = fs::rename(&element, format!("{}{:0>n_digits$}{}", new_name, counter, item_ext));
        dbg_log(format!("Renamed from {} to {}", element, format!("{}{:0>n_digits$}{}", new_name, counter, item_ext)));
        counter += step_by;
    }
}

#[tauri::command]
async fn add_favorite(arr_favorites: Vec<String>) {
    let app_config_file = File::open(app_config_dir(&Config::default()).unwrap().join("com.rdpFX.dev/app_config.json")).unwrap();
    let app_config_reader = BufReader::new(app_config_file);
    let mut app_config: Value = serde_json::from_reader(app_config_reader).unwrap();
    app_config["arr_favorites"] = arr_favorites.clone().into_iter().map(|x| Value::String(x)).collect();
    let _ = serde_json::to_writer_pretty(
        File::create(app_config_dir(&Config::default())
            .unwrap()
            .join("com.rdpFX.dev/app_config.json")
            .to_str()
            .unwrap()
            .to_string(),
        ).unwrap(),
        &app_config,
    );
    dbg_log(format!("Saved favorites: {:?}", arr_favorites));
}

#[tauri::command]
async fn get_installed_apps() -> Vec<(String, String)>{
    #[cfg(not(target_os = "macos"))]
    return vec![];

    let list_apps = get_apps();
    let mut arr_apps: Vec<(String, String)> = vec![];
    for app in list_apps {
        arr_apps.push((app.name.clone(), app.app_path_exe.to_str().unwrap().to_string()));
    }
    return arr_apps;
}

#[tauri::command]
async fn open_with(file_path: String, app_path: String) {
    open_file_with(PathBuf::from(file_path), PathBuf::from(app_path));
}

#[tauri::command]
async fn find_duplicates(app_window: Window, path: String, depth: u32) -> Vec<Vec<DirWalkerEntry>> {
    let files = DirWalker::new()
        .depth(depth)
        .run(&path)
        .ext(vec![
            "png", "jpg", "jpeg", "txt", "svg", "gif", "mp4", "mp3", "wav",
            "pdf", "docx", "xlsx", "doc", "zip", "rar", "7z", "dmg", "iso",
            "exe", "msi", "jar", "deb", "sh", "py", "htm", "html"
        ])
        .get_items();
    let mut seen_items: Vec<DirWalkerEntry> = Vec::new();
    let mut duplicates: Vec<Vec<DirWalkerEntry>> = Vec::new();
    for item in files.into_par_iter().collect::<Vec<DirWalkerEntry>>() {
        let seen_item = seen_items.par_iter().find_any(|x| x.is_file == true && x.size == item.size && x.size > 0 && x.file_name.contains(&item.file_name.substring(0, item.file_name.len()-3)));
        if *&seen_item.is_some() {
            if duplicates.len() == 0 {
                duplicates.push(vec![seen_item.unwrap().clone(), item.clone()]);
            }
            else {
                let collection = duplicates.par_iter_mut().find_any(|x| x[0].size == seen_item.unwrap().size && x[0].size > 0 && x[0].file_name.contains(&item.file_name.substring(0, item.file_name.len()-3)));
                if *&collection.is_some() {
                    collection.unwrap().push(item.clone());
                }
                else {
                    duplicates.push(vec![item.clone(), seen_item.unwrap().clone()]);
                }
            }
        }
        else {
            seen_items.push(item);
        }
    }
    for (idx, arr_duplicate) in duplicates.clone().iter().enumerate() {
        let var_idx = &idx.clone().to_string();
        let mut inner_html = String::new();
        let mut js_query = String::new()+"
            var duplicate"+var_idx+" = document.createElement('div');
            duplicate"+var_idx+".setAttribute('itempaneside', '');
            duplicate"+var_idx+".setAttribute('itemisdir', '0');
            duplicate"+var_idx+".setAttribute('itemext', '');
            duplicate"+var_idx+".setAttribute('isftp', '0');
            duplicate"+var_idx+".className = 'list-item duplicate-item';
        ";
        for (idx, item) in arr_duplicate.clone().iter().enumerate() {
           inner_html.push_str(&(String::new()+"
                <div style='display: flex; align-items: center; justify-content: space-between;'>
                    <div>
                        <h4>"+&item.file_name+"</h3>
                        <h4 class='text-2'>"+&item.path+"</h4>
                        <h4 class='text-2'>"+&format_bytes(item.size)+"</h4>
                    </div>
            "));
           if item.file_name.ends_with("jpg")
           || item.file_name.ends_with("jpeg")
           || item.file_name.ends_with("png")
           || item.file_name.ends_with("gif")
           || item.file_name.ends_with("svg")
           || item.file_name.ends_with("webp")
           || item.file_name.ends_with("jfif")
           || item.file_name.ends_with("tiff")
           {
               inner_html.push_str(&(String::new()+"
                    <img style='box-shadow: 0px 0px 10px 1px var(--transparentColorActive); border-radius: 5px;' width='64px' height='auto' src='asset://localhost/"+&item.path+"'>
                    </div>
                "));
           }
           else {
               inner_html.push_str(&(String::new()+"
                    </div>
                "));
           }
           js_query.push_str(&(String::new()+"
                duplicate"+var_idx+".setAttribute('"+&format!("itempath-{}", idx)+"', '"+&item.path+"');
            "));
        }
        js_query.push_str(&(String::new()+"
            duplicate"+var_idx+".innerHTML = `"+&inner_html+"`;
            duplicate"+var_idx+".oncontextmenu = (e) => showExtraContextMenu(e, duplicate"+var_idx+");
            document.querySelector('.duplicates-list').append(duplicate"+var_idx+");
        "));
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
        0 => desktop_dir().unwrap_or_default().to_string_lossy().to_string(),
        1 => download_dir().unwrap_or_default().to_string_lossy().to_string(),
        2 => document_dir().unwrap_or_default().to_string_lossy().to_string(),
        3 => picture_dir().unwrap_or_default().to_string_lossy().to_string(),
        4 => video_dir().unwrap_or_default().to_string_lossy().to_string(),
        5 => audio_dir().unwrap_or_default().to_string_lossy().to_string(),
        _ => current_dir().unwrap().to_string_lossy().to_string(),
    }
}
