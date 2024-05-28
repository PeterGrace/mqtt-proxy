use thiserror::Error;

#[derive(Error,Clone,Debug)]
pub enum MQTTError {
    #[error("MQTT Error: {0}")]
    Misc(String),
    #[error("Received request for thread exit")]
    ExitingThread
}
