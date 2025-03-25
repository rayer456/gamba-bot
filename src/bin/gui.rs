use std::{process::exit, rc::Rc};
use anyhow::Result;
use slint::SharedString;
use gamba_bot::config::{self, Config, ConfigError};

slint::include_modules!();
fn main() -> Result<()> {
    use slint::Model;

    // Read from file or if bullshit give default instance of Config defined in Config
    let cfg = match Config::from_path("settings.toml") {
        Err(e) => {
                match e {
                    ConfigError::FileNotFound => println!("settings not found yo"),
                    ConfigError::PermissionDenied => println!("not allowed to read yo"),
                    ConfigError::FileNotParseable => println!("this ain't valid yaml OR couldn't be parsed to Config object"),
                    ConfigError::Unknown => println!("some unexpected shit happened yo"),
                };
                Config::default()
        },
        Ok(cfg) => {
            println!("settings seem okay :o");
            cfg
        }
    };


    // TODO: Need to implement custom error to determine whether the file doesn't exist OR is corrupted.

    // Try to read file
        // If not exists: create Config struct with default values and save to file
    
    // Try to parse file
        // If un-parseable: create Config struct with default values and save to file

    

    let main_window = MainWindow::new().unwrap();

    let mut config_fields: Vec<FieldData> = main_window.get_config_fields().iter().collect();

    // For config field with <id> fill in value found in Config
    for field_data in &mut config_fields {
        match field_data.id.as_str() {
            "client_id" => field_data.value = cfg.twitch_cfg.client_id.to_owned().into(),
            "client_secret" => field_data.value = cfg.twitch_cfg.client_secret.to_owned().into(),
            "account" => field_data.value = cfg.twitch_cfg.account.to_owned().into(),
            "channel" => field_data.value = cfg.twitch_cfg.channel.to_owned().into(),
            "redirect_uri" => field_data.value = cfg.twitch_cfg.redirect_uri.to_owned().into(),
            "listener" => field_data.value = cfg.twitch_cfg.listener.to_owned().into(),

            _ => println!("kys"),
        };
    }

    let config_fields_model = Rc::new(slint::VecModel::from(config_fields));
    main_window.set_config_fields(Rc::clone(&config_fields_model).into());

    let main_window_weak = main_window.as_weak();
    main_window.on_save_settings(move || {
        let main_window = main_window_weak.unwrap();
        let config_fields: Vec<FieldData> = main_window.get_config_fields().iter().collect();
        for cfg_field in config_fields {
            println!("{}: {}", cfg_field.label, cfg_field.value);
        }
        // println!("{:?}", config_fields);
    });


    main_window.run().unwrap();

    Ok(())
}
