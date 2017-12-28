# Karlson is mining rig GPU temperature/fan controller

This is a piece of logic that can help you to sustain optimal balance between 
GPUs temperature and fan speed. This is linux only service, compatible with AMD and nVidia GPUs.

Karlson main goal is to operate at minimum fan speed to extend its lifetime.

Intended for use in data centers on servers without any GUI. 

## How it works

Basically service work based on next parameters:

* optimal temparature
* hot temperature
* critical temperature
* optimal fan speed

Work logic is next:

* when GPU temperature is between optimal and hot, fan speed will be at optimal level
* if temperature drops below optimal, then fan speed will be reduced to some equilibrium level, when GPU temperature is not rising up
* if temperature rise above hot, fan speed will increase to hold temperature at hot level
* temperatures above critical force fan to use 100% speed.

As you can see now, fan speed/temperature relation in karlson is more complicated than just simple linear dependency.

## Building from sources

It should be very easy for you:

1. Install Rust `curl https://sh.rustup.rs -sSf | sh`
2. Clone this repo to your local machine
3. Build debug binary `cargo build --release`
4. Build release `cargo build --release`

Now you will be able to find binaries at `target/debug/karlson` or `target/release/karlson`

You can also build and run service using `cargo run -- -h`

## Configuration

Karlson read settings from *.toml configuration file.
You must set valid path to *.toml config in CLI arguments.

For more information look into example configuration karlson.toml in this repo.

The main thing you should notice about configuration 
is that for AMD GPU it use device index as in /sys/class/hwmon directory (hwmon42 index is 42),
nVidia GPU indexes are the same as in output of `nvidia-smi`.

## Running as a service

Karlson can be simply configured as a systemd service.

### AMD only configuration

Just create file /etc/systemd/system/karlson.service
With next content
```
[Unit]
Description=Karlson

[Service]
Type=simple
ExecStart=/path/to/bin/karlson -d /path/to/your/configuration/karlson.toml
User=root
Group=root
Restart=always

[Install]
WantedBy=multi-user.target
```

Next you should enable this service like `systemctl enable karlson.service`

Now you can start/stop it via `systemctl`

### nVidia/AMD configuration

Support for nVidia is ugly because it requires to have X server and it also requires to run karlson service with valid X display. Also it have to work without any GUI.

To make it work you should install X server on your system.
Next you have to create separate (virtual) display for working with karlson.
Add service config /etc/systemd/system/X.service
```
Description=Virtual X Display

[Service]
Type=simple
ExecStart=/usr/bin/X :99
User=some_non_root_user
Group=some_non_root_users
Restart=always
RestartSec=5

[Install]
WantedBy=multi-user.target
```

This will create X display with id value :99

Karlson config /etc/systemd/system/karlson.service should now look like this
```
[Unit]
Description=Karlson
BindsTo=X.service

[Service]
Environment=DISPLAY=:99
Type=simple
ExecStart=/path/to/bin/karlson -d  /path/to/your/configuration/karlson.toml
User=root
Group=root
Restart=always
RestartSec=5

[Install]
WantedBy=multi-user.target
```

We just add dependency from X service and set up DISPLAY environment value.

Now you can enable this services and manage them via systemctl
```
systemctl enable X.service
systemctl enable karlson.service
```

### Read systemd logs

After you have configured service as described above,
you can read karlson logs using `journalctl -fu karlson`