//! Source text to tokens, and the preprocessor (`#define`, `#if` and its kin).

use crate::{Error, Limits};
use std::collections::HashMap;
use std::rc::Rc;

#[derive(Debug, Clone, PartialEq)]
pub(crate) enum Tok {
    Ident(String),
    Int(i32),
    Float(f32),
    /// Punctuation.
    P(&'static str),
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Token {
    pub tok: Tok,
    pub line: u32,
    /// First on its line: a `#` here starts a directive, which runs to the next such token.
    pub bol: bool,
    /// Whether space came before it (`#define f (x)` takes no arguments).
    pub space: bool,
}

/// Longest first, so `<<=` isn't read as `<` `<=`.
const PUNCTUATION: &[&str] = &[
    "<<=", ">>=", "++", "--", "<=", ">=", "==", "!=", "&&", "||", "^^", "+=", "-=", "*=", "/=", "%=", "&=",
    "|=", "^=", "<<", ">>", "(", ")", "[", "]", "{", "}", ".", ",", ";", ":", "?", "+", "-", "*", "/", "%",
    "<", ">", "=", "!", "~", "&", "|", "^", "#",
];

fn is_ident_start(c: u8) -> bool {
    c.is_ascii_alphabetic() || c == b'_'
}

fn is_ident(c: u8) -> bool {
    c.is_ascii_alphanumeric() || c == b'_'
}

/// Reads a number starting at `at`: the token and where it ends.
fn number(src: &[u8], at: usize, line: u32) -> Result<(Tok, usize), Error> {
    let bad = || Error::new(line, "a number that can't be read");
    let mut i = at;
    if src[i] == b'0' && matches!(src.get(i + 1), Some(b'x' | b'X')) {
        i += 2;
        let start = i;
        while src.get(i).is_some_and(u8::is_ascii_hexdigit) {
            i += 1;
        }
        let digits = std::str::from_utf8(&src[start..i]).map_err(|_| bad())?;
        let value = u64::from_str_radix(digits, 16).map_err(|_| bad())?;
        if matches!(src.get(i), Some(b'u' | b'U')) {
            i += 1;
        }
        return Ok((Tok::Int(value as u32 as i32), i));
    }
    while src.get(i).is_some_and(u8::is_ascii_digit) {
        i += 1;
    }
    let mut float = false;
    if src.get(i) == Some(&b'.') {
        float = true;
        i += 1;
        while src.get(i).is_some_and(u8::is_ascii_digit) {
            i += 1;
        }
    }
    if matches!(src.get(i), Some(b'e' | b'E')) {
        let mut j = i + 1;
        if matches!(src.get(j), Some(b'+' | b'-')) {
            j += 1;
        }
        if src.get(j).is_some_and(u8::is_ascii_digit) {
            float = true;
            while src.get(j).is_some_and(u8::is_ascii_digit) {
                j += 1;
            }
            i = j;
        }
    }
    let digits = std::str::from_utf8(&src[at..i]).map_err(|_| bad())?;
    if float {
        let value: f32 = digits.parse().map_err(|_| bad())?;
        // `1.0f` and `1.0lf`, as desktop GLSL writes them.
        if matches!(src.get(i), Some(b'l' | b'L')) && matches!(src.get(i + 1), Some(b'f' | b'F')) {
            i += 2;
        } else if matches!(src.get(i), Some(b'f' | b'F')) {
            i += 1;
        }
        return Ok((Tok::Float(value), i));
    }
    let value = if digits.len() > 1 && digits.starts_with('0') {
        u64::from_str_radix(digits, 8).map_err(|_| bad())?
    } else {
        digits.parse::<u64>().map_err(|_| bad())?
    };
    if matches!(src.get(i), Some(b'u' | b'U')) {
        i += 1;
    }
    Ok((Tok::Int(value as u32 as i32), i))
}

/// The tokens of `source`, comments left out.
pub(crate) fn lex(source: &str, limits: &Limits) -> Result<Vec<Token>, Error> {
    let src = source.as_bytes();
    let mut out = Vec::new();
    let (mut i, mut line, mut bol, mut space) = (0usize, 1u32, true, false);
    while i < src.len() {
        let c = src[i];
        match c {
            b'\n' => {
                line += 1;
                bol = true;
                space = true;
                i += 1;
            }
            b' ' | b'\t' | b'\r' | 0x0c => {
                space = true;
                i += 1;
            }
            // A line carried on to the next.
            b'\\' if matches!(src.get(i + 1), Some(b'\n')) => {
                line += 1;
                space = true;
                i += 2;
            }
            b'\\' if src.get(i + 1) == Some(&b'\r') && src.get(i + 2) == Some(&b'\n') => {
                line += 1;
                space = true;
                i += 3;
            }
            b'/' if src.get(i + 1) == Some(&b'/') => {
                while i < src.len() && src[i] != b'\n' {
                    i += 1;
                }
            }
            b'/' if src.get(i + 1) == Some(&b'*') => {
                let start = line;
                i += 2;
                loop {
                    match src.get(i) {
                        None => return Err(Error::new(start, "a comment that never ends")),
                        Some(b'*') if src.get(i + 1) == Some(&b'/') => {
                            i += 2;
                            break;
                        }
                        Some(b'\n') => {
                            line += 1;
                            i += 1;
                        }
                        Some(_) => i += 1,
                    }
                }
                space = true;
            }
            _ => {
                let (tok, end) = if is_ident_start(c) {
                    let mut j = i + 1;
                    while src.get(j).is_some_and(|&c| is_ident(c)) {
                        j += 1;
                    }
                    (Tok::Ident(source[i..j].to_string()), j)
                } else if c.is_ascii_digit() || (c == b'.' && src.get(i + 1).is_some_and(u8::is_ascii_digit))
                {
                    number(src, i, line)?
                } else {
                    let found = PUNCTUATION.iter().find(|p| src[i..].starts_with(p.as_bytes()));
                    match found {
                        Some(p) => (Tok::P(p), i + p.len()),
                        None => return Err(Error::new(line, "a character GLSL doesn't use")),
                    }
                };
                out.push(Token {
                    tok,
                    line,
                    bol,
                    space,
                });
                if out.len() > limits.tokens {
                    return Err(Error::new(line, "the shader is too long"));
                }
                bol = false;
                space = false;
                i = end;
            }
        }
    }
    Ok(out)
}

struct Macro {
    /// Its arguments' names; `None` for a macro written without parentheses.
    params: Option<Vec<String>>,
    body: Vec<Token>,
}

/// One `#if` … `#endif` being read.
struct Cond {
    /// Whether the lines around it are being read.
    outer: bool,
    /// Whether one of its branches has been read already.
    taken: bool,
    /// Whether the branch being read is the live one.
    live: bool,
    seen_else: bool,
}

struct Preprocessor<'a> {
    limits: &'a Limits,
    macros: HashMap<String, Rc<Macro>>,
    /// Tokens written while expanding, in all: what stops macros that double each other.
    spent: usize,
}

fn ident(token: Option<&Token>) -> Option<&str> {
    match token.map(|t| &t.tok) {
        Some(Tok::Ident(name)) => Some(name),
        _ => None,
    }
}

fn is_p(token: Option<&Token>, p: &str) -> bool {
    matches!(token.map(|t| &t.tok), Some(Tok::P(q)) if *q == p)
}

impl Preprocessor<'_> {
    fn spend(&mut self, line: u32, tokens: usize) -> Result<(), Error> {
        self.spent = self.spent.saturating_add(tokens);
        if self.spent > self.limits.tokens.saturating_mul(4) {
            return Err(Error::new(line, "its macros expand without end"));
        }
        Ok(())
    }

