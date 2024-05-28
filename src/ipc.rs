use crate::errors::MQTTError;

#[derive(Clone, Debug)]
pub struct MqttMessage {
    pub topic: String,
    pub payload: Vec<u8>
}
#[derive(Clone, Debug)]
pub enum IPCMessage {
    Inbound(MqttMessage),
    Outbound(MqttMessage),
    Error(MQTTError)
}