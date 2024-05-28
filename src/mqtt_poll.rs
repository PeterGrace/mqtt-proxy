use std::cmp::PartialEq;
use crate::consts::MQTT_POLL_INTERVAL_MILLIS;
use crate::ipc::{IPCMessage, MqttMessage};
use crate::mqtt_connection::{MqttConnection, ThreadCommunication};
use crate::errors::MQTTError;
use crate::{COMMS, SHUTDOWN};
use rumqttc::{Event, Incoming, Outgoing, QoS, SubscribeReasonCode};
use std::str;
use std::time::Duration;
use tokio::task::JoinHandle;

use tokio::sync::mpsc;
use tokio::sync::mpsc::error::TryRecvError;
use tokio::time::{sleep, timeout};
use crate::config_file::{MqttServer, MqttType};


pub async fn mqtt_poll_loop(
    server: MqttServer,
    mqtt: MqttConnection,
) -> Result<(), MQTTError> {
    let _server = server.clone();
    let task: JoinHandle<Result<(), MQTTError>> = tokio::spawn(async move {
        let mut conn = mqtt.event_loop;
        let mut dlq: Vec<u16> = vec![];

        loop {
            if SHUTDOWN.initialized() {
                return Err(MQTTError::ExitingThread);
            }
            let notification = match conn.poll().await {
                Ok(event) => event,
                Err(e) => {
                    let msg = format!("{}: Unable to poll mqtt: {e}", _server.name);
                    return Err(MQTTError::Misc(msg));
                }
            };
            match notification {
                Event::Incoming(i) => {
                    match i {
                        Incoming::Disconnect => {
                            // we should do something here.
                            error!("mqtt disconnect packet received.");
                            return Err(MQTTError::ExitingThread);
                        }
                        Incoming::ConnAck(_ca) => {
                            info!("{}: MQTT connection established.", _server.name);
                        }
                        Incoming::PubAck(pa) => {
                            info!("{}: Incoming PubAck!", &_server.name);
                            dlq.retain(|x| *x != pa.pkid);
                        }
                        Incoming::PingResp => {
                            trace!("Recv MQTT PONG");
                        }
                        Incoming::SubAck(sa) => {
                            if sa.return_codes.len() > 0 {
                                for code in sa.return_codes.iter() {
                                    match code {
                                        SubscribeReasonCode::Success(_) => {}
                                        SubscribeReasonCode::Failure => {
                                            return Err(MQTTError::Misc(format!("{}: Couldn't subscribe", _server.name)));
                                        }
                                    }
                                }
                            }
                        }
                        Incoming::Publish(pr) => {
                            debug!("Incoming publish: {:#?}", pr);
                            if _server.mqtt_type == MqttType::Sink {
                                continue;
                            }

                            let outbound_topic: String = match server.suffix_name {
                                None => format!("{}", pr.topic),
                                Some(s) => match s {
                                    true => format!("{}/{}",_server.name, pr.topic),
                                    false => format!("{}", pr.topic)
                                }
                            };
                            let message = MqttMessage {
                                topic: outbound_topic,
                                payload: pr.payload.to_vec()
                            };
                            let outbound = IPCMessage::Outbound(message);
                            let comms = COMMS.lock().await;
                            let mpsc = comms.get(&_server.name.clone()).unwrap();
                            let tx = mpsc.from_leaf_tx.clone();
                            drop(comms);
                            if let Err(e) = tx.send(outbound).await {
                                return Err(MQTTError::Misc("tx channel error: {e}".into()));
                            } else {
                                debug!("{}: Announced inbound message", _server.name);
                            }
                        }
                        _ => {
                            info!("mqtt incoming packet: {:#?}", i);
                        }
                    }
                }
                Event::Outgoing(o) => {
                    match o {
                        Outgoing::PingReq => {
                            trace!("Sent MQTT PING");
                        }
                        Outgoing::Publish(pb) => {
                            dlq.push(pb);
                        }
                        Outgoing::Subscribe(_) => {}
                        _ => {
                            info!("outgoing mqtt packet: {:#?}", o);
                        }
                    }
                }
            }
            if dlq.len() > 0 {
                trace!("DLQ is {}", dlq.len());
            }
        }
    });

    if server.mqtt_type == MqttType::Source {
        if let Err(e) = mqtt.client.subscribe(server.topic.clone(), QoS::AtLeastOnce).await {
            return Err(MQTTError::Misc("MQTT Source server {server.name} couldn't subscribe to topic: {e}".into()));
        }
    }


    loop {
        if task.is_finished() {
            let result = task.await;
            match result {
                Ok(t) => {
                    match t {
                        Ok(_) => {
                            error!("MQTT thread exited, closing app");
                            let _ = SHUTDOWN.set(true);
                            return Err(MQTTError::ExitingThread);
                        }
                        Err(e) => {
                            error!("{e}");
                            let _ = SHUTDOWN.set(true);
                            return Err(MQTTError::ExitingThread);
                        }
                    }
                }
                Err(e) => {
                    error!("{e}");
                    let _ = SHUTDOWN.set(true);
                    return Err(MQTTError::ExitingThread);
                }
            }
        };

//region MQTT loop channel handling

        let mut comms = COMMS.lock().await;
        let mut mpsc: &mut ThreadCommunication = comms.get_mut(&server.name.clone()).unwrap();
        match mpsc.to_leaf_rx.try_recv() {
            Ok(ipcm) => match ipcm {
                IPCMessage::Outbound(msg) => {
                    if server.mqtt_type == MqttType::Source {
                        continue;
                    }
                    let publish_topic = format!("{}/{}", server.topic, msg.topic);
                    match timeout(
                        Duration::from_secs(3),
                        mqtt.client
                            .publish(publish_topic, QoS::AtLeastOnce, false, msg.payload),
                    )
                        .await
                    {
                        Ok(result) => match result {
                            Ok(_) => {
                                info!("Published");
                            }
                            Err(e) => {
                                error!("Couldn't send message: {e}");
                            }
                        },
                        Err(_e) => {
                            error!("Timeout trying to mqtt publish!")
                        }
                    }
                }
                IPCMessage::Inbound(_) => {
                    unreachable!();
                }
                _ => {}
            },
            Err(e) => match e {
                TryRecvError::Empty => {}
                TryRecvError::Disconnected => {
                    error!("Cannot poll: disconnected mpsc channel!");
                }
            },
        }
// drop mutex since other tasks will want it.
        drop(comms);

//endregion
// trace!("mqtt tick");
        let _ = sleep(Duration::from_millis(MQTT_POLL_INTERVAL_MILLIS)).await;
    }
}
