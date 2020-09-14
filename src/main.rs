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

    /// Define network interface
    #[structopt(short, long, default_value = "wlan0")]
    iface: String,

    /// Save latest usage totals to file
    #[structopt(short, long)]
    save: bool,

    /// Update alert flags
    #[structopt(short, long)]
    update: bool,
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
        let network = network::read().expect("IO error when executing network command");
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

/// Warning and cutoff network traffic threshold (bytes)
struct Threshold {
    rx_warn: u64, // received bytes warning threshold
    tx_warn: u64, // transmitted bytes warning threshold
    rx_cut: u64,  // received bytes cutoff threshold
    tx_cut: u64,  // transmitted bytes cutoff threshold
}

impl Threshold {
    /// Retrieve latest alert threshold from the data store
    fn get(store: &Store) -> Threshold {
        let mut threshold = Vec::new();

        let rx_warn_val = store
            .get(&["net", "notify", "rx_warn"])
            .unwrap_or(Value::Uint(0));
        if let Value::Uint(rx) = rx_warn_val {
            threshold.push(rx);
        };

        let tx_warn_val = store
            .get(&["net", "notify", "tx_warn"])
            .unwrap_or(Value::Uint(0));
        if let Value::Uint(tx) = tx_warn_val {
            threshold.push(tx);
        };

        let rx_cut_val = store
            .get(&["net", "notify", "rx_cut"])
            .unwrap_or(Value::Uint(0));
        if let Value::Uint(rx) = rx_cut_val {
            threshold.push(rx);
        };

        let tx_cut_val = store
            .get(&["net", "notify", "tx_cut"])
            .unwrap_or(Value::Uint(0));
        if let Value::Uint(tx) = tx_cut_val {
            threshold.push(tx);
        };

        Threshold {
            rx_warn: threshold[0],
            tx_warn: threshold[1],
            rx_cut: threshold[2],
            tx_cut: threshold[3],
        }
    }
}

/// Evaluate traffic values against alert thresholds and set flags
fn set_alert_flags(store: &Store, threshold: &Threshold) -> Result<(), Error> {
    let rx_stored = store.get(&["net", "traffic", "rx"])?;
    if let Value::Uint(rx) = rx_stored {
        if rx > threshold.rx_warn {
            store.set(&["net", "alert", "rx_warn_alert"], &Value::Bool(true))?;
        } else {
            store.set(&["net", "alert", "rx_warn_alert"], &Value::Bool(false))?;
        }
        if rx > threshold.rx_cut {
            store.set(&["net", "alert", "rx_cut_alert"], &Value::Bool(true))?;
        } else {
            store.set(&["net", "alert", "rx_cut_alert"], &Value::Bool(false))?;
        }
    }

    let tx_stored = store.get(&["net", "traffic", "tx"])?;
    if let Value::Uint(tx) = tx_stored {
        if tx > threshold.tx_warn {
            store.set(&["net", "alert", "tx_warn_alert"], &Value::Bool(true))?;
        } else {
            store.set(&["net", "alert", "tx_warn_alert"], &Value::Bool(false))?;
        }
        if tx > threshold.tx_cut {
            store.set(&["net", "alert", "tx_cut_alert"], &Value::Bool(true))?;
        } else {
            store.set(&["net", "alert", "tx_cut_alert"], &Value::Bool(false))?;
        }
    }

    Ok(())
}

/// Calculate and store the latest network transmission totals
fn update_transmission_totals(iface: &str, store: &Store) -> Result<(), Error> {
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
    let traffic = Traffic::get(iface).expect("Error while retrieving network traffic statistics");

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
            "notify": "json",
            "alert": "json"
        }
    })
    .try_into()?;

    // create the data store
    let store = Store::new(path, schema);

    // update network transmission totals
    if opt.save {
        update_transmission_totals(&opt.iface, &store).unwrap();
    }

    // update alert flags
    if opt.update {
        // retrieve alert thresholds
        let threshold = Threshold::get(&store);

        // test transmission totals against alert thresholds and set flags
        set_alert_flags(&store, &threshold)?;
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
            let threshold = Threshold::get(&store);

            // test transmission totals against alert threshold and set flags
            set_alert_flags(&store, &threshold)?;

            thread::sleep(five_secs);
        }

        println!("Terminating gracefully...");
    }

    Ok(())
}
