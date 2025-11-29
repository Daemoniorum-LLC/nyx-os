//! Shell execution engine

use crate::config::UmbraConfig;
use crate::history::History;
use anyhow::{anyhow, Result};
use std::collections::HashMap;
use std::env;
use std::path::PathBuf;
use std::process::{Child, Command, Stdio};
use tokio::sync::mpsc;

/// Shell execution environment
pub struct Shell {
    config: UmbraConfig,
    env: HashMap<String, String>,
    cwd: PathBuf,
    history: History,
    jobs: HashMap<u32, Job>,
    next_job_id: u32,
    last_exit_code: i32,
}

pub struct Job {
    pub id: u32,
    pub command: String,
    pub child: Child,
    pub background: bool,
}

#[derive(Debug, Clone)]
pub enum ShellEvent {
    Output(String),
    Error(String),
    Exit(i32),
    JobStarted(u32),
    JobFinished(u32, i32),
    DirectoryChanged(PathBuf),
}

impl Shell {
    pub fn new(config: UmbraConfig) -> Result<Self> {
        let cwd = env::current_dir().unwrap_or_else(|_| PathBuf::from("/"));
        let mut env: HashMap<String, String> = env::vars().collect();

        // Add configured environment variables
        for var in &config.environment {
            env.insert(var.key.clone(), var.value.clone());
        }

        let history = History::new(&config.history)?;

        Ok(Self {
            config,
            env,
            cwd,
            history,
            jobs: HashMap::new(),
            next_job_id: 1,
            last_exit_code: 0,
        })
    }

    /// Execute a command line
    pub async fn execute(
        &mut self,
        input: &str,
        event_tx: mpsc::Sender<ShellEvent>,
    ) -> Result<i32> {
        let input = input.trim();
        if input.is_empty() {
            return Ok(0);
        }

        // Check for alias expansion
        let expanded = self.expand_aliases(input);

        // Parse the command
        let (cmd, background) = self.parse_command(&expanded)?;

        // Add to history
        self.history.add(input)?;

        // Handle built-in commands
        if let Some(exit_code) = self.try_builtin(&cmd, &event_tx).await? {
            self.last_exit_code = exit_code;
            return Ok(exit_code);
        }

        // Execute external command
        let exit_code = self.execute_external(&cmd, background, event_tx).await?;
        self.last_exit_code = exit_code;

        Ok(exit_code)
    }

    fn expand_aliases(&self, input: &str) -> String {
        let parts: Vec<&str> = input.splitn(2, char::is_whitespace).collect();
        if let Some(alias) = self.config.aliases.iter().find(|a| a.name == parts[0]) {
            if parts.len() > 1 {
                format!("{} {}", alias.command, parts[1])
            } else {
                alias.command.clone()
            }
        } else {
            input.to_string()
        }
    }

    fn parse_command(&self, input: &str) -> Result<(Vec<String>, bool)> {
        let mut tokens = Vec::new();
        let mut current = String::new();
        let mut in_single_quote = false;
        let mut in_double_quote = false;
        let mut escaped = false;
        let mut background = false;

        for ch in input.chars() {
            if escaped {
                current.push(ch);
                escaped = false;
                continue;
            }

            match ch {
                '\\' if !in_single_quote => escaped = true,
                '\'' if !in_double_quote => in_single_quote = !in_single_quote,
                '"' if !in_single_quote => in_double_quote = !in_double_quote,
                ' ' | '\t' if !in_single_quote && !in_double_quote => {
                    if !current.is_empty() {
                        tokens.push(current.clone());
                        current.clear();
                    }
                }
                '&' if !in_single_quote && !in_double_quote => {
                    if !current.is_empty() {
                        tokens.push(current.clone());
                        current.clear();
                    }
                    background = true;
                }
                '$' if !in_single_quote => {
                    // Variable expansion
                    current.push(ch);
                }
                _ => current.push(ch),
            }
        }

        if !current.is_empty() {
            tokens.push(current);
        }

        // Expand variables
        let tokens: Vec<String> = tokens.into_iter()
            .map(|t| self.expand_variables(&t))
            .collect();

        Ok((tokens, background))
    }

