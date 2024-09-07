// SPDX-License-Identifier: Apache-2.0

//! The majority of this module is adapted from indicatif's parallel iter impl.
//! See: https://github.com/console-rs/indicatif/blob/main/src/rayon.rs

use super::{
	progress_phase::{ProgressPhase, ProgressPhaseTracker},
	spinner_phase::{SpinnerPhase, SpinnerPhaseTracker},
};
use rayon::iter::{
	plumbing::{Consumer, Folder, Producer, ProducerCallback, UnindexedConsumer},
	IndexedParallelIterator, ParallelIterator,
};
use std::sync::Arc;

/// Trait implemented on "phase" types that link the associated (parallel) [Iteratoe]
trait HasTrackerType<I> {
	type Tracker;

	/// Attach this phase to an [Iterator], creating a "tracker".
	fn attach(self, iter: I) -> Self::Tracker;
}

impl<I> HasTrackerType<I> for SpinnerPhase {
	type Tracker = SpinnerPhaseTracker<I>;

	fn attach(self, iter: I) -> Self::Tracker {
		SpinnerPhaseTracker { phase: self, iter }
	}
}

impl<I> HasTrackerType<I> for ProgressPhase {
	type Tracker = ProgressPhaseTracker<I>;

	fn attach(self, iter: I) -> Self::Tracker {
		ProgressPhaseTracker { phase: self, iter }
	}
}

/// Trait implemented on all parallel iterators that lets the user create a progress spinner in the shell to track them.
#[allow(unused)]
pub trait ParallelTrackAsPhase: Sized + ParallelIterator {
	/// Add a spinner progress bar to the global shell that tracks this parallel iterator.
	fn track_as_spinner_phase(self, name: impl Into<Arc<str>>) -> SpinnerPhaseTracker<Self> {
		SpinnerPhaseTracker {
			phase: SpinnerPhase::start(name),
			iter: self,
		}
	}

	/// Add a progress bar to the global shell that tracks this parallel iterator.
	fn track_as_progress_phase(self, name: impl Into<Arc<str>>) -> ProgressPhaseTracker<Self>
	where
		Self: ExactSizeIterator,
	{
		ProgressPhaseTracker {
			phase: ProgressPhase::start(self.len() as u64, name),
			iter: self,
		}
	}

	/// Add a progress bar to the global shell that tracks this parallel iterator and format progress units of bytes.
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

impl<I: ParallelIterator> ParallelTrackAsPhase for I {}

impl<S: Send, T: ParallelIterator<Item = S>> ParallelIterator for SpinnerPhaseTracker<T> {
	type Item = S;

	fn drive_unindexed<C: UnindexedConsumer<Self::Item>>(self, consumer: C) -> C::Result {
		let spinner_consumer = PhaseConsumer {
			base: consumer,
			phase: self.phase,
		};
		self.iter.drive_unindexed(spinner_consumer)
	}
}

impl<S: Send, T: ParallelIterator<Item = S>> ParallelIterator for ProgressPhaseTracker<T> {
	type Item = S;

	fn drive_unindexed<C: UnindexedConsumer<Self::Item>>(self, consumer: C) -> C::Result {
		let spinner_consumer = PhaseConsumer {
			base: consumer,
			phase: self.phase,
		};
		self.iter.drive_unindexed(spinner_consumer)
	}
}

impl<I: IndexedParallelIterator> IndexedParallelIterator for SpinnerPhaseTracker<I> {
	fn len(&self) -> usize {
		self.iter.len()
	}

	fn drive<C: Consumer<Self::Item>>(self, consumer: C) -> <C as Consumer<Self::Item>>::Result {
		let consumer = PhaseConsumer {
			base: consumer,
			phase: self.phase,
		};
		self.iter.drive(consumer)
	}

	fn with_producer<CB: ProducerCallback<Self::Item>>(
		self,
		callback: CB,
	) -> <CB as ProducerCallback<Self::Item>>::Output {
		return self.iter.with_producer(Callback {
			callback,
			phase: self.phase,
		});

		struct Callback<CB> {
			callback: CB,
			phase: SpinnerPhase,
		}

		impl<T, CB: ProducerCallback<T>> ProducerCallback<T> for Callback<CB> {
			type Output = CB::Output;

			fn callback<P>(self, base: P) -> CB::Output
			where
				P: Producer<Item = T>,
			{
				let producer = PhaseProducer {
					base,
					phase: self.phase,
				};
				self.callback.callback(producer)
			}
		}
	}
}

impl<I: IndexedParallelIterator> IndexedParallelIterator for ProgressPhaseTracker<I> {
	fn len(&self) -> usize {
		self.iter.len()
	}

	fn drive<C: Consumer<Self::Item>>(self, consumer: C) -> <C as Consumer<Self::Item>>::Result {
		let consumer = PhaseConsumer {
			base: consumer,
			phase: self.phase,
		};
		self.iter.drive(consumer)
	}

