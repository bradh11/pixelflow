//! Tokens to a shader's functions and globals.

use crate::ast::{BinOp, Declared, Expr, Function, Global, Node, Param, Passing, Stmt, Ty, UnOp, Unit};
use crate::lex::{Tok, Token};
use crate::{Error, Limits};

/// Most functions and globals one shader declares.
const MAX_DECLARED: usize = 4096;
/// Most levels an expression's tree has. A long sum is a tree as deep as it is long, and every
/// walk over a tree (to check it, and to free it) goes as deep as the tree does.
const MAX_TREE: u16 = 192;

/// How many levels the tree under `e` has, `e` included.
fn levels(e: &Expr) -> u16 {
    let below = match e {
        Expr::Int(_) | Expr::Float(_) | Expr::Bool(_) | Expr::Var(_) => 0,
        Expr::Call(_, args) => args.iter().map(|a| a.depth).max().unwrap_or(0),
        Expr::Field(a, _) | Expr::Unary(_, a) => a.depth,
        Expr::Step { target, .. } => target.depth,
        Expr::Index(a, b) | Expr::Binary(_, a, b) | Expr::Assign(_, a, b) | Expr::Comma(a, b) => {
            a.depth.max(b.depth)
        }
        Expr::Ternary(a, b, c) => a.depth.max(b.depth).max(c.depth),
    };
    below + 1
}

struct Parser<'a> {
    tokens: &'a [Token],
    at: usize,
    depth: u32,
    limit: u32,
}

/// What comes before a type in a declaration.
#[derive(Default)]
struct Qualifiers {
    uniform: bool,
    constant: bool,
    passing: Option<Passing>,
}

/// Binary operators from loosest to tightest, each level's with the operation.
const LEVELS: &[&[(&str, BinOp)]] = &[
    &[("||", BinOp::Or)],
    &[("^^", BinOp::Xor)],
    &[("&&", BinOp::And)],
    &[("|", BinOp::BitOr)],
    &[("^", BinOp::BitXor)],
    &[("&", BinOp::BitAnd)],
    &[("==", BinOp::Eq), ("!=", BinOp::Ne)],
    &[
        ("<=", BinOp::Le),
        (">=", BinOp::Ge),
        ("<", BinOp::Lt),
        (">", BinOp::Gt),
    ],
    &[("<<", BinOp::Shl), (">>", BinOp::Shr)],
    &[("+", BinOp::Add), ("-", BinOp::Sub)],
    &[("*", BinOp::Mul), ("/", BinOp::Div), ("%", BinOp::Rem)],
];

const ASSIGNMENTS: &[(&str, Option<BinOp>)] = &[
    ("=", None),
    ("+=", Some(BinOp::Add)),
    ("-=", Some(BinOp::Sub)),
    ("*=", Some(BinOp::Mul)),
    ("/=", Some(BinOp::Div)),
    ("%=", Some(BinOp::Rem)),
    ("<<=", Some(BinOp::Shl)),
    (">>=", Some(BinOp::Shr)),
    ("&=", Some(BinOp::BitAnd)),
    ("|=", Some(BinOp::BitOr)),
    ("^=", Some(BinOp::BitXor)),
];

