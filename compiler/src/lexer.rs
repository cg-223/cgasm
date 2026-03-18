use std::str::Chars;

#[derive(Debug)]
pub struct Unit {
    src: String,
    lines: Vec<Line>,
}

#[derive(Debug)]
pub struct Line {
    src: String,
    tokens: Vec<Token>,
}

#[derive(Debug)]
pub struct LineBuilder {
    chrs: Vec<char>,
    cursor: usize,
}

impl LineBuilder {
    fn lex_line(&mut self) -> Vec<Token> {
        let mut tokens = Vec::new();
        while self.cursor < self.chrs.len() {
            match self.next_token() {
                Ok(Token::None) => (),
                Ok(tok) => tokens.push(tok),
                Err(err_msg) => {
                    eprintln!("{err_msg}");
                    break;
                }
            }
        }

        if matches!(tokens.last(), Some(Token::EOF)) {
            tokens.pop();
        }

        tokens
    }

    fn next_token(&mut self) -> Result<Token, String> {
        while let Some(char) = self.advance() {
            match char {
                '#' => {
                    self.cursor = self.chrs.len();
                    return Ok(Token::None);
                }
                '\'' => match self.lex_ident(None) {
                    Some(Ok(strng)) => return Ok(Token::Label(strng)),
                    Some(Err(strng)) => return Ok(Token::Ident(strng)),
                    None => return Err("Malformed identifier".into()),
                },
                '&' => match self.lex_memory() {
                    Ok(Memory::Doubled(int)) if int < i64::MAX as u64 => return Ok(Token::DoubleMemory(int as i64)),
                    Ok(Memory::Integer(int)) if int < i64::MAX as u64  => return Ok(Token::Memory(int as i64)),
                    Ok(Memory::Keyword(kw)) => match kw.as_str() {
                        "cmp" => return Ok(Token::Memory(-1)),
                        _ => return Err(format!("Unrecognized special memory address: {kw}")),
                    },
                    Ok(Memory::Doubled(_) | Memory::Integer(_)) => return Err("Illegal negative memory address".to_string()),
                    Err(e) => return Err(e),
                },
                'a'..='z' | 'A'..='Z' => match self.lex_ident(Some(char)) {
                    Some(Ok(strng)) => return Ok(Token::Ident(strng)),
                    _ => return Err("Malformed identifier".into()),
                },
                '"' => match self.lex_string() {
                    Some(strng) => return Ok(Token::String(strng)),
                    None => return Err("Malformed string".into()),
                },
                '0'..='9' | '-' => match self.lex_number(Some(char)) {
                    Err(strng) => return Err(strng),
                    Ok(Number::Float(flt)) => return Ok(Token::Float(flt)),
                    Ok(Number::Integer(int)) => return Ok(Token::Integer(int)),
                },
                ' ' | '\t' | '\r' | '\n' => {}
                _ => return Err(format!("Unrecognized input: {char}")),
            }
        }
        Ok(Token::EOF)
    }

    fn lex_ident(&mut self, first_char: Option<char>) -> Option<Result<String, String>> {
        let mut strng = String::new();
        if let Some(char) = first_char {
            strng.push(char);
        }
        while let Some(char) = self.advance() {
            match char {
                'a'..='z' | 'A'..='Z' | '_' | '0'..='9' => strng.push(char),
                ' ' | '\t' | '\r' | '\n' => {
                    self.backtrack();
                    break;
                }
                '\'' if strng.len() == 1 => return Some(Err(strng)),
                ':' if first_char.is_some() => return None,
                ':' => return Some(Ok(strng)),
                _ => return None,
            }
        }

        Some(Ok(strng))
    }

    fn lex_string(&mut self) -> Option<String> {
        let mut strng = String::new();
        let mut was_last_backslash = false;
        while let Some(char) = self.advance() {
            match char {
                '\\' => {
                    was_last_backslash = !was_last_backslash;
                    strng.push(char);
                }
                '"' if !was_last_backslash => return Some(strng),
                _ => strng.push(char),
            }
        }

        None
    }

