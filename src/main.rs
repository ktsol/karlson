//#![allow(unstable)]
//#![feature(collections)]
use std::fs::File;
use std::collections::VecDeque;
use std::collections::HashMap;
use std::collections::HashSet;
use std::path::Path;
use std::path::PathBuf;
use std::io::Read;
use std::io::Error;
use std::io::Write;

extern crate getopts;
use getopts::Options;
use std::env;

extern crate toml;
use toml::Value;
use toml::value::Table;

static HWMONS: &'static str = "/sys/class/hwmon";
static TEMP_MEMORY: usize = 6;
static TEMP_SCALE: i32 = 1000;
static PWM_STEP: i32 = 5;
static DEBUG: bool = true;

fn read_file(p: &str) -> Result<String, Error> {
    let path = Path::new(p);
    match File::open(&path) {
        Ok(mut f) => {
            let mut s = String::new();
            match f.read_to_string(&mut s) {
                Ok(_) => Ok(s),
                Err(e) => Err(e)
            }
        },
        Err(err) => Err(err)
    }
}


fn read_file_int(p: &str) -> i32 {
    match read_file(p) {
        Ok(v) => match v.trim().parse::<i32>() {
            Ok(v) => v,
            Err(_) => -1
        },
        Err(_) => -1
    }
}

//use std::str::;

fn get_idx(s: &str) -> i32 {
    match s.chars().last() {
        Some(c) => match c.to_digit(12) {
            Some(i) => i as i32,
            None => -1
        },
        None => -1
    }
}

struct Settings {
    pwm_ok: i32,
    pwm_max: i32,
    pwm_min: i32,
    temp_ok: i32,
    temp_hot: i32,
    temp_crit: i32,
}

impl Settings {

    fn from_table(t: &Table) -> Settings {
        //println!("{} {:?}",  t["temp_ok"].as_integer().unwrap_or(00), t);
        Settings {
            pwm_ok: if t.contains_key("pwm_ok") {
                t["pwm_ok"].as_integer().unwrap_or(100) as i32
            } else { 100 },
            pwm_max: if t.contains_key("pwm_max") {
                t["pwm_max"].as_integer().unwrap_or(-1) as i32
            } else { -1 },
            pwm_min: if t.contains_key("pwm_min") {
                t["pwm_min"].as_integer().unwrap_or(0) as i32
            } else { 0 },
            temp_ok: if t.contains_key("temp_ok") {
                t["temp_ok"].as_integer().unwrap_or(65) as i32
            } else { 65 },
            temp_hot: if t.contains_key("temp_hot") {
                t["temp_hot"].as_integer().unwrap_or(75) as i32
            } else { 75 },
            temp_crit: if t.contains_key("temp_crit") {
                t["temp_crit"].as_integer().unwrap_or(85) as i32
            } else { 85 },
        }
    }
    
    fn from_table_settigs(t: &Table, s: &Settings) -> Settings {
        Settings {
            pwm_ok: if t.contains_key("pwm_ok") {
                t["pwm_ok"].as_integer().unwrap_or(s.pwm_ok as i64) as i32
            } else { s.pwm_ok },
            pwm_max: if t.contains_key("pwm_max") {
                t["pwm_max"].as_integer().unwrap_or(s.pwm_max as i64) as i32
            } else { s.pwm_max },
            pwm_min: if t.contains_key("pwm_min") {
                t["pwm_min"].as_integer().unwrap_or(s.pwm_min as i64) as i32
            } else { s.pwm_min },
            temp_ok: if t.contains_key("temp_ok") {
                t["temp_ok"].as_integer().unwrap_or(s.temp_ok as i64) as i32
            } else { s.temp_ok },
            temp_hot: if t.contains_key("temp_hot") {
                t["temp_hot"].as_integer().unwrap_or(s.temp_hot as i64) as i32
            } else { s.temp_hot },
            temp_crit: if t.contains_key("temp_crit") {
                t["temp_crit"].as_integer().unwrap_or(s.temp_crit as i64) as i32
            } else { s.temp_crit },
        }
    }
}


