//#![allow(unstable)]
//#![feature(collections)]

extern crate regex;

mod core;
use core::Settings;
use core::read_file;

mod dsys;
mod dnv;

mod karlson;
use karlson::Karlson;


use std::collections::HashMap;
use std::collections::HashSet;
use std::path::Path;
use std::path::PathBuf;
use std::time::SystemTime;

extern crate getopts;
use getopts::Options;
use std::env;

extern crate toml;
use toml::Value;
//use toml::value::Table;


fn settings_default(tconf: &Value) -> Settings {
    if tconf.is_table() {
        Settings::from(&tconf)
    } else {
        Settings::from(&Value::from(false))
    }
}

/// Return (sys settings, nv settings)
fn settings_propellers(
    tconf: &Value,
    setdef: &Settings,
) -> (HashMap<i32, Settings>, HashMap<i32, Settings>) {

    let cempty = Vec::new();
    let mut scfg: HashMap<i32, Settings> = HashMap::new();
    let mut nvcfg: HashMap<i32, Settings> = HashMap::new();

    let props = tconf.get("propellers");
    if props.is_none() {
        return (scfg, nvcfg);
    }

    if !props.unwrap().is_array() {
        println!("ERROR in config! Wrong blocks format for [[propellers]]");
        return (scfg, nvcfg);
    }

    let ccfg = props.unwrap().as_array().unwrap_or(&cempty);

    for c in ccfg {
        if !c.is_table() || !c.as_table().unwrap().contains_key("idx") {
            continue;
        }
        let idx = c["idx"].as_array().unwrap_or(&cempty);

        for idv in idx {
            if !idv.is_integer() {
                continue;
            }
            let id = idv.as_integer().unwrap();
            if id >= 0 {
                let dev_set = Settings::from_with(&c, &setdef);
                match dev_set.dev_type.as_ref() {
                    "nv" => nvcfg.insert(id as i32, dev_set),
                    _ => scfg.insert(id as i32, dev_set),
                };
            }
        }
    }

    (scfg, nvcfg)
}

/*
fn format_info(devs:&Vec<Karlson>) -> String{
    let d: usize = 0;
    let forms:Vec<String> = devs.iter()
        .map(|it| format!("{}:{} {}C",
                         it.device.dir_name,
                         it.device.propeller.clone().map(|v| v.pwm() as isize).unwrap_or(-1),
                          it.device.temps().iter().max().unwrap_or(&d))
        ).collect();
    
    forms.join(", ")
}
*/

fn extract_ids(tconf: &Value, property_name: &str) -> HashSet<i32> {
    let empty_vec = Vec::new();

    (match tconf.as_table() {
         Some(t) => {
             match t.get(property_name) {
                 Some(i) => i.as_array().unwrap_or(&empty_vec),
                 None => &empty_vec,
             }
         }
         None => &empty_vec,
     }).iter()
        .filter_map(|v| v.as_integer())
        .map(|v| v as i32)
        .collect()
}

fn init_karlsons(tconf: &Value, set_def: &Settings) -> Vec<Karlson> {

    let (sys_set, nv_set) = settings_propellers(tconf, &set_def);

    let sys_ids = extract_ids(tconf, "idx");
    let nv_ids = extract_ids(tconf, "nv_idx");

    let mut karlsons: Vec<Karlson> = Vec::new();
    let devs = karlson::list_devices();

    #[cfg(debug_assertions)]
    {
        println!("KARLSONS {:?}", devs);
    }

    for d in devs {
        let skipit = match d.dev_type.as_ref() {
            "nv" => !nv_ids.contains(&d.id),
            _ => !sys_ids.contains(&d.id),
        };
        if skipit {
            #[cfg(debug_assertions)]
            {
                println!("Skip device #{} {}", d.id, d.name);
            }
            continue;
        }

        let ns = match d.dev_type.as_ref() {
            "nv" => nv_set.get(&d.id).unwrap_or(&set_def),
            _ => sys_set.get(&d.id).unwrap_or(&set_def),
        };

        karlsons.push(Karlson::new(&d, &ns));

    }

    if karlsons.is_empty() {
        if !sys_ids.is_empty() {
            println!("Allowed system devices ids {:?}", sys_ids);
        }
        if !nv_set.is_empty() {
            println!("Allowed Nvidia devices ids {:?}", nv_ids);
        }
    }

    karlsons
}

