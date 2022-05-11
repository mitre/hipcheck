// SPDX-License-Identifier: Apache-2.0

//! `pathbuf` provides a single macro, [`pathbuf!`][pathbuf], which gives a [`vec!`][std_vec]-like syntax
//! for constructing [`PathBuf`][std_path_pathbuf]s.
//!
//! # Example
//!
//! ```
//! # use hc_pathbuf::pathbuf;
//! # use std::path::Path;
//! #
//! fn do_something(dir: &Path) {
//!     let file_name = pathbuf![dir, "filename.txt"];
//!
//!     if file_name.exists() {
//!         // do something...
//!     }
//! }
//! ```
//!
//! [pathbuf]: macro.pathbuf.html
//! [std_vec]: https://doc.rust-lang.org/std/macro.vec.html "Documentation for std::vec (macro)"
//! [std_path_pathbuf]: https://doc.rust-lang.org/std/path/struct.PathBuf.html "Documentation for std::path::PathBuf (struct)"

/// Creates a [`PathBuf`][std_path_pathbuf] containing the arguments.
///
/// `pathbuf!` allows [`PathBuf`][std_path_pathbuf]s to be defined with the same syntax as array expressions, like so:
///
/// ```
/// # use hc_pathbuf::pathbuf;
/// # use std::path::Path;
/// #
/// fn do_something(dir: &Path) {
///     let file_name = pathbuf![dir, "filename.txt"];
///     
///     if file_name.exists() {
///         // do something...
///     }
/// }
/// ```
///
/// [std_path_pathbuf]: https://doc.rust-lang.org/std/path/struct.PathBuf.html "Documentation for std::path::PathBuf (struct)"
#[macro_export]
macro_rules! pathbuf {
    ( $( $part:expr ),* ) => {{
        use std::path::PathBuf;

        let mut temp = PathBuf::new();

        $(
            temp.push($part);
        )*

        temp
    }};

    ($( $part:expr, )*) => ($crate::pathbuf![$($part),*])
}

#[cfg(test)]
mod tests {
	use crate::pathbuf;
	use std::path::PathBuf;

	#[test]
	fn it_works() {
		let p = pathbuf!["hello", "filename.txt"];

		let expected = {
			let mut temp = PathBuf::new();
			temp.push("hello");
			temp.push("filename.txt");
			temp
		};

		assert_eq!(p, expected);
	}
}
