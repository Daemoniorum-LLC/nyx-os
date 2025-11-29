//! Command parsing and pipeline support

use anyhow::{anyhow, Result};
use std::collections::VecDeque;

/// Parsed command representation
#[derive(Debug, Clone)]
pub enum Command {
    Simple(SimpleCommand),
    Pipeline(Vec<SimpleCommand>),
    And(Box<Command>, Box<Command>),
    Or(Box<Command>, Box<Command>),
    Subshell(Box<Command>),
}

#[derive(Debug, Clone)]
pub struct SimpleCommand {
    pub program: String,
    pub args: Vec<String>,
    pub redirects: Vec<Redirect>,
    pub background: bool,
}

#[derive(Debug, Clone)]
pub enum Redirect {
    StdoutFile(String),        // > file
    StdoutAppend(String),      // >> file
    StderrFile(String),        // 2> file
    StderrAppend(String),      // 2>> file
    StdoutStderr(String),      // &> file
    StdinFile(String),         // < file
    Heredoc(String),           // << EOF
    StderrToStdout,            // 2>&1
}

/// Token types for command parsing
#[derive(Debug, Clone, PartialEq)]
pub enum Token {
    Word(String),
    Pipe,           // |
    And,            // &&
    Or,             // ||
    Background,     // &
    Semicolon,      // ;
    RedirectOut,    // >
    RedirectAppend, // >>
    RedirectIn,     // <
    RedirectErr,    // 2>
    RedirectErrAppend, // 2>>
    RedirectAll,    // &>
    StderrToStdout, // 2>&1
    OpenParen,      // (
    CloseParen,     // )
    Newline,
}

/// Tokenize input string
pub fn tokenize(input: &str) -> Result<Vec<Token>> {
    let mut tokens = Vec::new();
    let mut chars: VecDeque<char> = input.chars().collect();
    let mut current_word = String::new();
    let mut in_single_quote = false;
    let mut in_double_quote = false;
    let mut escaped = false;

    while let Some(ch) = chars.pop_front() {
        if escaped {
            current_word.push(ch);
            escaped = false;
            continue;
        }

        if ch == '\\' && !in_single_quote {
            escaped = true;
            continue;
        }

        if ch == '\'' && !in_double_quote {
            in_single_quote = !in_single_quote;
            continue;
        }

        if ch == '"' && !in_single_quote {
            in_double_quote = !in_double_quote;
            continue;
        }

        if in_single_quote || in_double_quote {
            current_word.push(ch);
            continue;
        }

        match ch {
            ' ' | '\t' => {
                if !current_word.is_empty() {
                    tokens.push(Token::Word(current_word.clone()));
                    current_word.clear();
                }
            }
            '\n' => {
                if !current_word.is_empty() {
                    tokens.push(Token::Word(current_word.clone()));
                    current_word.clear();
                }
                tokens.push(Token::Newline);
            }
            '|' => {
                if !current_word.is_empty() {
                    tokens.push(Token::Word(current_word.clone()));
                    current_word.clear();
                }
                if chars.front() == Some(&'|') {
                    chars.pop_front();
                    tokens.push(Token::Or);
                } else {
                    tokens.push(Token::Pipe);
                }
            }
            '&' => {
                if !current_word.is_empty() {
                    tokens.push(Token::Word(current_word.clone()));
                    current_word.clear();
                }
                match chars.front() {
                    Some(&'&') => {
                        chars.pop_front();
                        tokens.push(Token::And);
                    }
                    Some(&'>') => {
                        chars.pop_front();
                        tokens.push(Token::RedirectAll);
                    }
                    _ => tokens.push(Token::Background),
                }
            }
            ';' => {
                if !current_word.is_empty() {
                    tokens.push(Token::Word(current_word.clone()));
                    current_word.clear();
                }
                tokens.push(Token::Semicolon);
            }
            '>' => {
                if !current_word.is_empty() {
                    // Check for 2>
                    if current_word == "2" {
                        current_word.clear();
                        if chars.front() == Some(&'>') {
                            chars.pop_front();
                            tokens.push(Token::RedirectErrAppend);
                        } else if chars.front() == Some(&'&') {
                            chars.pop_front();
                            if chars.front() == Some(&'1') {
                                chars.pop_front();
                                tokens.push(Token::StderrToStdout);
                            }
                        } else {
                            tokens.push(Token::RedirectErr);
                        }
                        continue;
                    }
                    tokens.push(Token::Word(current_word.clone()));
                    current_word.clear();
                }
                if chars.front() == Some(&'>') {
                    chars.pop_front();
                    tokens.push(Token::RedirectAppend);
                } else {
                    tokens.push(Token::RedirectOut);
                }
            }
            '<' => {
                if !current_word.is_empty() {
                    tokens.push(Token::Word(current_word.clone()));
                    current_word.clear();
                }
                tokens.push(Token::RedirectIn);
            }
            '(' => {
                if !current_word.is_empty() {
                    tokens.push(Token::Word(current_word.clone()));
                    current_word.clear();
                }
                tokens.push(Token::OpenParen);
            }
            ')' => {
                if !current_word.is_empty() {
                    tokens.push(Token::Word(current_word.clone()));
                    current_word.clear();
                }
                tokens.push(Token::CloseParen);
            }
            _ => current_word.push(ch),
        }
    }

    if !current_word.is_empty() {
        tokens.push(Token::Word(current_word));
    }

    if in_single_quote || in_double_quote {
        return Err(anyhow!("Unclosed quote"));
    }

    Ok(tokens)
}