    /// Expands the macros in `input` onto `out`. `hidden` names the macros being expanded, which
    /// aren't expanded again inside themselves.
    fn expand(
        &mut self,
        input: &[Token],
        hidden: &mut Vec<String>,
        out: &mut Vec<Token>,
        depth: u32,
    ) -> Result<(), Error> {
        let mut i = 0;
        while i < input.len() {
            let token = &input[i];
            let found = match &token.tok {
                Tok::Ident(name) if !hidden.contains(name) => self.macros.get(name).cloned(),
                _ => None,
            };
            let Some(found) = found else {
                self.spend(token.line, 1)?;
                out.push(token.clone());
                i += 1;
                continue;
            };
            let Tok::Ident(name) = &token.tok else {
                unreachable!("only names are macros");
            };
            if depth >= self.limits.depth {
                return Err(Error::new(token.line, "its macros nest too deeply"));
            }
            let body = match &found.params {
                None => {
                    i += 1;
                    found.body.clone()
                }
                // Named without arguments: left as it is.
                Some(_) if !is_p(input.get(i + 1), "(") => {
                    self.spend(token.line, 1)?;
                    out.push(token.clone());
                    i += 1;
                    continue;
                }
                Some(params) => {
                    let (args, after) = arguments(input, i + 1)?;
                    i = after;
                    let none = params.is_empty() && args.len() == 1 && args[0].is_empty();
                    if !none && args.len() != params.len() {
                        return Err(Error::new(
                            token.line,
                            format!("macro {name} takes {} arguments", params.len()),
                        ));
                    }
                    let mut expanded = Vec::with_capacity(args.len());
                    for arg in &args {
                        let mut one = Vec::new();
                        self.expand(arg, hidden, &mut one, depth + 1)?;
                        expanded.push(one);
                    }
                    let mut body = Vec::new();
                    for part in &found.body {
                        let place = ident(Some(part)).and_then(|p| params.iter().position(|q| q == p));
                        match place {
                            Some(k) if !none => {
                                self.spend(token.line, expanded[k].len())?;
                                body.extend(expanded[k].iter().cloned());
                            }
                            _ => body.push(part.clone()),
                        }
                    }
                    body
                }
            };
            self.spend(token.line, body.len())?;
            let body: Vec<Token> = body
                .into_iter()
                .map(|t| Token {
                    line: token.line,
                    bol: false,
                    ..t
                })
                .collect();
            hidden.push(name.clone());
            let done = self.expand(&body, hidden, out, depth + 1);
            hidden.pop();
            done?;
        }
        Ok(())
    }

