use std::error::Error as eError;
use std::fmt::Debug;
use std::fs::File;
use std::io::Error;
use std::io::Read;
use std::path::PathBuf;
use std::str::FromStr;

extern crate toml;
use toml::Value;
use toml::value::Table;

pub fn read_file(p: &PathBuf) -> Result<String, Error> {
    match File::open(p) {
        Ok(mut f) => {
            let mut s = String::new();
            match f.read_to_string(&mut s) {
                Ok(_) => Ok(s),
                Err(e) => Err(e),
            }
        }
        Err(err) => Err(err),
    }
}

pub fn read_file_val<N>(p: &PathBuf) -> Result<N, String>
where
    N: FromStr,
{
    match read_file(p) {
        Ok(v) => {
            match v.trim().parse::<N>() {
                Ok(v) => Ok(v),
                Err(_) => Err(format!("Can not parse value {:?}", v)),
            }
        }
        Err(e) => Err(String::from(e.description())),
    }
}

pub fn read_file_val_or<N>(p: &PathBuf, def: N) -> N
where
    N: FromStr,
{
    match read_file(p) {
        Ok(v) => {
            match v.trim().parse::<N>() {
                Ok(v) => v,
                Err(_) => def,
            }
        }
        Err(_) => def,
    }
}

#[derive(Debug, Clone)]
pub struct Device {
    pub id: i32,
    pub dev_type: String,
    pub name: String,
    pub propeller: Option<Box<Propeller>>,
    pub termometers: Vec<Box<Thermometer>>,
}

#[derive(Debug, Clone)]
pub struct Settings {
    pub name: Option<String>,
    /// Device type
    ///  - sys
    ///  - nv
    pub dev_type: String,
    /// Minimum limit for FAN in percents
    /// Consider that absolute minimum fan value is 0
    pub pwm_min: usize,
    /// Normal fan value in percents
    pub pwm_ok: usize,
    // Absolute value for MAX fan
    //pwm_max_abs: usize,
    /// Fan step in %
    pub pwm_step_up: isize,
    /// Fan step in %
    pub pwm_step_down: isize,
    pub temp_ok: usize,
    pub temp_hot: usize,
    pub temp_crit: usize,
    pub queue_size: usize,

    // Nvidia settings //
    pub nv_temp_ids: Vec<usize>,

    // Sys devices settings //
    pub sys_temp_files: Vec<PathBuf>,
    // Sys PWM file name in folder
    pub sys_pwm_file: PathBuf,
}

impl Settings {
    pub fn from(cfg: &Value) -> Settings {
        Settings::from_with(
            cfg,
            &Settings {
                name: None,
                //pwm_max_abs: 255,
                dev_type: String::from("sys"),
                pwm_ok: 60,
                pwm_min: 0,
                pwm_step_up: 2,
                pwm_step_down: 1,
                temp_ok: 65,
                temp_hot: 75,
                temp_crit: 80,
                queue_size: 15,
                sys_pwm_file: PathBuf::from("pwm1"),
                sys_temp_files: vec![PathBuf::from("temp1_input")],
                nv_temp_ids: Vec::new(),
            },
        )
    }

    pub fn from_with(cfg: &Value, s: &Settings) -> Settings {
        let vt = "stub_key='stub_val'".parse::<Value>().unwrap();
        let t: &Table = cfg.as_table().unwrap_or(vt.as_table().unwrap());

        let sstr = String::from("sys");

        Settings {
            name: t.get("name")
                .map(|v| v.as_str().map(|s| String::from(s)))
                .or(Some(s.name.clone()))
                .unwrap_or_default(),
            dev_type: t.get("type")
                .map(|v| v.as_str().map(|s| String::from(s)))
                .and_then(|v| v)
                .unwrap_or(sstr),
            //pwm_max: t.get("pwm_max").unwrap_or(&Value::from(s.pwm_max as i64)).as_integer().unwrap() as usize,
            pwm_ok: t.get("pwm_ok")
                .unwrap_or(&Value::from(s.pwm_ok as i64))
                .as_integer()
                .unwrap() as usize,
            pwm_min: t.get("pwm_min")
                .unwrap_or(&Value::from(s.pwm_min as i64))
                .as_integer()
                .unwrap() as usize,
            pwm_step_up: 5,
            pwm_step_down: 2,
            temp_ok: t.get("temp_ok")
                .unwrap_or(&Value::from(s.temp_ok as i64))
                .as_integer()
                .unwrap() as usize,
            temp_hot: t.get("temp_hot")
                .unwrap_or(&Value::from(s.temp_hot as i64))
                .as_integer()
                .unwrap() as usize,
            temp_crit: t.get("temp_crit")
                .unwrap_or(&Value::from(s.temp_crit as i64))
                .as_integer()
                .unwrap() as usize,
            queue_size: t.get("queue_size")
                .unwrap_or(&Value::from(s.queue_size as i64))
                .as_integer()
                .unwrap() as usize,
            sys_pwm_file: if t.contains_key("pwm_file") && t["pwm_file"].is_str() {
                PathBuf::from(t["pwm_file"].as_str().unwrap())
            } else {
                s.sys_pwm_file.clone()
            },
            sys_temp_files: if t.contains_key("sys_temp_input") {
                t["sys_temp_input"]
                    .as_array()
                    .unwrap_or(&Vec::<Value>::new())
                    .iter()
                    .filter_map(|v| v.as_str())
                    .map(|v| PathBuf::from(v))
                    .collect::<Vec<PathBuf>>()
            } else {
                s.sys_temp_files.clone()
            },
            nv_temp_ids: if t.contains_key("nv_temp_input") {
                t["nv_temp_input"]
                    .as_array()
                    .unwrap_or(&Vec::<Value>::new())
                    .iter()
                    .filter_map(|v| v.as_integer())
                    .map(|i| i as usize)
                    .collect::<Vec<usize>>()
            } else {
                s.nv_temp_ids.clone()
            },
        }
    }
}


pub trait Propeller: Debug {
    fn pwm(&self) -> Result<usize, String>;

    fn pwm_set(&self, val: usize) -> Result<usize, String>;

    fn box_clone(&self) -> Box<Propeller>;

    fn configure(&mut self, set: &Settings);
}

impl Clone for Box<Propeller> {
    fn clone(&self) -> Box<Propeller> {
        self.box_clone()
    }
}

pub trait Thermometer: Debug {
    fn temp(&self) -> usize;

    fn box_clone(&self) -> Box<Thermometer>;
}

impl Clone for Box<Thermometer> {
    fn clone(&self) -> Box<Thermometer> {
        self.box_clone()
    }
}