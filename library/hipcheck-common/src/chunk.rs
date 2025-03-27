// SPDX-License-Identifier: Apache-2.0

use crate::{
	error::Error,
	proto::{Query as PluginQuery, QueryState},
	types::Query,
};
use anyhow::{anyhow, Result};
use std::result::Result as StdResult;

/// Max size of a single GRPC message (4 MB)
pub const GRPC_MAX_SIZE_BYTES: usize = 1024 * 1024 * 4;

const GRPC_EFFECTIVE_MAX_SIZE: usize = GRPC_MAX_SIZE_BYTES - 1024; // Minus one KB

#[derive(Clone, Debug)]
enum DrainedString {
	/// The entire string was small enough to be used, so take all of it
	CompleteString(String),
	/// The entire string was too large, so drain as much as possible
	PartialString {
		/// contains as much as could possibly be drained (while being cognizant of not falling
		/// between a char boundary)
		drained_portion: String,
		/// what is left of the string that could not be drained
		remainder: String,
	},
}

/// Try to drain `max` bytes from `buf`, or the full string, whichever is shortest.
/// If `max` bytes is somewhere within `buf` but lands within a char boundary,
/// walk backwards to the end of the previous char. Returns the substring
/// drained from `buf`.
fn drain_at_most_n_bytes(mut buf: String, max: usize) -> DrainedString {
	let mut to_drain = std::cmp::min(buf.len(), max);
	if buf.len() <= to_drain {
		return DrainedString::CompleteString(buf);
	}
	while to_drain > 0 && !buf.is_char_boundary(to_drain) {
		to_drain -= 1;
	}
	let drained_portion = buf.drain(0..to_drain).collect::<String>();
	let remainder = buf;
	DrainedString::PartialString {
		drained_portion,
		remainder,
	}
}

/// determine if there is any data remaining in any of the `Vec<String>` fields
///
/// true => all chunkable fields have been consumed
/// false => there is still data to consume
fn all_chunkable_data_consumed(msg: &PluginQuery) -> bool {
	msg.key.is_empty() && msg.output.is_empty() && msg.concern.is_empty()
}

pub fn chunk_with_size(msg: PluginQuery, max_est_size: usize) -> Result<Vec<PluginQuery>> {
	// in_progress_state - the state the PluginQuery is in for all queries in the resulting Vec,
	// EXCEPT the last one
	//
	// completion_state - the state the PluginQuery is in if it is the last chunked message
	let (in_progress_state, completion_state) = match msg.state() {
		// if the message gets chunked, then it must either be a reply or submission that is in process
		QueryState::Unspecified => return Err(anyhow!("msg in Unspecified query state")),
		QueryState::SubmitInProgress | QueryState::SubmitComplete => {
			(QueryState::SubmitInProgress, QueryState::SubmitComplete)
		}
		QueryState::ReplyInProgress | QueryState::ReplyComplete => {
			(QueryState::ReplyInProgress, QueryState::ReplyComplete)
		}
	};

	let null_key = msg.key.is_empty();
	let null_output = msg.output.is_empty();

	let mut out: Vec<PluginQuery> = vec![];
	let mut base: PluginQuery = msg;

	// Track whether we did anything on each iteration to avoid infinite loop
	let mut made_progress = true;
	while !all_chunkable_data_consumed(&base) {
		if !made_progress {
			return Err(anyhow!("Message could not be chunked"));
		}
		made_progress = false;

		// For this loop, we want to take at most MAX_SIZE bytes because that's
		// all that can fit in a PluginQuery
		let mut remaining = max_est_size;
		let mut chunked_query = PluginQuery {
			id: base.id,
			state: in_progress_state as i32,
			publisher_name: base.publisher_name.clone(),
			plugin_name: base.plugin_name.clone(),
			query_name: base.query_name.clone(),
			key: vec![],
			output: vec![],
			concern: vec![],
			split: false,
		};

		for (source, sink) in [
			(&mut base.key, &mut chunked_query.key),
			(&mut base.output, &mut chunked_query.output),
			(&mut base.concern, &mut chunked_query.concern),
		] {
			let split_occurred = drain_vec_string(source, sink, &mut remaining, &mut made_progress);
			if split_occurred {
				chunked_query.split = true;
				break;
			}
			if remaining == 0 {
				break;
			}
		}

		// @Compatibility - pre-RFD9 will expect exactly 1 field, if empty, need to increase to empty
		if cfg!(feature = "rfd9-compat") {
			// if a key was empty in this query, then insert a null placeholder
			if chunked_query.key.is_empty() {
				chunked_query.key.push("".to_owned());
			}
			// if an output was empty in this query, then insert a null placeholder
			if chunked_query.output.is_empty() {
				chunked_query.output.push("".to_owned());
			}
		}

		out.push(chunked_query);
	}

	// ensure the last message in the chunked messages is set to the appropriate Complete state
	if let Some(last) = out.last_mut() {
		last.state = completion_state as i32;
	}

	// @Compatibility - pre-RFD9 expects concatenation of all `key` fields to be a valid JSON
	// string, same with `output`. This ensures if either were all blank, at least the first says
	// "null"
	if cfg!(feature = "rfd9-compat") && (null_key || null_output) {
		if let Some(first) = out.first_mut() {
			if null_key {
				if let Some(k) = first.key.first_mut() {
					*k = "null".to_owned()
				}
			}
			if null_output {
				if let Some(o) = first.output.first_mut() {
					*o = "null".to_owned()
				}
			}
		}
	}

	Ok(out)
}

