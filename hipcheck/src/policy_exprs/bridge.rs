// SPDX-License-Identifier: Apache-2.0

// The following code is copied from the `logos-nom-bridge` crate, which uses
// an outdated version of `logos` and thus can't be used directly here.
//
// The original code which we have copied and modified is MIT licensed, and
// used under the terms of that license here.

//! # logos-nom-bridge
//!
//! A [`logos::Lexer`] wrapper than can be used as an input for
//! [nom](https://docs.rs/nom/7.0.0/nom/index.html).
//!

use core::fmt;
use logos::{Lexer, Logos, Span, SpannedIter};
use nom::{InputIter, InputLength, InputTake};

/// A [`logos::Lexer`] wrapper than can be used as an input for
/// [nom](https://docs.rs/nom/7.0.0/nom/index.html).
///
/// You can find an example in the [module-level docs](..).
pub struct Tokens<'i, T>
where
	T: Logos<'i>,
{
	lexer: Lexer<'i, T>,
}

impl<'i, T> Clone for Tokens<'i, T>
where
	T: Logos<'i> + Clone,
	T::Extras: Clone,
{
	fn clone(&self) -> Self {
		Self {
			lexer: self.lexer.clone(),
		}
	}
}

// Helper type returned by the logos parser.
type ParseResult<'i, T> = Result<T, <T as Logos<'i>>::Error>;

impl<'i, T> Tokens<'i, T>
where
	T: Logos<'i, Source = str> + Clone,
	T::Extras: Default + Clone,
{
	/// Create a new token parser.
	pub fn new(input: &'i str) -> Self {
		Tokens {
			lexer: Lexer::new(input),
		}
	}

	/// Get the length of the remaining source to parse.
	pub fn len(&self) -> usize {
		self.lexer.source().len() - self.lexer.span().end
	}

	/// See if the remaining length to parse is empty.
	#[allow(unused)]
	pub fn is_empty(&self) -> bool {
		self.len() == 0
	}

	/// Peek at the next token, possibly with a parsing error.
	pub fn peek(&self) -> Option<(ParseResult<'i, T>, &'i str)> {
		let mut iter = self.lexer.clone().spanned();
		iter.next().map(|(t, span)| (t, &self.lexer.source()[span]))
	}

	/// Advance the parser one step.
	pub fn advance(mut self) -> Self {
		self.lexer.next();
		self
	}

	/// Get the underlying lexer.
	pub fn lexer(&self) -> &Lexer<'i, T> {
		&self.lexer
	}
}

impl<'i, T> PartialEq for Tokens<'i, T>
where
	T: PartialEq + Logos<'i> + Clone,
	T::Extras: Clone,
{
	fn eq(&self, other: &Self) -> bool {
		Iterator::eq(self.lexer.clone(), other.lexer.clone())
	}
}

impl<'i, T> Eq for Tokens<'i, T>
where
	T: Eq + Logos<'i> + Clone,
	T::Extras: Clone,
{
}

impl<'i, T> fmt::Debug for Tokens<'i, T>
where
	T: fmt::Debug + Logos<'i, Source = str>,
{
	fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
		let source = self.lexer.source();
		let start = self.lexer.span().start;
		f.debug_tuple("Tokens").field(&&source[start..]).finish()
	}
}

impl<'i, T> fmt::Display for Tokens<'i, T>
where
	T: fmt::Debug + fmt::Display + Logos<'i, Source = str>,
{
	fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
		(self as &dyn fmt::Debug).fmt(f)
	}
}

impl<'i, T> Default for Tokens<'i, T>
where
	T: Logos<'i, Source = str>,
	T::Extras: Default,
{
	fn default() -> Self {
		Tokens {
			lexer: Lexer::new(""),
		}
	}
}

/// An iterator, that (similarly to [`std::iter::Enumerate`]) produces byte offsets of the tokens.
pub struct IndexIterator<'i, T>
where
	T: Logos<'i>,
{
	logos: Lexer<'i, T>,
}

