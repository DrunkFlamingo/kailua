use std::str;
use std::fmt;

use kailua_diag::{Localize, Localized};

#[derive(Clone, Debug, PartialEq)]
pub enum Tok {
    Error, // dummy token
    Comment,
    Punct(Punct),
    Keyword(Keyword),
    Num(f64),
    Name(Vec<u8>),
    Str(Vec<u8>),
    EOF,
}

impl Localize for Tok {
    fn fmt_localized(&self, f: &mut fmt::Formatter, lang: &str) -> fmt::Result {
        match (lang, self) {
            ("ko", &Tok::Error)      => write!(f, "잘못된 문자"),
            (_,    &Tok::Error)      => write!(f, "an invalid character"),
            ("ko", &Tok::Comment)    => write!(f, "주석"),
            (_,    &Tok::Comment)    => write!(f, "a comment"),
            (_,    &Tok::Punct(p))   => write!(f, "{}", Localized::new(&p, lang)),
            (_,    &Tok::Keyword(w)) => write!(f, "{}", Localized::new(&w, lang)),
            ("ko", &Tok::Num(_))     => write!(f, "숫자"),
            (_,    &Tok::Num(_))     => write!(f, "a number"),
            ("ko", &Tok::Name(_))    => write!(f, "이름"),
            (_,    &Tok::Name(_))    => write!(f, "a name"),
            ("ko", &Tok::Str(_))     => write!(f, "문자열 리터럴"),
            (_,    &Tok::Str(_))     => write!(f, "a string literal"),
            ("ko", &Tok::EOF)        => write!(f, "파일의 끝"),
            (_,    &Tok::EOF)        => write!(f, "the end of file"),
        }
    }
}

impl<'a> Localize for &'a Tok {
    fn fmt_localized(&self, f: &mut fmt::Formatter, lang: &str) -> fmt::Result {
        (**self).fmt_localized(f, lang)
    }
}

macro_rules! define_puncts {
    ($ty:ident |$lang:ident|: $($i:ident $t:expr,)*) => (
        #[derive(Copy, Clone, Debug, PartialEq, Eq)]
        pub enum $ty { $($i,)* }

        impl Localize for $ty {
            fn fmt_localized(&self, f: &mut fmt::Formatter, $lang: &str) -> fmt::Result {
                let text = match *self { $($ty::$i => $t,)* };
                fmt::Display::fmt(text, f)
            }
        }
    );
}

define_puncts! { Punct |lang|:
    Plus        "`+`",
    Dash        "`-`",
    Star        "`*`",
    Slash       "`/`",
    Percent     "`%`",
    Caret       "`^`",
    Hash        "`#`",
    EqEq        "`==`",
    TildeEq     "`~=`",
    LtEq        "`<=`",
    GtEq        "`>=`",
    Lt          "`<`",
    Gt          "`>`",
    Eq          "`=`",
    Amp         "`&`",  // 5.3+
    Tilde       "`~`",  // 5.3+
    Pipe        "`|`",  // 5.3+, meta
    LtLt        "`<<`", // 5.3+
    GtGt        "`>>`", // 5.3+
    SlashSlash  "`//`", // 5.3+
    LParen      "`(`",
    RParen      "`)`",
    LBrace      "`{`",
    RBrace      "`}`",
    LBracket    "`[`",
    RBracket    "`]`",
    Semicolon   "`;`",
    Colon       "`:`",
    ColonColon  "`::`", // 5.2+
    Comma       "`,`",
    Dot         "`.`",
    DotDot      "`..`",
    DotDotDot   "`...`",

    // Kailua extensions
    DashDashHash    "`--#`",
    DashDashV       "`--v`",
    DashDashColon   "`--:`",
    DashDashGt      "`-->`",
    Ques            "`?`",
    Bang            "`!`",
    Newline         match lang { "ko" => "개행문자", _ => "a newline" },
}

macro_rules! define_keywords {
    ($ty:ident: everywhere { $($i:ident $t:expr,)* } meta_only { $($mi:ident $mt:expr,)* }) => (
        #[derive(Copy, Clone, Debug, PartialEq, Eq)]
        pub enum $ty { $($i,)* $($mi,)* }

        impl $ty {
            pub fn from(s: &[u8], in_meta: bool) -> Option<Keyword> {
                match (in_meta, s) {
                    $((_, $t) => Some(Keyword::$i),)*
                    $((true, $mt) => Some(Keyword::$mi),)*
                    (_, _) => None,
                }
            }

            pub fn name(&self) -> &'static [u8] {
                match *self { $($ty::$i => $t,)* $($ty::$mi => $mt,)* }
            }
        }

        impl Localize for $ty {
            fn fmt_localized(&self, f: &mut fmt::Formatter, lang: &str) -> fmt::Result {
                let name = str::from_utf8(self.name()).unwrap();
                match lang {
                    "ko" => write!(f, "예약어 `{}`", name),
                    _ => write!(f, "a keyword `{}`", name),
                }
            }
        }
    );
}

define_keywords! { Keyword:
    everywhere {
        And         b"and",
        Break       b"break",
        Do          b"do",
        Else        b"else",
        Elseif      b"elseif",
        End         b"end",
        False       b"false",
        For         b"for",
        Function    b"function",
        Goto        b"goto",        // 5.2+ (5.1 will treat this as a name)
        If          b"if",
        In          b"in",
        Local       b"local",
        Nil         b"nil",
        Not         b"not",
        Or          b"or",
        Repeat      b"repeat",
        Return      b"return",
        Then        b"then",
        True        b"true",
        Until       b"until",
        While       b"while",
    }

    meta_only { // Kailua extensions
        Assume      b"assume",
        Const       b"const",
        Global      b"global",
        Map         b"map",
        Module      b"module",
        Once        b"once",
        Open        b"open",
        Type        b"type",
        Var         b"var",
        Vector      b"vector",
    }
}

mod lexer;
mod nesting;

pub use self::lexer::Lexer;
pub use self::nesting::{Nest, NestedToken, NestingCategory, NestingSerial};
