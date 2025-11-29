//! Terminal UI and line editing

use crate::completion::{Completer, Completion};
use crate::config::UmbraConfig;
use crate::history::History;
use crate::prompt::Prompt;
use anyhow::Result;
use crossterm::{
    cursor,
    event::{self, Event, KeyCode, KeyEvent, KeyModifiers},
    execute,
    style::{Color, Print, ResetColor, SetForegroundColor},
    terminal::{self, ClearType},
};
use std::io::{self, Write};
use std::path::PathBuf;

/// Line editor with completion support
pub struct LineEditor {
    buffer: String,
    cursor_pos: usize,
    prompt: Prompt,
    completer: Completer,
    completion_state: Option<CompletionState>,
    search_mode: bool,
    search_buffer: String,
}

struct CompletionState {
    completions: Vec<Completion>,
    selected: usize,
    original: String,
}

impl LineEditor {
    pub fn new(config: &UmbraConfig) -> Self {
        Self {
            buffer: String::new(),
            cursor_pos: 0,
            prompt: Prompt::new(config.prompt.clone()),
            completer: Completer::new(),
            completion_state: None,
            search_mode: false,
            search_buffer: String::new(),
        }
    }

    /// Read a line of input
    pub fn read_line(
        &mut self,
        cwd: &PathBuf,
        last_exit_code: i32,
        config: &UmbraConfig,
        history: &mut History,
    ) -> Result<Option<String>> {
        terminal::enable_raw_mode()?;

        // Show prompt
        let prompt_str = self.prompt.render(cwd, last_exit_code);
        print!("{}", prompt_str);
        io::stdout().flush()?;

        self.buffer.clear();
        self.cursor_pos = 0;
        self.completion_state = None;
        self.search_mode = false;
        history.reset_position();

        let result = self.input_loop(config, history);

        terminal::disable_raw_mode()?;
        println!();

        result
    }

    fn input_loop(
        &mut self,
        config: &UmbraConfig,
        history: &mut History,
    ) -> Result<Option<String>> {
        loop {
            if event::poll(std::time::Duration::from_millis(100))? {
                if let Event::Key(key) = event::read()? {
                    match self.handle_key(key, config, history)? {
                        InputResult::Continue => {}
                        InputResult::Submit => {
                            return Ok(Some(self.buffer.clone()));
                        }
                        InputResult::Cancel => {
                            return Ok(None);
                        }
                        InputResult::Exit => {
                            return Ok(Some("exit".to_string()));
                        }
                    }
                }
            }
        }
    }