impl<'i, T> Iterator for IndexIterator<'i, T>
where
	T: Logos<'i>,
{
	type Item = (usize, (ParseResult<'i, T>, Span));

	fn next(&mut self) -> Option<Self::Item> {
		self.logos.next().map(|t| {
			let span = self.logos.span();
			(span.start, (t, span))
		})
	}
}

impl<'i, T> InputIter for Tokens<'i, T>
where
	T: Logos<'i, Source = str> + Clone,
	T::Extras: Default + Clone,
{
	type Item = (ParseResult<'i, T>, Span);
	type Iter = IndexIterator<'i, T>;
	type IterElem = SpannedIter<'i, T>;

	fn iter_indices(&self) -> Self::Iter {
		IndexIterator {
			logos: self.lexer.clone(),
		}
	}

	fn iter_elements(&self) -> Self::IterElem {
		self.lexer.clone().spanned()
	}

	fn position<P>(&self, predicate: P) -> Option<usize>
	where
		P: Fn(Self::Item) -> bool,
	{
		let mut iter = self.lexer.clone().spanned();
		iter.find(|t| predicate(t.clone()))
			.map(|(_, span)| span.start)
	}

	fn slice_index(&self, count: usize) -> Result<usize, nom::Needed> {
		let mut cnt = 0;
		for (_, span) in self.lexer.clone().spanned() {
			if cnt == count {
				return Ok(span.start);
			}
			cnt += 1;
		}
		if cnt == count {
			return Ok(self.len());
		}
		Err(nom::Needed::Unknown)
	}
}

impl<'i, T> InputLength for Tokens<'i, T>
where
	T: Logos<'i, Source = str> + Clone,
	T::Extras: Default + Clone,
{
	fn input_len(&self) -> usize {
		self.len()
	}
}

impl<'i, T> InputTake for Tokens<'i, T>
where
	T: Logos<'i, Source = str>,
	T::Extras: Default,
{
	fn take(&self, count: usize) -> Self {
		Tokens {
			lexer: Lexer::new(&self.lexer.source()[..count]),
		}
	}

	fn take_split(&self, count: usize) -> (Self, Self) {
		let (a, b) = self.lexer.source().split_at(count);
		(
			Tokens {
				lexer: Lexer::new(a),
			},
			Tokens {
				lexer: Lexer::new(b),
			},
		)
	}
}

#[macro_export]
#[doc(hidden)]
macro_rules! token_parser {
    (
        token: $token_ty:ty $(,)?
    ) => {
        $crate::token_parser!(
            token: $token_ty,
            error<'source>(input, token): ::nom::error::Error<$crate::policy_exprs::Tokens<'source, $token_ty>> =
                nom::error::Error::new(input, nom::error::ErrorKind::IsA),
        );
    };

    (
        token: $token_ty:ty,
        error: $error_ty:ty = $error:expr $(,)?
    ) => {
        $crate::token_parser!(
            token: $token_ty,
            error<'source>(input, token): $error_ty = $error,
        );
    };

    (
        token: $token_ty:ty,
        error<$lt:lifetime>($input:ident, $token:ident): $error_ty:ty = $error:expr $(,)?
    ) => {
        #[allow(unused)]
        impl<$lt> ::nom::Parser<
            $crate::policy_exprs::Tokens<$lt, $token_ty>,
            &$lt str,
            $error_ty,
        > for $token_ty {
            fn parse(
                &mut self,
                $input: $crate::policy_exprs::Tokens<$lt, $token_ty>,
            ) -> ::nom::IResult<
                $crate::policy_exprs::Tokens<$lt, $token_ty>,
                &$lt str,
                $error_ty,
            > {
                match $input.peek() {
                    ::std::option::Option::Some((::std::result::Result::Ok(__token), __s)) if __token == *self => {
                        ::std::result::Result::Ok(($input.advance(), __s))
                    }
                    ::std::option::Option::Some((::std::result::Result::Err(__err), __s)) => {
                        // Technically this could just be the subsequent case as well, but I am
                        // deciding to distinguish it here.
                        ::std::result::Result::Err(::nom::Err::Error($error))
                    }
                    _ => {
                        // This was in the original code. It appears to be unused, but I am leaving it here
                        // as a sort of Chesterton's Fence situation.
                        let $token = self;
                        ::std::result::Result::Err(::nom::Err::Error($error))
                    },
                }
            }
        }
    };
}

/// Generates a nom parser function to parse an enum variant that contains data.
#[macro_export]
#[doc(hidden)]
macro_rules! data_variant_parser {
    (
        fn $fn_name:ident($input:ident) -> Result<$ok_ty:ty>;

        pattern = $type:ident :: $variant:ident $data:tt => $res:expr;
    ) => {
        $crate::data_variant_parser! {
            fn $fn_name<'src>($input) -> Result<
                $ok_ty,
                ::nom::error::Error<$crate::policy_exprs::Tokens<'src, $type>>,
            >;

            pattern = $type :: $variant $data => $res;
            error = ::nom::error::Error::new($input, ::nom::error::ErrorKind::IsA);
        }
    };

    (
        fn $fn_name:ident($input:ident) -> Result<$ok_ty:ty, $error_ty:ty $(,)?>;

        pattern = $type:ident :: $variant:ident $data:tt => $res:expr;
        error = $error:expr;
    ) => {
        $crate::data_variant_parser! {
            fn $fn_name<'src>($input) -> Result<$ok_ty, $error_ty>;

            pattern = $type :: $variant $data => $res;
            error = $error;
        }
    };

    (
        fn $fn_name:ident<$lt:lifetime>($input:ident) -> Result<$ok_ty:ty, $error_ty:ty $(,)?>;

        pattern = $type:ident :: $variant:ident $data:tt => $res:expr;
        error = $error:expr;
    ) => {
        fn $fn_name<$lt>($input: $crate::policy_exprs::Tokens<$lt, $type>) -> ::nom::IResult<
            $crate::policy_exprs::Tokens<$lt, $type>,
            $ok_ty,
            $error_ty,
        > {
            match $input.peek() {
                ::std::option::Option::Some((::std::result::Result::Ok($type::$variant $data), _)) => {
                    Ok(($input.advance(), $res))
                }
                _ => ::std::result::Result::Err(::nom::Err::Error($error)),
            }
        }
    };
}
