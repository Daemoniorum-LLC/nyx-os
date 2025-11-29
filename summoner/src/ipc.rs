//! IPC server for Summoner

use crate::actions::Launcher;
use crate::index::AppIndex;
use crate::recent::RecentApps;
use crate::search::SearchEngine;
use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::path::Path;
use std::sync::Arc;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::{UnixListener, UnixStream};
use tokio::sync::RwLock;

/// IPC request types
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", content = "data")]
pub enum IpcRequest {
    // Search
    Search { query: String },
    SearchWithOptions { query: String, max_results: Option<usize>, include_hidden: Option<bool> },

    // Launch
    Launch { app_id: String, files: Option<Vec<String>> },
    LaunchAction { app_id: String, action_id: String, files: Option<Vec<String>> },
    QuickLaunch { command: String },

    // Index
    GetApp { app_id: String },
    ListApps { category: Option<String> },
    ListCategories,
    RefreshIndex,

    // Recent
    GetRecent { limit: Option<usize> },
    GetFrequent { limit: Option<usize> },
    ClearRecent,

    // Status
    GetStats,
    IsRunning { app_id: String },
}

/// IPC response
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "status")]
pub enum IpcResponse {
    Success { data: serde_json::Value },
    Error { message: String },
}

/// IPC server
pub struct SummonerIpcServer {
    index: Arc<RwLock<AppIndex>>,
    search: Arc<SearchEngine>,
    launcher: Arc<RwLock<Launcher>>,
    recent: Arc<RwLock<RecentApps>>,
}

impl SummonerIpcServer {
    pub fn new(
        index: Arc<RwLock<AppIndex>>,
        search: Arc<SearchEngine>,
        launcher: Arc<RwLock<Launcher>>,
        recent: Arc<RwLock<RecentApps>>,
    ) -> Self {
        Self {
            index,
            search,
            launcher,
            recent,
        }
    }

    pub async fn start(&self, socket_path: &Path) -> Result<()> {
        let _ = std::fs::remove_file(socket_path);

        let listener = UnixListener::bind(socket_path)?;
        tracing::info!("Summoner IPC server listening on {:?}", socket_path);

        loop {
            match listener.accept().await {
                Ok((stream, _)) => {
                    let index = Arc::clone(&self.index);
                    let search = Arc::clone(&self.search);
                    let launcher = Arc::clone(&self.launcher);
                    let recent = Arc::clone(&self.recent);

                    tokio::spawn(async move {
                        if let Err(e) = handle_client(stream, index, search, launcher, recent).await {
                            tracing::error!("Client error: {}", e);
                        }
                    });
                }
                Err(e) => {
                    tracing::error!("Accept error: {}", e);
                }
            }
        }
    }
}

async fn handle_client(
    stream: UnixStream,
    index: Arc<RwLock<AppIndex>>,
    search: Arc<SearchEngine>,
    launcher: Arc<RwLock<Launcher>>,
    recent: Arc<RwLock<RecentApps>>,
) -> Result<()> {
    let (reader, mut writer) = stream.into_split();
    let mut reader = BufReader::new(reader);
    let mut line = String::new();

    while reader.read_line(&mut line).await? > 0 {
        let response = match serde_json::from_str::<IpcRequest>(&line) {
            Ok(request) => process_request(request, &index, &search, &launcher, &recent).await,
            Err(e) => IpcResponse::Error {
                message: format!("Invalid request: {}", e),
            },
        };

        let response_json = serde_json::to_string(&response)?;
        writer.write_all(response_json.as_bytes()).await?;
        writer.write_all(b"\n").await?;
        writer.flush().await?;

        line.clear();
    }

    Ok(())
}

