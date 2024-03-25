use anyhow::Result;
use cpp_demangle::{DemangleNodeType, DemangleOptions};
use cpp_demangle::{DemangleWrite, Symbol};
use nom::combinator::opt;
use nom::error::ErrorKind;
use nom::sequence::delimited;
use nom::{error_position, IResult};
use std::num::NonZeroUsize;

#[derive(Debug, Clone, PartialEq, Eq)]
enum Token {
    Label(String),
    Type(DemangleNodeType),
    DoubleColon,
    OpenParen,
    CloseParen,
    Comma,
    Star,

    End,
}

pub fn structured_demangle(sym: &Symbol<&str>) -> Result<Vec<Token>> {
    struct S {
        results: Vec<Token>,
    }

    impl DemangleWrite for S {
        fn push_demangle_node(&mut self, nt: DemangleNodeType) {
            self.results.push(Token::Type(nt));
        }

        fn write_string(&mut self, s: &str) -> std::fmt::Result {
            self.results.push(match s {
                "(" => Token::OpenParen,
                ")" => Token::CloseParen,
                "::" => Token::DoubleColon,
                "*" => Token::Star,
                "," => Token::Comma,
                _ => Token::Label(s.to_string()),
            });
            Ok(())
        }

        fn pop_demangle_node(&mut self) {
            self.results.push(Token::End);
        }
    }

    let mut s = S {
        results: Vec::with_capacity(16),
    };
    sym.structured_demangle(&mut s, &DemangleOptions::default())?;

    Ok(s.results)
}

fn tag(expected: Token) -> impl FnMut(&[Token]) -> IResult<&[Token], ()> {
    move |input: &[Token]| -> IResult<&[Token], ()> {
        if input.is_empty() {
            return Err(nom::Err::Incomplete(nom::Needed::Size(
                NonZeroUsize::new(1).expect("static"),
            )));
        }

        if input[0] == expected {
            Ok((&input[1..], ()))
        } else {
            Err(nom::Err::Error(error_position!(input, ErrorKind::Tag)))
        }
    }
}

fn unqualified_name(input: &[Token]) -> IResult<&[Token], String> {
    delimited(
        tag(Token::Type(DemangleNodeType::UnqualifiedName)),
        label,
        tag(Token::End),
    )(input)
}

fn label(input: &[Token]) -> IResult<&[Token], String> {
    match input.get(0) {
        Some(Token::Label(s)) => Ok((&input[1..], s.to_string())),
        _ => Err(nom::Err::Error(error_position!(input, ErrorKind::Alpha))),
    }
}

fn prefix(input: &[Token]) -> IResult<&[Token], String> {
    delimited(
        tag(Token::Type(DemangleNodeType::Prefix)),
        unqualified_name,
        tag(Token::End),
    )(input)
}

fn nested_name(input: &[Token]) -> IResult<&[Token], (String, String)> {
    let (input, _) = tag(Token::Type(DemangleNodeType::NestedName))(input)?;
    let (input, prefix) = prefix(input)?;
    let (input, _) = tag(Token::DoubleColon)(input)?;
    let (input, suffix) = unqualified_name(input)?;
    let (input, _) = tag(Token::End)(input)?;
    Ok((input, (prefix, suffix)))
}

fn arg_list_OF_ONE(input: &[Token]) -> IResult<&[Token], Vec<(String, String)>> {
    // TODO: delimited by comma or something?
    let (input, name) = unqualified_name(input)?;
    let (input, star) = opt(tag(Token::Star))(input)?;

    Ok((
        input,
        vec![(
            name,
            star.map_or_else(|| "".to_string(), |_| "*".to_string()),
        )],
    ))
}

fn args(input: &[Token]) -> IResult<&[Token], Vec<(String, String)>> {
    delimited(
        tag(Token::OpenParen),
        arg_list_OF_ONE,
        tag(Token::CloseParen),
    )(input)
}

fn func(input: &[Token]) -> IResult<&[Token], Func> {
    let (input, (p, s)) = nested_name(input)?;
    let (input, args) = args(input)?;
    Ok((
        input,
        Func {
            name: format!("{p}::{s}"),
            args,
        },
    ))
}

#[derive(Debug)]
struct Func {
    pub name: String,
    pub args: Vec<(String, String)>,
}

pub fn demangle(sym: &Symbol<&str>) -> Result<Func> {
    let tokens = structured_demangle(sym)?;
    match func(&tokens) {
        Ok((rem, f)) => {
            assert!(rem.is_empty(), "{:#?}", rem);
            Ok(f)
        }
        Err(e) => {
            eprintln!("{:?}", e);
            unimplemented!("{:#?}", tokens);
        }
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn mongle() {
        insta::assert_debug_snapshot!(demangle(
            &Symbol::new("_ZN9LuaEntity23luaReadProductsFinishedEP9lua_State").unwrap()
        )
        .unwrap());
    }

    #[test]
    fn test_structured_demangle() -> Result<()> {
        let sym = Symbol::new("_ZN9LuaEntity23luaReadProductsFinishedEP9lua_State")?;
        assert_eq!(
            [
                "LuaEntity",
                "::",
                "luaReadProductsFinished",
                "(",
                "lua_State",
                "*",
                ")"
            ],
            structured_demangle(&sym)?
                .into_iter()
                .filter_map(|s| match s {
                    // bit garbage
                    Token::Label(s) => Some(s),
                    Token::DoubleColon => Some("::".to_string()),
                    Token::OpenParen => Some("(".to_string()),
                    Token::CloseParen => Some(")".to_string()),
                    Token::Comma => Some(",".to_string()),
                    _ => None,
                })
                .collect::<Vec<_>>()
                .as_slice()
        );
        Ok(())
    }
}
