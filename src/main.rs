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

    /// Define time interval for updating alert flags (seconds)
    #[structopt(short = "t", long, default_value = "60")]
    interval: u64,

    /// Update alert flags
    #[structopt(short, long)]
    update: bool,
}

/// Network traffic total (bytes)
#[derive(Debug)]
struct Traffic {
    total: u64, // bytes
}

impl Traffic {
    /// Retrieve latest statistics for traffic
    fn get(iface: &str) -> Option<Traffic> {
        let network = network::read().expect("IO error when executing network command");
        for (interface, data) in network.interfaces {
            if interface == iface {
                let rx = data.received;
                let tx = data.transmitted;
                let total = rx + tx;
                let t = Traffic { total };
                return Some(t);
            };
        }
        None
    }
}

/// Warning and cutoff network traffic threshold (bytes)
struct Threshold {
    warn: u64, // warning threshold (bytes)
    cut: u64,  // cutoff threshold (bytes)
}

impl Threshold {
    /// Retrieve latest alert threshold from the data store
    fn get(store: &Store) -> Threshold {
        let mut threshold = Vec::new();

        let warn_val = store
            .get(&["net", "notify", "warn"])
            .unwrap_or(Value::Uint(0));
        if let Value::Uint(val) = warn_val {
            threshold.push(val);
        };

        let cut_val = store
            .get(&["net", "notify", "cut"])
            .unwrap_or(Value::Uint(0));
        if let Value::Uint(val) = cut_val {
            threshold.push(val);
        };

        Threshold {
            warn: threshold[0],
            cut: threshold[1],
        }
    }
}

/// Convert a megabyte value to bytes
fn to_bytes(val: u64) -> u64 {
    (val * 1024) * 1024
}

/// Evaluate traffic values against alert thresholds and set flags
fn set_alert_flags(store: &Store, threshold: &Threshold) -> Result<(), Error> {
    let stored_total = store.get(&["net", "traffic", "total"])?;
    if let Value::Uint(total) = stored_total {
        // total is in bytes while warn is in megabytes
        if total > to_bytes(threshold.warn) {
            store.set(&["net", "alert", "warn_alert"], &Value::Bool(true))?;
        } else {
            store.set(&["net", "alert", "warn_alert"], &Value::Bool(false))?;
        }
        if total > to_bytes(threshold.cut) {
            store.set(&["net", "alert", "cut_alert"], &Value::Bool(true))?;
        } else {
            store.set(&["net", "alert", "cut_alert"], &Value::Bool(false))?;
        }
    }

    Ok(())
}

/// Calculate and store the latest network transmission totals
fn update_transmission_totals(iface: &str, store: &Store) -> Result<(), Error> {
    // retrieve previous network traffic statistics
    let stored_total = match store.get(&["net", "traffic", "total"]) {
        Ok(total) => total,
        // return 0 if no value exists
        Err(_) => Value::Uint(u64::MIN),
    };

    // retrieve latest network traffic statistics
    let traffic = Traffic::get(iface).expect("Error while retrieving network traffic statistics");

    // store updated network traffic statistics (totals)
    if let Value::Uint(total) = stored_total {
        let updated_total = total + traffic.total;
        let total_value = Value::Uint(updated_total);
        store.set(&["net", "traffic", "total"], &total_value)?;
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

        let interval = time::Duration::from_secs(opt.interval);

        // run loop until SIGINT or SIGTERM is received
        while running.load(Ordering::SeqCst) {
            // retrieve alert thresholds
            let threshold = Threshold::get(&store);

            // test transmission totals against alert threshold and set flags
            set_alert_flags(&store, &threshold)?;

            thread::sleep(interval);
        }

        println!("Terminating gracefully...");
    }

    Ok(())
}