    /// Whether an `#if` line's expression is true.
    fn condition(&mut self, line: &[Token], at: u32) -> Result<bool, Error> {
        // `defined NAME` and `defined(NAME)` first: they ask about a macro, they don't use it.
        let mut plain = Vec::new();
        let mut i = 0;
        while i < line.len() {
            if ident(line.get(i)) == Some("defined") {
                let (name, next) = if is_p(line.get(i + 1), "(") {
                    (ident(line.get(i + 2)), i + 4)
                } else {
                    (ident(line.get(i + 1)), i + 2)
                };
                let name = name.ok_or_else(|| Error::new(at, "`defined` of nothing"))?;
                plain.push(Token {
                    tok: Tok::Int(i32::from(self.macros.contains_key(name))),
                    line: at,
                    bol: false,
                    space: true,
                });
                i = next;
            } else {
                plain.push(line[i].clone());
                i += 1;
            }
        }
        let mut expanded = Vec::new();
        self.expand(&plain, &mut Vec::new(), &mut expanded, 0)?;
        let mut reader = IfReader {
            tokens: &expanded,
            at: 0,
            line: at,
            depth: 0,
            limit: self.limits.depth,
        };
        let value = reader.or()?;
        if reader.at != expanded.len() {
            return Err(Error::new(at, "an #if that can't be read"));
        }
        Ok(value != 0)
    }
}