/// Parse tokens into a command
pub fn parse(tokens: &[Token]) -> Result<Command> {
    let mut parser = Parser::new(tokens);
    parser.parse_command()
}

struct Parser<'a> {
    tokens: &'a [Token],
    pos: usize,
}

impl<'a> Parser<'a> {
    fn new(tokens: &'a [Token]) -> Self {
        Self { tokens, pos: 0 }
    }

    fn peek(&self) -> Option<&Token> {
        self.tokens.get(self.pos)
    }

    fn advance(&mut self) -> Option<&Token> {
        let token = self.tokens.get(self.pos);
        self.pos += 1;
        token
    }

    fn parse_command(&mut self) -> Result<Command> {
        self.parse_or()
    }

    fn parse_or(&mut self) -> Result<Command> {
        let mut left = self.parse_and()?;

        while self.peek() == Some(&Token::Or) {
            self.advance();
            let right = self.parse_and()?;
            left = Command::Or(Box::new(left), Box::new(right));
        }

        Ok(left)
    }

    fn parse_and(&mut self) -> Result<Command> {
        let mut left = self.parse_pipeline()?;

        while self.peek() == Some(&Token::And) {
            self.advance();
            let right = self.parse_pipeline()?;
            left = Command::And(Box::new(left), Box::new(right));
        }

        Ok(left)
    }

    fn parse_pipeline(&mut self) -> Result<Command> {
        let mut commands = vec![self.parse_simple()?];

        while self.peek() == Some(&Token::Pipe) {
            self.advance();
            commands.push(self.parse_simple()?);
        }

        if commands.len() == 1 {
            Ok(Command::Simple(commands.remove(0)))
        } else {
            Ok(Command::Pipeline(commands))
        }
    }

    fn parse_simple(&mut self) -> Result<SimpleCommand> {
        let mut args = Vec::new();
        let mut redirects = Vec::new();
        let mut background = false;

        loop {
            match self.peek() {
                Some(Token::Word(w)) => {
                    args.push(w.clone());
                    self.advance();
                }
                Some(Token::RedirectOut) => {
                    self.advance();
                    if let Some(Token::Word(file)) = self.advance() {
                        redirects.push(Redirect::StdoutFile(file.clone()));
                    }
                }
                Some(Token::RedirectAppend) => {
                    self.advance();
                    if let Some(Token::Word(file)) = self.advance() {
                        redirects.push(Redirect::StdoutAppend(file.clone()));
                    }
                }
                Some(Token::RedirectIn) => {
                    self.advance();
                    if let Some(Token::Word(file)) = self.advance() {
                        redirects.push(Redirect::StdinFile(file.clone()));
                    }
                }
                Some(Token::RedirectErr) => {
                    self.advance();
                    if let Some(Token::Word(file)) = self.advance() {
                        redirects.push(Redirect::StderrFile(file.clone()));
                    }
                }
                Some(Token::RedirectErrAppend) => {
                    self.advance();
                    if let Some(Token::Word(file)) = self.advance() {
                        redirects.push(Redirect::StderrAppend(file.clone()));
                    }
                }
                Some(Token::RedirectAll) => {
                    self.advance();
                    if let Some(Token::Word(file)) = self.advance() {
                        redirects.push(Redirect::StdoutStderr(file.clone()));
                    }
                }
                Some(Token::StderrToStdout) => {
                    self.advance();
                    redirects.push(Redirect::StderrToStdout);
                }
                Some(Token::Background) => {
                    self.advance();
                    background = true;
                    break;
                }
                _ => break,
            }
        }

        if args.is_empty() {
            return Err(anyhow!("Empty command"));
        }

        Ok(SimpleCommand {
            program: args.remove(0),
            args,
            redirects,
            background,
        })
    }
}

/// Glob expansion for wildcards
pub fn expand_glob(pattern: &str) -> Vec<String> {
    match glob::glob(pattern) {
        Ok(paths) => {
            let expanded: Vec<String> = paths
                .filter_map(|p| p.ok())
                .map(|p| p.to_string_lossy().to_string())
                .collect();

            if expanded.is_empty() {
                vec![pattern.to_string()]
            } else {
                expanded
            }
        }
        Err(_) => vec![pattern.to_string()],
    }
}

/// Tilde expansion
pub fn expand_tilde(path: &str) -> String {
    if path.starts_with('~') {
        if let Some(home) = dirs::home_dir() {
            return path.replacen('~', home.to_string_lossy().as_ref(), 1);
        }
    }
    path.to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_tokenize_simple() {
        let tokens = tokenize("ls -la").unwrap();
        assert_eq!(tokens.len(), 2);
    }

    #[test]
    fn test_tokenize_pipe() {
        let tokens = tokenize("cat file | grep foo").unwrap();
        assert!(tokens.contains(&Token::Pipe));
    }

    #[test]
    fn test_tokenize_redirect() {
        let tokens = tokenize("echo hello > out.txt").unwrap();
        assert!(tokens.contains(&Token::RedirectOut));
    }
}
