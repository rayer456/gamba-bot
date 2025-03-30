use std::{env, fs, path::{Path, PathBuf}, process::exit, rc::Rc};
use anyhow::Result;
use futures::stream::PollNext;
use slint::{ComponentHandle, LogicalSize, SharedString};
use gamba_bot::{config::{self, Config, ConfigError}, helpers};

slint::include_modules!();
fn main() -> Result<()> {
    use slint::Model;


    let main_window = MainWindow::new().unwrap();

    /*
    Setting file found:
        Try reading and parsing:
            Cannot parse => use default config + save to existing path
            Can parse => use config
    Setting file not found:
        Try saving default config in APPDATA OR program directory:
            Create subfolders as needed
            Try saving:
                If fails, undo created directories, try secondary path
                If both paths fail:
                    Continue with default, show user message of failure
    */

    let mut empty_path = false;
    let mut cfg = match config::find_settings_path() {
        // SETTING FILE FOUND
        Some(settings_path) => match Config::from_path(settings_path.clone()) {
            Err(e) => {
                    let mut show_error_message = true;
                    let msg = match e {
                        ConfigError::FileNotFound => { // shouldn't even be possible
                            show_error_message = false;
                            "settings not found yo"
                        },
                        ConfigError::PermissionDenied => "permission error",
                        ConfigError::FileNotParseable => "invalid configuration",
                        ConfigError::Unknown => "unknown error",
                    };
                    if show_error_message {
                        main_window.set_popup_text(format!("Failed to load the config: {}", msg).into());
                        main_window.set_show_popup(true);
                    }
    
                    let mut cfg = Config::default();
                    cfg.save_path = settings_path;
                    let _ = cfg.update_file();
                    cfg
            },
            Ok(cfg) => {
                println!("settings seem okay :o");
                cfg
            }
        },
        // SETTING FILE NOT FOUND
        None => {
            let path1 = env::var("APPDATA").map_or_else(|_| "./".into(), |s| PathBuf::from(s).join("gamba-bot")).join("config/settings.toml");
            let path2 = PathBuf::from("./config/settings.toml");
            match config::try_saving_config_here([path1, path2]) {
                Ok(cfg) => cfg,
                _ => {
                    empty_path = true;
                    Config::default()
                }
            }
        }
    };


    let mut config_fields: Vec<FieldData> = main_window.get_config_fields().iter().collect();
    populate_config_fields(&mut config_fields, &cfg);

    let config_fields_model = Rc::new(slint::VecModel::from(config_fields));
    main_window.set_config_fields(Rc::clone(&config_fields_model).into());

    let main_window_weak = main_window.as_weak();
    main_window.on_save_settings(move || {
        let main_window = main_window_weak.unwrap();
        let config_fields: Vec<FieldData> = main_window.get_config_fields().iter().collect();
        update_cfg_with_fields(&config_fields, &mut cfg);
        if let Err(e) = cfg.update_file() {
            main_window.set_popup_text(format!("Unable to save settings: {e}").into());
            main_window.set_show_popup(true);
        }
    });

    if empty_path {
        main_window.set_popup_text("Failed to find a path. Settings won't be saved.".into());
        main_window.set_show_popup(true);
    }

    
    main_window.run().unwrap();

    Ok(())
}



fn populate_config_fields(config_fields: &mut Vec<FieldData>, cfg: &Config) {
    for field_data in config_fields {
        match field_data.id.as_str() {
            "client_id" => field_data.value = cfg.twitch_cfg.client_id.to_owned().into(),
            "client_secret" => field_data.value = cfg.twitch_cfg.client_secret.to_owned().into(),
            "account" => field_data.value = cfg.twitch_cfg.account.to_owned().into(),
            "channel" => field_data.value = cfg.twitch_cfg.channel.to_owned().into(),
            "redirect_uri" => field_data.value = cfg.twitch_cfg.redirect_uri.to_owned().into(),
            "listener" => field_data.value = cfg.twitch_cfg.listener.to_owned().into(),

            _ => panic!("FieldData id '{}' was not implemented.", field_data.id),
        };
    }
}

fn update_cfg_with_fields(config_fields: &Vec<FieldData>, cfg: &mut Config) {
    for field_data in config_fields {
        let value: String = field_data.value.clone().into();
        match field_data.id.as_str() {
            "client_id" => cfg.twitch_cfg.client_id = value,
            "client_secret" => cfg.twitch_cfg.client_secret = value,
            "account" => cfg.twitch_cfg.account = value,
            "channel" => cfg.twitch_cfg.channel = value,
            "redirect_uri" => cfg.twitch_cfg.redirect_uri = value,
            "listener" => cfg.twitch_cfg.listener = value,

            _ => panic!("FieldData id'{}' was not implemented.", field_data.id),
        };
    }
}

