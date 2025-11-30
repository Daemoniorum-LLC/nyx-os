//! Audio client management

use crate::stream::{AudioStream, StreamDirection, StreamInfo};
use std::collections::HashMap;
use std::sync::atomic::{AtomicU32, Ordering};

/// Client ID counter
static CLIENT_ID: AtomicU32 = AtomicU32::new(1);

/// Audio client
pub struct AudioClient {
    /// Client ID
    pub id: u32,
    /// Application name
    pub app_name: String,
    /// Process ID
    pub pid: Option<u32>,
    /// Icon name
    pub icon: Option<String>,
    /// Streams owned by this client
    pub streams: Vec<u32>,
}

impl AudioClient {
    pub fn new(app_name: &str, pid: Option<u32>) -> Self {
        Self {
            id: CLIENT_ID.fetch_add(1, Ordering::SeqCst),
            app_name: app_name.to_string(),
            pid,
            icon: None,
            streams: Vec::new(),
        }
    }
}

/// Client manager
pub struct ClientManager {
    clients: HashMap<u32, AudioClient>,
    streams: HashMap<u32, AudioStream>,
    pid_to_client: HashMap<u32, u32>, // PID -> client ID
}

impl ClientManager {
    pub fn new() -> Self {
        Self {
            clients: HashMap::new(),
            streams: HashMap::new(),
            pid_to_client: HashMap::new(),
        }
    }

    /// Register a new client
    pub fn register_client(&mut self, app_name: &str, pid: Option<u32>) -> u32 {
        let client = AudioClient::new(app_name, pid);
        let id = client.id;

        if let Some(pid) = pid {
            self.pid_to_client.insert(pid, id);
        }

        self.clients.insert(id, client);
        id
    }

    /// Unregister a client
    pub fn unregister_client(&mut self, client_id: u32) {
        if let Some(client) = self.clients.remove(&client_id) {
            // Remove all streams
            for stream_id in client.streams {
                self.streams.remove(&stream_id);
            }

            // Remove PID mapping
            if let Some(pid) = client.pid {
                self.pid_to_client.remove(&pid);
            }
        }
    }

    /// Create a stream for a client
    pub fn create_stream(
        &mut self,
        client_id: u32,
        name: &str,
        direction: StreamDirection,
        format: crate::config::AudioFormat,
        target: &str,
    ) -> Option<u32> {
        let client = self.clients.get_mut(&client_id)?;

        let stream = AudioStream::new(name, &client.app_name, direction, format, target);
        let stream_id = stream.id;

        client.streams.push(stream_id);
        self.streams.insert(stream_id, stream);

        Some(stream_id)
    }

    /// Destroy a stream
    pub fn destroy_stream(&mut self, stream_id: u32) {
        if let Some(stream) = self.streams.remove(&stream_id) {
            // Find and update the owning client
            for client in self.clients.values_mut() {
                client.streams.retain(|&id| id != stream_id);
            }
        }
    }

    /// Get a stream by ID
    pub fn get_stream(&self, stream_id: u32) -> Option<&AudioStream> {
        self.streams.get(&stream_id)
    }

    /// Get a mutable stream by ID
    pub fn get_stream_mut(&mut self, stream_id: u32) -> Option<&mut AudioStream> {
        self.streams.get_mut(&stream_id)
    }

    /// Get client by PID
    pub fn get_client_by_pid(&self, pid: u32) -> Option<&AudioClient> {
        self.pid_to_client.get(&pid)
            .and_then(|id| self.clients.get(id))
    }

    /// Get all streams
    pub fn all_streams(&self) -> impl Iterator<Item = &AudioStream> {
        self.streams.values()
    }

    /// Get stream info list
    pub fn stream_info_list(&self) -> Vec<StreamInfo> {
        self.streams.values().map(StreamInfo::from).collect()
    }

    /// Get all clients
    pub fn all_clients(&self) -> impl Iterator<Item = &AudioClient> {
        self.clients.values()
    }

    /// Get stream count
    pub fn stream_count(&self) -> usize {
        self.streams.len()
    }

    /// Get client count
    pub fn client_count(&self) -> usize {
        self.clients.len()
    }

    /// Find streams by target (sink/source name)
    pub fn streams_by_target(&self, target: &str) -> Vec<&AudioStream> {
        self.streams.values()
            .filter(|s| s.target == target)
            .collect()
    }

    /// Move stream to different target
    pub fn move_stream(&mut self, stream_id: u32, new_target: &str) -> bool {
        if let Some(stream) = self.streams.get_mut(&stream_id) {
            stream.target = new_target.to_string();
            true
        } else {
            false
        }
    }
}

impl Default for ClientManager {
    fn default() -> Self {
        Self::new()
    }
}
