use super::Parser;
use anyhow::{Result, anyhow};

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum Token {
    LParen,
    RParen,
    Atom(String),
}

impl Parser<'_> {
    pub(super) fn expect_head(&mut self, expected: &str) -> Result<()> {
        let head = self.expect_atom()?;
        if head == expected {
            Ok(())
        } else {
            Err(self.error(format!("expected head '{expected}', found '{head}'")))
        }
    }

    pub(super) fn expect_lparen(&mut self) -> Result<()> {
        match self.next_token()? {
            Some(Token::LParen) => Ok(()),
            Some(Token::Atom(value)) => Err(self.error(format!("expected '(', found '{value}'"))),
            Some(Token::RParen) => Err(self.error("expected '(', found ')'")),
            None => Err(self.error("unexpected end of EDIF input")),
        }
    }

    pub(super) fn expect_rparen(&mut self) -> Result<()> {
        match self.next_token()? {
            Some(Token::RParen) => Ok(()),
            Some(Token::Atom(value)) => Err(self.error(format!("expected ')', found '{value}'"))),
            Some(Token::LParen) => Err(self.error("expected ')', found '('")),
            None => Err(self.error("unexpected end of EDIF input")),
        }
    }

    pub(super) fn expect_atom(&mut self) -> Result<String> {
        match self.next_token()? {
            Some(Token::Atom(value)) => Ok(value),
            Some(Token::LParen) => Err(self.error("expected atom, found '('")),
            Some(Token::RParen) => Err(self.error("expected atom, found ')'")),
            None => Err(self.error("unexpected end of EDIF input")),
        }
    }

    pub(super) fn peek_is_lparen(&mut self) -> Result<bool> {
        Ok(matches!(self.peek_token()?, Some(Token::LParen)))
    }

    pub(super) fn peek_is_rparen(&mut self) -> Result<bool> {
        Ok(matches!(self.peek_token()?, Some(Token::RParen)))
    }

    pub(super) fn skip_value(&mut self) -> Result<()> {
        match self.next_token()? {
            Some(Token::LParen) => self.skip_open_list(),
            Some(Token::Atom(_)) => Ok(()),
            Some(Token::RParen) => Err(self.error("unexpected ')' in EDIF input")),
            None => Err(self.error("unexpected end of EDIF input")),
        }
    }

    pub(super) fn skip_current_list(&mut self) -> Result<()> {
        let mut depth = 1usize;
        while depth > 0 {
            match self.next_token()? {
                Some(Token::LParen) => depth += 1,
                Some(Token::RParen) => depth = depth.saturating_sub(1),
                Some(Token::Atom(_)) => {}
                None => return Err(self.error("unterminated EDIF list")),
            }
        }
        Ok(())
    }

    pub(super) fn skip_open_list(&mut self) -> Result<()> {
        let mut depth = 1usize;
        while depth > 0 {
            match self.next_token()? {
                Some(Token::LParen) => depth += 1,
                Some(Token::RParen) => depth = depth.saturating_sub(1),
                Some(Token::Atom(_)) => {}
                None => return Err(self.error("unterminated EDIF list")),
            }
        }
        Ok(())
    }

    pub(super) fn peek_token(&mut self) -> Result<Option<Token>> {
        if self.peeked.is_none() {
            self.peeked = self.read_token()?;
        }
        Ok(self.peeked.clone())
    }

    pub(super) fn next_token(&mut self) -> Result<Option<Token>> {
        if let Some(token) = self.peeked.take() {
            return Ok(Some(token));
        }
        self.read_token()
    }

    fn read_token(&mut self) -> Result<Option<Token>> {
        self.skip_whitespace_and_comments();
        let Some(ch) = self.peek_char() else {
            return Ok(None);
        };
        let token = match ch {
            '(' => {
                self.bump_char();
                Token::LParen
            }
            ')' => {
                self.bump_char();
                Token::RParen
            }
            '"' => Token::Atom(self.read_quoted_string()?),
            _ => Token::Atom(self.read_atom()),
        };
        Ok(Some(token))
    }

    fn skip_whitespace_and_comments(&mut self) {
        loop {
            while self.peek_char().is_some_and(char::is_whitespace) {
                self.bump_char();
            }
            if self.peek_char() == Some(';') {
                while let Some(ch) = self.bump_char() {
                    if ch == '\n' {
                        break;
                    }
                }
            } else {
                break;
            }
        }
    }

    fn read_quoted_string(&mut self) -> Result<String> {
        match self.bump_char() {
            Some('"') => {}
            _ => return Err(self.error("expected string literal")),
        }
        let mut value = String::new();
        while let Some(ch) = self.bump_char() {
            match ch {
                '"' => return Ok(value),
                '\\' => {
                    let escaped = self
                        .bump_char()
                        .ok_or_else(|| self.error("unterminated escape sequence"))?;
                    value.push(escaped);
                }
                other => value.push(other),
            }
        }
        Err(self.error("unterminated string literal"))
    }

    fn read_atom(&mut self) -> String {
        let start = self.cursor;
        while let Some(ch) = self.peek_char() {
            if ch.is_whitespace() || matches!(ch, '(' | ')') {
                break;
            }
            self.bump_char();
        }
        self.source[start..self.cursor].to_string()
    }

    fn peek_char(&self) -> Option<char> {
        self.source[self.cursor..].chars().next()
    }

    fn bump_char(&mut self) -> Option<char> {
        let ch = self.peek_char()?;
        self.cursor += ch.len_utf8();
        Some(ch)
    }

    pub(super) fn error(&self, message: impl Into<String>) -> anyhow::Error {
        anyhow!("{} at byte {}", message.into(), self.cursor)
    }
}