impl std::fmt::Debug for Settings {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(f, "Settings {{ pwm ok:{} max:{} min:{}; temp ok:{} hot:{} crit:{} }}", self.pwm_ok, self.pwm_max, self.pwm_min, self.temp_ok, self.temp_hot, self.temp_crit)
    }
}

impl Clone for Settings {
    fn clone(&self) -> Settings {
        Settings {
            pwm_ok: self.pwm_ok,
            pwm_max: self.pwm_max,
            pwm_min: self.pwm_min,
            temp_ok: self.temp_ok,
            temp_hot: self.temp_hot,
            temp_crit: self.temp_crit,
        }
    }
}

struct Propeller {
    path: String,
    dirname: String,
    name: String,
    pwm_file: String,
    pwm_min: i32,
    pwm_max: i32,
    temp_file: String,
    valid: bool,
}


impl Propeller {

    fn new(pbdir: &PathBuf ) -> Propeller {
        let mut pbname = pbdir.clone();
        let mut pbpwm = pbdir.clone();
        let mut pbtemp = pbdir.clone();
        let mut valid = true;
        
        pbname.push("name");
        pbpwm.push("pwm1");
        pbtemp.push("temp1_input");

        let mut path_name = pbname.to_string_lossy().into_owned();
        let mut path_pwm = pbpwm.to_string_lossy().into_owned();
        let mut path_temp = pbtemp.to_string_lossy().into_owned();
        
        if !pbpwm.is_file() || !pbtemp.is_file() {
            path_pwm = String::new();
            path_temp = String::new();
            valid = false;
        }
        
        let name = if pbname.is_file() {
            String::from(read_file(&path_name).unwrap_or(String::from("Err")).trim())
        } else {
            String::from("N/A")
        };

        // Min and max values
        let (mut pmin, mut pmax) = (pbdir.clone(), pbdir.clone());
        pmin.push("pwm1_min");
        pmax.push("pwm1_max");

        let mut min = 0;
        if pmin.is_file() {
            let val = read_file_int(pmin.to_str().unwrap());
            if val >0 {
                min = val;
            }
        }
        
        let mut max = 255;
        if pmax.is_file() {
            let val = read_file_int(pmax.to_str().unwrap());
            if val > 0 {
                max = val;
            }
        }
                

        Propeller {
            dirname: pbdir.file_name().unwrap().to_string_lossy().into_owned(),
            path: pbdir.to_string_lossy().into_owned(),
            name: name,
            pwm_file: path_pwm,
            pwm_min: min,
            pwm_max: max,
            temp_file: path_temp,
            valid: valid,
        }
    }

    fn new_with_settings(pbdir: &PathBuf, s: &Settings ) -> Propeller {
        let mut p = Propeller::new(pbdir);

        if s.pwm_max > 0 {
            p.pwm_max = s.pwm_max;
        }

        if s.pwm_min > 0 {
            p.pwm_min = s.pwm_min;
        }
        return p;
    }

    fn list() -> Vec<Propeller> {
        let base = Path::new(HWMONS);
        if !base.exists() || !base.is_dir() {
            print!("Can not access or open directory {}", HWMONS);
            return Vec::new();
        }
        
        let mut paths:Vec<Propeller>  = std::fs::read_dir(base).unwrap()
            .map(|r| r.unwrap())
            .map(|it| Propeller::new(&it.path()))
            .collect();
        
        paths.sort_by_key(|p| p.path.clone());

        return paths;
    }

    fn pwm(&self) -> i32 {
        read_file_int(&self.pwm_file.as_str())
    }