pub fn chunk(msg: PluginQuery) -> Result<Vec<PluginQuery>> {
	chunk_with_size(msg, GRPC_EFFECTIVE_MAX_SIZE)
}

pub fn prepare(msg: Query) -> Result<Vec<PluginQuery>> {
	chunk(msg.try_into()?)
}

/// Drain as much from a `Vec<String>` as possible
///
/// `true` -> a `PartialString` was written to sink, indicating `split = true` for this message and no
/// more data can be fit into this GRPC message
/// `false` -> only `CompleteString` were written to sink, indicating `split = false` currently for
/// this message
fn drain_vec_string(
	source: &mut Vec<String>,
	sink: &mut Vec<String>,
	remaining: &mut usize,
	made_progress: &mut bool,
) -> bool {
	while !source.is_empty() {
		// SAFETY: this is safe because source is not empty
		//
		// by removing from the front, ordering of any Vec<String> fields is maintained
		let s_to_drain = source.remove(0);
		let drained_str = drain_at_most_n_bytes(s_to_drain, *remaining);
		match drained_str {
			DrainedString::CompleteString(complete) => {
				*made_progress = true;
				*remaining -= complete.len();
				sink.push(complete);
			}
			DrainedString::PartialString {
				drained_portion,
				remainder,
			} => {
				// if any amount was drained, then a split was required
				let split = !drained_portion.is_empty();
				if split {
					*made_progress = true;
					*remaining -= drained_portion.len();
					sink.push(drained_portion);
				}
				// since the string being processed was pulled from the front via `source.remove(0)`,
				// source.insert(0,...) needs to be used to maintain ordering
				source.insert(0, remainder);
				return split;
			}
		}
	}
	false
}

/// Determine whether or not the given `QueryState` represents an intermediate InProgress state
fn in_progress_state(state: &QueryState) -> bool {
	matches!(
		state,
		QueryState::ReplyInProgress | QueryState::SubmitInProgress
	)
}

/// represents the 3 fields in a `PluginQuery` that hold `Vec<String>` data
#[derive(Debug)]
enum QueryVecField {
	Key,
	Output,
	Concern,
}

/// determines which field in `PluginQuery` is the "latest" one with data
///
/// checks for data in reverse order:
/// 1. concern
/// 2. output
/// 3. key
fn last_field_to_have_content(query: &PluginQuery) -> QueryVecField {
	if !query.concern.is_empty() {
		return QueryVecField::Concern;
	}
	// @Compatibility - for backwards compatibility, the query.output field will always contain
	// at least one field. if the length is one and is null, then that is equivalent to an empty
	// output field
	if cfg!(feature = "rfd9-compat") {
		if !(query.output.len() == 1
			&& (query.output.first().unwrap() == "" || query.output.first().unwrap() == "null"))
		{
			return QueryVecField::Output;
		}
	} else if !query.output.is_empty() {
		return QueryVecField::Output;
	}
	QueryVecField::Key
}

#[derive(Default)]
pub struct QuerySynthesizer {
	raw: Option<PluginQuery>,
}

