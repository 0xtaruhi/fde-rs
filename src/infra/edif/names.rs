use super::{Parser, Token};
use anyhow::Result;

#[derive(Debug, Clone)]
pub(super) struct ParsedName {
    pub(super) display: String,
    pub(super) stable_name: String,
    pub(super) member: Option<ParsedMember>,
}

#[derive(Debug, Clone)]
pub(super) struct ParsedMember {
    pub(super) base_key: String,
    pub(super) ordinal: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct ArrayRange {
    pub(super) msb: usize,
    pub(super) lsb: usize,
}

impl ArrayRange {
    pub(super) fn from_width(width: usize) -> Self {
        Self {
            msb: width.saturating_sub(1),
            lsb: 0,
        }
    }

    pub(super) fn width(self) -> usize {
        self.msb.abs_diff(self.lsb).saturating_add(1)
    }

    pub(super) fn member_name(self, base: &str, ordinal: usize) -> Option<String> {
        if ordinal >= self.width() {
            return None;
        }
        let index = if self.msb >= self.lsb {
            self.msb - ordinal
        } else {
            self.msb + ordinal
        };
        Some(indexed_name(base, index))
    }

    pub(super) fn declaration_names(self, base: &str) -> Vec<String> {
        (0..self.width())
            .filter_map(|ordinal| self.member_name(base, ordinal))
            .collect()
    }
}

#[derive(Debug, Clone)]
pub(super) struct ArraySpec {
    pub(super) display_base: String,
    pub(super) range: ArrayRange,
}

#[derive(Debug, Clone)]
pub(super) struct PortDecl {
    pub(super) names: Vec<String>,
    pub(super) array_key: Option<String>,
    pub(super) array_spec: Option<ArraySpec>,
}

impl Parser<'_> {
    pub(super) fn parse_name_expr(&mut self) -> Result<Option<ParsedName>> {
        match self.peek_token()? {
            Some(Token::Atom(_)) => Ok(self.parse_atom_value()?.map(|value| ParsedName {
                display: value.clone(),
                stable_name: value,
                member: None,
            })),
            Some(Token::LParen) => {
                self.expect_lparen()?;
                let head = self.expect_atom()?;
                let parsed = match head.as_str() {
                    "rename" => {
                        let stable_name = self
                            .parse_name_expr()?
                            .map(|name| name.display)
                            .unwrap_or_default();
                        let display = self
                            .parse_name_expr()?
                            .map(|name| name.display)
                            .unwrap_or_else(|| stable_name.clone());
                        while !self.peek_is_rparen()? {
                            self.skip_value()?;
                        }
                        self.expect_rparen()?;
                        Some(ParsedName {
                            display,
                            stable_name,
                            member: None,
                        })
                    }
                    "array" => {
                        let value = self
                            .parse_name_expr()?
                            .map(|name| name.display)
                            .unwrap_or_default();
                        while !self.peek_is_rparen()? {
                            self.skip_value()?;
                        }
                        self.expect_rparen()?;
                        Some(ParsedName {
                            display: value.clone(),
                            stable_name: value,
                            member: None,
                        })
                    }
                    "member" => {
                        let value = self.parse_name_expr()?.unwrap_or(ParsedName {
                            display: String::new(),
                            stable_name: String::new(),
                            member: None,
                        });
                        let index = self
                            .parse_atom_value()?
                            .and_then(|value| value.parse::<usize>().ok())
                            .unwrap_or(0);
                        while !self.peek_is_rparen()? {
                            self.skip_value()?;
                        }
                        self.expect_rparen()?;
                        let indexed = indexed_name(&value.display, index);
                        Some(ParsedName {
                            display: indexed.clone(),
                            stable_name: indexed,
                            member: Some(ParsedMember {
                                base_key: value.stable_name,
                                ordinal: index,
                            }),
                        })
                    }
                    _ => {
                        self.skip_current_list()?;
                        None
                    }
                };
                Ok(parsed)
            }
            Some(Token::RParen) | None => Ok(None),
        }
    }