/// The arguments of a macro call whose `(` is at `open`: each one's tokens, and the place after
/// the closing `)`.
fn arguments(input: &[Token], open: usize) -> Result<(Vec<Vec<Token>>, usize), Error> {
    let line = input[open].line;
    let mut args = vec![Vec::new()];
    let mut nesting = 0u32;
    let mut i = open + 1;
    loop {
        let Some(token) = input.get(i) else {
            return Err(Error::new(line, "a macro's arguments never end"));
        };
        match &token.tok {
            Tok::P(")") if nesting == 0 => return Ok((args, i + 1)),
            Tok::P(",") if nesting == 0 => args.push(Vec::new()),
            tok => {
                match tok {
                    Tok::P("(") => nesting += 1,
                    Tok::P(")") => nesting -= 1,
                    _ => {}
                }
                if let Some(last) = args.last_mut() {
                    last.push(token.clone());
                }
            }
        }
        i += 1;
    }
}

/// Reads the whole-number expression of an `#if`.
struct IfReader<'a> {
    tokens: &'a [Token],
    at: usize,
    line: u32,
    depth: u32,
    limit: u32,
}

impl IfReader<'_> {
    fn eat(&mut self, p: &str) -> bool {
        let found = is_p(self.tokens.get(self.at), p);
        self.at += usize::from(found);
        found
    }

    fn or(&mut self) -> Result<i64, Error> {
        let mut value = self.and()?;
        while self.eat("||") {
            let right = self.and()?;
            value = i64::from(value != 0 || right != 0);
        }
        Ok(value)
    }

    fn and(&mut self) -> Result<i64, Error> {
        let mut value = self.compare()?;
        while self.eat("&&") {
            let right = self.compare()?;
            value = i64::from(value != 0 && right != 0);
        }
        Ok(value)
    }

    fn compare(&mut self) -> Result<i64, Error> {
        let left = self.sum()?;
        for (p, test) in [
            ("==", (|a, b| a == b) as fn(i64, i64) -> bool),
            ("!=", |a, b| a != b),
            ("<=", |a, b| a <= b),
            (">=", |a, b| a >= b),
            ("<", |a, b| a < b),
            (">", |a, b| a > b),
        ] {
            if self.eat(p) {
                let right = self.sum()?;
                return Ok(i64::from(test(left, right)));
            }
        }
        Ok(left)
    }

    fn sum(&mut self) -> Result<i64, Error> {
        let mut value = self.product()?;
        loop {
            if self.eat("+") {
                value = value.wrapping_add(self.product()?);
            } else if self.eat("-") {
                value = value.wrapping_sub(self.product()?);
            } else {
                return Ok(value);
            }
        }
    }

    fn product(&mut self) -> Result<i64, Error> {
        let mut value = self.unary()?;
        loop {
            if self.eat("*") {
                value = value.wrapping_mul(self.unary()?);
            } else if self.eat("/") {
                value = value.checked_div(self.unary()?).unwrap_or(0);
            } else if self.eat("%") {
                value = value.checked_rem(self.unary()?).unwrap_or(0);
            } else {
                return Ok(value);
            }
        }
    }

    fn unary(&mut self) -> Result<i64, Error> {
        self.depth += 1;
        if self.depth > self.limit {
            return Err(Error::new(self.line, "an #if nested too deeply"));
        }
        let value = if self.eat("!") {
            i64::from(self.unary()? == 0)
        } else if self.eat("-") {
            self.unary()?.wrapping_neg()
        } else if self.eat("+") {
            self.unary()?
        } else if self.eat("(") {
            let inner = self.or()?;
            if !self.eat(")") {
                return Err(Error::new(self.line, "an #if that can't be read"));
            }
            inner
        } else {
            let value = match self.tokens.get(self.at).map(|t| &t.tok) {
                Some(Tok::Int(n)) => i64::from(*n),
                // A name that is no macro counts as 0.
                Some(Tok::Ident(_)) => 0,
                _ => return Err(Error::new(self.line, "an #if that can't be read")),
            };
            self.at += 1;
            value
        };
        self.depth -= 1;
        Ok(value)
    }
}

