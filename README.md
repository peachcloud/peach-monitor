# peach-monitor

Monitor network data usage and set alert flags based on user-defined thresholds.

`peach-monitor` is a CLI tool capable of running as a one-shot data store updater or as a daemon for continually updating data usage alert flags.

The utility is intended to be run with the `--save` flag prior to each system reboot or shutdown. This allows network transmission totals (upload and download) to be persisted to the filesystem in the form of a JSON data store.

When the `--update` flag is set, `peach-monitor` retrieves user-defined alert thresholds from the data store, calculates the latest data usage statistics and sets alert flags accordingly. These flag values can be accessed from other parts of the PeachCloud system to alert the user (for example, by `peach-web` for web application display).

The `--daemon` flag executes the `--update` functionality in a loop and is intended to be run as a background process for convenient alert flag updates. The optional `--interval` argument defines the frequency with which the alert flags are updated. The default update frequency is once every 60 seconds.

The `--iface` argument is used to define the network interface from which to retrieve network traffic data statistics. This defaults to `wlan0` if not defined.

### Usage

`peach-monitor [FLAGS] [OPTIONS]`

```
FLAGS:
    -d, --daemon     Run daemon
    -h, --help       Prints help information
    -s, --save       Save latest usage totals to file
    -u, --update     Update alert flags
    -V, --version    Prints version information

OPTIONS:
    -i, --iface <iface>    Define network interface [default: wlan0]
    -t, --interval <interval>    Define time interval for updating alert flags (seconds) [default: 60]
```

### Data Store

`~/.local/share/peachcloud`

```
.
└── net
    ├── alert.json          // programatically-defined alert flags
    ├── notify.json         // user-defined alert thresholds
    └── traffic.json        // network transmission totals
```

### Alert Types

`peach-monitor` defines warning and critical thresholds and corresponding alert flags for total network data traffic. The critical threshold may allow a disable-network feature in future implementations of `peach-monitor`.

### Roadmap

- Add Debian packaging  
- Add disk-usage tracking and alerts  

### Licensing

AGPL-3.0