async fn process_request(
    request: IpcRequest,
    index: &RwLock<AppIndex>,
    search: &SearchEngine,
    launcher: &RwLock<Launcher>,
    recent: &RwLock<RecentApps>,
) -> IpcResponse {
    match request {
        IpcRequest::Search { query } => {
            let idx = index.read().await;
            let results = search.search(&query, &idx).await;

            let apps: Vec<_> = results.iter().map(|r| {
                serde_json::json!({
                    "id": r.app.id,
                    "name": r.app.entry.name,
                    "icon": r.app.entry.icon,
                    "comment": r.app.entry.comment,
                    "score": r.score,
                    "match_type": format!("{:?}", r.match_type),
                })
            }).collect();

            IpcResponse::Success {
                data: serde_json::json!({ "results": apps }),
            }
        }

        IpcRequest::SearchWithOptions { query, max_results, include_hidden: _ } => {
            let idx = index.read().await;
            let mut results = search.search(&query, &idx).await;

            if let Some(max) = max_results {
                results.truncate(max);
            }

            let apps: Vec<_> = results.iter().map(|r| {
                serde_json::json!({
                    "id": r.app.id,
                    "name": r.app.entry.name,
                    "icon": r.app.entry.icon,
                    "exec": r.app.entry.exec,
                    "categories": r.app.entry.categories,
                    "score": r.score,
                })
            }).collect();

            IpcResponse::Success {
                data: serde_json::json!({ "results": apps }),
            }
        }

        IpcRequest::Launch { app_id, files } => {
            let idx = index.read().await;

            if let Some(app) = idx.get(&app_id).await {
                let files = files.unwrap_or_default();
                let mut launcher_guard = launcher.write().await;

                match launcher_guard.launch(&app.entry, &files).await {
                    Ok(pid) => {
                        // Record in recent
                        recent.write().await.record(&app_id);
                        idx.record_use(&app_id).await;

                        IpcResponse::Success {
                            data: serde_json::json!({ "pid": pid, "app_id": app_id }),
                        }
                    }
                    Err(e) => IpcResponse::Error { message: e.to_string() },
                }
            } else {
                IpcResponse::Error {
                    message: format!("App not found: {}", app_id),
                }
            }
        }

        IpcRequest::LaunchAction { app_id, action_id, files } => {
            let idx = index.read().await;

            if let Some(app) = idx.get(&app_id).await {
                let files = files.unwrap_or_default();
                let mut launcher_guard = launcher.write().await;

                match launcher_guard.launch_action(&app.entry, &action_id, &files).await {
                    Ok(pid) => IpcResponse::Success {
                        data: serde_json::json!({ "pid": pid }),
                    },
                    Err(e) => IpcResponse::Error { message: e.to_string() },
                }
            } else {
                IpcResponse::Error {
                    message: format!("App not found: {}", app_id),
                }
            }
        }

        IpcRequest::QuickLaunch { command } => {
            match crate::actions::quick_launch(&command).await {
                Ok(pid) => IpcResponse::Success {
                    data: serde_json::json!({ "pid": pid }),
                },
                Err(e) => IpcResponse::Error { message: e.to_string() },
            }
        }

        IpcRequest::GetApp { app_id } => {
            let idx = index.read().await;

            if let Some(app) = idx.get(&app_id).await {
                IpcResponse::Success {
                    data: serde_json::json!({
                        "id": app.id,
                        "name": app.entry.name,
                        "generic_name": app.entry.generic_name,
                        "comment": app.entry.comment,
                        "icon": app.entry.icon,
                        "exec": app.entry.exec,
                        "categories": app.entry.categories,
                        "keywords": app.entry.keywords,
                        "terminal": app.entry.terminal,
                        "actions": app.entry.actions.iter().map(|a| {
                            serde_json::json!({
                                "id": a.id,
                                "name": a.name,
                                "icon": a.icon,
                            })
                        }).collect::<Vec<_>>(),
                        "use_count": app.use_count,
                    }),
                }
            } else {
                IpcResponse::Error {
                    message: format!("App not found: {}", app_id),
                }
            }
        }

        IpcRequest::ListApps { category } => {
            let idx = index.read().await;

            let apps = if let Some(cat) = category {
                idx.by_category(&cat).await
            } else {
                idx.all().await
            };

            let app_list: Vec<_> = apps.iter().map(|app| {
                serde_json::json!({
                    "id": app.id,
                    "name": app.entry.name,
                    "icon": app.entry.icon,
                })
            }).collect();

            IpcResponse::Success {
                data: serde_json::json!({ "apps": app_list }),
            }
        }

        IpcRequest::ListCategories => {
            let idx = index.read().await;
            let categories = idx.categories().await;

            IpcResponse::Success {
                data: serde_json::json!({ "categories": categories }),
            }
        }

        IpcRequest::RefreshIndex => {
            // This would trigger a re-scan of desktop files
            IpcResponse::Success {
                data: serde_json::json!({ "refreshed": true }),
            }
        }

        IpcRequest::GetRecent { limit } => {
            let recent_guard = recent.read().await;
            let limit = limit.unwrap_or(10);
            let apps: Vec<_> = recent_guard.get_recent().into_iter().take(limit).collect();

            IpcResponse::Success {
                data: serde_json::json!({ "recent": apps }),
            }
        }

        IpcRequest::GetFrequent { limit } => {
            let recent_guard = recent.read().await;
            let limit = limit.unwrap_or(10);
            let apps: Vec<_> = recent_guard.get_frequent(limit);

            IpcResponse::Success {
                data: serde_json::json!({ "frequent": apps }),
            }
        }

        IpcRequest::ClearRecent => {
            recent.write().await.clear();
            IpcResponse::Success {
                data: serde_json::json!({ "cleared": true }),
            }
        }

        IpcRequest::GetStats => {
            let idx = index.read().await;
            let recent_guard = recent.read().await;

            IpcResponse::Success {
                data: serde_json::json!({
                    "total_apps": idx.len().await,
                    "categories": idx.categories().await.len(),
                    "recent_count": recent_guard.len(),
                }),
            }
        }

        IpcRequest::IsRunning { app_id } => {
            let launcher_guard = launcher.read().await;
            let running = launcher_guard.is_running(&app_id);

            IpcResponse::Success {
                data: serde_json::json!({ "running": running }),
            }
        }
    }
}

