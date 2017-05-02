//#![allow(unstable)]
//#![feature(collections)]
mod karlson;
use karlson::read_file;
use karlson::Settings;
use karlson::Karlson;
use karlson::Device;

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


fn get_idx(s: &str) -> i32 {
    match s.chars().last() {
        Some(c) => match c.to_digit(12) {
            Some(i) => i as i32,
            None => -1
        },
        None => -1
    }
}

fn prepare_settings(cfg: &Value) -> (Settings, HashMap<i32, Settings>, Vec<(PathBuf, Settings)>) {
    let mut config: HashMap<i32, Settings> = HashMap::new();
    let mut cdevs = Vec::<(PathBuf, Settings)>::new();
    let cdef = if cfg.is_table() {
        Settings::new(&cfg)
    } else {
        Settings::new(&Value::from(false))
    };
    let cempty = Vec::new();

    let cfgs = cfg.get("propellers");
    if cfgs.is_some() {
        if !cfgs.unwrap().is_array() {
            println!("ERROR in config! Wrong blocks format for [[propellers]]");
        } else {
            let ccfg = cfgs.unwrap().as_array().unwrap_or(&cempty);
            
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
                    if id >= 0  {
                        config.insert(id as i32, Settings::new_with(&c, &cdef));
                    }
                }
            }
        }
    }

    let cfgdev = cfg.get("devices");
    if cfgdev.is_some() {
        if !cfgdev.unwrap().is_array() {
            println!("ERROR in config! Wrong blocks format for [[devices]]");
        } else {
            let dcfg = cfgdev.unwrap().as_array().unwrap_or(&cempty);
            for c in dcfg {
                if !c.is_table() {
                    continue;
                }
                let t = c.as_table().unwrap();
                if !t.contains_key("path") {
                    println!("ERROR can not add [[devices]] without \"path\" property");
                    continue;
                }
                let p = PathBuf::from(t["path"].as_str().unwrap_or_default());
                if !p.is_dir() {
                    println!("ERROR path for [[devices]] is not directory {:?}", p);
                    continue;
                }
                if !t.contains_key("temp_files") || !t["temp_files"].is_array() {
                    println!("ERROR can not add [[devices]] without valid \"temp_files\" property");
                    continue;
                }
                cdevs.push( (p, Settings::new_with(&c, &cdef)) );
            }
        }
    }
    
    (cdef, config, cdevs)
}


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

fn run_daemon(ctoml: &Value) {
    let (cdef, cidx, cdevs) = prepare_settings(ctoml);
    let idx_empty = Vec::new();

    let idxs = match ctoml.as_table() {
        Some(t) => match t.get("idx") {
            Some(i) => i.as_array().unwrap_or(&idx_empty),
            None => &idx_empty
        },
        None => &idx_empty
    };
    
    let idxx: HashSet<i32> = idxs.iter().filter_map(|v| v.as_integer()).map(|v| v as i32).collect();

    
    let base = Path::new(karlson::DIR_DEVICES);
    if !base.exists() || !base.is_dir() {
        print!("ERROR: Can not access or open directory {}", karlson::DIR_DEVICES);
        return;
    }

    let mut paths:Vec<std::fs::DirEntry>  = std::fs::read_dir(base).unwrap()
        .map(|r| r.unwrap())
        .collect();
    paths.sort_by_key(|d| d.path());



    if cfg!(debug_assertions) {
        println!("{:?}", idxx);
    }

    let mut karlsons: Vec<Karlson> = Vec::new();
    for p in paths {
        let idx = get_idx(p.path().to_str().unwrap());

        if !idxx.contains(&idx) {
            println!("SKIP device {} not allowed in config", idx);
            continue;
        }

        let c: &Settings  = cidx.get(&idx).unwrap_or(&cdef);
        if cfg!(debug_assertions) {
            println!("PROPELLER {} {:?} {:?}", idx, p.path(), c);
        }
        let dev = Device::new(&p.path(), &c);
        if dev.propeller.is_none() {
            println!("ERROR SKIP propeller {} pwm not found for {:?}", idx, p.path());
            continue;
        }
        
        karlsons.push(Karlson::new(dev, &c));
        if cfg!(debug_assertions) {
            println!("ADD propeller {}", idx);
        }
    }

    let mut devices = Vec::<Karlson>::new();
    for (dp, dc) in cdevs {
        if cfg!(debug_assertions) {
            println!("DEVICE {:?} {:?}", dp, dc);
        }
        let dev = Device::new(&dp, &dc);
        if dev.propeller.is_none() {
            println!("ERROR SKIP device, pwm not found for {:?}", dp);
            continue;
        }

        devices.push(Karlson::new(dev, &dc));
        if cfg!(debug_assertions) {
            println!("ADD device {:?}", dp);
        }
    }


    let mut t = SystemTime::now();
    let mut start = true;
    loop {
        if karlsons.is_empty() && devices.is_empty() {
            println!("(X_X) No devices was added to service. Just do nothing and sleep!");
            if !idxx.is_empty() {
                println!("Allowed propellers ids {:?}", idxx);
            }
            std::thread::sleep(std::time::Duration::from_secs(10));
        }

        for k in &mut karlsons {
            k.spin();
        }

        for d in &mut devices {
            d.spin();
        }
        
        for _ in 0..20 {
            std::thread::sleep(std::time::Duration::from_millis(500));
        }
        
        let n = SystemTime::now();
        let d = n.duration_since(t).ok()
            .map(|it| it.as_secs())
            .unwrap_or(0);
        
        if d > 120 || start {
            start = false;
            t = n;
            if !karlsons.is_empty() {
                println!("STAT: {}", format_info(&karlsons));
            }
            if !devices.is_empty() {
                println!("STAT DEV: {}", format_info(&devices));
            }
        }
    }
}


fn print_devices() {
    let list = Device::list();
    for p in list {
        println!("{} \"{}\" valid:{}", p.dir_name, p.name, p.propeller.is_some())
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
    opts.optopt("d", "daemon", "run daemon with settings from file", "SETTINGS.toml");


    let matches;
    match opts.parse(&args[1..]) {
        Ok(m) => {
            matches = m;
        },
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
    /*
    let toml_f = Path::new(&toml_path);
    if !toml_f.exists() || !toml_f.is_file() {
        return print_help(&program, opts);
        //return;
    }
     */

    match read_file(&toml_path) {
        Ok(s) => {
            match s.parse::<Value>() {
                Ok(c) => {
                    run_daemon(&c);
                },
                Err(e) => {
                    println!("ERROR {:?}", e);
                    return print_help(&program, opts);
                }
            };
        },
        Err(e) => {
            println!("ERROR {:?}", e);
            return print_help(&program, opts);
        }
    }
}