fn init_devices(tconf: &Value, set_def: &Settings) -> Vec<Karlson> {
    let mut devices = Vec::<Karlson>::new();
    let cempty = Vec::new();

    let cfgdev = tconf.get("devices");

    if cfgdev.is_none() {
        println!("[[devices]] configuration is empty");
        return devices;
    }

    if !cfgdev.unwrap().is_array() {
        println!("ERROR in config! Wrong blocks format for [[devices]]");
        return devices;
    }
    let dcfg = cfgdev.unwrap().as_array().unwrap_or(&cempty);
    let mut id: i32 = 0;
    for c in dcfg {
        if !c.is_table() {
            println!("ERROR! [[devices]] #{} block is not table", id);
            continue;
        }
        let t = c.as_table().unwrap();

        let has_sys = t.contains_key("sys_temp_input") && t["sys_temp_input"].is_array();
        let has_nv = t.contains_key("nv_temp_input") && t["nv_temp_input"].is_array();

        if !has_nv && !has_sys {
            println!("ERROR can not add [[devices]] without temperature inputs");
            continue;
        }
        let dev_set = Settings::from_with(&c, set_def);
        devices.push(Karlson::new_device(id, &dev_set));
        id += 1;
    }

    #[cfg(debug_assertions)]
    {
        println!("DEVICES {:?}", devices);
    }
    devices
}


fn loop_daemon(mut karlsons: Vec<Karlson>, mut devices: Vec<Karlson>) {
    let mut t = SystemTime::now();
    let mut start = true;
    loop {
        if karlsons.is_empty() && devices.is_empty() {
            println!("(X_X) No devices was added to service. Just do nothing and sleep!");
            std::thread::sleep(std::time::Duration::from_secs(10));
        }

        for d in &mut devices {
            d.spin();
        }

        for k in &mut karlsons {
            k.spin();
        }

        for _ in 0..20 {
            std::thread::sleep(std::time::Duration::from_millis(500));
        }

        let n = SystemTime::now();
        let d = n.duration_since(t).ok().map(|it| it.as_secs()).unwrap_or(0);

        if d > 120 || start {
            start = false;
            t = n;
            // if !karlsons.is_empty() {
            //     println!("STAT: {}", format_info(&karlsons));
            // }
            // if !devices.is_empty() {
            //     println!("STAT DEV: {}", format_info(&devices));
            // }
        }
    }
}

fn run_daemon(tconf: &Value) {
    let set_def = settings_default(tconf);

    let karlsons = init_karlsons(tconf, &set_def);
    let devices = init_devices(tconf, &set_def);

    loop_daemon(karlsons, devices);
}

fn print_devices() {
    let list = karlson::list_devices();
    for d in list {
        println!("{}#{} {}", d.dev_type, d.id, d.name)
    }
}


fn print_help(program: &str, opts: Options) {
    let brief = format!("Usage: {} FILE [options]", program);
    print!("{}", opts.usage(&brief));
}


fn main() {
    let args: Vec<String> = env::args().collect();
    let program = args[0].clone();

    let mut opts = Options::new();
    opts.optflag("l", "list", "list available devices with pwm1 interface");
    opts.optflag("h", "help", "print this help menu");
    opts.optopt(
        "d",
        "daemon",
        "run daemon with settings from file",
        "SETTINGS.toml",
    );


    let matches;
    match opts.parse(&args[1..]) {
        Ok(m) => {
            matches = m;
        }
        Err(f) => {
            println!("ERROR: {}\n", f.to_string());
            print_help(&program, opts);
            return;
        }
    };

    if matches.opt_present("h") {
        print_help(&program, opts);
        return;
    }

    if matches.opt_present("l") {
        print_devices();
        return;
    }

    // DAEMON
    let daemon = matches.opt_str("d");

    if daemon.is_none() {
        return print_help(&program, opts);
    }

    let toml_path = PathBuf::from(daemon.unwrap_or(String::from("")));

    // Check if file exists
    let toml_f = Path::new(&toml_path);
    if !toml_f.exists() || !toml_f.is_file() {
        return print_help(&program, opts);
    }

    match read_file(&toml_path) {
        Ok(s) => {
            match s.parse::<Value>() {
                Ok(c) => {
                    run_daemon(&c);
                }
                Err(e) => {
                    println!("ERROR {:?}", e);
                    return print_help(&program, opts);
                }
            };
        }
        Err(e) => {
            println!("ERROR {:?}", e);
            return print_help(&program, opts);
        }
    }
}