/// IPC client
pub struct SummonerClient {
    socket_path: std::path::PathBuf,
}

impl SummonerClient {
    pub fn new(socket_path: impl Into<std::path::PathBuf>) -> Self {
        Self {
            socket_path: socket_path.into(),
        }
    }

    pub async fn search(&self, query: &str) -> Result<Vec<serde_json::Value>> {
        let response = self.send(IpcRequest::Search {
            query: query.to_string(),
        }).await?;

        match response {
            IpcResponse::Success { data } => {
                Ok(data.get("results")
                    .and_then(|v| v.as_array())
                    .cloned()
                    .unwrap_or_default())
            }
            IpcResponse::Error { message } => Err(anyhow::anyhow!(message)),
        }
    }

    pub async fn launch(&self, app_id: &str) -> Result<u32> {
        let response = self.send(IpcRequest::Launch {
            app_id: app_id.to_string(),
            files: None,
        }).await?;

        match response {
            IpcResponse::Success { data } => {
                data.get("pid")
                    .and_then(|v| v.as_u64())
                    .map(|p| p as u32)
                    .ok_or_else(|| anyhow::anyhow!("No PID in response"))
            }
            IpcResponse::Error { message } => Err(anyhow::anyhow!(message)),
        }
    }

    async fn send(&self, request: IpcRequest) -> Result<IpcResponse> {
        let mut stream = UnixStream::connect(&self.socket_path).await?;

        let request_json = serde_json::to_string(&request)?;
        stream.write_all(request_json.as_bytes()).await?;
        stream.write_all(b"\n").await?;
        stream.flush().await?;

        let mut reader = BufReader::new(stream);
        let mut line = String::new();
        reader.read_line(&mut line).await?;

        Ok(serde_json::from_str(&line)?)
    }
}