impl QuerySynthesizer {
	pub fn add<I>(&mut self, mut chunks: I) -> StdResult<Option<Query>, Error>
	where
		I: Iterator<Item = PluginQuery>,
	{
		if self.raw.is_none() {
			self.raw = match chunks.next() {
				Some(x) => Some(x),
				None => {
					return Ok(None);
				}
			};
		}
		let raw = self.raw.as_mut().unwrap(); // We know its `Some`, was set above

		let initial_state: QueryState = raw
			.state
			.try_into()
			.map_err(|_| Error::UnspecifiedQueryState)?;
		// holds state of current chunk
		let mut current_state: QueryState = initial_state;

		// holds whether the last message was split, if it was then it holds the "latest" field
		// with data that should have the first element of the next message appended to it
		let mut last_message_split: Option<QueryVecField> = if raw.split {
			Some(last_field_to_have_content(raw))
		} else {
			None
		};

		// If response is the first of a set of chunks, handle
		if in_progress_state(&current_state) {
			while in_progress_state(&current_state) {
				// We expect another message. Pull it off the existing queue,
				// or get a new one if we have run out
				let mut next = match chunks.next() {
					Some(msg) => msg,
					None => {
						return Ok(None);
					}
				};

				// By now we have our "next" message
				current_state = next
					.state
					.try_into()
					.map_err(|_| Error::UnspecifiedQueryState)?;
				match (initial_state, current_state) {
					// initial_state has been checked and is known to be XInProgress
					(QueryState::Unspecified, _)
					| (QueryState::ReplyComplete, _)
					| (QueryState::SubmitComplete, _) => {
						unreachable!()
					}

					// error out if any states are unspecified
					(_, QueryState::Unspecified) => return Err(Error::UnspecifiedQueryState),
					// error out if expecting a Submit messages and a Reply is received
					(QueryState::SubmitInProgress, QueryState::ReplyInProgress)
					| (QueryState::SubmitInProgress, QueryState::ReplyComplete) => {
						return Err(Error::ReceivedReplyWhenExpectingSubmitChunk)
					}
					// error out if expecting a Reply message and Submit is received
					(QueryState::ReplyInProgress, QueryState::SubmitInProgress)
					| (QueryState::ReplyInProgress, QueryState::SubmitComplete) => {
						return Err(Error::ReceivedSubmitWhenExpectingReplyChunk)
					}
					// otherwise we got an expected message type
					(_, _) => {
						if current_state == QueryState::ReplyComplete {
							raw.set_state(QueryState::ReplyComplete);
						}
						if current_state == QueryState::SubmitComplete {
							raw.set_state(QueryState::SubmitComplete);
						}

						let next_message_split = if next.split {
							Some(last_field_to_have_content(&next))
						} else {
							None
						};

						// if the last message set `split = true`, then the first element in the
						// "next" message must be appended to the last message of the "latest"
						// field that has content (per RFD #0009)
						//
						// SAFETY: the unwrap() calls in the `if let Some...` block are safe because RFD
						// 0009 guarantees that `split = true` ONLY when there is already data in
						// the corresponding `Vec<String>` field
						if let Some(split_field) = last_message_split {
							match split_field {
								QueryVecField::Key => {
									raw.key
										.last_mut()
										.unwrap()
										.push_str(next.key.remove(0).as_str());
								}
								QueryVecField::Output => {
									raw.output
										.last_mut()
										.unwrap()
										.push_str(next.output.remove(0).as_str());
								}
								QueryVecField::Concern => {
									raw.concern
										.last_mut()
										.unwrap()
										.push_str(next.concern.remove(0).as_str());
								}
							}
						}

						raw.key.extend(next.key);
						raw.output.extend(next.output);
						raw.concern.extend(next.concern);

						// save off whether or not the message that was just processed was split
						last_message_split = next_message_split;
					}
				};
			}

			// Sanity check - after we've left this loop, there should be no left over message
			if chunks.next().is_some() {
				return Err(Error::MoreAfterQueryComplete {
					id: raw.id as usize,
				});
			}
		}

		self.raw.take().unwrap().try_into().map(Some)
	}
}

#[cfg(test)]
mod test {

	use super::*;

	#[test]
	fn test_bounded_char_draining() {
		let orig_key = "aこれは実験です".to_owned();
		let max_size = 10;
		let res = drain_at_most_n_bytes(orig_key.clone(), max_size);
		let (drained, remainder) = match res {
			DrainedString::CompleteString(_) => panic!("expected to return PartialString"),
			DrainedString::PartialString {
				drained_portion,
				remainder,
			} => (drained_portion, remainder),
		};
		assert!((0..=max_size).contains(&drained.bytes().len()));

		// ensure reassembling drained + remainder yields the original value
		let mut reassembled = drained;
		reassembled.push_str(remainder.as_str());
		assert_eq!(orig_key, reassembled);
	}

