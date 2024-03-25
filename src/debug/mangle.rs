use std::hash::Hasher;
use std::num::NonZeroUsize;

use anyhow::{anyhow, Context, Result};
use cpp_demangle::{DemangleNodeType, DemangleOptions};
use cpp_demangle::{DemangleWrite, Symbol};
use nom::branch::alt;
use nom::combinator::{complete, opt};
use nom::error::{ErrorKind, VerboseError};
use nom::multi::separated_list0;
use nom::sequence::{delimited, preceded};
use nom::{error_position, Finish, IResult};

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

fn tag(
    expected: Token,
) -> impl FnMut(&[Token]) -> IResult<&[Token], Token, VerboseError<&[Token]>> {
    move |input: &[Token]| -> IResult<&[Token], Token, VerboseError<&[Token]>> {
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

fn unqualified_name(input: &[Token]) -> IResult<&[Token], String, VerboseError<&[Token]>> {
    delimited(
        tag(Token::Type(DemangleNodeType::UnqualifiedName)),
        label,
        tag(Token::End),
    )(input)
}

fn label(input: &[Token]) -> IResult<&[Token], String, VerboseError<&[Token]>> {
    match input.get(0) {
        Some(Token::Label(s)) => Ok((&input[1..], s.to_string())),
        _ => Err(nom::Err::Error(error_position!(input, ErrorKind::Alpha))),
    }
}

fn prefix(input: &[Token]) -> IResult<&[Token], String, VerboseError<&[Token]>> {
    delimited(
        tag(Token::Type(DemangleNodeType::Prefix)),
        unqualified_name,
        tag(Token::End),
    )(input)
}

fn nested_name(input: &[Token]) -> IResult<&[Token], (String, String), VerboseError<&[Token]>> {
    let (input, _) = tag(Token::Type(DemangleNodeType::NestedName))(input)?;
    let (input, prefix) = prefix(input)?;
    let (input, _) = tag(Token::DoubleColon)(input)?;
    let (input, suffix) = unqualified_name(input)?;
    let (input, _) = tag(Token::End)(input)?;
    Ok((input, (prefix, suffix)))
}

fn space_const(input: &[Token]) -> IResult<&[Token], (), VerboseError<&[Token]>> {
    let (input, _) = opt(tag(Token::Space))(input)?;
    let (input, _) = opt(tag(Token::Const))(input)?;
    Ok((input, ()))
}

fn typ(input: &[Token]) -> IResult<&[Token], String, VerboseError<&[Token]>> {
    alt((unqualified_name, label))(input)
}

fn arg(input: &[Token]) -> IResult<&[Token], (String, String), VerboseError<&[Token]>> {
    let (input, name) = typ(input)?;
    let (input, _) = space_const(input)?;
    let (input, suffix) = opt(alt((tag(Token::Star), tag(Token::Amp))))(input)?;

    let suffix = match suffix {
        Some(Token::Star) => "*",
        Some(Token::Amp) => "&",
        None => "",
        other => todo!("serialisation for {other:#?}"),
    };

    Ok((input, (name, suffix.to_string())))
}

fn arg_list(input: &[Token]) -> IResult<&[Token], Vec<(String, String)>, VerboseError<&[Token]>> {
    separated_list0(tag(Token::Comma), arg)(input)
}

fn args(input: &[Token]) -> IResult<&[Token], Vec<(String, String)>, VerboseError<&[Token]>> {
    delimited(tag(Token::OpenParen), arg_list, tag(Token::CloseParen))(input)
}

fn opt_tag(tag: Token) -> impl FnMut(&[Token]) -> IResult<&[Token], (), VerboseError<&[Token]>> {
    move |input: &[Token]| -> IResult<&[Token], (), VerboseError<&[Token]>> {
        match input.get(0) {
            Some(t) if t == &tag => Ok((&input[1..], ())),
            _ => Ok((input, ())),
        }
    }
}

fn func(input: &[Token]) -> IResult<&[Token], Func, VerboseError<&[Token]>> {
    let (input, (p, s)) = nested_name(input)?;
    let (mut input, args) = args(input)?;
    if matches!(input.get(0), Some(Token::Space)) {
        input = &input[1..];
    }
    let (input, _) = opt_tag(Token::Space)(input)?;
    let (input, _) = opt_tag(Token::Const)(input)?;
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
    let v = complete(func)(&tokens);
    match v.finish() {
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

    #[test]
    fn insta_multi() -> Result<()> {
        insta::assert_debug_snapshot!(demangle(&Symbol::new(
            "_ZNK24CraftingMachinePrototype15canHandleRecipeERK6RecipeRK9ForceData"
        )?)?);
        Ok(())
    }
}