    fn expand_variables(&self, token: &str) -> String {
        let mut result = String::new();
        let mut chars = token.chars().peekable();

        while let Some(ch) = chars.next() {
            if ch == '$' {
                let mut var_name = String::new();

                // Handle ${VAR} syntax
                if chars.peek() == Some(&'{') {
                    chars.next();
                    while let Some(&c) = chars.peek() {
                        if c == '}' {
                            chars.next();
                            break;
                        }
                        var_name.push(chars.next().unwrap());
                    }
                } else {
                    // Handle $VAR syntax
                    while let Some(&c) = chars.peek() {
                        if c.is_alphanumeric() || c == '_' {
                            var_name.push(chars.next().unwrap());
                        } else {
                            break;
                        }
                    }
                }

                // Special variables
                match var_name.as_str() {
                    "?" => result.push_str(&self.last_exit_code.to_string()),
                    "PWD" => result.push_str(self.cwd.to_string_lossy().as_ref()),
                    "HOME" => {
                        if let Some(home) = dirs::home_dir() {
                            result.push_str(home.to_string_lossy().as_ref());
                        }
                    }
                    _ => {
                        if let Some(value) = self.env.get(&var_name) {
                            result.push_str(value);
                        }
                    }
                }
            } else {
                result.push(ch);
            }
        }

        result
    }

    async fn try_builtin(
        &mut self,
        cmd: &[String],
        event_tx: &mpsc::Sender<ShellEvent>,
    ) -> Result<Option<i32>> {
        if cmd.is_empty() {
            return Ok(Some(0));
        }

        match cmd[0].as_str() {
            "cd" => {
                let path = if cmd.len() > 1 {
                    PathBuf::from(&cmd[1])
                } else {
                    dirs::home_dir().unwrap_or_else(|| PathBuf::from("/"))
                };

                let new_cwd = if path.is_absolute() {
                    path
                } else {
                    self.cwd.join(path)
                };

                match new_cwd.canonicalize() {
                    Ok(canonical) => {
                        self.cwd = canonical.clone();
                        env::set_current_dir(&canonical)?;
                        let _ = event_tx.send(ShellEvent::DirectoryChanged(canonical)).await;
                        Ok(Some(0))
                    }
                    Err(e) => {
                        let _ = event_tx.send(ShellEvent::Error(format!("cd: {}", e))).await;
                        Ok(Some(1))
                    }
                }
            }

            "pwd" => {
                let _ = event_tx.send(ShellEvent::Output(
                    self.cwd.to_string_lossy().to_string()
                )).await;
                Ok(Some(0))
            }

            "export" => {
                for arg in cmd.iter().skip(1) {
                    if let Some((key, value)) = arg.split_once('=') {
                        self.env.insert(key.to_string(), value.to_string());
                        env::set_var(key, value);
                    }
                }
                Ok(Some(0))
            }

            "unset" => {
                for arg in cmd.iter().skip(1) {
                    self.env.remove(arg);
                    env::remove_var(arg);
                }
                Ok(Some(0))
            }

            "exit" => {
                let code = cmd.get(1)
                    .and_then(|s| s.parse().ok())
                    .unwrap_or(0);
                Err(anyhow!("exit:{}", code))
            }

            "jobs" => {
                for job in self.jobs.values() {
                    let _ = event_tx.send(ShellEvent::Output(
                        format!("[{}] {} {}", job.id, if job.background { "Running" } else { "Stopped" }, job.command)
                    )).await;
                }
                Ok(Some(0))
            }

            "history" => {
                for (i, entry) in self.history.entries().iter().enumerate() {
                    let _ = event_tx.send(ShellEvent::Output(
                        format!("{:5}  {}", i + 1, entry)
                    )).await;
                }
                Ok(Some(0))
            }

            "alias" => {
                if cmd.len() == 1 {
                    for alias in &self.config.aliases {
                        let _ = event_tx.send(ShellEvent::Output(
                            format!("alias {}='{}'", alias.name, alias.command)
                        )).await;
                    }
                }
                Ok(Some(0))
            }

            "source" | "." => {
                if cmd.len() > 1 {
                    // Source file (simplified - just read and execute)
                    let path = PathBuf::from(&cmd[1]);
                    if let Ok(contents) = std::fs::read_to_string(&path) {
                        for line in contents.lines() {
                            let line = line.trim();
                            if !line.is_empty() && !line.starts_with('#') {
                                self.execute(line, event_tx.clone()).await?;
                            }
                        }
                    }
                }
                Ok(Some(0))
            }

            "type" => {
                for arg in cmd.iter().skip(1) {
                    if self.config.aliases.iter().any(|a| a.name == *arg) {
                        let _ = event_tx.send(ShellEvent::Output(format!("{} is an alias", arg))).await;
                    } else if matches!(arg.as_str(), "cd" | "pwd" | "export" | "exit" | "jobs" | "history" | "alias" | "source" | "." | "type" | "echo") {
                        let _ = event_tx.send(ShellEvent::Output(format!("{} is a shell builtin", arg))).await;
                    } else if let Ok(path) = which::which(arg) {
                        let _ = event_tx.send(ShellEvent::Output(format!("{} is {}", arg, path.display()))).await;
                    } else {
                        let _ = event_tx.send(ShellEvent::Error(format!("{}: not found", arg))).await;
                    }
                }
                Ok(Some(0))
            }

            "echo" => {
                let output = cmd[1..].join(" ");
                let _ = event_tx.send(ShellEvent::Output(output)).await;
                Ok(Some(0))
            }

            _ => Ok(None),
        }
    }

