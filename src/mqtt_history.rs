use chrono::{DateTime, Local};
use rumqttc::{Connection, Publish};
use std::collections::HashMap;
use std::error::Error;
use std::sync::{Arc, Mutex};
use std::thread;
use std::thread::JoinHandle;

#[derive(Debug)]
pub struct HistoryEntry {
    pub packet: Publish,
    pub time: DateTime<Local>,
}

pub type HistoryArc = Arc<Mutex<HashMap<String, Vec<HistoryEntry>>>>;

pub fn start(mut connection: Connection) -> Result<(HistoryArc, JoinHandle<()>), Box<dyn Error>> {
    // TODO: weird workaround. Is there a better solution?
    // Iterate once. This is the initial connection attempt. When this fails it still fails in the main thread which is less messy. Happens for example when the host is wrong.
    connection.iter().next().unwrap()?;

    let history = Arc::new(Mutex::new(HashMap::new()));

    let thread_history = Arc::clone(&history);
    let handle = thread::Builder::new()
        .name("mqtt connection".into())
        .spawn(move || thread_logic(connection, &thread_history))?;

    Ok((history, handle))
}

fn thread_logic(mut connection: Connection, arc_history: &HistoryArc) {
    for notification in connection.iter() {
        // While only writing to history on Incoming Publish locking the mutex here is still useful
        // When something panics here, it will poison the mutex and end the main process
        let mut history = arc_history.lock().unwrap();

        match notification.expect("connection error") {
            rumqttc::Event::Incoming(packet) => {
                if let rumqttc::Packet::Publish(publish) = packet {
                    if publish.dup {
                        continue;
                    }

                    let time = Local::now();
                    let topic = &publish.topic;

                    if !history.contains_key(topic) {
                        history.insert(topic.to_owned(), Vec::new());
                    }

                    let vec = history.get_mut(topic).unwrap();
                    vec.push(HistoryEntry {
                        packet: publish,
                        time,
                    });
                }
            }
            rumqttc::Event::Outgoing(packet) => {
                if let rumqttc::Outgoing::Disconnect = packet {
                    break;
                }
            }
        };
    }
}

pub struct TopicMessagesLastPayload {
    pub topic: String,
    pub messages: usize,
    pub last_payload: Vec<u8>,
}

/// Get History Entries into the simpler `TopicMessagesLastPayload`
pub fn history_to_tmlp<'a, I>(items: I) -> Vec<TopicMessagesLastPayload>
where
    I: IntoIterator<Item = (&'a String, &'a Vec<HistoryEntry>)>,
{
    let mut result = Vec::new();
    for (topic, history) in items {
        result.push(TopicMessagesLastPayload {
            topic: topic.to_owned(),
            messages: history.len(),
            last_payload: history.last().unwrap().packet.payload.to_vec(),
        });
    }
    result
}