    fn handle_key(
        &mut self,
        key: KeyEvent,
        config: &UmbraConfig,
        history: &mut History,
    ) -> Result<InputResult> {
        // Handle search mode separately
        if self.search_mode {
            return self.handle_search_key(key, history);
        }

        // Handle completion state
        if self.completion_state.is_some() {
            match key.code {
                KeyCode::Tab => return self.cycle_completion(false),
                KeyCode::BackTab => return self.cycle_completion(true),
                KeyCode::Enter => {
                    self.accept_completion();
                    return Ok(InputResult::Continue);
                }
                KeyCode::Esc => {
                    self.cancel_completion();
                    return Ok(InputResult::Continue);
                }
                _ => {
                    self.cancel_completion();
                }
            }
        }

        match (key.code, key.modifiers) {
            // Submit
            (KeyCode::Enter, _) => {
                return Ok(InputResult::Submit);
            }

            // Cancel
            (KeyCode::Char('c'), KeyModifiers::CONTROL) => {
                print!("^C");
                self.buffer.clear();
                return Ok(InputResult::Cancel);
            }

            // Exit
            (KeyCode::Char('d'), KeyModifiers::CONTROL) => {
                if self.buffer.is_empty() {
                    return Ok(InputResult::Exit);
                }
            }

            // Clear screen
            (KeyCode::Char('l'), KeyModifiers::CONTROL) => {
                execute!(io::stdout(), terminal::Clear(ClearType::All), cursor::MoveTo(0, 0))?;
                self.redraw()?;
            }

            // Tab completion
            (KeyCode::Tab, _) => {
                self.start_completion(config, history)?;
            }

            // History navigation
            (KeyCode::Up, _) | (KeyCode::Char('p'), KeyModifiers::CONTROL) => {
                if let Some(entry) = history.previous() {
                    self.buffer = entry.to_string();
                    self.cursor_pos = self.buffer.len();
                    self.redraw()?;
                }
            }

            (KeyCode::Down, _) | (KeyCode::Char('n'), KeyModifiers::CONTROL) => {
                if let Some(entry) = history.next() {
                    self.buffer = entry.to_string();
                } else {
                    self.buffer.clear();
                }
                self.cursor_pos = self.buffer.len();
                self.redraw()?;
            }

            // Reverse search
            (KeyCode::Char('r'), KeyModifiers::CONTROL) => {
                self.start_search();
                self.redraw_search()?;
            }

            // Cursor movement
            (KeyCode::Left, _) | (KeyCode::Char('b'), KeyModifiers::CONTROL) => {
                if self.cursor_pos > 0 {
                    self.cursor_pos -= 1;
                    execute!(io::stdout(), cursor::MoveLeft(1))?;
                }
            }

            (KeyCode::Right, _) | (KeyCode::Char('f'), KeyModifiers::CONTROL) => {
                if self.cursor_pos < self.buffer.len() {
                    self.cursor_pos += 1;
                    execute!(io::stdout(), cursor::MoveRight(1))?;
                }
            }

            (KeyCode::Home, _) | (KeyCode::Char('a'), KeyModifiers::CONTROL) => {
                self.cursor_pos = 0;
                self.redraw()?;
            }

            (KeyCode::End, _) | (KeyCode::Char('e'), KeyModifiers::CONTROL) => {
                self.cursor_pos = self.buffer.len();
                self.redraw()?;
            }

            // Word movement
            (KeyCode::Left, KeyModifiers::ALT) | (KeyCode::Char('b'), KeyModifiers::ALT) => {
                self.cursor_pos = self.find_word_start();
                self.redraw()?;
            }

            (KeyCode::Right, KeyModifiers::ALT) | (KeyCode::Char('f'), KeyModifiers::ALT) => {
                self.cursor_pos = self.find_word_end();
                self.redraw()?;
            }

            // Delete
            (KeyCode::Backspace, _) | (KeyCode::Char('h'), KeyModifiers::CONTROL) => {
                if self.cursor_pos > 0 {
                    self.cursor_pos -= 1;
                    self.buffer.remove(self.cursor_pos);
                    self.redraw()?;
                }
            }

            (KeyCode::Delete, _) | (KeyCode::Char('d'), KeyModifiers::CONTROL) => {
                if self.cursor_pos < self.buffer.len() {
                    self.buffer.remove(self.cursor_pos);
                    self.redraw()?;
                }
            }

            // Kill line
            (KeyCode::Char('k'), KeyModifiers::CONTROL) => {
                self.buffer.truncate(self.cursor_pos);
                self.redraw()?;
            }

            (KeyCode::Char('u'), KeyModifiers::CONTROL) => {
                self.buffer = self.buffer[self.cursor_pos..].to_string();
                self.cursor_pos = 0;
                self.redraw()?;
            }

            // Kill word
            (KeyCode::Char('w'), KeyModifiers::CONTROL) => {
                let start = self.find_word_start();
                self.buffer.replace_range(start..self.cursor_pos, "");
                self.cursor_pos = start;
                self.redraw()?;
            }

            // Transpose
            (KeyCode::Char('t'), KeyModifiers::CONTROL) => {
                if self.cursor_pos > 0 && self.cursor_pos < self.buffer.len() {
                    let chars: Vec<char> = self.buffer.chars().collect();
                    let mut new_chars = chars.clone();
                    new_chars.swap(self.cursor_pos - 1, self.cursor_pos);
                    self.buffer = new_chars.into_iter().collect();
                    self.cursor_pos += 1;
                    self.redraw()?;
                }
            }

            // Character input
            (KeyCode::Char(c), KeyModifiers::NONE | KeyModifiers::SHIFT) => {
                self.buffer.insert(self.cursor_pos, c);
                self.cursor_pos += 1;
                self.redraw()?;
            }

            _ => {}
        }

        Ok(InputResult::Continue)
    }