impl Parser<'_> {
    fn peek(&self) -> Option<&Tok> {
        self.tokens.get(self.at).map(|t| &t.tok)
    }

    fn peek_at(&self, ahead: usize) -> Option<&Tok> {
        self.tokens.get(self.at + ahead).map(|t| &t.tok)
    }

    /// The line being read (the last one, at the end of the file).
    fn line(&self) -> u32 {
        self.tokens
            .get(self.at)
            .or(self.tokens.last())
            .map_or(0, |t| t.line)
    }

    fn fail<T>(&self, message: impl Into<String>) -> Result<T, Error> {
        Err(Error::new(self.line(), message))
    }

    fn eat(&mut self, p: &str) -> bool {
        let found = matches!(self.peek(), Some(Tok::P(q)) if *q == p);
        self.at += usize::from(found);
        found
    }

    fn expect(&mut self, p: &str) -> Result<(), Error> {
        if self.eat(p) {
            Ok(())
        } else {
            self.fail(format!("`{p}` expected"))
        }
    }

    fn word(&self) -> Option<&str> {
        match self.peek() {
            Some(Tok::Ident(name)) => Some(name),
            _ => None,
        }
    }

    fn eat_word(&mut self, word: &str) -> bool {
        let found = self.word() == Some(word);
        self.at += usize::from(found);
        found
    }

    fn name(&mut self) -> Result<String, Error> {
        match self.peek() {
            Some(Tok::Ident(name)) if Ty::named(name).is_none() => {
                let name = name.clone();
                self.at += 1;
                Ok(name)
            }
            _ => self.fail("a name expected"),
        }
    }

    fn node(&self, e: Expr, line: u32) -> Result<Node, Error> {
        let depth = levels(&e);
        if depth > MAX_TREE {
            return Err(Error::new(line, "an expression too long to work out"));
        }
        Ok(Node { e, line, depth })
    }

    fn enter(&mut self) -> Result<(), Error> {
        self.depth += 1;
        if self.depth > self.limit {
            return self.fail("nested too deeply");
        }
        Ok(())
    }

    fn qualifiers(&mut self) -> Result<Qualifiers, Error> {
        let mut found = Qualifiers::default();
        loop {
            match self.word() {
                Some("uniform") => found.uniform = true,
                Some("in") => found.passing = Some(Passing::In),
                Some("out") => found.passing = Some(Passing::Out),
                Some("inout") => found.passing = Some(Passing::InOut),
                Some("const") => found.constant = true,
                Some(
                    "attribute" | "varying" | "highp" | "mediump" | "lowp" | "invariant" | "flat" | "smooth"
                    | "centroid",
                ) => {}
                Some("layout") => {
                    self.at += 1;
                    self.expect("(")?;
                    while !self.eat(")") {
                        if self.peek().is_none() {
                            return self.fail("`)` expected");
                        }
                        self.at += 1;
                    }
                    continue;
                }
                _ => return Ok(found),
            }
            self.at += 1;
        }
    }

    fn ty(&mut self) -> Result<Ty, Error> {
        match self.word() {
            Some("struct") => self.fail("structs aren't supported yet"),
            Some(name @ ("sampler2D" | "sampler2DRect" | "samplerCube" | "sampler3D")) => {
                self.fail(format!("{name}: pictures aren't supported yet"))
            }
            Some(name) => match Ty::named(name) {
                Some(ty) => {
                    self.at += 1;
                    Ok(ty)
                }
                None => self.fail(format!("a type expected, not `{name}`")),
            },
            None => self.fail("a type expected"),
        }
    }

    /// Whether a declaration starts here: a type (after any qualifiers) that isn't being called
    /// as a constructor.
    fn at_declaration(&self) -> bool {
        let mut ahead = 0;
        while let Some(Tok::Ident(word)) = self.peek_at(ahead) {
            match word.as_str() {
                "const" | "highp" | "mediump" | "lowp" => ahead += 1,
                "struct" => return true,
                _ => break,
            }
        }
        matches!(self.peek_at(ahead), Some(Tok::Ident(name)) if Ty::named(name).is_some())
            && !matches!(self.peek_at(ahead + 1), Some(Tok::P("(")))
    }

    /// The names after a declaration's type, up to and with its `;`. `first` is one already read.
    fn declared(&mut self, first: Option<(String, u32)>) -> Result<Vec<Declared>, Error> {
        let mut vars = Vec::new();
        let mut first = first;
        loop {
            let (name, line) = match first.take() {
                Some(read) => read,
                None => {
                    let line = self.line();
                    (self.name()?, line)
                }
            };
            let array = if self.eat("[") {
                if self.eat("]") {
                    return self.fail("an array needs its length");
                }
                let length = self.expression()?;
                self.expect("]")?;
                Some(length)
            } else {
                None
            };
            let init = if self.eat("=") {
                Some(self.assignment()?)
            } else {
                None
            };
            vars.push(Declared {
                name,
                array,
                init,
                line,
            });
            if vars.len() > MAX_DECLARED {
                return self.fail("too many variables in one declaration");
            }
            if self.eat(";") {
                return Ok(vars);
            }
            self.expect(",")?;
        }
    }

    fn params(&mut self) -> Result<Vec<Param>, Error> {
        let mut params = Vec::new();
        if self.eat(")") {
            return Ok(params);
        }
        if self.word() == Some("void") && matches!(self.peek_at(1), Some(Tok::P(")"))) {
            self.at += 2;
            return Ok(params);
        }
        loop {
            let qualifiers = self.qualifiers()?;
            let ty = self.ty()?;
            if ty == Ty::Void {
                return self.fail("an argument can't be void");
            }
            let name = match self.peek() {
                Some(Tok::Ident(_)) => self.name()?,
                _ => String::new(),
            };
            if self.eat("[") {
                return self.fail("arrays as arguments aren't supported yet");
            }
            params.push(Param {
                name,
                ty,
                passing: qualifiers.passing.unwrap_or(Passing::In),
            });
            if params.len() > 64 {
                return self.fail("too many arguments");
            }
            if self.eat(")") {
                return Ok(params);
            }
            self.expect(",")?;
        }
    }

    fn block(&mut self) -> Result<Vec<Stmt>, Error> {
        self.expect("{")?;
        let mut body = Vec::new();
        while !self.eat("}") {
            if self.peek().is_none() {
                return self.fail("`}` expected");
            }
            body.push(self.statement()?);
        }
        Ok(body)
    }

    fn statement(&mut self) -> Result<Stmt, Error> {
        self.enter()?;
        let line = self.line();
        let stmt = match self.peek() {
            Some(Tok::P("{")) => Stmt::Block(self.block()?),
            Some(Tok::P(";")) => {
                self.at += 1;
                Stmt::Empty
            }
            Some(Tok::Ident(word)) => match word.as_str() {
                "if" => {
                    self.at += 1;
                    self.expect("(")?;
                    let cond = self.expression()?;
                    self.expect(")")?;
                    let then = Box::new(self.statement()?);
                    let otherwise = if self.eat_word("else") {
                        Some(Box::new(self.statement()?))
                    } else {
                        None
                    };
                    Stmt::If(cond, then, otherwise)
                }
                "for" => {
                    self.at += 1;
                    self.expect("(")?;
                    let init = if self.eat(";") {
                        None
                    } else if self.at_declaration() {
                        let constant = self.qualifiers()?.constant;
                        let ty = self.ty()?;
                        Some(Box::new(Stmt::Decl {
                            ty,
                            constant,
                            vars: self.declared(None)?,
                        }))
                    } else {
                        let first = self.expression()?;
                        self.expect(";")?;
                        Some(Box::new(Stmt::Expr(first)))
                    };
                    let cond = if self.eat(";") {
                        None
                    } else {
                        let cond = self.expression()?;
                        self.expect(";")?;
                        Some(cond)
                    };
                    let step = if self.eat(")") {
                        None
                    } else {
                        let step = self.expression()?;
                        self.expect(")")?;
                        Some(step)
                    };
                    Stmt::For {
                        init,
                        cond,
                        step,
                        body: Box::new(self.statement()?),
                    }
                }
                "while" => {
                    self.at += 1;
                    self.expect("(")?;
                    let cond = self.expression()?;
                    self.expect(")")?;
                    Stmt::While(cond, Box::new(self.statement()?))
                }
                "do" => {
                    self.at += 1;
                    let body = Box::new(self.statement()?);
                    if !self.eat_word("while") {
                        return self.fail("`while` expected");
                    }
                    self.expect("(")?;
                    let cond = self.expression()?;
                    self.expect(")")?;
                    self.expect(";")?;
                    Stmt::DoWhile(body, cond)
                }
                "return" => {
                    self.at += 1;
                    let value = if self.eat(";") {
                        None
                    } else {
                        let value = self.expression()?;
                        self.expect(";")?;
                        Some(value)
                    };
                    Stmt::Return(value, line)
                }
                "break" => {
                    self.at += 1;
                    self.expect(";")?;
                    Stmt::Break(line)
                }
                "continue" => {
                    self.at += 1;
                    self.expect(";")?;
                    Stmt::Continue(line)
                }
                "discard" => {
                    self.at += 1;
                    self.expect(";")?;
                    Stmt::Discard
                }
                "switch" => return self.fail("switch isn't supported yet"),
                "precision" => {
                    while !self.eat(";") {
                        if self.peek().is_none() {
                            return self.fail("`;` expected");
                        }
                        self.at += 1;
                    }
                    Stmt::Empty
                }
                _ if self.at_declaration() => {
                    let constant = self.qualifiers()?.constant;
                    let ty = self.ty()?;
                    if ty == Ty::Void {
                        return self.fail("a variable can't be void");
                    }
                    Stmt::Decl {
                        ty,
                        constant,
                        vars: self.declared(None)?,
                    }
                }
                _ => {
                    let e = self.expression()?;
                    self.expect(";")?;
                    Stmt::Expr(e)
                }
            },
            Some(_) => {
                let e = self.expression()?;
                self.expect(";")?;
                Stmt::Expr(e)
            }
            None => return self.fail("the shader ends in the middle of a function"),
        };
        self.depth -= 1;
        Ok(stmt)
    }

    /// Expressions joined by commas.
    fn expression(&mut self) -> Result<Node, Error> {
        let mut left = self.assignment()?;
        while matches!(self.peek(), Some(Tok::P(","))) {
            let line = self.line();
            self.at += 1;
            let right = self.assignment()?;
            left = self.node(Expr::Comma(Box::new(left), Box::new(right)), line)?;
        }
        Ok(left)
    }

    fn assignment(&mut self) -> Result<Node, Error> {
        self.enter()?;
        let line = self.line();
        let left = self.binary(0)?;
        let node = if self.eat("?") {
            let yes = self.expression()?;
            self.expect(":")?;
            let no = self.assignment()?;
            self.node(Expr::Ternary(Box::new(left), Box::new(yes), Box::new(no)), line)?
        } else if let Some(&(_, op)) = ASSIGNMENTS
            .iter()
            .find(|(p, _)| matches!(self.peek(), Some(Tok::P(q)) if q == p))
        {
            self.at += 1;
            let right = self.assignment()?;
            self.node(Expr::Assign(op, Box::new(left), Box::new(right)), line)?
        } else {
            left
        };
        self.depth -= 1;
        Ok(node)
    }

    fn binary(&mut self, level: usize) -> Result<Node, Error> {
        let Some(operators) = LEVELS.get(level) else {
            return self.unary();
        };
        let mut left = self.binary(level + 1)?;
        loop {
            let found = operators
                .iter()
                .find(|(p, _)| matches!(self.peek(), Some(Tok::P(q)) if q == p));
            let Some(&(_, op)) = found else {
                return Ok(left);
            };
            let line = self.line();
            self.at += 1;
            let right = self.binary(level + 1)?;
            left = self.node(Expr::Binary(op, Box::new(left), Box::new(right)), line)?;
        }
    }

    fn unary(&mut self) -> Result<Node, Error> {
        self.enter()?;
        let line = self.line();
        let node = if self.eat("-") {
            let inner = self.unary()?;
            // A literal with its sign, so `-1` is a number like any other.
            let e = match inner.e {
                Expr::Int(n) => Expr::Int(n.wrapping_neg()),
                Expr::Float(f) => Expr::Float(-f),
                _ => Expr::Unary(UnOp::Neg, Box::new(inner)),
            };
            self.node(e, line)?
        } else if self.eat("+") {
            self.unary()?
        } else if self.eat("!") {
            let inner = Box::new(self.unary()?);
            self.node(Expr::Unary(UnOp::Not, inner), line)?
        } else if self.eat("~") {
            let inner = Box::new(self.unary()?);
            self.node(Expr::Unary(UnOp::BitNot, inner), line)?
        } else if self.eat("++") || self.eat("--") {
            let up = matches!(self.tokens[self.at - 1].tok, Tok::P("++"));
            let target = Box::new(self.unary()?);
            self.node(
                Expr::Step {
                    target,
                    up,
                    post: false,
                },
                line,
            )?
        } else {
            self.postfix()?
        };
        self.depth -= 1;
        Ok(node)
    }

    fn postfix(&mut self) -> Result<Node, Error> {
        let line = self.line();
        let mut node = match self.peek().cloned() {
            Some(Tok::Int(n)) => {
                self.at += 1;
                self.node(Expr::Int(n), line)?
            }
            Some(Tok::Float(f)) => {
                self.at += 1;
                self.node(Expr::Float(f), line)?
            }
            Some(Tok::P("(")) => {
                self.at += 1;
                let inner = self.expression()?;
                self.expect(")")?;
                inner
            }
            Some(Tok::Ident(name)) => {
                self.at += 1;
                if self.eat("(") {
                    let mut args = Vec::new();
                    if !self.eat(")") {
                        // `f(void)`.
                        if self.word() == Some("void") && matches!(self.peek_at(1), Some(Tok::P(")"))) {
                            self.at += 2;
                        } else {
                            loop {
                                args.push(self.assignment()?);
                                if args.len() > 64 {
                                    return self.fail("too many arguments");
                                }
                                if self.eat(")") {
                                    break;
                                }
                                self.expect(",")?;
                            }
                        }
                    }
                    self.node(Expr::Call(name, args), line)?
                } else {
                    let e = match name.as_str() {
                        "true" => Expr::Bool(true),
                        "false" => Expr::Bool(false),
                        _ if Ty::named(&name).is_some() => {
                            return self.fail(format!("`{name}` used as a value"));
                        }
                        _ => Expr::Var(name),
                    };
                    self.node(e, line)?
                }
            }
            _ => return self.fail("a value expected"),
        };
        loop {
            let line = self.line();
            if self.eat(".") {
                let Some(Tok::Ident(field)) = self.peek().cloned() else {
                    return self.fail("a component expected after `.`");
                };
                self.at += 1;
                node = self.node(Expr::Field(Box::new(node), field), line)?;
            } else if self.eat("[") {
                let index = self.expression()?;
                self.expect("]")?;
                node = self.node(Expr::Index(Box::new(node), Box::new(index)), line)?;
            } else if self.eat("++") || self.eat("--") {
                let up = matches!(self.tokens[self.at - 1].tok, Tok::P("++"));
                let target = Box::new(node);
                node = self.node(
                    Expr::Step {
                        target,
                        up,
                        post: true,
                    },
                    line,
                )?;
            } else {
                return Ok(node);
            }
        }
    }
}

