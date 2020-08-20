extern crate ctrlc;

use std::convert::TryInto;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::{thread, time};

use nest::{Error, Store, Value};
use probes::network;
use serde_json::json;
use structopt::StructOpt;

#[derive(StructOpt, Debug)]
#[structopt(
    name = "peach-monitor",
    about = "Monitor data usage and set alert flags"
)]
struct Opt {
    /// Run daemon
    #[structopt(short, long)]
    daemon: bool,

    /// List usage totals, thresholds and alert flags
    #[structopt(short, long)]
    list: bool,

    /// Save latest usage totals to file
    #[structopt(short, long)]
    save: bool,
}

/// Received and transmitted network traffic (bytes)
#[derive(Debug)]
struct Traffic {
    rx: u64, // total bytes received
    tx: u64, // total bytes transmitted
}

impl Traffic {
    /// Retrieve latest statistics for received and transmitted traffic
    fn get(iface: &str) -> Option<Traffic> {
        let network = network::read().unwrap();
        for (interface, data) in network.interfaces {
            if interface == iface {
                let rx = data.received;
                let tx = data.transmitted;
                let t = Traffic { rx, tx };
                return Some(t);
            };
        }
        None
    }
}

/// Warning and critical network traffic thresholds (bytes)
struct Thresholds {
    rx_warn: u64, // received bytes warning threshold
    tx_warn: u64, // transmitted bytes warning threshold
    rx_crit: u64, // received bytes critical threshold
    tx_crit: u64, // transmitted bytes critical threshold
}

impl Thresholds {
    /// Retrieve latest alert thresholds from the data store
    fn get(store: &Store) -> Thresholds {
        let mut thresholds = Vec::new();

        let rx_warn_val = store
            .get(&["net", "thresholds", "rx_warn"])
            .unwrap_or(Value::Uint(0));
        if let Value::Uint(rx) = rx_warn_val {
            thresholds.push(rx);
        };

        let tx_warn_val = store
            .get(&["net", "thresholds", "tx_warn"])
            .unwrap_or(Value::Uint(0));
        if let Value::Uint(tx) = tx_warn_val {
            thresholds.push(tx);
        };

        let rx_crit_val = store
            .get(&["net", "thresholds", "rx_crit"])
            .unwrap_or(Value::Uint(0));
        if let Value::Uint(rx) = rx_crit_val {
            thresholds.push(rx);
        };

        let tx_crit_val = store
            .get(&["net", "thresholds", "tx_crit"])
            .unwrap_or(Value::Uint(0));
        if let Value::Uint(tx) = tx_crit_val {
            thresholds.push(tx);
        };

        Thresholds {
            rx_warn: thresholds[0],
            tx_warn: thresholds[1],
            rx_crit: thresholds[2],
            tx_crit: thresholds[3],
        }
    }
}

/// Evaluate traffic values against alert thresholds and set flags
fn set_alert_flags(store: &Store, thresholds: &Thresholds) {
    let rx_stored = store.get(&["net", "traffic", "rx"]).unwrap();
    if let Value::Uint(rx) = rx_stored {
        if rx > thresholds.rx_warn {
            store
                .set(&["net", "alerts", "rx_warn_alert"], &Value::Bool(true))
                .unwrap();
        } else {
            store
                .set(&["net", "alerts", "rx_warn_alert"], &Value::Bool(false))
                .unwrap();
        }
        if rx > thresholds.rx_crit {
            store
                .set(&["net", "alerts", "rx_crit_alert"], &Value::Bool(true))
                .unwrap();
        } else {
            store
                .set(&["net", "alerts", "rx_crit_alert"], &Value::Bool(false))
                .unwrap();
        }
    }

    let tx_stored = store.get(&["net", "traffic", "tx"]).unwrap();
    if let Value::Uint(tx) = tx_stored {
        if tx > thresholds.tx_warn {
            store
                .set(&["net", "alerts", "tx_warn_alert"], &Value::Bool(true))
                .unwrap();
        } else {
            store
                .set(&["net", "alerts", "tx_warn_alert"], &Value::Bool(false))
                .unwrap();
        }
        if tx > thresholds.tx_crit {
            store
                .set(&["net", "alerts", "tx_crit_alert"], &Value::Bool(true))
                .unwrap();
        } else {
            store
                .set(&["net", "alerts", "tx_crit_alert"], &Value::Bool(false))
                .unwrap();
        }
    }
}

/// Calculate and store the latest network transmission totals
fn update_transmission_totals(store: &Store) -> Result<(), Error> {
    // retrieve previous network traffic statistics
    let rx_stored = match store.get(&["net", "traffic", "rx"]) {
        Ok(rx) => rx,
        // return 0 if no value exists
        Err(_) => Value::Uint(u64::MIN),
    };
    let tx_stored = match store.get(&["net", "traffic", "tx"]) {
        Ok(tx) => tx,
        // return 0 if no value exists
        Err(_) => Value::Uint(u64::MIN),
    };

    // retrieve latest network traffic statistics
    let traffic = Traffic::get("wlan0").unwrap();

    // store updated network traffic statistics (totals)
    if let Value::Uint(rx) = rx_stored {
        let rx_total = rx + traffic.rx;
        let rx_value = Value::Uint(rx_total);
        store.set(&["net", "traffic", "rx"], &rx_value)?;
    };
    if let Value::Uint(tx) = tx_stored {
        let tx_total = tx + traffic.tx;
        let tx_value = Value::Uint(tx_total);
        store.set(&["net", "traffic", "tx"], &tx_value)?;
    };

    Ok(())
}

fn main() -> Result<(), Error> {
    // parse cli arguments
    let opt = Opt::from_args();

    // define the path
    let path = xdg::BaseDirectories::new()
        .unwrap()
        .create_data_directory("peachcloud")
        .unwrap();

    // define the schema
    let schema = json!({
        "net": {
            "traffic": "json",
            "thresholds": "json",
            "alerts": "json"
        }
    })
    .try_into()?;

    // create the data store
    let store = Store::new(path, schema);

    // update network transmission totals
    if opt.save {
        update_transmission_totals(&store).unwrap();
    }

    // list stored totals, thresholds and alert flags
    if opt.list {
        println!("Pretty list of data");
    }

    if opt.daemon {
        let running = Arc::new(AtomicBool::new(true));
        let r = running.clone();
        ctrlc::set_handler(move || {
            r.store(false, Ordering::SeqCst);
        })
        .expect("Error setting Ctrl-C handler");

        let five_secs = time::Duration::from_millis(5000);

        // run loop until SIGINT or SIGTERM is received
        while running.load(Ordering::SeqCst) {
            // retrieve alert thresholds
            let thresholds = Thresholds::get(&store);

            // test transmission totals against alert thresholds and set flags
            set_alert_flags(&store, &thresholds);

            thread::sleep(five_secs);
        }

        println!("Terminating gracefully...");
    }

    Ok(())
}
