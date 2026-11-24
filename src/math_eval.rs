//! AuraOS v5.0 Instant Spotlight Calculator & Unit Converter (`Alt + Space` / `Win + S`)
//! - Evaluates arithmetic (`+`, `-`, `*`, `/`, `%`, `^`, parentheses, `sqrt`, `pi`, percentages)
//! - Evaluates daily unit conversions (`km`↔`mi`, `m`↔`ft`, `cm`↔`in`, `kg`↔`lb`, `c`↔`f`, `gb`↔`mb`, `tb`↔`gb`)

#[derive(Debug, Clone)]
pub struct CalcResult {
    pub expression: String,
    pub formatted: String,
    pub subtitle: String,
}

pub fn evaluate_query(raw: &str) -> Option<CalcResult> {
    let trimmed = raw.trim().trim_start_matches('=').trim();
    if trimmed.len() < 2 {
        return None;
    }

    // 1. Check unit conversions first (e.g., "32 c to f", "128 gb en mb", "10 km a mi")
    if let Some(conv) = try_unit_conversion(trimmed) {
        return Some(conv);
    }

    // 2. Check percentage phrases like "15% de 480" or "15% of 480"
    let low = trimmed.to_lowercase();
    for sep in ["% de ", "% of "] {
        if let Some(pos) = low.find(sep) {
            let lhs = trimmed[..pos].trim().replace(',', ".");
            let rhs = trimmed[pos + sep.len()..].trim().replace(',', ".");
            if let (Ok(pct), Ok(base)) = (lhs.parse::<f64>(), rhs.parse::<f64>()) {
                let val = (pct / 100.0) * base;
                return Some(CalcResult {
                    expression: trimmed.to_string(),
                    formatted: format_num(val),
                    subtitle: format!("{}% de {} · Pulsa Enter para copiar", format_num(pct), format_num(base)),
                });
            }
        }
    }

    // Must contain at least one digit and one math operator/function so normal app searches ("chrome", "pdf") don't trigger!
    let has_digit = trimmed.chars().any(|c| c.is_ascii_digit());
    let has_op = trimmed.chars().any(|c| matches!(c, '+' | '-' | '*' | '/' | '^' | '%' | '(' | ')'))
        || low.starts_with("sqrt(")
        || low.contains("pi");
    if !has_digit && !low.contains("pi") {
        return None;
    }
    if !has_op {
        return None;
    }
    // Reject paths/URLs/commands
    if trimmed.contains('\\') || trimmed.contains("://") || trimmed.starts_with('>') || trimmed.starts_with('@') {
        return None;
    }

    let normalized = low
        .replace("×", "*")
        .replace("÷", "/")
        .replace("pi", "3.141592653589793");

    let mut parser = ExprParser::new(&normalized);
    let val = parser.parse_expr()?;
    parser.skip_ws();
    if parser.pos < parser.bytes.len() || !val.is_finite() {
        return None;
    }

    // Ignore trivial single number like "-5" unless explicit
    if trimmed.parse::<f64>().is_ok() {
        return None;
    }

    Some(CalcResult {
        expression: trimmed.to_string(),
        formatted: format_num(val),
        subtitle: "Calculadora AuraOS · Pulsa Enter para copiar al portapapeles".into(),
    })
}

fn try_unit_conversion(q: &str) -> Option<CalcResult> {
    let low = q.to_lowercase();
    let cleaned = low
        .replace(" to ", " ")
        .replace(" en ", " ")
        .replace(" a ", " ")
        .replace(" -> ", " ");
    let parts: Vec<&str> = cleaned.split_whitespace().collect();
    if parts.len() != 3 {
        return None;
    }
    let val: f64 = parts[0].replace(',', ".").parse().ok()?;
    let from = parts[1].trim_start_matches('°');
    let to = parts[2].trim_start_matches('°');

    let (out, unit_label) = match (from, to) {
        ("km", "mi") => (val * 0.621_371, "mi"),
        ("mi", "km") => (val * 1.609_344, "km"),
        ("m", "ft") => (val * 3.280_84, "ft"),
        ("ft", "m") => (val / 3.280_84, "m"),
        ("cm", "in") => (val / 2.54, "in"),
        ("in", "cm") => (val * 2.54, "cm"),
        ("kg", "lb") | ("kg", "lbs") => (val * 2.204_622_6, "lb"),
        ("lb", "kg") | ("lbs", "kg") => (val / 2.204_622_6, "kg"),
        ("c", "f") => (val * 9.0 / 5.0 + 32.0, "°F"),
        ("f", "c") => ((val - 32.0) * 5.0 / 9.0, "°C"),
        ("gb", "mb") => (val * 1024.0, "MB"),
        ("mb", "gb") => (val / 1024.0, "GB"),
        ("tb", "gb") => (val * 1024.0, "GB"),
        ("gb", "tb") => (val / 1024.0, "TB"),
        ("px", "rem") => (val / 16.0, "rem"),
        ("rem", "px") => (val * 16.0, "px"),
        _ => return None,
    };

    Some(CalcResult {
        expression: q.to_string(),
        formatted: format!("{} {}", format_num(out), unit_label),
        subtitle: "Conversor de unidades · Pulsa Enter para copiar".into(),
    })
}

