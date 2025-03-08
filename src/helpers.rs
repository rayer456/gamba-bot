use std::{collections::HashMap};

use rand::Rng;

pub fn extract_parameters(url: &str) -> HashMap<String, String> {
    let mut res = HashMap::new();
    let para_str: String = url
        .chars()
        .skip_while(|c| *c != '?')
        .skip(1)
        .take_while(|c| *c != '#')
        .collect();

    let tuple_paras = para_str.split('&').collect::<Vec<&str>>();
    for tuple_para in tuple_paras {
        let kv = tuple_para.split('=').collect::<Vec<&str>>();

        if kv.len() != 2 { continue }
        res.insert(kv[0].to_string(), kv[1].to_string());
    }

    return res
}

pub fn get_rand_string(length: u8) -> String {
    let chars = "abcdefghijklmnopqrstuvwxyz0123456789"; // 36 chars
    let mut rand_str = String::new();
    let mut rng = rand::thread_rng();
    for i in 0..length {
        rand_str.push(chars.chars().nth(rng.gen_range(0..36)).unwrap());
    }

    rand_str
}
