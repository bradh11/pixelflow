//! A shader as written: its functions and globals, before types are checked.

/// What one component of a value is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Base {
    Bool,
    Int,
    Float,
}

/// A GLSL type, arrays aside.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Ty {
    Void,
    Scalar(Base),
    Vector(Base, u8),
    /// A square matrix of floats, stored a column after another.
    Matrix(u8),
}

impl Ty {
    pub const FLOAT: Ty = Ty::Scalar(Base::Float);
    pub const INT: Ty = Ty::Scalar(Base::Int);
    pub const BOOL: Ty = Ty::Scalar(Base::Bool);

    /// How many numbers a value of it is.
    pub fn components(self) -> usize {
        match self {
            Ty::Void => 0,
            Ty::Scalar(_) => 1,
            Ty::Vector(_, n) => usize::from(n),
            Ty::Matrix(n) => usize::from(n) * usize::from(n),
        }
    }

    pub fn base(self) -> Base {
        match self {
            Ty::Scalar(b) | Ty::Vector(b, _) => b,
            Ty::Void | Ty::Matrix(_) => Base::Float,
        }
    }

    /// The type GLSL calls `name`.
    pub fn named(name: &str) -> Option<Ty> {
        Some(match name {
            "void" => Ty::Void,
            "float" => Ty::FLOAT,
            "int" | "uint" => Ty::INT,
            "bool" => Ty::BOOL,
            "vec2" => Ty::Vector(Base::Float, 2),
            "vec3" => Ty::Vector(Base::Float, 3),
            "vec4" => Ty::Vector(Base::Float, 4),
            "ivec2" | "uvec2" => Ty::Vector(Base::Int, 2),
            "ivec3" | "uvec3" => Ty::Vector(Base::Int, 3),
            "ivec4" | "uvec4" => Ty::Vector(Base::Int, 4),
            "bvec2" => Ty::Vector(Base::Bool, 2),
            "bvec3" => Ty::Vector(Base::Bool, 3),
            "bvec4" => Ty::Vector(Base::Bool, 4),
            "mat2" => Ty::Matrix(2),
            "mat3" => Ty::Matrix(3),
            "mat4" => Ty::Matrix(4),
            _ => return None,
        })
    }

    pub fn name(self) -> &'static str {
        match self {
            Ty::Void => "void",
            Ty::Scalar(Base::Float) => "float",
            Ty::Scalar(Base::Int) => "int",
            Ty::Scalar(Base::Bool) => "bool",
            Ty::Vector(Base::Float, 2) => "vec2",
            Ty::Vector(Base::Float, 3) => "vec3",
            Ty::Vector(Base::Float, _) => "vec4",
            Ty::Vector(Base::Int, 2) => "ivec2",
            Ty::Vector(Base::Int, 3) => "ivec3",
            Ty::Vector(Base::Int, _) => "ivec4",
            Ty::Vector(Base::Bool, 2) => "bvec2",
            Ty::Vector(Base::Bool, 3) => "bvec3",
            Ty::Vector(Base::Bool, _) => "bvec4",
            Ty::Matrix(2) => "mat2",
            Ty::Matrix(3) => "mat3",
            Ty::Matrix(_) => "mat4",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum BinOp {
    Add,
    Sub,
    Mul,
    Div,
    Rem,
    Shl,
    Shr,
    BitAnd,
    BitOr,
    BitXor,
    Lt,
    Le,
    Gt,
    Ge,
    Eq,
    Ne,
    And,
    Or,
    Xor,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum UnOp {
    Neg,
    Not,
    BitNot,
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Node {
    pub e: Expr,
    pub line: u32,
    /// How many levels the tree under it has, itself included.
    pub depth: u16,
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) enum Expr {
    Int(i32),
    Float(f32),
    Bool(bool),
    Var(String),
    /// A function or a constructor.
    Call(String, Vec<Node>),
    /// `.xyz` and the like.
    Field(Box<Node>, String),
    Index(Box<Node>, Box<Node>),
    Unary(UnOp, Box<Node>),
    /// `++` and `--`, before (`post` false) or after.
    Step {
        target: Box<Node>,
        up: bool,
        post: bool,
    },
    Binary(BinOp, Box<Node>, Box<Node>),
    /// `=`, or `+=` and the like with the operation.
    Assign(Option<BinOp>, Box<Node>, Box<Node>),
    Ternary(Box<Node>, Box<Node>, Box<Node>),
    Comma(Box<Node>, Box<Node>),
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Declared {
    pub name: String,
    /// The length of an array, as written.
    pub array: Option<Node>,
    pub init: Option<Node>,
    pub line: u32,
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) enum Stmt {
    Decl {
        ty: Ty,
        constant: bool,
        vars: Vec<Declared>,
    },
    Expr(Node),
    If(Node, Box<Stmt>, Option<Box<Stmt>>),
    For {
        init: Option<Box<Stmt>>,
        cond: Option<Node>,
        step: Option<Node>,
        body: Box<Stmt>,
    },
    While(Node, Box<Stmt>),
    DoWhile(Box<Stmt>, Node),
    Block(Vec<Stmt>),
    Return(Option<Node>, u32),
    Break(u32),
    Continue(u32),
    Discard,
    Empty,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Passing {
    In,
    Out,
    InOut,
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Param {
    pub name: String,
    pub ty: Ty,
    pub passing: Passing,
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Function {
    pub name: String,
    pub ret: Ty,
    pub params: Vec<Param>,
    pub body: Vec<Stmt>,
    pub line: u32,
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Global {
    pub ty: Ty,
    pub var: Declared,
    /// Declared `uniform`: given from outside.
    pub uniform: bool,
    pub constant: bool,
}

#[derive(Debug, Clone, PartialEq, Default)]
pub(crate) struct Unit {
    pub globals: Vec<Global>,
    pub functions: Vec<Function>,
}
