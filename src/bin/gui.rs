use std::rc::Rc;

use slint::SharedString;

slint::include_modules!();
fn main() {
    use slint::Model;

    let main_window = MainWindow::new().unwrap();

    let mut config_fields: Vec<FieldData> = vec![
        FieldData {
            label: "Client ID".into(),
            id: "client_id".into(),
            is_secret: false,
            value: SharedString::new(),
            default_value: "".into(),
        },
        FieldData {
            label: "Client Secret".into(),
            id: "client_secret".into(),
            is_secret: true,
            value: SharedString::new(),
            default_value: "".into(),
        },
        FieldData {
            label: "Bot Username".into(),
            id: "account".into(),
            is_secret: false,
            value: SharedString::new(),
            default_value: "".into(),
        },
        FieldData {
            label: "Streamer Username".into(),
            id: "channel".into(),
            is_secret: false,
            value: SharedString::new(),
            default_value: "".into(),
        },
        FieldData {
            label: "Redirect URI".into(),
            id: "redirect_urit".into(),
            is_secret: false,
            value: SharedString::new(),
            default_value: "http://localhost:8777".into(),
        },
        FieldData {
            label: "Listener Address".into(),
            id: "listener".into(),
            is_secret: false,
            value: SharedString::new(),
            default_value: "127.0.0.1:8777".into(),
        }
    ];

    for field_data in &mut config_fields {
        field_data.value = field_data.default_value.clone();
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
}
