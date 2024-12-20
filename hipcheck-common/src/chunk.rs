// SPDX-License-Identifier: Apache-2.0

use crate::{
	error::Error,
	proto::{Query as PluginQuery, QueryState},
	types::Query,
};

use std::{ops::Not, result::Result as StdResult};

use anyhow::{anyhow, Result};

pub const GRPC_MAX_SIZE_BYTES: usize = 1024 * 1024 * 4; // 4MB

const GRPC_EFFECTIVE_MAX_SIZE: usize = GRPC_MAX_SIZE_BYTES - 1024; // Minus one KB

/// Try to drain `max` bytes from `buf`, or the full string, whichever is shortest.
/// If `max` bytes is somewhere within `buf` but lands within a char boundary,
/// walk backwards to the start of the previous char. Returns the substring
/// drained from `buf`.
fn drain_at_most_n_bytes(buf: &mut String, max: usize) -> Result<String> {
	let mut to_drain = std::cmp::min(buf.bytes().len(), max);
	while to_drain > 0 && buf.is_char_boundary(to_drain).not() {
		to_drain -= 1;
	}
	if to_drain == 0 {
		return Err(anyhow!("Could not drain any whole char from string"));
	}
	Ok(buf.drain(0..to_drain).collect::<String>())
}

/// estimate the size of all of the keys in `PluginQuery`
fn estimate_key_size(msg: &PluginQuery) -> usize {
	msg.key.iter().map(|x| x.bytes().len()).sum::<usize>()
}

/// estimate the size of all of the outputs in `PluginQuery`
fn estimate_output_size(msg: &PluginQuery) -> usize {
	msg.output.iter().map(|x| x.bytes().len()).sum::<usize>()
}

/// estimate the size of all of the concerns in `PluginQuery`
fn estimate_concern_size(msg: &PluginQuery) -> usize {
	msg.concern.iter().map(|x| x.bytes().len()).sum::<usize>()
}

/// estimate the size of a `PluginQuery` by calculating the size of each field in `PluginQuery`
/// which holds a Vec
fn estimate_size(msg: &PluginQuery) -> usize {
	estimate_key_size(msg) + estimate_output_size(msg) + estimate_concern_size(msg)
}

pub fn chunk_with_size(msg: PluginQuery, max_est_size: usize) -> Result<Vec<PluginQuery>> {
	// Chunking only does something on response objects, mostly because
	// we don't have a state to represent "SubmitInProgress"
	if msg.state == QueryState::Submit as i32 {
		return Ok(vec![msg]);
	}

	let mut out: Vec<PluginQuery> = vec![];
	let mut base: PluginQuery = msg;

	// Track whether we did anything on each iteration to avoid infinite loop
	let mut made_progress = true;
	while estimate_size(&base) > max_est_size {
		log::trace!("Estimated size is too large, chunking");

		if !made_progress {
			return Err(anyhow!("Message could not be chunked"));
		}
		made_progress = false;

		// For this loop, we want to take at most MAX_SIZE bytes because that's
		// all that can fit in a PluginQuery
		let mut remaining = max_est_size;
		let mut query = PluginQuery {
			id: base.id,
			state: QueryState::ReplyInProgress as i32,
			publisher_name: base.publisher_name.clone(),
			plugin_name: base.plugin_name.clone(),
			query_name: base.query_name.clone(),
			key: vec![],
			output: vec![],
			concern: vec![],
		};

		drain_vec_of_string(
			&mut base.key,
			&mut query.key,
			&mut remaining,
			max_est_size,
			&mut made_progress,
		)?;

		drain_vec_of_string(
			&mut base.output,
			&mut query.output,
			&mut remaining,
			max_est_size,
			&mut made_progress,
		)?;

		drain_vec_of_string(
			&mut base.concern,
			&mut query.concern,
			&mut remaining,
			max_est_size,
			&mut made_progress,
		)?;
		out.push(query);
	}
	out.push(base);
	Ok(out)
}

fn drain_vec_of_string(
	input_buffer: &mut Vec<String>,
	output_buffer: &mut Vec<String>,
	remaining: &mut usize,
	max_est_size: usize,
	made_progress: &mut bool,
) -> Result<()> {
	while let Some(str) = input_buffer.pop() {
		if *remaining == 0 {
			break;
		}
		let str_len = str.bytes().len();
		if str_len > max_est_size {
			return Err(anyhow!(
				"Query cannot be chunked, there is a value that is larger than max chunk size"
			));
		} else if str_len <= *remaining {
			output_buffer.push(str);
			*remaining -= str_len;
			*made_progress = true;
			// if we hit this branch, we did not process this value and we need to put it back
		} else {
			input_buffer.push(str);
		}
	}
	Ok(())
}

pub fn chunk(msg: PluginQuery) -> Result<Vec<PluginQuery>> {
	chunk_with_size(msg, GRPC_EFFECTIVE_MAX_SIZE)
}

pub fn prepare(msg: Query) -> Result<Vec<PluginQuery>> {
	chunk(msg.try_into()?)
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
		let mut state = raw
			.state
			.try_into()
			.map_err(|_| Error::UnspecifiedQueryState)?;

		// If response is the first of a set of chunks, handle
		if matches!(state, QueryState::ReplyInProgress) {
			while matches!(state, QueryState::ReplyInProgress) {
				// We expect another message. Pull it off the existing queue,
				// or get a new one if we have run out
				let next = match chunks.next() {
					Some(msg) => msg,
					None => {
						return Ok(None);
					}
				};

				// By now we have our "next" message
				state = next
					.state
					.try_into()
					.map_err(|_| Error::UnspecifiedQueryState)?;
				match state {
					QueryState::Unspecified => return Err(Error::UnspecifiedQueryState),
					QueryState::Submit => return Err(Error::ReceivedSubmitWhenExpectingReplyChunk),
					QueryState::ReplyInProgress | QueryState::ReplyComplete => {
						if state == QueryState::ReplyComplete {
							raw.state = QueryState::ReplyComplete.into();
						}

						raw.key.extend_from_slice(next.key.as_slice());
						raw.output.extend_from_slice(next.output.as_slice());
						raw.concern.extend_from_slice(next.concern.as_slice());
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

		let mut key = orig_key.clone();
		let res = drain_at_most_n_bytes(&mut key, 10).unwrap();
		let num_bytes = res.bytes().len();

		assert!(num_bytes > 0 && num_bytes <= 10);

		// Make sure the drained str + retained str combine to re-create original
		let mut reassembled = res.clone();
		reassembled.push_str(&key);

		assert_eq!(orig_key, reassembled);
	}

	#[test]
	fn test_chunking() {
		let query = PluginQuery {
			id: 0,
			state: QueryState::ReplyComplete as i32,
			publisher_name: "".to_owned(),
			plugin_name: "".to_owned(),
			query_name: "".to_owned(),
			// This key will cause query to be split into two parts
			key: vec!["0123456789".to_owned(), "0123456789".to_owned()],
			output: vec![],
			concern: vec![],
		};
		let res = match chunk_with_size(query, 10) {
			Ok(r) => r,
			Err(e) => {
				panic!("{e}");
			}
		};
		// this should have been chunked into 2 messages
		assert_eq!(res.len(), 2);
		// first message should be progress
		assert_eq!(res.first().unwrap().state(), QueryState::ReplyInProgress);
		// last message should be complete
		assert_eq!(res.get(1).unwrap().state(), QueryState::ReplyComplete);
	}
}
