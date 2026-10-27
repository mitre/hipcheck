// SPDX-License-Identifier: Apache-2.0

//! The majority of this module is adapted from indicatif's iter impl.
//! See: https://github.com/console-rs/indicatif/blob/main/src/iter.rs

use super::{
	progress_phase::{ProgressPhase, ProgressPhaseTracker},
	spinner_phase::{SpinnerPhase, SpinnerPhaseTracker},
};
use std::{iter::FusedIterator, sync::Arc};

#[allow(unused)]
/// Trait implemented on all iterators that lets the user create a progress spinner in the shell to track them.
pub trait TrackAsPhase: Sized + Iterator {
	/// Add a spinner progress bar to the global shell that tracks this iterator.
	fn track_as_spinner_phase(self, name: impl Into<Arc<str>>) -> SpinnerPhaseTracker<Self> {
		SpinnerPhaseTracker {
			phase: SpinnerPhase::start(name),
			iter: self,
		}
	}

	/// Add a progress bar to the global shell that tracks this iterator.
	fn track_as_progress_phase(self, name: impl Into<Arc<str>>) -> ProgressPhaseTracker<Self>
	where
		Self: ExactSizeIterator,
	{
		ProgressPhaseTracker {
			phase: ProgressPhase::start(self.len() as u64, name),
			iter: self,
		}
	}

	/// Add a progress bar to the global shell that tracks this iterator and format progress units of bytes.
	fn track_bytes_as_progress_phase(self, name: impl Into<Arc<str>>) -> ProgressPhaseTracker<Self>
	where
		Self: ExactSizeIterator,
	{
		ProgressPhaseTracker {
			phase: ProgressPhase::start_bytes(self.len() as u64, name),
			iter: self,
		}
	}
}

impl<I: Iterator> TrackAsPhase for I {}

impl<I: Iterator> Iterator for SpinnerPhaseTracker<I> {
	type Item = I::Item;

	fn next(&mut self) -> Option<Self::Item> {
		let item = self.iter.next();

		if item.is_some() {
			self.phase.inc();
		} else if !self.phase.bar.is_finished() {
			self.phase.finish_successful();
		}

		item
	}
}

impl<I: Iterator> Iterator for ProgressPhaseTracker<I> {
	type Item = I::Item;

	fn next(&mut self) -> Option<Self::Item> {
		let item = self.iter.next();

		if item.is_some() {
			self.phase.inc(1);
		} else if !self.phase.bar.is_finished() {
			// Don't print a "done" message by default.
			self.phase.finish_successful(false);
		}

		item
	}
}

impl<I: ExactSizeIterator> ExactSizeIterator for SpinnerPhaseTracker<I> {
	fn len(&self) -> usize {
		self.iter.len()
	}
}

impl<I: ExactSizeIterator> ExactSizeIterator for ProgressPhaseTracker<I> {
	fn len(&self) -> usize {
		self.iter.len()
	}
}

impl<I: FusedIterator> FusedIterator for SpinnerPhaseTracker<I> {}
impl<I: FusedIterator> FusedIterator for ProgressPhaseTracker<I> {}

impl<I: DoubleEndedIterator> DoubleEndedIterator for SpinnerPhaseTracker<I> {
	fn next_back(&mut self) -> Option<Self::Item> {
		let item = self.iter.next_back();

		if item.is_some() {
			self.phase.inc();
		} else if !self.phase.bar.is_finished() {
			self.phase.finish_successful();
		}

		item
	}
}

impl<I: DoubleEndedIterator> DoubleEndedIterator for ProgressPhaseTracker<I> {
	fn next_back(&mut self) -> Option<Self::Item> {
		let item = self.iter.next_back();

		if item.is_some() {
			self.phase.inc(1);
		} else if !self.phase.bar.is_finished() {
			// Don't print a "done" message by default.
			self.phase.finish_successful(false);
		}

		item
	}
}