    pub(super) fn parse_port_decl_names(&mut self) -> Result<PortDecl> {
        match self.peek_token()? {
            Some(Token::Atom(_)) => Ok(PortDecl {
                names: self
                    .parse_atom_value()?
                    .map(|name| vec![name])
                    .unwrap_or_default(),
                array_key: None,
                array_spec: None,
            }),
            Some(Token::LParen) => {
                self.expect_lparen()?;
                let head = self.expect_atom()?;
                let decl = match head.as_str() {
                    "array" => {
                        let base = self.parse_name_expr()?.unwrap_or(ParsedName {
                            display: String::new(),
                            stable_name: String::new(),
                            member: None,
                        });
                        let width = self
                            .parse_atom_value()?
                            .and_then(|value| value.parse::<usize>().ok())
                            .unwrap_or(1);
                        while !self.peek_is_rparen()? {
                            self.skip_value()?;
                        }
                        self.expect_rparen()?;
                        let (display_base, range) = match parse_bus_range_name(&base.display) {
                            Some((display_base, range)) => {
                                if range.width() != width {
                                    return Err(self.error(format!(
                                        "array size mismatch for '{}' (declared {width}, range width {})",
                                        base.display,
                                        range.width()
                                    )));
                                }
                                (display_base, range)
                            }
                            None => (base.display.clone(), ArrayRange::from_width(width)),
                        };
                        PortDecl {
                            names: range.declaration_names(&display_base),
                            array_key: Some(base.stable_name),
                            array_spec: Some(ArraySpec {
                                display_base,
                                range,
                            }),
                        }
                    }
                    "rename" => {
                        let _ = self.parse_name_expr()?;
                        let display = self
                            .parse_name_expr()?
                            .map(|name| name.display)
                            .unwrap_or_default();
                        while !self.peek_is_rparen()? {
                            self.skip_value()?;
                        }
                        self.expect_rparen()?;
                        PortDecl {
                            names: vec![display],
                            array_key: None,
                            array_spec: None,
                        }
                    }
                    _ => {
                        self.skip_current_list()?;
                        PortDecl {
                            names: Vec::new(),
                            array_key: None,
                            array_spec: None,
                        }
                    }
                };
                Ok(decl)
            }
            Some(Token::RParen) | None => Ok(PortDecl {
                names: Vec::new(),
                array_key: None,
                array_spec: None,
            }),
        }
    }

    pub(super) fn resolve_current_port_member(&self, member: &ParsedMember) -> Option<String> {
        self.current_port_arrays
            .get(&member.base_key)
            .and_then(|spec| spec.range.member_name(&spec.display_base, member.ordinal))
    }

    pub(super) fn parse_atom_value(&mut self) -> Result<Option<String>> {
        match self.next_token()? {
            Some(Token::Atom(value)) => Ok(Some(value)),
            Some(Token::LParen) => {
                self.skip_open_list()?;
                Ok(None)
            }
            Some(Token::RParen) | None => Ok(None),
        }
    }
}

fn indexed_name(base: &str, index: usize) -> String {
    format!("{base}[{index}]")
}

fn parse_bus_range_name(name: &str) -> Option<(String, ArrayRange)> {
    let (open, close) = [('[', ']'), ('(', ')'), ('<', '>')]
        .into_iter()
        .find(|(open, close)| name.ends_with(*close) && name.contains(*open))?;
    let split = name.rfind(open)?;
    let base = name[..split].to_string();
    let range = &name[split + 1..name.len().checked_sub(close.len_utf8())?];
    let (msb, lsb) = range.split_once(':')?;
    Some((
        base,
        ArrayRange {
            msb: msb.parse().ok()?,
            lsb: lsb.parse().ok()?,
        },
    ))
}
