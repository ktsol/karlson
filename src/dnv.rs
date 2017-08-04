// Nvidia only implementation

use core::Device;
use core::Propeller;
use core::Thermometer;
use core::Settings;

use std::process::Command;
use regex::Regex;

#[derive(Debug, Clone)]
pub struct PropellerNv {
    id: i32,
    // speed: usize,
    min: usize,
    max: usize,
}

#[derive(Debug, Clone)]
pub struct ThermometerNv {
    id: i32,
}

pub fn nv_termometer_from(id: i32) -> Result<Box<Thermometer>, String> {
    Ok(Box::new(ThermometerNv { id: id }))
}
/*
    let test = "GPU 0: GeForce GTX 1070 (UUID: GPU-09b509f0-961c-189e-bf2e-a1fd2d999b49)
GPU 1: GeForce GTX 1060 3GB (UUID: GPU-51e5ba3b-3f42-d70b-075b-06b36565f091)
GPU 2: GeForce GTX 1060 3GB (UUID: GPU-920ff0df-2fb4-b27d-76e1-08d7e9bb4f0c)
GPU 3: GeForce GTX 1070 (UUID: GPU-beabe8a7-de7e-f455-baac-eb473505320d)  
";
    return create_devices(&String::from(test));
*/

pub fn nv_devices() -> Vec<Device> {
    let rout = Command::new("nvidia-smi").arg("--list-gpus").output();

    if rout.is_ok() {
        let resout = rout.unwrap();
        let out = String::from(String::from_utf8_lossy(&resout.stdout));
        create_devices(&out)
    } else {
        println!("ERROR: Can not execute nvidia-smi {}", rout.err().unwrap());
        Vec::new()
    }
}

fn create_devices(nvout: &String) -> Vec<Device> {
    let re = Regex::new(
        r"(?m)^\s*GPU\s+(?P<id>\d+):\s+(GeForce\s+)*(?P<name>.+\S+)\s*\(UUID:.+$",
    ).unwrap();

    re.captures_iter(nvout)
        .map(|caps| {
            create_device(
                String::from(&caps["id"]).parse::<i32>().unwrap_or(-1),
                String::from(&caps["name"]),
            )
        })
        .collect()
}

fn create_device(nv_id: i32, name: String) -> Device {
    Device {
        dev_type: String::from("nv"),
        id: nv_id,
        name: name,
        termometers: vec![Box::new(ThermometerNv { id: nv_id })],
        propeller: Some(Box::new(PropellerNv {
            id: nv_id,
            min: 0,
            max: 100,
        })),
    }
}

impl Thermometer for ThermometerNv {
    fn box_clone(&self) -> Box<Thermometer> {
        Box::new((*self).clone())
    }

    fn temp(&self) -> usize {
        let rout = Command::new("nvidia-smi")
            .arg("--query-gpu=temperature.gpu")
            .arg("--format=csv,noheader")
            .arg("-i")
            .arg(format!("{}", self.id))
            .output();

        if rout.is_ok() {
            let resout = rout.unwrap();
            let out = String::from(String::from_utf8_lossy(&resout.stdout));
            out.trim().parse::<usize>().unwrap_or(0)
        } else {
            println!(
                "ERROR: NV#{} Can not read temperature nvidia-smi {}",
                self.id,
                rout.err().unwrap()
            );
            0
        }
    }
}


/*

  Attribute 'GPUFanControlState' (miner3:2[gpu:0]): 0.
    'GPUFanControlState' is a boolean attribute; valid values are: 1 (on/true) and 0 (off/false).
    'GPUFanControlState' can use the following target types: GPU.

*/

impl PropellerNv {
    /// Return true only if GPUFanControlState or return false in other cases.
    fn fan_state(&self) -> bool {
        let rout = Command::new("nvidia-settings")
            .arg("-q")
            .arg(format!("[gpu:{}]/GPUFanControlState", self.id))
            .output();

        let ss = format!("[gpu:{}]): 1", self.id);

        if rout.is_ok() {
            let resout = rout.unwrap();
            let out = String::from(String::from_utf8_lossy(&resout.stdout));
            if out.contains(ss.as_str()) {
                true
            } else {
                false
            }
        } else {
            println!("ERROR: NV#{} Can not read GPUFanControlState", self.id);
            false
        }
    }
}

impl Propeller for PropellerNv {
    fn box_clone(&self) -> Box<Propeller> {
        Box::new((*self).clone())
    }

    fn pwm(&self) -> Result<usize, String> {
        let rout = Command::new("nvidia-smi")
            .arg("--query-gpu=fan.speed")
            .arg("--format=csv,noheader")
            .arg("-i")
            .arg(format!("{}", self.id))
            .output();

        if rout.is_ok() {
            let resout = rout.unwrap();
            let out = String::from(String::from_utf8_lossy(&resout.stdout));
            // 95 %
            let po = out.replace("%", "").trim().parse::<usize>();
            if po.is_ok() {
                Ok(po.unwrap())
            } else {
                Err(format!("NV#{} Can not read  fan speed", self.id))
            }
        } else {
            Err(format!(
                "NV#{} Can not read pwm speed. {}",
                self.id,
                rout.err().unwrap()
            ))
        }
    }

    fn pwm_set(&self, val: usize) -> Result<usize, String> {
        let mut nval = val;
        if val > 100 || val > self.max {
            nval = self.max;
        }
        /* val < 0 || */
        if val < self.min {
            nval = self.min;
        }

        let mut cmd = Command::new("nvidia-settings");
        if !self.fan_state() {
            cmd.arg("-a").arg(format!(
                "[gpu:{}]/GPUFanControlState=1",
                self.id
            ));
            #[cfg(debug_assertions)]
            {
                println!("NV#{} update with GPUFanControlState=1", self.id);
            }
        }

        let out = cmd.arg("-a")
            .arg(format!("[fan:{}]/GPUTargetFanSpeed={}", self.id, nval))
            .output();

        match out {
            Ok(o) => {
                if o.status.success() {
                    #[cfg(debug_assertions)]
                    {
                        println!(
                            "NV#{} PWM updated to {} now value {}",
                            self.id,
                            nval,
                            self.pwm().unwrap_or(0)
                        );
                    }
                    self.pwm()
                } else {
                    Err(format!(
                        "NV#{} Change fan fail. {}",
                        self.id,
                        String::from_utf8_lossy(&o.stderr)
                    ))
                }
            }
            Err(e) => Err(format!(
                "NV#{} Can not chage fan to {} {}",
                self.id,
                nval,
                e
            )),
        }
    }

    fn configure(&mut self, set: &Settings) {}
}