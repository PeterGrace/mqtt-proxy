use crate::consts::*;
use rumqttc::{AsyncClient, EventLoop, MqttOptions, QoS};
use std::fmt::{Debug, Formatter};
use std::ops::{Deref, DerefMut};
use tokio::sync::mpsc::{Receiver, Sender};
use tokio::time::Duration;
use crate::errors::MQTTError;
use crate::config_file::MqttServer;
use crate::ipc::IPCMessage;


#[derive(Debug)]
pub struct ThreadCommunication {
    pub(crate) to_leaf_rx: Receiver<IPCMessage>,
    pub(crate) to_leaf_tx: Sender<IPCMessage>,
    pub(crate) from_leaf_rx: Receiver<IPCMessage>,
    pub(crate) from_leaf_tx: Sender<IPCMessage>
}

#[derive(Debug)]
#[allow(dead_code)]
pub struct MqttConnection {
    client_name: String,
    server_addr: String,
    port: u16,
    username: Option<String>,
    password: Option<String>,
    pub(crate) client: AsyncClient,
    pub(crate) event_loop: MyEventLoop,
}

pub(crate) struct MyEventLoop(EventLoop);

impl Deref for MyEventLoop {
    type Target = EventLoop;

    //Target = EventLoop;
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}
impl DerefMut for MyEventLoop {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}
impl Debug for MyEventLoop {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "EventLoop has no Debug.")
    }
}
impl MqttConnection {
    pub async fn create(server: MqttServer) -> Result<Self, MQTTError>{
        let client_name = server.client_name.unwrap_or("mqttproxy".to_string());
        let mut mqttoptions = MqttOptions::new(&client_name, &server.address, server.port);
        mqttoptions.set_keep_alive(Duration::from_secs(MQTT_KEEPALIVE_TIME));
        if server.username.is_some() && server.password.is_some() {
            mqttoptions.set_credentials(server.username.clone().unwrap(), server.password.clone().unwrap());
        }
        let (mqtt_client, eventloop) = AsyncClient::new(mqttoptions, MQTT_THREAD_CHANNEL_CAPACITY);
        Ok(MqttConnection {
            client_name,
            server_addr: server.address,
            port: server.port,
            username: server.username,
            password: server.password,
            client: mqtt_client,
            event_loop: MyEventLoop(eventloop),
        })
    }
    pub async fn new(
        client: String,
        addr: String,
        port: u16,
        username: Option<String>,
        password: Option<String>,
    ) -> Result<Self, MQTTError> {
        let mut mqttoptions = MqttOptions::new(&client, &addr, port);
        mqttoptions.set_keep_alive(Duration::from_secs(MQTT_KEEPALIVE_TIME));
        if username.is_some() && password.is_some() {
            mqttoptions.set_credentials(username.clone().unwrap(), password.clone().unwrap());
        }
        let (mqtt_client, eventloop) = AsyncClient::new(mqttoptions, MQTT_THREAD_CHANNEL_CAPACITY);

        Ok(MqttConnection {
            client_name: client,
            server_addr: addr,
            port,
            username,
            password,
            client: mqtt_client,
            event_loop: MyEventLoop(eventloop),
        })
    }
}
