use std::{sync::Arc, time::Duration};

use prodash::{
	render::line::{JoinHandle, StreamKind},
	tree::Root,
};

use crate::shell::{verbosity::Verbosity, Shell};

/// holds handle to thread rendering gix progress, if `Verbosity` allows for displaying progress
pub struct GitProgressRenderHandle {
	_join_handle: Option<JoinHandle>,
}

impl GitProgressRenderHandle {
	/// Create a handle to the thread responsible for rendering git operation progress, if the
	/// Shell Verbosity allows for output, otherwise, do nothing
	pub fn new(root: Arc<Root>) -> Self {
		let join_handle = match Shell::get_verbosity() {
			Verbosity::Normal => {
				let render_line = prodash::render::line(
					std::io::stderr(),
					Arc::downgrade(&root),
					prodash::render::line::Options {
						frames_per_second: 30.0,
						keep_running_if_progress_is_empty: false,
						// prevent spamming of short-lived tasks
						initial_delay: Some(Duration::from_millis(500)),
						// prevent too many layers of output by stopping at 3
						level_filter: Some(0..=3),
						..Default::default()
					}
					.auto_configure(StreamKind::Stderr),
				);
				Some(render_line)
			}
			Verbosity::Quiet | Verbosity::Silent => None,
		};

		Self {
			_join_handle: join_handle,
		}
	}
}
