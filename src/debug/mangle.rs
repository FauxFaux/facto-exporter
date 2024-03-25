use std::num::NonZeroUsize;

use anyhow::{anyhow, Context, Result};
use cpp_demangle::{DemangleNodeType, DemangleOptions};
use cpp_demangle::{DemangleWrite, Symbol};
use nom::branch::alt;
use nom::combinator::opt;
use nom::error::ErrorKind;
use nom::sequence::{delimited, preceded};
use nom::{error_position, IResult};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Token {
    Label(String),
    Type(DemangleNodeType),
    DoubleColon,
    OpenParen,
    CloseParen,
    Comma,
    Star,
    TypeInfo,
    Space,
    Const,
    Amp,
    Tilde,

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
                "&" => Token::Amp,
                "typeinfo name for " => Token::TypeInfo,
                " " => Token::Space,
                "const" => Token::Const,
                "," => Token::Comma,
                ", " => Token::Comma,
                "~" => Token::Tilde,
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

fn tag(expected: Token) -> impl FnMut(&[Token]) -> IResult<&[Token], Token> {
    move |input: &[Token]| -> IResult<&[Token], Token> {
        if input.is_empty() {
            return Err(nom::Err::Incomplete(nom::Needed::Size(
                NonZeroUsize::new(1).expect("static"),
            )));
        }

        if input[0] == expected {
            Ok((&input[1..], expected.clone()))
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

fn space_const(input: &[Token]) -> IResult<&[Token], ()> {
    let (input, _) = opt(tag(Token::Space))(input)?;
    let (input, _) = opt(tag(Token::Const))(input)?;
    Ok((input, ()))
}

fn typ(input: &[Token]) -> IResult<&[Token], String> {
    alt((unqualified_name, label))(input)
}

fn arg_list_OF_ONE(input: &[Token]) -> IResult<&[Token], Vec<(String, String)>> {
    // TODO: delimited by comma or something?
    let (input, name) = typ(input)?;
    let (input, suffix) = opt(alt((tag(Token::Star), tag(Token::Amp))))(input)?;

    let suffix = match suffix {
        Some(Token::Star) => "*",
        Some(Token::Amp) => "&",
        None => "",
        other => todo!("serialisation for {other:#?}"),
    };

    Ok((input, vec![(name, suffix.to_string())]))
}

fn args(input: &[Token]) -> IResult<&[Token], Vec<(String, String)>> {
    delimited(
        tag(Token::OpenParen),
        opt(arg_list_OF_ONE),
        tag(Token::CloseParen),
    )(input)
    .map(|(input, args)| (input, args.unwrap_or_else(Vec::new)))
}

fn func(input: &[Token]) -> IResult<&[Token], Func> {
    let (input, (p, s)) = nested_name(input)?;
    let (input, args) = args(input)?;
    let (input, _) = opt(tag(Token::Space))(input)?;
    let (input, _) = opt(tag(Token::Const))(input)?;
    Ok((
        input,
        Func {
            name: format!("{p}::{s}"),
            args,
        },
    ))
}

#[derive(Debug)]
pub struct Func {
    pub name: String,
    pub args: Vec<(String, String)>,
}

pub fn demangle(sym: &Symbol<&str>) -> Result<Func> {
    let tokens = structured_demangle(sym)?;
    match func(&tokens) {
        Ok((rem, f)) if rem.is_empty() => Ok(f),
        Ok((rem, f)) => {
            Err(anyhow!("leftover tokens: {:#?}", rem)).with_context(|| anyhow!("parsed: {:#?}", f))
        }
        Err(e) => Err(anyhow!("nom internal: {e:?}")),
    }
    .with_context(|| anyhow!("input: {:#?}", tokens))
    .with_context(|| anyhow!("original: {}", sym))
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn insta_products_finished() -> Result<()> {
        insta::assert_debug_snapshot!(demangle(&Symbol::new(
            "_ZN9LuaEntity23luaReadProductsFinishedEP9lua_State"
        )?)?);
        Ok(())
    }

    #[test]
    fn insta_unsigned_char() -> Result<()> {
        insta::assert_debug_snapshot!(demangle(&Symbol::new(
            "_ZNK15CraftingMachine16canSortInventoryEh"
        )?)?);
        Ok(())
    }
}