    fn backtrack(&mut self) {
        self.cursor.checked_sub(1).unwrap();
    }

    fn lex_memory(&mut self) -> Result<Memory, String> {
        let mut strng = String::new();

        let mut is_special = false;
        let mut is_number = false;
        let mut is_doubled = false;

        while let Some(char) = self.advance() {
            match char {
                '&' if !is_number && !is_special && !is_doubled => {
                    is_doubled = true;
                }
                'a'..='z' if !is_number && !is_doubled => {
                    is_special = true;
                    strng.push(char);
                }
                '0'..='9' if !is_special => {
                    is_number = true;
                    strng.push(char);
                }
                '_' => {}
                ' ' | '\t' | '\r' | '\n' => {
                    self.backtrack();
                    break;
                }
                _ => return Err(format!("unrecognized char in memory address: {char:?}")),
            }
        }

        if is_number && let Ok(num) = strng.parse::<u64>() {
            if is_doubled {
                Ok(Memory::Doubled(num))
            } else {
                Ok(Memory::Integer(num))
            }
        } else if is_special {
            Ok(Memory::Keyword(strng))
        } else {
            Err(format!("malformed memory address: {strng}"))
        }
    }

    fn lex_number(&mut self, first_char: Option<char>) -> Result<Number, String> {
        let mut strng = String::new();
        if let Some(char) = first_char {
            strng.push(char);
        }

        while let Some(char) = self.advance() {
            match char {
                '0'..='9' | '.' | ',' => strng.push(char),
                '_' => {}
                ' ' | '\t' | '\r' | '\n' => {
                    self.backtrack();
                    break;
                }
                _ => return Err(format!("unrecognized char: {char:?}")),
            }
        }

        if let Ok(num) = strng.parse::<i64>() {
            Ok(Number::Integer(num))
        } else if let Ok(num) = strng.parse::<f64>() {
            Ok(Number::Float(num))
        } else {
            Err(format!("malformed number: {strng}"))
        }
    }

    fn advance(&mut self) -> Option<char> {
        let chr = self.chrs.get(self.cursor).copied();
        self.cursor += 1;
        chr
    }
}

#[derive(Debug)]
enum Number {
    Float(f64),
    Integer(i64),
}

#[derive(Debug)]
enum Memory {
    Doubled(u64),
    Integer(u64),
    Keyword(String),
}

impl Line {

    #[must_use]
    pub fn lex_line(line: &str) -> Self {
        let mut line_builder = LineBuilder {
            chrs: line.chars().collect(),
            cursor: 0,
        };

        let tokens = line_builder.lex_line();

        Self {
            src: line.to_string(),
            tokens,
        }
    }

    #[must_use]
    pub fn src(&self) -> &str {
        &self.src
    }

    #[must_use]
    pub fn tokens(&self) -> &[Token] {
        &self.tokens
    }
}

#[derive(Debug)]
pub enum Token {
    Ident(String),
    Label(String),
    Integer(i64),
    Float(f64),
    Memory(i64),
    DoubleMemory(i64),
    String(String),
    EOF,
    None,
}

pub enum Instruction {}

impl Unit {
    pub fn lex_source(src: impl Into<String>) -> Self {
        let mut ret = Self {
            src: src.into(),
            lines: Vec::default(),
        };

        ret.fully_lex();

        ret.remove_empty_lines();

        ret
    }

    #[must_use]
    pub fn lex_lines(lines: Vec<String>) -> Self {
        let mut src = String::new();
        for line in lines {
            src.push_str(&line);
        }

        Self::lex_source(src)
    }

    fn fully_lex(&mut self) {
        for line in self.src.lines() {
            let line_lexed = Line::lex_line(line);
            self.lines.push(line_lexed);
        }
    }

    fn remove_empty_lines(&mut self) {
        self.lines.retain(|x| !x.tokens.is_empty());
    }

    #[must_use]
    pub fn lines(&self) -> &[Line] {
        &self.lines
    }
}
