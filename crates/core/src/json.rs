use std::collections::BTreeMap;

#[derive(Clone, Debug, PartialEq)]
pub enum JsonValue {
    Null,
    Boolean(bool),
    Number(f64),
    String(String),
    Array(Vec<JsonValue>),
    Object(BTreeMap<String, JsonValue>),
}

impl JsonValue {
    pub fn as_object(&self) -> Option<&BTreeMap<String, JsonValue>> {
        match self {
            Self::Object(value) => Some(value),
            _ => None,
        }
    }

    pub fn as_array(&self) -> Option<&[JsonValue]> {
        match self {
            Self::Array(value) => Some(value),
            _ => None,
        }
    }

    pub fn as_str(&self) -> Option<&str> {
        match self {
            Self::String(value) => Some(value),
            _ => None,
        }
    }

    pub fn as_f64(&self) -> Option<f64> {
        match self {
            Self::Number(value) if value.is_finite() => Some(*value),
            _ => None,
        }
    }

    pub fn get(&self, name: &str) -> Option<&JsonValue> {
        self.as_object()?.get(name)
    }
}

pub fn parse(bytes: &[u8]) -> Result<JsonValue, String> {
    let input = std::str::from_utf8(bytes).map_err(|_| "input is not valid UTF-8".to_owned())?;
    let mut parser = Parser { input, offset: 0 };
    let value = parser.parse_value()?;
    parser.skip_whitespace();
    if parser.offset != input.len() {
        return Err(parser.error("unexpected trailing content"));
    }
    Ok(value)
}

struct Parser<'a> {
    input: &'a str,
    offset: usize,
}

