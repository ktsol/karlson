extern crate std;
use std::collections::VecDeque;
use std::fs::File;
use std::io::Error;
use std::io::Read;
use std::io::Write;
use std::path::Path;
use std::path::PathBuf;

extern crate toml;
use toml::Value;
use toml::value::Table;


pub static DIR_DEVICES: &'static str = "/sys/class/hwmon";
static TEMP_SCALE: usize = 1000;
static PWM_STEP: isize = 5;


pub fn read_file(p: &PathBuf) -> Result<String, Error> {
    match File::open(p) {
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


fn read_file_val<N>(p: &PathBuf, def:N) -> N
where N: std::str::FromStr {
    match read_file(p) {
        Ok(v) => match v.trim().parse::<N>() {
            Ok(v) => v,
            Err(_) => def
        },
        Err(_) => def
    }
}


#[derive(Debug, Clone)]
pub struct Settings {
    pwm_ok: usize,
    pwm_max: usize,
    pwm_min: usize,
    pwm_file: PathBuf,
    temp_ok: usize,
    temp_hot: usize,
    temp_crit: usize,
    temp_files: Vec<PathBuf>,
    queue_size: usize,
    //temp_global: bool,
}


#[derive(Debug, Clone)]
pub struct Propeller {
    pwm_file: PathBuf,
    pwm_min: usize,
    pwm_max: usize,
}


#[derive(Debug, Clone)]
pub struct Device {
    pub name: String,
    pub dir_name: String,
    dir_path: PathBuf,
    temp_files: Vec<PathBuf>,
    queue_size: usize,
    pub propeller: Option<Propeller>,
}


#[derive(Debug, Clone)]
struct Jam {
    pwm_ok: usize,
    temp_ok: usize,
    temp_hot: usize,
    temp_crit: usize,
}


#[derive(Debug, Clone)]
pub struct Karlson {
    pub device: Device,
    jam: Jam,
    pwm_speed: usize,
    tlog: VecDeque<usize>,
    tlog_size: usize,
}


impl Settings {

    pub fn new(cfg: &Value) -> Settings {
        Settings::new_with(cfg, &Settings {
            pwm_ok: 100,
            pwm_max: 255,
            pwm_min: 0,
            pwm_file: PathBuf::from("pwm1"),
            temp_ok: 65,
            temp_hot: 75,
            temp_crit: 80,
            temp_files: vec!(PathBuf::from("temp1_input")),
            queue_size: 15
        })
    }

    pub fn new_with(cfg: &Value, s: &Settings) -> Settings {
        let vt = "a='b'".parse::<Value>().unwrap();

        let t:&Table = cfg.as_table().unwrap_or(vt.as_table().unwrap());

        let tfiles:Vec<PathBuf> = if t.contains_key("temp_files") {
            t["temp_files"].as_array().unwrap_or(&Vec::<Value>::new()).iter()
                .filter_map(|v| v.as_str())
                .map(|v| PathBuf::from(v))
                .collect::<Vec<PathBuf>>()
        } else {
            s.temp_files.clone()
        };
        
        Settings {
            pwm_ok: t.get("pwm_ok").unwrap_or(&Value::from(s.pwm_ok as i64)).as_integer().unwrap() as usize,
            pwm_max: t.get("pwm_max").unwrap_or(&Value::from(s.pwm_max as i64)).as_integer().unwrap() as usize,
            pwm_min: t.get("pwm_min").unwrap_or(&Value::from(s.pwm_min as i64)).as_integer().unwrap() as usize,
            temp_ok: t.get("temp_ok").unwrap_or(&Value::from(s.temp_ok as i64)).as_integer().unwrap() as usize,
            temp_hot: t.get("temp_hot").unwrap_or(&Value::from(s.temp_hot as i64)).as_integer().unwrap() as usize,
            temp_crit: t.get("temp_crit").unwrap_or(&Value::from(s.temp_crit as i64)).as_integer().unwrap() as usize,
            queue_size: t.get("queue_size").unwrap_or(&Value::from(s.queue_size as i64)).as_integer().unwrap() as usize,
            temp_files: tfiles,
            pwm_file: if t.contains_key("pwm_file") && t["pwm_file"].is_str() {
                PathBuf::from(t["pwm_file"].as_str().unwrap())
            } else {s.pwm_file.clone()},
        }
    }
}

/*
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
*/



impl Propeller {

    fn new(dir_path: &PathBuf, s: &Settings) -> Option<Propeller> {
        let mut path_pwm = dir_path.clone();
        path_pwm.push(s.pwm_file.clone());

        if !path_pwm.is_file() {
            println!("ERROR PWM file not available {:?}", path_pwm);
            return None;
        }

        let dname = path_pwm.file_name().unwrap().to_string_lossy();
        let mut path_pmin = dir_path.clone();
        path_pmin.push(format!("{}_min", dname));
        
        let mut path_pmax = dir_path.clone();
        path_pmax.push(format!("{}_max", dname));

        let mut pmin = if path_pmin.is_file() { read_file_val(&path_pmin, s.pwm_min) } else { s.pwm_min };
        let mut pmax = if path_pmax.is_file() { read_file_val(&path_pmax, s.pwm_max) } else { s.pwm_max };

        if pmin < 0 {
            pmin = s.pwm_min;
        }
        
        if pmax < pmin {
            pmax = s.pwm_max;
        }
        
        Some(Propeller {
            pwm_file: path_pwm.clone(),
            pwm_min: pmin,
            pwm_max: pmax,
        })
    }


    /*
    fn new_std(dir_path: &PathBuf) -> Option<Propeller> {
        Propeller::new(dir_path, &Settings::new(&Value::from(false)))
    }
    */
    

    pub fn pwm(&self) -> usize {
        read_file_val(&self.pwm_file, 0)
    }


    fn pwm_set(&self, val: usize) -> Result<usize, String> {
        let mut nval = val;
        if val > self.pwm_max {
            nval = self.pwm_max;
        }
        if val < self.pwm_min {
            nval = self.pwm_min;
        }


        let mut fopts = std::fs::OpenOptions::new();
        fopts.write(true);
        
        match fopts.open(self.pwm_file.clone()) {
            Ok(mut f) => {
                match write!(f, "{}", nval) {
                    Ok(_) => Ok(nval),
                    Err(e) => Err(format!("Can not write {} to {} {}", nval, self.pwm_file.to_str().unwrap_or_default(), e))
                }
            },
            Err(e) => Err(format!("Can not write {} to {} {}", nval, self.pwm_file.to_str().unwrap_or_default(), e))
        }
    }
}


impl Device {

    pub fn new_std(dir: &PathBuf) -> Device {
        Device::new(dir, &Settings::new(&Value::from(false)))
    }

    pub fn new(dir: &PathBuf, s: &Settings) -> Device {
        let mut npath = dir.clone();
        npath.push("name");


        let tpath = dir.clone();

        let tmps = s.temp_files.iter()
            .map(|p| tpath.clone().join(p).canonicalize())
            .filter(|p| p.is_ok())
            .map(|p| p.unwrap())
            .collect::<Vec<PathBuf>>();
        
        if tmps.is_empty() && cfg!(debug_assertions) {
            println!("ERROR TEMP files not available temp_files {:?}", s.temp_files);
        }

        let fval = read_file(&npath).unwrap_or(String::from("N/A"));
        
        Device {
            name: String::from(fval.trim()),
            dir_name: String::from(dir.file_name().and_then(|v| v.to_str()).unwrap_or("N/A")),
            dir_path: dir.clone(),
            temp_files: tmps,
            queue_size: s.queue_size,
            propeller: Propeller::new(dir, s),
        }
    }

    pub fn list() -> Vec<Device> {
        let base = Path::new(DIR_DEVICES);
        if !base.exists() || !base.is_dir() {
            println!("ERROR: Can not read directory {}", DIR_DEVICES);
            return Vec::new();
        }

        if cfg!(debug_assertions) {
            println!("READING {:?}", base);
        }
        
        let mut paths:Vec<Device> = std::fs::read_dir(base)
            .unwrap()
            .map(|r| r.unwrap())
            .map(|it| Device::new_std(&it.path()))
            .collect();
        
        paths.sort_by_key(|p| p.dir_path.clone());

        return paths;
    }


    /// Return temps readings from all inputs
    pub fn temps(&self) -> Vec<usize> {
        if self.temp_files.is_empty() {
            return Vec::new();
        }

        if cfg!(debug_assertions) {
            let temps:Vec<String> = self.temp_files
                .iter()
                .map(|ref p| read_file_val(p, 0) / TEMP_SCALE)
                .map(|it| format!("{}C", it)
                ).collect();
            
            println!("DEBUG temps {} for {:?}", temps.join(", "), self.dir_path);
        }

        self.temp_files.iter()
            .map(|ref p| read_file_val(p, 0) / TEMP_SCALE)
            .collect()
    }
}


impl Karlson {

    pub fn new(d: Device, s:&Settings) -> Karlson {
        let tfiles = if d.temp_files.len() > 0 {
            d.temp_files.len()
        } else {
            1
        };
        
        Karlson {
            tlog: VecDeque::new(),
            tlog_size: d.queue_size * tfiles,
            pwm_speed: d.propeller.clone().and_then(|v| Some(v.pwm())).unwrap_or(0),
            device: d,
            jam: Jam {
                pwm_ok: s.pwm_ok,
                temp_ok: s.temp_ok,
                temp_hot: s.temp_hot,
                temp_crit: s.temp_crit,
            }
        }
    }


    fn load_temp(&mut self) -> (usize, usize) {
        let temps = self.device.temps();

        let mut tmax = 0;

        for t in temps {
            if tmax < t {
                tmax = t
            }
            self.tlog.push_front(t);
        }

        for _ in self.tlog_size..self.tlog.len() {
            self.tlog.pop_back();
        }

        if self.tlog.is_empty() {
            return (0, 0);
        }

        let sum:usize = self.tlog.iter().sum();
        return (tmax, ( sum as f64 / self.tlog.len() as f64).round() as usize);
    }

    
    fn adjust_pwm(&mut self, tmax: usize, tavg: usize) {
        if tmax <= 0 {
            println!("ERROR temparature is {}C for device at {}",
                     tmax,
                     self.device.dir_path.to_string_lossy().as_ref()
            );
            return
        }

        let pwm_now = self.pwm_speed as isize;

        if tmax <= self.jam.temp_ok {
            // Not hot at all. Only decrease temp here
            if tmax < self.jam.temp_ok {
                if self.jam.temp_ok as isize - tavg as isize > 1 {
                    self.pwm_update(pwm_now - PWM_STEP, tmax);
                } else if self.pwm_speed > self.jam.pwm_ok {
                    self.pwm_update(pwm_now - PWM_STEP, tmax);
                }
            }
        } else if tmax > self.jam.temp_ok && tmax < self.jam.temp_hot {
            // In this interval JUST normalize pwm up to OK level
            if self.pwm_speed < self.jam.pwm_ok {
                self.pwm_update(pwm_now + PWM_STEP, tmax);
            }
            if self.pwm_speed > self.jam.pwm_ok && self.jam.temp_hot as isize - tavg as isize > 1 {
                self.pwm_update(pwm_now - PWM_STEP, tmax);
            }
        } else {
            // Hot temp increase pwm only
            if self.pwm_speed < self.jam.pwm_ok {
                // Just in case
                let pwm_ok = self.jam.pwm_ok;
                self.pwm_update(pwm_ok as isize + PWM_STEP * 10, tmax);
            } else {
                self.pwm_update(pwm_now + PWM_STEP * 2, tmax);
            }
        }

        if tmax > self.jam.temp_crit {
            // If super hot, just set PWM at max
            let m = self.device.clone().propeller.unwrap().pwm_max;
            self.pwm_update(m.clone() as isize, tmax);
        }
    }


    fn pwm_update(&mut self, pwm:isize, temp:usize) {
        if self.device.propeller.is_none() {
            println!("ERROR can not file propeller for device at {}",
                     self.device.dir_path.to_string_lossy().as_ref());
            return
        }
        
        if pwm == self.pwm_speed as isize || pwm < 0 {
            return
        }

        let pwm_set = if pwm > 0 {
            pwm as usize
        } else {
            0
        };
        
        match self.device.propeller.clone().unwrap().pwm_set(pwm_set) {
            Ok(p) => {
                let updated = if p != self.pwm_speed { true } else { false };
                let ud = if self.pwm_speed > p { "DOWN" } else { "UP" };
                self.pwm_speed = p;
                if updated {
                    println!("{} PWM {} to {} temp {}C {}",
                             self.device.dir_name,
                             ud,
                             p,
                             temp,
                             self.device.propeller.clone().unwrap().pwm_file.to_str().as_ref().unwrap()
                    );
                }
            },
            Err(e) => println!("ERROR {}", e)
        }
    }
    

    /// Do some stuff to adjust Propeller speed
    pub fn spin(&mut self) {
        if self.device.propeller.is_none() {
            println!("ERROR can not file propeller for device at {:?}", self.device.dir_path);
            return
        }

        let (tmax, tavg) = self.load_temp();

        //#[cfg(debug_assertions)]
        if cfg!(debug_assertions) {
            println!("{}({}) TEMP:{}C ok:{}C hot:{}C PWM:{} ok:{}",
                     self.device.dir_name, self.device.name,
                     tmax, self.jam.temp_ok, self.jam.temp_hot,
                     self.pwm_speed, self.jam.pwm_ok);
        }

        self.adjust_pwm(tmax, tavg);
    }
}
