// Implementation for core compatible devices
// that using syscalls and link to /sys/class/hwmon/

use core::Device;
use core::Propeller;
use core::Thermometer;
use core::Settings;
use core::read_file;
use core::read_file_val;
use core::read_file_val_or;

use std::fs;
use std::fs::OpenOptions;
use std::io::Write;
// use std::marker::Sized;
use std::path::Path;
use std::path::PathBuf;

use regex::Regex;

pub static DIR_DEVICES: &'static str = "/sys/class/hwmon";
static TEMP_SCALE: usize = 1000;
static FAN_SCALE: f64 = 2.55;


pub fn sys_devices() -> Vec<Device> {
    let base = Path::new(DIR_DEVICES);
    if !base.exists() || !base.is_dir() {
        println!("ERROR: Can not read directory {}", DIR_DEVICES);
        return Vec::new();
    }

    //if cfg!(debug_assertions) {
    #[cfg(debug_assertions)]
    {
        println!("READING {:?}", base);
    }

    let mut paths: Vec<Device> = fs::read_dir(base)
        .unwrap()
        .map(|r| r.unwrap())
        .map(|it| create_device(&it.path()))
        .collect();

    paths.sort_by_key(|p| p.id);

    return paths;
}

fn create_device(dir: &PathBuf) -> Device {
    let reavg = Regex::new(r"hwmon(\d+)[/\s]*$").unwrap();
    let co = reavg.captures(&dir.to_str().unwrap());

    let mut npath = dir.clone();
    npath.push("name");

    let fval = read_file(&npath).unwrap_or(String::from("?!?"));
    let dn = format!(
        "{}({})",
        dir.file_name().and_then(|v| v.to_str()).unwrap_or("ERR"),
        fval.trim()
    );

    Device {
        id: if co.is_some() {
            let ts = String::from(co.unwrap().get(1).unwrap().as_str());
            ts.parse::<i32>().unwrap_or(-1)
        } else {
            println!("ERROR can not find id in {}", dir.to_str().unwrap());
            -1
        },
        dev_type: String::from("sys"),
        name: String::from(dn),
        termometers: Vec::new(),
        propeller: None,
    }
}

/// Update device with provided settings
/// Add thermometers and propellers if any
pub fn sys_device_update(d: &Device, set: &Settings) -> Device {
    let mut dd = d.clone();

    let mut p = PathBuf::from(DIR_DEVICES);
    p.push(format!("hwmon{}", d.id));
    let pd = p.clone();
    p.push(set.sys_pwm_file.clone());


    match sys_propeller_from(&p, set) {
        Ok(p) => dd.propeller = Some(p),
        Err(e) => println!("ERROR! {}", e),
    }

    dd.termometers = set.sys_temp_files
        .clone()
        .into_iter()
        .filter_map(|pb| {
            let mut tp = pd.clone();
            tp.push(pb);
            sys_termometer_from(&tp).ok()
        })
        .collect();
    dd
}

pub fn sys_termometer_from(p: &PathBuf) -> Result<Box<Thermometer>, String> {
    if p.exists() {
        Ok(Box::new(ThermometerSys { temp_file: p.clone() }))
    } else {
        Err(format!(
            "ERROR! TEMP file does not exist {}",
            p.to_string_lossy()
        ))
    }
}

pub fn sys_propeller_from(p: &PathBuf, set: &Settings) -> Result<Box<Propeller>, String> {
    if !p.exists() {
        return Err(format!("PWM file does not exist {}", p.to_string_lossy()));
    }

    Ok(Box::new(PropellerSys {
        pfile: p.clone(),
        min: set.pwm_min,
        max: 100,
    }))
}

#[derive(Debug, Clone)]
pub struct PropellerSys {
    pfile: PathBuf,
    // speed: usize,
    min: usize,
    max: usize,
}

#[derive(Debug, Clone)]
pub struct ThermometerSys {
    temp_file: PathBuf,
}

fn scale_to_sys(from: usize) -> usize {
    ((from as f64) * FAN_SCALE).round() as usize
}

fn scale_from_sys(sys: usize) -> usize {
    ((sys as f64) / FAN_SCALE).round() as usize
}

impl Propeller for PropellerSys {
    fn box_clone(&self) -> Box<Propeller> {
        Box::new((*self).clone())
    }

    fn pwm(&self) -> Result<usize, String> {
        read_file_val::<usize>(&self.pfile).map(|v| scale_from_sys(v))
    }

    fn pwm_set(&self, val: usize) -> Result<usize, String> {
        let mut nval = scale_to_sys(val);
        if val > 100 || val > self.max {
            nval = scale_to_sys(self.max);
        }
        /* val < 0 || */

        if val < self.min {
            nval = scale_to_sys(self.min);
        }

        let mut fopts = OpenOptions::new();
        fopts.write(true);

        match fopts.open(self.pfile.clone()) {
            Ok(mut f) => {
                match write!(f, "{}", nval) {
                    Ok(_) => Ok(scale_from_sys(nval)),
                    Err(e) => Err(format!(
                        "Can not write {} to {} {}",
                        nval,
                        self.pfile.to_str().unwrap_or_default(),
                        e
                    )),
                }
            }
            Err(e) => Err(format!(
                "Can not write {} to {} {}",
                nval,
                self.pfile.to_str().unwrap_or_default(),
                e
            )),
        }
    }

    fn configure(&mut self, set: &Settings) {
        self.min = set.pwm_min;
    }
}

impl Thermometer for ThermometerSys {
    fn box_clone(&self) -> Box<Thermometer> {
        Box::new((*self).clone())
    }

    fn temp(&self) -> usize {
        read_file_val_or(&self.temp_file, 0) / TEMP_SCALE
    }
}


// Automatically load max-min limit for PWM
// impl Propeller {
//     fn new(dir_path: &PathBuf, s: &Settings) -> Option<Propeller> {
//         let mut path_pwm = dir_path.clone();
//         path_pwm.push(s.pwm_file.clone());

//         if !path_pwm.is_file() {
//             println!("ERROR PWM file not available {:?}", path_pwm);
//             return None;
//         }

//         let dname = path_pwm.file_name().unwrap().to_string_lossy();
//         let mut path_pmin = dir_path.clone();
//         path_pmin.push(format!("{}_min", dname));

//         let mut path_pmax = dir_path.clone();
//         path_pmax.push(format!("{}_max", dname));

//         let mut pmin = if path_pmin.is_file() {
//             read_file_val(&path_pmin, s.pwm_min)
//         } else {
//             s.pwm_min
//         };
//         let mut pmax = if path_pmax.is_file() {
//             read_file_val(&path_pmax, s.pwm_max)
//         } else {
//             s.pwm_max
//         };

//         if pmin < 0 {
//             pmin = s.pwm_min;
//         }

//         if pmax < pmin {
//             pmax = s.pwm_max;
//         }
//     }
// }