    fn handle_search_key(
        &mut self,
        key: KeyEvent,
        history: &mut History,
    ) -> Result<InputResult> {
        match key.code {
            KeyCode::Enter | KeyCode::Esc => {
                if let Some(result) = history.stop_search() {
                    self.buffer = result.to_string();
                    self.cursor_pos = self.buffer.len();
                }
                self.search_mode = false;
                self.search_buffer.clear();
                self.redraw()?;
            }

            KeyCode::Char('r') if key.modifiers == KeyModifiers::CONTROL => {
                // Search previous
                if let Some(result) = history.update_search(&self.search_buffer) {
                    self.buffer = result.to_string();
                }
                self.redraw_search()?;
            }

            KeyCode::Char('c') if key.modifiers == KeyModifiers::CONTROL => {
                self.search_mode = false;
                self.search_buffer.clear();
                history.stop_search();
                self.buffer.clear();
                return Ok(InputResult::Cancel);
            }

            KeyCode::Backspace => {
                self.search_buffer.pop();
                if let Some(result) = history.update_search(&self.search_buffer) {
                    self.buffer = result.to_string();
                }
                self.redraw_search()?;
            }

            KeyCode::Char(c) => {
                self.search_buffer.push(c);
                if let Some(result) = history.update_search(&self.search_buffer) {
                    self.buffer = result.to_string();
                }
                self.redraw_search()?;
            }

            _ => {}
        }

        Ok(InputResult::Continue)
    }

    fn start_search(&mut self) {
        self.search_mode = true;
        self.search_buffer.clear();
    }

    fn redraw_search(&mut self) -> Result<()> {
        execute!(
            io::stdout(),
            cursor::MoveToColumn(0),
            terminal::Clear(ClearType::CurrentLine),
            SetForegroundColor(Color::Yellow),
            Print(format!("(reverse-i-search)`{}': {}", self.search_buffer, self.buffer)),
            ResetColor,
        )?;
        io::stdout().flush()?;
        Ok(())
    }

    fn start_completion(
        &mut self,
        config: &UmbraConfig,
        history: &History,
    ) -> Result<()> {
        let completions = self.completer.complete(
            &self.buffer,
            self.cursor_pos,
            config,
            history,
        )?;

        if completions.is_empty() {
            return Ok(());
        }

        if completions.len() == 1 {
            // Single completion - apply directly
            self.apply_completion(&completions[0]);
            self.redraw()?;
        } else {
            // Multiple completions - show menu
            self.completion_state = Some(CompletionState {
                completions,
                selected: 0,
                original: self.buffer.clone(),
            });
            self.show_completions()?;
        }

        Ok(())
    }

    fn cycle_completion(&mut self, reverse: bool) -> Result<InputResult> {
        if let Some(ref mut state) = self.completion_state {
            if reverse {
                state.selected = if state.selected == 0 {
                    state.completions.len() - 1
                } else {
                    state.selected - 1
                };
            } else {
                state.selected = (state.selected + 1) % state.completions.len();
            }

            // Apply selected completion
            let completion = &state.completions[state.selected].clone();
            self.apply_completion(completion);
            self.show_completions()?;
        }

        Ok(InputResult::Continue)
    }

