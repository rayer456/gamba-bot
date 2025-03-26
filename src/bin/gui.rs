use std::{env, process::exit, rc::Rc};
use anyhow::Result;
use slint::SharedString;
use gamba_bot::config::{self, Config, ConfigError};

slint::include_modules!();
fn main() -> Result<()> {
    use slint::Model;

    let main_window = MainWindow::new().unwrap();

    // TODO: Probably show an error to the user
    let mut cfg = match Config::from_path("settings.toml") {
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

    // TODO: Add some advanced settings

    //let shit = TestDialog::new().unwrap().show();

    // Show testing dialog
    // main_window.set_show_test_dialog(true);
    
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
            println!("Unable to save settings: {e}");
        }

    });


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