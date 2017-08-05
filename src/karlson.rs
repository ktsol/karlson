extern crate std;
use std::collections::VecDeque;

use core::Settings;
use core::Thermometer;
pub use core::Device;

use dsys;
use dnv;


#[derive(Debug, Clone)]
struct Jam {
    pwm_ok: usize,
    temp_ok: usize,
    temp_hot: usize,
    temp_crit: usize,
}

#[derive(Debug, Clone)]
pub struct Karlson {
    // pub name: String,
    // p: Box<Propeller>,
    // ts: Vec<Box<Thermometer>>,
    pub dev: Device,
    jam: Jam,
    pub pwm_speed: usize,
    pwm_up: isize,
    pwm_down: isize,
    tlog: VecDeque<usize>,
    tlog_size: usize,
}

pub fn list_devices() -> Vec<Device> {
    let mut res: Vec<Device> = Vec::new();

    res.extend(dsys::sys_devices());
    res.extend(dnv::nv_devices());
    res
}

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

//         Some(Propeller {
//             pwm_file: path_pwm.clone(),
//             pwm_min: pmin,
//             pwm_max: pmax,
//         })
//     }
// }

impl Karlson {
    pub fn new(dev: &Device, s: &Settings) -> Karlson {
        let device = match dev.dev_type.as_ref() {
            "sys" => dsys::sys_device_update(dev, s),
            _ => dev.clone(),
        };

        let temps = if device.termometers.len() > 0 {
            device.termometers.len()
        } else {
            1
        };

        let dp = device.propeller.clone();
        let speed = dp.as_ref()
            .ok_or(String::from("Err"))
            .and_then(|p| p.pwm())
            .unwrap_or(0);

        Karlson {
            dev: device,
            pwm_speed: dp.as_ref().map_or(
                0,
                |p| p.pwm_set(s.pwm_ok).unwrap_or(speed),
            ),
            tlog: VecDeque::new(),
            tlog_size: s.queue_size * temps,
            pwm_up: s.pwm_step_up,
            pwm_down: s.pwm_step_down,
            jam: Jam {
                pwm_ok: s.pwm_ok,
                temp_ok: s.temp_ok,
                temp_hot: s.temp_hot,
                temp_crit: s.temp_crit,
            },
        }
    }

    /// Create hybrid device with many inputs and custom propeller
    pub fn new_device(id: i32, s: &Settings) -> Karlson {
        let mut terms: Vec<Box<Thermometer>> = Vec::new();

        terms.extend(
            s.sys_temp_files
                .clone()
                .into_iter()
                .filter_map(|p| dsys::sys_termometer_from(&p).ok())
                .collect::<Vec<Box<Thermometer>>>(),
        );

        terms.extend(
            s.nv_temp_ids
                .clone()
                .into_iter()
                .filter_map(|p| dnv::nv_termometer_from(p as i32).ok())
                .collect::<Vec<Box<Thermometer>>>(),
        );

        Karlson::new(
            &Device {
                id: id,
                dev_type: String::from("dev"),
                name: s.name.clone().unwrap_or_default(),
                termometers: terms,
                propeller: dsys::sys_propeller_from(&s.sys_pwm_file, s).ok(),
            },
            s,
        )
    }


    /// Return (max_temp, max_temp_from_log)
    fn load_temp(&mut self) -> (usize, usize) {
        let temps: Vec<usize> = self.dev.termometers.iter().map(|t| t.temp()).collect();

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

        let lmax: usize = self.tlog.iter().max().unwrap_or(&(0 as usize)).clone();
        return (tmax, lmax);
    }