    fn apply_completion(&mut self, completion: &Completion) {
        // Find the word being completed
        let word_start = self.buffer[..self.cursor_pos]
            .rfind(|c: char| c.is_whitespace())
            .map(|i| i + 1)
            .unwrap_or(0);

        // Replace the word with completion
        self.buffer.replace_range(word_start..self.cursor_pos, &completion.text);
        self.cursor_pos = word_start + completion.text.len();
    }

    fn accept_completion(&mut self) {
        self.completion_state = None;
        self.redraw().ok();
    }

    fn cancel_completion(&mut self) {
        if let Some(state) = self.completion_state.take() {
            self.buffer = state.original;
            self.cursor_pos = self.buffer.len();
            self.redraw().ok();
        }
    }

    fn show_completions(&mut self) -> Result<()> {
        if let Some(ref state) = self.completion_state {
            // Clear and show completion menu
            println!();

            let max_display = 10;
            let start = (state.selected / max_display) * max_display;
            let end = (start + max_display).min(state.completions.len());

            for (i, completion) in state.completions[start..end].iter().enumerate() {
                let idx = start + i;
                if idx == state.selected {
                    execute!(
                        io::stdout(),
                        SetForegroundColor(Color::Black),
                        Print(format!(" > {}\n", completion.display)),
                        ResetColor,
                    )?;
                } else {
                    println!("   {}", completion.display);
                }
            }

            if state.completions.len() > max_display {
                println!("   [{}/{}]", state.selected + 1, state.completions.len());
            }

            // Redraw input line
            self.redraw()?;
        }

        Ok(())
    }

    fn redraw(&self) -> Result<()> {
        execute!(
            io::stdout(),
            cursor::MoveToColumn(0),
            terminal::Clear(ClearType::CurrentLine),
        )?;

        // Re-render prompt would need cwd and exit code, simplified here
        print!("$ {}", self.buffer);

        // Position cursor
        let cursor_offset = self.buffer.len() - self.cursor_pos;
        if cursor_offset > 0 {
            execute!(io::stdout(), cursor::MoveLeft(cursor_offset as u16))?;
        }

        io::stdout().flush()?;
        Ok(())
    }

    fn find_word_start(&self) -> usize {
        let chars: Vec<char> = self.buffer.chars().collect();
        let mut pos = self.cursor_pos;

        // Skip whitespace
        while pos > 0 && chars[pos - 1].is_whitespace() {
            pos -= 1;
        }

        // Find word start
        while pos > 0 && !chars[pos - 1].is_whitespace() {
            pos -= 1;
        }

        pos
    }

    fn find_word_end(&self) -> usize {
        let chars: Vec<char> = self.buffer.chars().collect();
        let mut pos = self.cursor_pos;

        // Skip current word
        while pos < chars.len() && !chars[pos].is_whitespace() {
            pos += 1;
        }

        // Skip whitespace
        while pos < chars.len() && chars[pos].is_whitespace() {
            pos += 1;
        }

        pos
    }
}

enum InputResult {
    Continue,
    Submit,
    Cancel,
    Exit,
}

/// Output renderer with syntax highlighting
pub struct OutputRenderer {
    colors_enabled: bool,
}

impl OutputRenderer {
    pub fn new(colors_enabled: bool) -> Self {
        Self { colors_enabled }
    }

    pub fn print_output(&self, text: &str) {
        println!("{}", text);
    }

    pub fn print_error(&self, text: &str) {
        if self.colors_enabled {
            execute!(
                io::stdout(),
                SetForegroundColor(Color::Red),
                Print(text),
                Print("\n"),
                ResetColor,
            ).ok();
        } else {
            eprintln!("{}", text);
        }
    }

    pub fn print_info(&self, text: &str) {
        if self.colors_enabled {
            execute!(
                io::stdout(),
                SetForegroundColor(Color::Cyan),
                Print(text),
                Print("\n"),
                ResetColor,
            ).ok();
        } else {
            println!("{}", text);
        }
    }
}