/// Carries out the directives in `tokens` and expands its macros.
pub(crate) fn preprocess(tokens: &[Token], limits: &Limits) -> Result<Vec<Token>, Error> {
    let mut pp = Preprocessor {
        limits,
        macros: HashMap::new(),
        spent: 0,
    };
    let mut out = Vec::new();
    let mut conds: Vec<Cond> = Vec::new();
    let mut i = 0;
    while i < tokens.len() {
        let reading = conds.last().is_none_or(|c| c.outer && c.live);
        // To the end of this line, if it's a directive; else to the next directive.
        let directive = tokens[i].bol && tokens[i].tok == Tok::P("#");
        let mut end = i + 1;
        while end < tokens.len() && !(tokens[end].bol && (directive || tokens[end].tok == Tok::P("#"))) {
            end += 1;
        }
        let run = &tokens[i..end];
        i = end;
        if !directive {
            if reading {
                pp.expand(run, &mut Vec::new(), &mut out, 0)?;
                if out.len() > limits.tokens {
                    return Err(Error::new(
                        run[0].line,
                        "the shader is too long once its macros are expanded",
                    ));
                }
            }
            continue;
        }
        let at = run[0].line;
        let rest = run.get(2..).unwrap_or_default();
        match ident(run.get(1)) {
            Some("define") if reading => {
                let name = ident(rest.first()).ok_or_else(|| Error::new(at, "a #define with no name"))?;
                let with_args = is_p(rest.get(1), "(") && !rest[1].space;
                let (params, body) = if with_args {
                    let mut params = Vec::new();
                    let mut k = 2;
                    loop {
                        match rest.get(k).map(|t| &t.tok) {
                            Some(Tok::P(")")) => break,
                            Some(Tok::Ident(p)) => params.push(p.clone()),
                            Some(Tok::P(",")) => {}
                            _ => return Err(Error::new(at, "a #define whose arguments can't be read")),
                        }
                        k += 1;
                    }
                    (Some(params), &rest[k + 1..])
                } else {
                    (None, &rest[1..])
                };
                if body.iter().any(|t| t.tok == Tok::P("#")) {
                    return Err(Error::new(at, "`#` inside a macro isn't supported"));
                }
                if pp.macros.len() >= limits.tokens / 16 {
                    return Err(Error::new(at, "too many macros"));
                }
                pp.macros.insert(
                    name.to_string(),
                    Rc::new(Macro {
                        params,
                        body: body.to_vec(),
                    }),
                );
            }
            Some("undef") if reading => {
                if let Some(name) = ident(rest.first()) {
                    pp.macros.remove(name);
                }
            }
            Some(kind @ ("ifdef" | "ifndef" | "if")) => {
                if conds.len() >= limits.depth as usize {
                    return Err(Error::new(at, "#if nested too deeply"));
                }
                let live = reading
                    && match kind {
                        "ifdef" => ident(rest.first()).is_some_and(|n| pp.macros.contains_key(n)),
                        "ifndef" => !ident(rest.first()).is_some_and(|n| pp.macros.contains_key(n)),
                        _ => pp.condition(rest, at)?,
                    };
                conds.push(Cond {
                    outer: reading,
                    taken: live,
                    live,
                    seen_else: false,
                });
            }
            Some(kind @ ("elif" | "else")) => {
                let (outer, taken, seen_else) = match conds.last() {
                    Some(c) => (c.outer, c.taken, c.seen_else),
                    None => return Err(Error::new(at, format!("#{kind} with no #if"))),
                };
                if seen_else {
                    return Err(Error::new(at, format!("#{kind} after #else")));
                }
                let live = outer && !taken && (kind == "else" || pp.condition(rest, at)?);
                if let Some(c) = conds.last_mut() {
                    c.live = live;
                    c.taken |= live;
                    c.seen_else = kind == "else";
                }
            }
            Some("endif") => {
                if conds.pop().is_none() {
                    return Err(Error::new(at, "#endif with no #if"));
                }
            }
            Some("error") if reading => return Err(Error::new(at, "the shader stops itself with #error")),
            // `#version`, `#extension`, `#pragma`, `#line`, and anything in a part left out.
            _ => {}
        }
    }
    if !conds.is_empty() {
        return Err(Error::new(0, "an #if with no #endif"));
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn text(tokens: &[Token]) -> String {
        let mut out = String::new();
        for t in tokens {
            if !out.is_empty() {
                out.push(' ');
            }
            match &t.tok {
                Tok::Ident(s) => out.push_str(s),
                Tok::Int(n) => out.push_str(&n.to_string()),
                Tok::Float(f) => out.push_str(&format!("{f:?}")),
                Tok::P(p) => out.push_str(p),
            }
        }
        out
    }

    fn pre(source: &str) -> Result<String, Error> {
        let limits = Limits::default();
        Ok(text(&preprocess(&lex(source, &limits)?, &limits)?))
    }

    #[test]
    fn numbers_names_and_punctuation_are_told_apart() {
        let limits = Limits::default();
        let tokens = lex(
            "a_1 += 2.5e1 * .5 - 1. / 0x1F; // gone\n/* and\nthis */ b<<=3u",
            &limits,
        )
        .unwrap();
        assert_eq!(text(&tokens), "a_1 += 25.0 * 0.5 - 1.0 / 31 ; b <<= 3");
        assert_eq!(tokens.last().unwrap().line, 3);
        assert!(lex("float x = 1.0; $", &limits).is_err());
        assert!(lex("/* open", &limits).is_err());
    }

    #[test]
    fn macros_expand_with_and_without_arguments() {
        let source = "#define TWO 2.0\n#define HALF 1.0/TWO\n#define sq(x) ((x)*(x))\n#define pair(a, b) vec2(a, b)\nfloat y = sq(HALF + 1.0) + pair(TWO, sq(3.0)).x;";
        assert_eq!(
            pre(source).unwrap(),
            "float y = ( ( 1.0 / 2.0 + 1.0 ) * ( 1.0 / 2.0 + 1.0 ) ) + vec2 ( 2.0 , ( ( 3.0 ) * ( 3.0 ) ) ) . x ;"
        );
        // A macro with arguments named without them is left alone; one isn't expanded in itself.
        assert_eq!(
            pre("#define f(x) x\n#define g g + 1\nf g f(2)").unwrap(),
            "f g + 1 2"
        );
        // Not arguments: there is a space before the parenthesis.
        assert_eq!(pre("#define f (x)\nf").unwrap(), "( x )");
    }

    #[test]
    fn conditionals_keep_one_branch() {
        let source = "#define A 2\n#ifdef GL_ES\nprecision mediump float;\n#endif\n#if A > 1 && defined(A)\nyes\n#elif 1\nno\n#else\nnever\n#endif\n#ifndef B\nb\n#else\nc\n#endif";
        assert_eq!(pre(source).unwrap(), "yes b");
        assert!(pre("#if 1\n").is_err());
        assert!(pre("#endif\n").is_err());
        assert!(pre("#error stop\n").is_err());
        assert_eq!(pre("#if 0\n#error stop\n#endif\nok").unwrap(), "ok");
    }

    #[test]
    fn macros_that_double_each_other_are_stopped() {
        let mut source = String::from("#define M0 x x\n");
        for i in 1..40 {
            source.push_str(&format!("#define M{i} M{} M{}\n", i - 1, i - 1));
        }
        source.push_str("M39");
        assert!(pre(&source).unwrap_err().message.contains("without end"));
        let mut nested = String::from("#define d(x) x x\n");
        nested.push_str(&"d(".repeat(40));
        nested.push('y');
        nested.push_str(&")".repeat(40));
        assert!(pre(&nested).is_err());
    }
}