    fn adjust_pwm(&mut self, tmax: usize, tlog_max: usize) {
        if tmax <= 0 {
            // println!(
            //     "ERROR temparature is {}C for device at {}",
            //     tmax,
            //     self.device.dir_path.to_string_lossy().as_ref()
            // );
            return;
        }

        let pwm_now = self.pwm_speed as isize;
        let pdown = self.pwm_down;
        let pup = self.pwm_up;

        if tmax <= self.jam.temp_ok {
            // Not hot at all. Only decrease temp here
            if tmax <= self.jam.temp_ok {
                if self.jam.temp_ok as isize - tlog_max as isize > 1 {
                    self.pwm_update(pwm_now - pdown, tmax);
                } else if self.pwm_near(self.jam.pwm_ok, self.pwm_up) > 0 {
                    self.pwm_update(pwm_now - pdown, tmax);
                }
            }
        } else if tmax > self.jam.temp_ok && tmax < self.jam.temp_hot {
            // In this interval JUST normalize pwm up to OK level
            if self.pwm_near(self.jam.pwm_ok, self.pwm_up) < 0 {
                self.pwm_update(pwm_now + pup, tmax);
            }
            if self.pwm_near(self.jam.pwm_ok, self.pwm_up) > 0 &&
                self.jam.temp_hot as isize - tlog_max as isize > 1
            {
                self.pwm_update(pwm_now - pdown, tmax);
            }
        } else {
            // Hot temp increase pwm only
            if self.pwm_near(self.jam.pwm_ok, self.pwm_up) < 0 {
                // Just in case
                let pwm_ok = self.jam.pwm_ok;
                self.pwm_update(pwm_ok as isize + pup * 4, tmax);
            } else {
                self.pwm_update(pwm_now + pup * 2, tmax);
            }
        }

        if tmax > self.jam.temp_crit {
            // If super hot, just set PWM at max
            self.pwm_update(100, tmax);
        }
    }


    fn pwm_near(&self, val: usize, delta_up: isize) -> isize {
        if self.pwm_speed < val {
            return -1;
        } else if self.pwm_speed >= val && (self.pwm_speed as isize) < ((val as isize) + delta_up) {
            return 0;
        } else {
            return 1;
        }
    }


    fn pwm_update(&mut self, pwm: isize, temp: usize) {
        let pwm_val = if pwm > 0 {
            if pwm > 100 { 100 } else { pwm as usize }
        } else {
            0
        };
        let ref prop = &self.dev.propeller;
        if prop.is_none() {
            println!(
                "ERROR can not file propeller for device #{} {}",
                self.dev.id,
                self.dev.name
            );
            return;
        }
        //  || pwm_val < 0
        if pwm_val == self.pwm_speed as usize {
            return;
        }


        match prop.as_ref().unwrap().pwm_set(pwm_val) {
            Ok(p) => {
                // let updated = if p != self.pwm_speed { true } else { false };
                let ud = if self.pwm_speed > pwm_val { "DOWN" } else { "UP" };
                self.pwm_speed = p;
                // if updated {
                println!(
                    "{}#{} PWM {} to {}% temp {}C -> {}",
                    self.dev.dev_type,
                    self.dev.id,
                    ud,
                    p,
                    temp,
                    self.dev.name
                );
                // }
            }
            Err(e) => println!("ERROR {}", e),
        }
    }


    /// Do some stuff to adjust Propeller speed
    /// This is only place where PWM speed updated before all logick run
    pub fn spin(&mut self) {
        if self.dev.propeller.is_none() {
            println!(
                "ERROR! Can not find propeller for device {}#{} {}",
                self.dev.dev_type,
                self.dev.id,
                self.dev.name
            );
            return;
        }

        match self.dev.propeller.as_ref().unwrap().pwm() {
            Ok(s) => self.pwm_speed = s,
            Err(e) => {
                println!(
                    "ERROR! Can not read PWM speed for device {}#{} {} -> {}",
                    self.dev.dev_type,
                    self.dev.id,
                    self.dev.name,
                    e
                );
                return;
            }
        };

        let (tmax, tlog_max) = self.load_temp();

        //if cfg!(debug_assertions) {
        #[cfg(debug_assertions)]
        {
            println!(
                "{}#{} TEMP:{}C ({}..{}) PWM:{}% ({}%) :: {}",
                self.dev.dev_type,
                self.dev.id,
                tmax,
                self.jam.temp_ok,
                self.jam.temp_hot,
                self.pwm_speed,
                self.jam.pwm_ok,
                self.dev.name
            );
        }

        self.adjust_pwm(tmax, tlog_max);
    }
}