impl Parser<'_> {
    fn parse_value(&mut self) -> Result<JsonValue, String> {
        self.skip_whitespace();
        match self.peek() {
            Some('n') => self.literal("null", JsonValue::Null),
            Some('t') => self.literal("true", JsonValue::Boolean(true)),
            Some('f') => self.literal("false", JsonValue::Boolean(false)),
            Some('"') => self.parse_string().map(JsonValue::String),
            Some('[') => self.parse_array(),
            Some('{') => self.parse_object(),
            Some('-' | '0'..='9') => self.parse_number(),
            Some(_) => Err(self.error("unexpected token")),
            None => Err(self.error("unexpected end of input")),
        }
    }

    fn literal(&mut self, literal: &str, value: JsonValue) -> Result<JsonValue, String> {
        if self.input[self.offset..].starts_with(literal) {
            self.offset += literal.len();
            Ok(value)
        } else {
            Err(self.error("invalid literal"))
        }
    }

    fn parse_string(&mut self) -> Result<String, String> {
        self.expect('"')?;
        let mut value = String::new();
        loop {
            match self.bump() {
                Some('"') => return Ok(value),
                Some('\\') => match self.bump() {
                    Some('"') => value.push('"'),
                    Some('\\') => value.push('\\'),
                    Some('/') => value.push('/'),
                    Some('b') => value.push('\u{08}'),
                    Some('f') => value.push('\u{0c}'),
                    Some('n') => value.push('\n'),
                    Some('r') => value.push('\r'),
                    Some('t') => value.push('\t'),
                    Some('u') => value.push(self.parse_unicode_escape()?),
                    _ => return Err(self.error("invalid string escape")),
                },
                Some(character) if character >= '\u{20}' => value.push(character),
                Some(_) => return Err(self.error("unescaped control character")),
                None => return Err(self.error("unterminated string")),
            }
        }
    }

    fn parse_unicode_escape(&mut self) -> Result<char, String> {
        let first = self.parse_hex_quad()?;
        let scalar = if (0xd800..=0xdbff).contains(&first) {
            if self.bump() != Some('\\') || self.bump() != Some('u') {
                return Err(self.error("missing low surrogate"));
            }
            let second = self.parse_hex_quad()?;
            if !(0xdc00..=0xdfff).contains(&second) {
                return Err(self.error("invalid low surrogate"));
            }
            0x1_0000 + ((first - 0xd800) << 10) + (second - 0xdc00)
        } else if (0xdc00..=0xdfff).contains(&first) {
            return Err(self.error("unexpected low surrogate"));
        } else {
            first
        };
        char::from_u32(scalar).ok_or_else(|| self.error("invalid Unicode scalar"))
    }

    fn parse_hex_quad(&mut self) -> Result<u32, String> {
        let mut value = 0;
        for _ in 0..4 {
            value = value * 16
                + self
                    .bump()
                    .and_then(|character| character.to_digit(16))
                    .ok_or_else(|| self.error("invalid Unicode escape"))?;
        }
        Ok(value)
    }

    fn parse_array(&mut self) -> Result<JsonValue, String> {
        self.expect('[')?;
        let mut values = Vec::new();
        self.skip_whitespace();
        if self.peek() == Some(']') {
            self.bump();
            return Ok(JsonValue::Array(values));
        }
        loop {
            values.push(self.parse_value()?);
            self.skip_whitespace();
            match self.bump() {
                Some(',') => {}
                Some(']') => return Ok(JsonValue::Array(values)),
                _ => return Err(self.error("expected comma or closing bracket")),
            }
        }
    }

    fn parse_object(&mut self) -> Result<JsonValue, String> {
        self.expect('{')?;
        let mut values = BTreeMap::new();
        self.skip_whitespace();
        if self.peek() == Some('}') {
            self.bump();
            return Ok(JsonValue::Object(values));
        }
        loop {
            self.skip_whitespace();
            let key = self.parse_string()?;
            self.skip_whitespace();
            self.expect(':')?;
            let value = self.parse_value()?;
            if values.insert(key, value).is_some() {
                return Err(self.error("duplicate object key"));
            }
            self.skip_whitespace();
            match self.bump() {
                Some(',') => {}
                Some('}') => return Ok(JsonValue::Object(values)),
                _ => return Err(self.error("expected comma or closing brace")),
            }
        }
    }

    fn parse_number(&mut self) -> Result<JsonValue, String> {
        let start = self.offset;
        if self.peek() == Some('-') {
            self.bump();
        }
        match self.peek() {
            Some('0') => {
                self.bump();
                if self.peek().is_some_and(|value| value.is_ascii_digit()) {
                    return Err(self.error("leading zero in number"));
                }
            }
            Some('1'..='9') => {
                while self.peek().is_some_and(|value| value.is_ascii_digit()) {
                    self.bump();
                }
            }
            _ => return Err(self.error("invalid number")),
        }
        if self.peek() == Some('.') {
            self.bump();
            let fraction_start = self.offset;
            while self.peek().is_some_and(|value| value.is_ascii_digit()) {
                self.bump();
            }
            if self.offset == fraction_start {
                return Err(self.error("missing fractional digits"));
            }
        }
        if self.peek().is_some_and(|value| matches!(value, 'e' | 'E')) {
            self.bump();
            if self.peek().is_some_and(|value| matches!(value, '+' | '-')) {
                self.bump();
            }
            let exponent_start = self.offset;
            while self.peek().is_some_and(|value| value.is_ascii_digit()) {
                self.bump();
            }
            if self.offset == exponent_start {
                return Err(self.error("missing exponent digits"));
            }
        }
        let number = self.input[start..self.offset]
            .parse::<f64>()
            .map_err(|_| self.error("number is outside the supported range"))?;
        if !number.is_finite() {
            return Err(self.error("number is outside the supported range"));
        }
        Ok(JsonValue::Number(number))
    }

    fn expect(&mut self, expected: char) -> Result<(), String> {
        if self.bump() == Some(expected) {
            Ok(())
        } else {
            Err(self.error(&format!("expected `{expected}`")))
        }
    }

    fn skip_whitespace(&mut self) {
        while self
            .peek()
            .is_some_and(|value| matches!(value, ' ' | '\n' | '\r' | '\t'))
        {
            self.bump();
        }
    }

    fn peek(&self) -> Option<char> {
        self.input[self.offset..].chars().next()
    }

    fn bump(&mut self) -> Option<char> {
        let value = self.peek()?;
        self.offset += value.len_utf8();
        Some(value)
    }

    fn error(&self, message: &str) -> String {
        format!("{message} at byte {}", self.offset)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_nested_json_and_unicode() {
        let parsed =
            parse(br#"{"name":"Example \u015e","items":[1,true,null]}"#).expect("valid JSON");
        assert_eq!(
            parsed.get("name").and_then(JsonValue::as_str),
            Some("Example Ş")
        );
        assert_eq!(
            parsed
                .get("items")
                .and_then(JsonValue::as_array)
                .map(<[JsonValue]>::len),
            Some(3)
        );
    }

    #[test]
    fn rejects_duplicate_keys_and_invalid_numbers() {
        assert!(parse(br#"{"a":1,"a":2}"#).is_err());
        assert!(parse(b"01").is_err());
    }
}