    async fn execute_external(
        &mut self,
        cmd: &[String],
        background: bool,
        event_tx: mpsc::Sender<ShellEvent>,
    ) -> Result<i32> {
        if cmd.is_empty() {
            return Ok(0);
        }

        let program = &cmd[0];
        let args = &cmd[1..];

        let mut command = Command::new(program);
        command.args(args)
            .current_dir(&self.cwd)
            .envs(&self.env);

        if background {
            command.stdin(Stdio::null())
                .stdout(Stdio::piped())
                .stderr(Stdio::piped());
        } else {
            command.stdin(Stdio::inherit())
                .stdout(Stdio::inherit())
                .stderr(Stdio::inherit());
        }

        match command.spawn() {
            Ok(child) => {
                if background {
                    let job_id = self.next_job_id;
                    self.next_job_id += 1;

                    let _ = event_tx.send(ShellEvent::JobStarted(job_id)).await;
                    let _ = event_tx.send(ShellEvent::Output(
                        format!("[{}] {}", job_id, child.id())
                    )).await;

                    self.jobs.insert(job_id, Job {
                        id: job_id,
                        command: cmd.join(" "),
                        child,
                        background: true,
                    });

                    Ok(0)
                } else {
                    let output = child.wait_with_output()?;
                    let exit_code = output.status.code().unwrap_or(-1);
                    let _ = event_tx.send(ShellEvent::Exit(exit_code)).await;
                    Ok(exit_code)
                }
            }
            Err(e) => {
                let _ = event_tx.send(ShellEvent::Error(
                    format!("{}: {}", program, e)
                )).await;
                Ok(127)
            }
        }
    }

    /// Check for completed background jobs
    pub fn check_jobs(&mut self) -> Vec<(u32, i32)> {
        let mut completed = Vec::new();

        self.jobs.retain(|id, job| {
            match job.child.try_wait() {
                Ok(Some(status)) => {
                    completed.push((*id, status.code().unwrap_or(-1)));
                    false
                }
                _ => true,
            }
        });

        completed
    }

    pub fn cwd(&self) -> &PathBuf {
        &self.cwd
    }

    pub fn env(&self) -> &HashMap<String, String> {
        &self.env
    }

    pub fn history(&self) -> &History {
        &self.history
    }

    pub fn last_exit_code(&self) -> i32 {
        self.last_exit_code
    }
}