/// Reads a whole shader.
pub(crate) fn parse(tokens: &[Token], limits: &Limits) -> Result<Unit, Error> {
    let mut p = Parser {
        tokens,
        at: 0,
        depth: 0,
        limit: limits.depth,
    };
    let mut unit = Unit::default();
    while p.peek().is_some() {
        if p.eat(";") {
            continue;
        }
        if p.eat_word("precision") {
            while !p.eat(";") {
                if p.peek().is_none() {
                    return p.fail("`;` expected");
                }
                p.at += 1;
            }
            continue;
        }
        let qualifiers = p.qualifiers()?;
        let ty = p.ty()?;
        let line = p.line();
        let name = p.name()?;
        if unit.functions.len() + unit.globals.len() >= MAX_DECLARED {
            return p.fail("too many functions and globals");
        }
        if p.eat("(") {
            let params = p.params()?;
            // A declaration ahead of the function itself says nothing new.
            if p.eat(";") {
                continue;
            }
            p.depth = 0;
            let body = p.block()?;
            unit.functions.push(Function {
                name,
                ret: ty,
                params,
                body,
                line,
            });
            continue;
        }
        if ty == Ty::Void {
            return p.fail("a variable can't be void");
        }
        for var in p.declared(Some((name, line)))? {
            unit.globals.push(Global {
                ty,
                var,
                uniform: qualifiers.uniform,
                constant: qualifiers.constant,
            });
        }
    }
    Ok(unit)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lex::{lex, preprocess};

    fn unit(source: &str) -> Result<Unit, Error> {
        let limits = Limits::default();
        parse(&preprocess(&lex(source, &limits)?, &limits)?, &limits)
    }

    #[test]
    fn functions_globals_and_statements_are_read() {
        let u = unit(
            "precision mediump float;\nuniform float speed;\nconst int N = 4;\nvec2 a = vec2(1.0), b;\n\
             float f(in vec2 p, out float q);\n\
             float f(in vec2 p, out float q) { q = p.x; return p.y * 2.0; }\n\
             void main() { float s = 0.0; for (int i = 0; i < N; ++i) { if (s > 2.0) break; else s += f(a, s); }\n\
             do { s--; } while (s > 0.0); gl_FragColor = vec4(s); }",
        )
        .unwrap();
        assert_eq!(u.globals.len(), 4);
        assert!(u.globals[0].uniform && !u.globals[1].uniform);
        assert_eq!(u.globals[3].var.name, "b");
        assert_eq!(u.functions.len(), 2);
        assert_eq!(u.functions[0].params[1].passing, Passing::Out);
        assert_eq!(u.functions[1].body.len(), 4);
    }

    #[test]
    fn operators_bind_as_glsl_says() {
        let u = unit("void main() { x = a + b * c < d || e ? f : g; }").unwrap();
        let Stmt::Expr(Node {
            e: Expr::Assign(None, _, right),
            ..
        }) = &u.functions[0].body[0]
        else {
            panic!("an assignment");
        };
        let Expr::Ternary(cond, _, _) = &right.e else {
            panic!("a choice");
        };
        let Expr::Binary(BinOp::Or, less, _) = &cond.e else {
            panic!("an or");
        };
        let Expr::Binary(BinOp::Lt, sum, _) = &less.e else {
            panic!("a comparison");
        };
        assert!(
            matches!(&sum.e, Expr::Binary(BinOp::Add, _, product) if matches!(product.e, Expr::Binary(BinOp::Mul, _, _)))
        );
    }

    #[test]
    fn what_cant_be_read_is_refused_with_its_line() {
        assert_eq!(unit("void main() {\n  float x = ;\n}").unwrap_err().line, 2);
        assert!(unit("void main() { float x = 1.0 }").is_err());
        assert!(unit("struct S { float a; };").is_err());
        assert!(unit("uniform sampler2D tex;").is_err());
        assert!(unit("void main() {").is_err());
        assert!(unit("void main() { switch (1) {} }").is_err());
    }

    #[test]
    fn deep_nesting_is_refused_not_overflowed() {
        let deep = format!(
            "void main() {{ float x = {}1.0{}; }}",
            "(".repeat(5000),
            ")".repeat(5000)
        );
        assert!(unit(&deep).unwrap_err().message.contains("deeply"));
        let blocks = format!("void main() {}{}", "{".repeat(5000), "}".repeat(5000));
        assert!(unit(&blocks).unwrap_err().message.contains("deeply"));
        let minuses = format!("void main() {{ float x = {}y; }}", "-".repeat(5000));
        assert!(unit(&minuses).unwrap_err().message.contains("deeply"));
        // A long sum nests nothing as written, and is a tree as deep as it is long.
        let sum = format!("void main() {{ float x = 1.0{}; }}", " + 1.0".repeat(5000));
        assert!(unit(&sum).unwrap_err().message.contains("too long"));
        let dots = format!("void main() {{ float x = v{}; }}", ".x".repeat(5000));
        assert!(unit(&dots).unwrap_err().message.contains("too long"));
    }
}