	/// Ensure draining a source 1 byte at a time yields the proper number of elements in sink
	#[test]
	fn test_draining_vec() {
		let mut source = vec!["123456".to_owned()];
		let mut sink = vec![];

		// ensure chunking with max size of 1 yields 6 chunks in sink
		while !source.is_empty() {
			let mut made_progress = false;
			let partial = drain_vec_string(&mut source, &mut sink, &mut 1, &mut made_progress);
			// all fields except the last one should be PartialString
			assert_eq!(partial, !source.is_empty())
		}
		assert_eq!(sink.len(), 6);
		assert!(source.is_empty());

		// ensure chunking with max size of 3 yields 2 chunks in sink
		let mut source = vec!["123456".to_owned()];
		let mut sink = vec![];
		while !source.is_empty() {
			let mut made_progress = false;
			let partial = drain_vec_string(&mut source, &mut sink, &mut 3, &mut made_progress);
			// all fields except the last one should be PartialString
			assert_eq!(partial, !source.is_empty())
		}
		assert_eq!(sink.len(), 2);
		assert!(source.is_empty());
	}

	// Verify that max size is respecting char boundary, if a unicode character is encountered
	#[test]
	fn test_char_boundary_respected() {
		let mut source = vec!["実".to_owned()];
		let mut sink = vec![];
		let mut made_progress = false;
		// it should not be possible to read this source because it is not possible to split "実"
		// on a char boundary and keep the resulting string withing 1 byte
		drain_vec_string(&mut source, &mut sink, &mut 1, &mut made_progress);
		assert!(!made_progress);
	}

	// Ensure ability to make progress with non-ascii data
	#[test]
	fn test_non_ascii_drain_vec_string_makes_progress() {
		let mut source = vec!["1234".to_owned(), "aこれ".to_owned(), "abcdef".to_owned()];
		let mut sink = vec![];

		while !source.is_empty() {
			// force 4 byte chunks max
			let remaining = &mut 4;
			let made_progress = &mut false;
			drain_vec_string(&mut source, &mut sink, remaining, made_progress);
			assert!(*made_progress);
		}
		// drain_vec_string will walk forwards through source
		assert_eq!(sink.first().unwrap(), "1234");
		assert!(source.is_empty());
	}

	#[test]
	fn test_drain_vec_string_split_detection() {
		// this should force `drain_vec_string` to report split was necessary and leave only "4" in
		// source
		let mut max_len = 3;
		let mut source = vec!["1234".to_owned()];
		let mut sink = vec![];
		let mut made_progress = false;
		let split = drain_vec_string(&mut source, &mut sink, &mut max_len, &mut made_progress);
		assert!(split);
		assert_eq!(source, vec!["4"]);
		assert!(made_progress);
		assert_eq!(source.len(), 1);
		assert_eq!(sink.len(), 1);

		// this should force `drain_vec_string` to report split was not necessary and source should
		// be empty, as it is able to be completely drained
		let mut max_len = 10;
		let mut source = vec!["123456789".to_owned()];
		let mut sink = vec![];
		let mut made_progress = false;
		let split = drain_vec_string(&mut source, &mut sink, &mut max_len, &mut made_progress);
		assert!(!split);
		assert!(source.is_empty());
		assert!(made_progress);
		assert_eq!(sink.len(), 1);
	}

	#[test]
	fn test_chunking_and_query_reconstruction() {
		// test both reply and submission chunking
		let states = [
			(QueryState::SubmitInProgress, QueryState::SubmitComplete),
			(QueryState::ReplyInProgress, QueryState::ReplyComplete),
		];

		for (intermediate_state, final_state) in states.into_iter() {
			let output = if cfg!(feature = "rfd9-compat") {
				vec!["null".to_owned()]
			} else {
				vec![]
			};
			let orig_query = PluginQuery {
				id: 0,
				state: final_state as i32,
				publisher_name: "".to_owned(),
				plugin_name: "".to_owned(),
				query_name: "".to_owned(),
				// This key will cause the chunk not to occur on a char boundary
				key: vec![serde_json::to_string("aこれは実験です").unwrap()],
				output,
				concern: vec![
					"< 10".to_owned(),
					"0123456789".to_owned(),
					"< 10#2".to_owned(),
				],
				split: false,
			};
			let res = match chunk_with_size(orig_query.clone(), 10) {
				Ok(r) => r,
				Err(e) => {
					panic!("chunk_with_size unexpectedly errored: {e}");
				}
			};

			// ensure all except last element are ...InProgress
			res[..res.len() - 1]
				.iter()
				.for_each(|x| assert_eq!(x.state(), intermediate_state));
			// ensure last one is ...Complete
			assert_eq!(res.last().unwrap().state(), final_state);

			// attempt to reassemble message
			let mut synth = QuerySynthesizer::default();
			let synthesized_query = synth.add(res.into_iter()).unwrap();

			let synthesized_plugin_query: PluginQuery =
				synthesized_query.unwrap().try_into().unwrap();
			assert_eq!(orig_query, synthesized_plugin_query);
		}
	}
}
