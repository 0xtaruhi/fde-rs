use crate::cil::SiteFunction;
use std::iter::Peekable;
use std::str::Chars;

pub(crate) fn address_count(function: &SiteFunction) -> Option<usize> {
    function
        .srams
        .iter()
        .filter_map(|sram| sram.address.map(|address| address as usize))
        .max()
        .map(|max| max + 1)
}

pub(crate) fn evaluate_equation(raw: &str, width: usize) -> Option<Vec<u8>> {
    let owned = normalize_equation_value(raw);
    let value = owned.as_str();
    if value == "0" {
        return Some(vec![0; width]);
    }
    if value == "1" {
        return Some(vec![1; width]);
    }
    if let Some(bits) = parse_bit_literal(value, width) {
        return Some(bits);
    }

    let mut parser = EquationParser::new(value);
    let expr = parser.parse_expression()?;
    parser.finish()?;
    Some(
        (0..width)
            .map(|index| expr.evaluate(index as u32) as u8)
            .collect(),
    )
}

pub(crate) fn lut_hex_to_equation(raw: &str, input_hint: usize) -> Option<String> {
    let normalized = normalize_lut_hex_digits(raw)?;
    let input_index = lut_equation_input_index(normalized.len(), input_hint)?;
    let mut expression = String::new();
    let mut minterm = 1usize;

    for digit in normalized.chars() {
        let value = digit.to_digit(16)? as usize;
        let mut mask = if input_index == 0 { 0b10 } else { 0b1000 };
        while mask != 0 {
            if value & mask != 0 {
                expression.push('+');
                expression.push_str(lut_equation_item(input_index, minterm)?);
            }
            mask >>= 1;
            minterm += 1;
        }
    }

    if expression.is_empty() {
        return Some(lut_equation_item(input_index, 0)?.to_string());
    }
    Some(expression.trim_start_matches('+').to_string())
}

pub(crate) fn parse_bit_literal(raw: &str, width: usize) -> Option<Vec<u8>> {
    let raw = raw.trim().replace('_', "");
    if raw.is_empty() {
        return None;
    }
    if let Some((_, value)) = raw.split_once('\'') {
        return parse_verilog_literal(value, width);
    }

    let parsed = if let Some(value) = raw.strip_prefix("0x").or_else(|| raw.strip_prefix("0X")) {
        u128::from_str_radix(value, 16).ok()?
    } else if let Some(value) = raw.strip_prefix("0b").or_else(|| raw.strip_prefix("0B")) {
        u128::from_str_radix(value, 2).ok()?
    } else {
        raw.parse::<u128>().ok()?
    };
    Some(
        (0..width)
            .map(|index| ((parsed >> index) & 1) as u8)
            .collect(),
    )
}

pub(crate) fn parse_verilog_literal(raw: &str, width: usize) -> Option<Vec<u8>> {
    let mut chars = raw.chars();
    let radix = chars.next()?.to_ascii_lowercase();
    let digits = chars.as_str();
    let parsed = match radix {
        'h' => u128::from_str_radix(digits, 16).ok()?,
        'b' => u128::from_str_radix(digits, 2).ok()?,
        'd' => digits.parse::<u128>().ok()?,
        _ => return None,
    };
    Some(
        (0..width)
            .map(|index| ((parsed >> index) & 1) as u8)
            .collect(),
    )
}

fn normalize_equation_value(raw: &str) -> String {
    let value = raw.trim();
    let value = value.strip_prefix("#LUT:").unwrap_or(value);
    let value = value.strip_prefix("#RAM:").unwrap_or(value);
    value.strip_prefix("D=").unwrap_or(value).trim().to_string()
}

fn normalize_lut_hex_digits(raw: &str) -> Option<String> {
    let value = raw.trim().replace('_', "");
    if value.is_empty() {
        return None;
    }
    if value.starts_with("#LUT:") {
        return Some(normalize_equation_value(&value));
    }
    if let Some((_, literal)) = value.split_once('\'') {
        let mut chars = literal.chars();
        let radix = chars.next()?.to_ascii_lowercase();
        let digits = chars.as_str();
        let normalized = match radix {
            'h' => digits.to_string(),
            'b' => format!("{:X}", u128::from_str_radix(digits, 2).ok()?),
            'd' => format!("{:X}", digits.parse::<u128>().ok()?),
            _ => return None,
        };
        return Some(normalized);
    }
    Some(
        value
            .strip_prefix("0x")
            .or_else(|| value.strip_prefix("0X"))
            .unwrap_or(&value)
            .to_ascii_uppercase(),
    )
}

fn lut_equation_input_index(hex_len: usize, input_hint: usize) -> Option<usize> {
    const MAX_INPUTS: usize = 4;
    let input_index = if input_hint <= 1 {
        0
    } else {
        let mut index = 1usize;
        let mut span = 1usize;
        while span < hex_len {
            index += 1;
            span *= 2;
        }
        index
    };
    (input_index < MAX_INPUTS).then_some(input_index)
}