	fn with_producer<CB: ProducerCallback<Self::Item>>(
		self,
		callback: CB,
	) -> <CB as ProducerCallback<Self::Item>>::Output {
		return self.iter.with_producer(Callback {
			callback,
			phase: self.phase,
		});

		struct Callback<CB> {
			callback: CB,
			phase: ProgressPhase,
		}

		impl<T, CB: ProducerCallback<T>> ProducerCallback<T> for Callback<CB> {
			type Output = CB::Output;

			fn callback<P>(self, base: P) -> CB::Output
			where
				P: Producer<Item = T>,
			{
				let producer = PhaseProducer {
					base,
					phase: self.phase,
				};
				self.callback.callback(producer)
			}
		}
	}
}

/// Folder to make spinner phase work with rayon.
struct PhaseFolder<P, C> {
	base: C,
	phase: P,
}

impl<T, C: Folder<T>> Folder<T> for PhaseFolder<SpinnerPhase, C> {
	type Result = C::Result;

	fn consume(self, item: T) -> Self {
		self.phase.inc();

		PhaseFolder {
			base: self.base.consume(item),
			phase: self.phase,
		}
	}

	fn complete(self) -> C::Result {
		self.base.complete()
	}

	fn full(&self) -> bool {
		self.base.full()
	}
}

impl<T, C: Folder<T>> Folder<T> for PhaseFolder<ProgressPhase, C> {
	type Result = C::Result;

	fn consume(self, item: T) -> Self {
		self.phase.inc(1);

		PhaseFolder {
			base: self.base.consume(item),
			phase: self.phase,
		}
	}

	fn complete(self) -> C::Result {
		self.base.complete()
	}

	fn full(&self) -> bool {
		self.base.full()
	}
}

/// Consumer struct to make this work with rayon.
struct PhaseConsumer<P, C> {
	base: C,
	phase: P,
}

impl<P, T, C> UnindexedConsumer<T> for PhaseConsumer<P, C>
where
	P: Clone + Send,
	C: UnindexedConsumer<T>,
	PhaseFolder<P, C::Folder>: Folder<T, Result = C::Result>,
{
	fn split_off_left(&self) -> Self {
		Self {
			base: self.base.split_off_left(),
			phase: self.phase.clone(),
		}
	}

	fn to_reducer(&self) -> Self::Reducer {
		self.base.to_reducer()
	}
}

impl<P, T, C> Consumer<T> for PhaseConsumer<P, C>
where
	C: Consumer<T>,
	P: Clone + Send,
	PhaseFolder<P, C::Folder>: Folder<T, Result = C::Result>,
{
	type Folder = PhaseFolder<P, C::Folder>;
	type Reducer = C::Reducer;
	type Result = C::Result;

	fn split_at(self, index: usize) -> (Self, Self, Self::Reducer) {
		let (left, right, reducer) = self.base.split_at(index);
		(
			PhaseConsumer {
				base: left,
				phase: self.phase.clone(),
			},
			PhaseConsumer {
				base: right,
				phase: self.phase,
			},
			reducer,
		)
	}

	fn into_folder(self) -> Self::Folder {
		PhaseFolder {
			base: self.base.into_folder(),
			phase: self.phase,
		}
	}

	fn full(&self) -> bool {
		self.base.full()
	}
}

/// Producer struct to make this work with rayon.
struct PhaseProducer<Phase, Prod> {
	base: Prod,
	phase: Phase,
}

impl<Phase, T, Prod> Producer for PhaseProducer<Phase, Prod>
where
	Prod: Producer<Item = T>,
	Phase: HasTrackerType<
		Prod::IntoIter,
		Tracker: ExactSizeIterator + DoubleEndedIterator + Iterator<Item = T>,
	>,
	Phase: Send + Clone,
{
	type Item = T;
	type IntoIter = Phase::Tracker;

	fn into_iter(self) -> Self::IntoIter {
		Phase::attach(self.phase, self.base.into_iter())
	}

	fn min_len(&self) -> usize {
		self.base.min_len()
	}

	fn max_len(&self) -> usize {
		self.base.max_len()
	}

	fn split_at(self, index: usize) -> (Self, Self) {
		let (left, right) = self.base.split_at(index);
		(
			PhaseProducer {
				base: left,
				phase: self.phase.clone(),
			},
			PhaseProducer {
				base: right,
				phase: self.phase.clone(),
			},
		)
	}
}

#[cfg(test)]
mod test {
	use super::{ParallelTrackAsPhase, SpinnerPhaseTracker};
	use crate::shell::{verbosity::Verbosity, Shell};
	use rayon::iter::{IntoParallelRefIterator, ParallelIterator};

	#[test]
	fn it_can_wrap_a_parallel_iterator() {
		// Initialize the global shell.
		Shell::init(Verbosity::Normal);

		let v = vec![1, 2, 3];
		fn wrap<'a, T: ParallelIterator<Item = &'a i32>>(it: SpinnerPhaseTracker<T>) {
			assert_eq!(it.map(|x| x * 2).collect::<Vec<_>>(), vec![2, 4, 6]);
		}

		wrap(v.par_iter().track_as_spinner_phase("test"));
	}
}
