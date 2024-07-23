//! The `indicatif-log-bridge` crate discards filtering information when using env_logger, so I'm writting my own.

use env_logger::Logger;
use log::SetLoggerError;

use crate::shell::Shell;

pub struct LogWrapper(pub Logger);

impl log::Log for LogWrapper {
	fn enabled(&self, metadata: &log::Metadata) -> bool {
		self.0.enabled(metadata)
	}

	fn log(&self, record: &log::Record) {
		// Don't suspend the shell if we're not gonna log the message.
		if log::logger().enabled(record.metadata()) {
			Shell::in_suspend(|| self.0.log(record))
		}
	}

	fn flush(&self) {
		Shell::in_suspend(|| self.0.flush())
	}
}

impl LogWrapper {
	pub fn try_init(self) -> Result<(), SetLoggerError> {
		if !Shell::is_init() {
			panic!("Initialize the global shell before initializing this logger");
		}

		let max_filter_level = self.0.filter();

		log::set_boxed_logger(Box::new(self))?;

		log::set_max_level(max_filter_level);

		Ok(())
	}
}