fn lut_equation_item(input_index: usize, minterm: usize) -> Option<&'static str> {
    const ITEMS: &[&[&str]] = &[
        &["~A1*A1", "A1", "~A1"],
        &[
            "(~A1*A1)+(~A2*A2)",
            "(A2*A1)",
            "(A2*~A1)",
            "(~A2*A1)",
            "(~A2*~A1)",
        ],
        &[
            "(~A1*A1)+(~A2*A2)+(~A3*A3)",
            "((A3*A2)*A1)",
            "((A3*A2)*~A1)",
            "((A3*~A2)*A1)",
            "((A3*~A2)*~A1)",
            "((~A3*A2)*A1)",
            "((~A3*A2)*~A1)",
            "((~A3*~A2)*A1)",
            "((~A3*~A2)*~A1)",
        ],
        &[
            "(~A1*A1)+(~A2*A2)+(~A3*A3)+(~A4*A4)",
            "(((A4*A3)*A2)*A1)",
            "(((A4*A3)*A2)*~A1)",
            "(((A4*A3)*~A2)*A1)",
            "(((A4*A3)*~A2)*~A1)",
            "(((A4*~A3)*A2)*A1)",
            "(((A4*~A3)*A2)*~A1)",
            "(((A4*~A3)*~A2)*A1)",
            "(((A4*~A3)*~A2)*~A1)",
            "(((~A4*A3)*A2)*A1)",
            "(((~A4*A3)*A2)*~A1)",
            "(((~A4*A3)*~A2)*A1)",
            "(((~A4*A3)*~A2)*~A1)",
            "(((~A4*~A3)*A2)*A1)",
            "(((~A4*~A3)*A2)*~A1)",
            "(((~A4*~A3)*~A2)*A1)",
            "(((~A4*~A3)*~A2)*~A1)",
        ],
    ];
    ITEMS
        .get(input_index)
        .and_then(|items| items.get(minterm))
        .copied()
}

#[derive(Debug, Clone)]
enum EquationExpr {
    Constant(bool),
    Variable(u32),
    Not(Box<EquationExpr>),
    And(Box<EquationExpr>, Box<EquationExpr>),
    Or(Box<EquationExpr>, Box<EquationExpr>),
}

impl EquationExpr {
    fn evaluate(&self, address: u32) -> bool {
        match self {
            Self::Constant(value) => *value,
            Self::Variable(index) => ((address >> (index - 1)) & 1) != 0,
            Self::Not(inner) => !inner.evaluate(address),
            Self::And(lhs, rhs) => lhs.evaluate(address) && rhs.evaluate(address),
            Self::Or(lhs, rhs) => lhs.evaluate(address) || rhs.evaluate(address),
        }
    }
}

struct EquationParser<'a> {
    chars: Peekable<Chars<'a>>,
}

impl<'a> EquationParser<'a> {
    fn new(input: &'a str) -> Self {
        Self {
            chars: input.chars().peekable(),
        }
    }

    fn parse_expression(&mut self) -> Option<EquationExpr> {
        let mut expr = self.parse_product()?;
        loop {
            self.skip_whitespace();
            if !self.consume('+') {
                return Some(expr);
            }
            let rhs = self.parse_product()?;
            expr = EquationExpr::Or(Box::new(expr), Box::new(rhs));
        }
    }

    fn parse_product(&mut self) -> Option<EquationExpr> {
        let mut expr = self.parse_unary()?;
        loop {
            self.skip_whitespace();
            if !self.consume('*') {
                return Some(expr);
            }
            let rhs = self.parse_unary()?;
            expr = EquationExpr::And(Box::new(expr), Box::new(rhs));
        }
    }

    fn parse_unary(&mut self) -> Option<EquationExpr> {
        self.skip_whitespace();
        if self.consume('~') {
            return Some(EquationExpr::Not(Box::new(self.parse_unary()?)));
        }
        self.parse_atom()
    }

    fn parse_atom(&mut self) -> Option<EquationExpr> {
        self.skip_whitespace();
        if self.consume('(') {
            let expr = self.parse_expression()?;
            self.skip_whitespace();
            self.consume(')').then_some(expr)
        } else if self.consume('0') {
            Some(EquationExpr::Constant(false))
        } else if self.consume('1') {
            Some(EquationExpr::Constant(true))
        } else {
            self.parse_variable()
        }
    }

    fn parse_variable(&mut self) -> Option<EquationExpr> {
        self.skip_whitespace();
        self.consume('A').then_some(())?;
        let mut digits = String::new();
        while let Some(ch) = self.chars.peek().copied() {
            if !ch.is_ascii_digit() {
                break;
            }
            digits.push(ch);
            self.chars.next();
        }
        let index = digits.parse::<u32>().ok()?;
        if index == 0 {
            return None;
        }
        Some(EquationExpr::Variable(index))
    }

    fn finish(&mut self) -> Option<()> {
        self.skip_whitespace();
        self.chars.peek().is_none().then_some(())
    }

    fn consume(&mut self, expected: char) -> bool {
        matches!(self.chars.peek(), Some(ch) if *ch == expected)
            .then(|| self.chars.next())
            .is_some()
    }

    fn skip_whitespace(&mut self) {
        while matches!(self.chars.peek(), Some(ch) if ch.is_ascii_whitespace()) {
            self.chars.next();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{evaluate_equation, lut_hex_to_equation};

    #[test]
    fn converts_lut_hex_into_cpp_style_equation() {
        assert_eq!(
            lut_hex_to_equation("10", 2),
            Some("((A3*~A2)*~A1)".to_string())
        );
        assert_eq!(
            lut_hex_to_equation("0", 4),
            Some("(~A1*A1)+(~A2*A2)".to_string())
        );
    }

    #[test]
    fn evaluates_cpp_style_lut_equations() {
        let bits = evaluate_equation("#LUT:D=((A3*~A2)*~A1)", 16).expect("equation bits");
        let ones = bits
            .iter()
            .enumerate()
            .filter_map(|(index, bit)| (*bit != 0).then_some(index))
            .collect::<Vec<_>>();
        assert_eq!(ones, vec![4, 12]);
        assert_eq!(evaluate_equation("#LUT:D=0", 8), Some(vec![0; 8]));
    }
}
