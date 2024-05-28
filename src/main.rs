mod config_file;
mod mqtt_connection;
mod consts;
mod mqtt_poll;
mod ipc;
mod errors;

#[macro_use]
extern crate tracing;

use crate::ipc::MqttMessage;
use std::collections::HashMap;
use tracing_subscriber::filter::{EnvFilter, LevelFilter};
use lazy_static::lazy_static;
use tokio::sync::{mpsc, Mutex, OnceCell, RwLock};
use tokio::sync::mpsc::error::TryRecvError;
use tokio::task::{JoinError, JoinSet};
use crate::config_file::AppConfig;
use crate::consts::MPSC_BUFFER_SIZE;
use crate::errors::MQTTError;
use crate::ipc::IPCMessage;
use crate::mqtt_connection::{MqttConnection, ThreadCommunication};
use crate::mqtt_poll::mqtt_poll_loop;

lazy_static! {
    static ref SHUTDOWN: OnceCell<bool> = OnceCell::new();
    static ref SETTINGS: OnceCell<AppConfig> = OnceCell::new();
    static ref COMMS: Mutex<HashMap<String,ThreadCommunication>> = Mutex::new(HashMap::new());
}

#[tokio::main]
async fn main() {

    {
        let cfg_file = std::env::var("CONFIG_FILE_PATH").unwrap_or_else(|_e| { "./config.yaml".to_string() });
        let config = AppConfig::from_file(cfg_file);
        SETTINGS.set(config).expect("Couldn't force config into oncecell");
    }

    let cfg = SETTINGS.get().unwrap().clone();

    let mut filter = EnvFilter::from_default_env();
    if cfg.log_level.is_some() {
        filter = filter.add_directive(cfg.log_level.unwrap().parse().expect("invalid value for log_level"));
    }
    tracing_subscriber::fmt()
        .with_env_filter(filter)
        .with_thread_names(true)
        .init();
    info!("starting up");

    let mut tasks: JoinSet<Result<(), MQTTError>> = JoinSet::new();
    for server in cfg.servers.iter().cloned() {
        let mut cn = match MqttConnection::create(server.clone()).await {
            Ok(cn) => cn,
            Err(e) => {
                error!("Can't connect to mqtt for {}: {e}", server.name);
                break;
            }
        };
        let (from_leaf_tx, from_leaf_rx) = mpsc::channel::<IPCMessage>(MPSC_BUFFER_SIZE);
        let (to_leaf_tx, to_leaf_rx) =mpsc::channel::<IPCMessage>(MPSC_BUFFER_SIZE);

        let _mpsc = ThreadCommunication {
            from_leaf_rx,
            from_leaf_tx,
            to_leaf_rx,
            to_leaf_tx
        };
        {
            let mut map = COMMS.lock().await;
            map.insert(server.name.clone(), _mpsc);
        }



        tasks.build_task()
            .name(&server.name.clone())
            .spawn(async move {
            mqtt_poll_loop(server.clone(), cn).await
        });

    }
    while !SHUTDOWN.initialized() {

        let mut mqtt_send_queue : Vec<MqttMessage> = vec![];
        let mut task_comms = COMMS.lock().await;
        for (server, mut mpsc) in task_comms.iter_mut() {
            match mpsc.from_leaf_rx.try_recv() {
                Ok(r) => {
                    match r {
                        IPCMessage::Outbound(o) => {
                            mqtt_send_queue.push(o.clone());
                        }
                        _ => {}
                    }
                }
                Err(e) => {
                    match e {
                        TryRecvError::Empty => {}
                        TryRecvError::Disconnected => {
                            error!("Can't read the leaf receiver for thread {}", server);
                            let _ = SHUTDOWN.set(true);
                        }
                    }

                }
            }
        }
        for msg in mqtt_send_queue.iter() {
            for (server, mut mpsc) in task_comms.iter_mut() {
                if let Err(e) = mpsc.to_leaf_tx.send(IPCMessage::Outbound(msg.clone())).await {
                    error!("Couldn't proxy message into server {}", server);
                    let _ = SHUTDOWN.set(true);
                }
            }
        }



        match tasks.try_join_next() {
            None => {}
            Some(s) => {
                match s {
                    Ok(rs) => {
                        match rs {
                            Ok(_) => {
                                error!("MQTT Thread exited normally, shutting down app");
                                let _ = SHUTDOWN.set(true);
                            }
                            Err(e) => {
                                error!("Error in MQTT Thread: {e}");
                                let _ = SHUTDOWN.set(true);
                            }
                        }
                    }
                    Err(e) => {
                        error!("Error in mqtt comms: {e}");
                        let _ = SHUTDOWN.set(true);
                    }
                }
            }
        }
    }

}