    fn pwm_set(&self, val: i32) -> i32 {
        let mut nval = val;
        if val > self.pwm_max {
            nval = self.pwm_max;
        }
        if val < self.pwm_min {
            nval = self.pwm_min;
        }

        let ppath = PathBuf::from(&self.pwm_file);


        let mut fopts = std::fs::OpenOptions::new();
        fopts.write(true);
        
        
        //match File::open(ppath) {
        match fopts.open(ppath) {
            Ok(mut f) => {
                match write!(f, "{}", nval) {
                    Ok(_) => nval,
                    Err(_) => {
                        println!("ERROR can not write {} to {}", nval, self.pwm_file);
                        -1
                    }
                }
            },
            Err(e) => {
                println!("ERROR: can not write {} to pwm {} {}", nval, self.pwm_file, e);
                -1
            }
        }
    }
    
    fn temp(&self) -> i32 {
        let t = read_file_int(&self.temp_file.as_str());
        if t < 0 {
            t
        } else {
            t / TEMP_SCALE
        }
    }

}


struct Jam {
    pwm_ok: i32,
    temp_ok: i32,
    temp_hot: i32,
    temp_crit: i32,
}


struct Karlson {
    propeller: Propeller,
    jam: Jam,
    pwm_speed: i32,
    temps: VecDeque<i32>,
}


impl Karlson {

    fn new(p: Propeller, s:&Settings) -> Karlson {
        Karlson {
            pwm_speed: p.pwm(),
            propeller: p,
            temps: VecDeque::new(),
            jam: Jam {
                pwm_ok: if s.pwm_ok > 0 { s.pwm_ok } else { 100 },
                temp_ok: if s.temp_ok > 0 { s.temp_ok } else { 65 },
                temp_hot: if s.temp_hot > 0 { s.temp_hot } else { 75 },
                temp_crit: if s.temp_crit > 0 { s.temp_crit } else { 85 }
            }
        }
    }

    
    /// Do some stuff to adjust Propeller speed
    fn spin(&mut self) {
        if !self.propeller.valid {
            return
        }
        
        let temp_now = self.propeller.temp();
        if temp_now <= 0 {
            println!("ERROR: temparature is {}C for {}", temp_now, self.propeller.path);
            return
        }

        self.temps.push_front(temp_now);
        //self.temps.truncate(TEMP_MEMORY);
        for _ in TEMP_MEMORY..self.temps.len() {
            self.temps.pop_back();
        }

        //let temp_old: i32 = self.temps[self.temps.len() - 1];
        let temp_avg = self.temp_avg();
        let pwm_now = self.pwm_speed;

        if DEBUG {
            //self.jam.
            println!("{}({}) TEMP:{}C ok:{}C hot:{}C PWM:{} ok:{}",
                     self.propeller.dirname, self.propeller.name,
                     temp_now, self.jam.temp_ok, self.jam.temp_hot,
                     pwm_now, self.jam.pwm_ok);
        }

        if temp_now <= self.jam.temp_ok {
            // Not hot at all. Only decrease temp here
            if temp_now < self.jam.temp_ok && self.jam.temp_ok - temp_avg > 2 {
                self.pwm_update(pwm_now - PWM_STEP, temp_now);
            } else if self.pwm_speed > self.jam.pwm_ok {
                self.pwm_update(pwm_now - PWM_STEP, temp_now);
            }
        } else if temp_now > self.jam.temp_ok && temp_now < self.jam.temp_hot {
            // In this interval JUST normalize pwm up to OK level
            if self.pwm_speed < self.jam.pwm_ok {
                self.pwm_update(pwm_now + PWM_STEP, temp_now);
            }
            if self.pwm_speed > self.jam.pwm_ok {
                self.pwm_update(pwm_now - PWM_STEP, temp_now);
            }
        } else {
            // Hot temp increase pwm only
            if self.pwm_speed < self.jam.pwm_ok {
                // Just in case
                let pwm_ok = self.jam.pwm_ok;
                self.pwm_update(pwm_ok + PWM_STEP * 5, temp_now);
            } else {
                self.pwm_update(pwm_now + PWM_STEP, temp_now);
            }
        }
    }