pub fn format_num(v: f64) -> String {
    if (v - v.round()).abs() < 1e-9 && v.abs() < 1e15 {
        format!("{}", v.round() as i64)
    } else {
        let s = format!("{:.6}", v);
        s.trim_end_matches('0').trim_end_matches('.').to_string()
    }
}

struct ExprParser<'a> {
    bytes: &'a [u8],
    pos: usize,
}

impl<'a> ExprParser<'a> {
    fn new(s: &'a str) -> Self {
        Self { bytes: s.as_bytes(), pos: 0 }
    }

    fn skip_ws(&mut self) {
        while self.pos < self.bytes.len() && self.bytes[self.pos].is_ascii_whitespace() {
            self.pos += 1;
        }
    }

    fn peek(&mut self) -> Option<u8> {
        self.skip_ws();
        self.bytes.get(self.pos).copied()
    }

    fn parse_expr(&mut self) -> Option<f64> {
        let mut lhs = self.parse_term()?;
        while let Some(op) = self.peek() {
            if op == b'+' || op == b'-' {
                self.pos += 1;
                let rhs = self.parse_term()?;
                // Support `200 + 19%` -> 238!
                if self.peek() == Some(b'%') {
                    self.pos += 1;
                    let delta = lhs * (rhs / 100.0);
                    lhs = if op == b'+' { lhs + delta } else { lhs - delta };
                } else {
                    lhs = if op == b'+' { lhs + rhs } else { lhs - rhs };
                }
            } else {
                break;
            }
        }
        Some(lhs)
    }

    fn parse_term(&mut self) -> Option<f64> {
        let mut lhs = self.parse_power()?;
        while let Some(op) = self.peek() {
            if op == b'*' || op == b'/' || op == b'%' {
                self.pos += 1;
                if op == b'%' {
                    // Could be postfix `%` (divide by 100) if next token is end/operator, or modulo if followed by number!
                    self.skip_ws();
                    let next_is_num = self
                        .bytes
                        .get(self.pos)
                        .map(|c| c.is_ascii_digit() || *c == b'(')
                        .unwrap_or(false);
                    if next_is_num {
                        let rhs = self.parse_power()?;
                        if rhs == 0.0 {
                            return None;
                        }
                        lhs %= rhs;
                    } else {
                        lhs /= 100.0;
                    }
                } else {
                    let rhs = self.parse_power()?;
                    if op == b'/' && rhs == 0.0 {
                        return None;
                    }
                    lhs = if op == b'*' { lhs * rhs } else { lhs / rhs };
                }
            } else {
                break;
            }
        }
        Some(lhs)
    }

    fn parse_power(&mut self) -> Option<f64> {
        let base = self.parse_unary()?;
        if self.peek() == Some(b'^') {
            self.pos += 1;
            let exp = self.parse_power()?;
            Some(base.powf(exp))
        } else {
            Some(base)
        }
    }

    fn parse_unary(&mut self) -> Option<f64> {
        match self.peek()? {
            b'+' => {
                self.pos += 1;
                self.parse_unary()
            }
            b'-' => {
                self.pos += 1;
                Some(-self.parse_unary()?)
            }
            _ => self.parse_primary(),
        }
    }

    fn parse_primary(&mut self) -> Option<f64> {
        self.skip_ws();
        if self.pos >= self.bytes.len() {
            return None;
        }
        if self.bytes[self.pos..].starts_with(b"sqrt(") {
            self.pos += 5;
            let v = self.parse_expr()?;
            if self.peek() != Some(b')') || v < 0.0 {
                return None;
            }
            self.pos += 1;
            return Some(v.sqrt());
        }
        if self.bytes[self.pos] == b'(' {
            self.pos += 1;
            let v = self.parse_expr()?;
            if self.peek() != Some(b')') {
                return None;
            }
            self.pos += 1;
            return Some(v);
        }

        let start = self.pos;
        let mut saw_dot = false;
        while self.pos < self.bytes.len() {
            let c = self.bytes[self.pos];
            if c.is_ascii_digit() {
                self.pos += 1;
            } else if (c == b'.' || c == b',') && !saw_dot {
                saw_dot = true;
                self.pos += 1;
            } else {
                break;
            }
        }
        if self.pos == start {
            return None;
        }
        let s = std::str::from_utf8(&self.bytes[start..self.pos]).ok()?.replace(',', ".");
        s.parse::<f64>().ok()
    }
}