    /// Average temparature in history
    fn temp_avg(&self) -> i32 {
        if self.temps.len() == 1 {
            return self.temps[0];
        } else if self.temps.len() < 1 {
            return 0;
        }

        let mut tsum:i32 = 0;
        for t in self.temps.iter() {
            tsum = tsum + t;
        }

        (tsum as f32 / self.temps.len() as f32).round() as i32
    }

    fn pwm_update(&mut self, pwm:i32, temp: i32) {
        if pwm != self.pwm_speed && pwm >= 0 {
            let p = self.propeller.pwm_set(pwm);
            if p >= 0 {
                self.pwm_speed = p;
                println!("{}({}) {}C PWM updated to {}", self.propeller.dirname, self.propeller.name, temp, p);
            }
        }
    }
}


fn prepare_settings(cfg: &Value) -> HashMap<i32, Settings> {
    let mut config: HashMap<i32, Settings> = HashMap::new();

    if cfg.is_table() {
        config.insert(-1, Settings::from_table(&cfg.as_table().unwrap()));
    } else {
        config.insert(-1, Settings::from_table(&Table::new()));
    }
    let dcfg: &Settings = &config[&-1].clone();

    let cempty = Vec::new();
    let ccfg = cfg["propellers"].as_array().unwrap_or(&cempty);

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
                //println!("ID {} {:?}", id, c);
                
                config.insert(id as i32, Settings::from_table_settigs(&c.as_table().unwrap(), dcfg));
            }
        }
    }
    return config;
}


fn run_daemon(ctoml: &Value) {
    let cfg = prepare_settings(ctoml);
    let idx_empty = Vec::new();

    let idxs = match ctoml.as_table() {
        Some(t) => match t.get("idx") {
            Some(i) => i.as_array().unwrap_or(&idx_empty),
            None => &idx_empty
        },
        None => &idx_empty
    };
    
    let idxx: HashSet<i32> = idxs.iter().filter_map(|v| v.as_integer()).map(|v| v as i32).collect();

    
    let base = Path::new(HWMONS);
    if !base.exists() || !base.is_dir() {
        print!("ERROR: Can not access or open directory {}", HWMONS);
        return;
    }

    let mut paths:Vec<std::fs::DirEntry>  = std::fs::read_dir(base).unwrap()
        .map(|r| r.unwrap())
        .collect();
    paths.sort_by_key(|d| d.path());



    println!("{:?}", idxx);
    let mut karlsons: Vec<Karlson> = Vec::new();
    for p in paths {
        let idx = get_idx(p.path().to_str().unwrap());

        if !idxx.is_empty() && !idxx.contains(&idx) {
            println!("SKIP device {} not allowed in config", idx);
            continue;
        }

        let c: &Settings  = cfg.get(&idx).unwrap_or(cfg.get(&-1).unwrap());
        if DEBUG {
            println!("device {} {:?} {:?}", idx, p.path(), c);
        }
        let pr = Propeller::new_with_settings(&p.path(), &c);
        if !pr.valid {
            continue;
        }
        karlsons.push(Karlson::new(pr, &c));
        println!("ADD device {}", idx);
    }


    loop {
        for k in &mut karlsons {
            k.spin();
        }

        if karlsons.is_empty() {
            println!("(X_X) No devices was added to service. Just do nothing and sleep!");
            if !idxx.is_empty() {
                println!("Allowed devices ids {:?}", idxx);
            }
            std::thread::sleep(std::time::Duration::from_secs(10));
        }
        
        for _ in 0..20 {
            std::thread::sleep(std::time::Duration::from_millis(500));
        }
    }
}


fn print_devices() {
    let list = Propeller::list();
    for p in list {
        println!("{} \"{}\" valid:{}", p.path, p.name, p.valid)
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
    
    let toml_path = daemon.unwrap_or(String::from(""));
    println!("DEBUG {}", toml_path);

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
