use criterion::criterion_group;
use criterion::criterion_main;
use criterion::Criterion;
use criterion::PlottingBackend;
use hc_common::CheckKind;
use hc_core::*;
use std::time::Duration;

fn criterion_benchmark(c: &mut Criterion) {
	c.bench_function("hc check repo https://github.com/expressjs/express", |b| {
		b.iter(|| {
			let repo = "https://github.com/expressjs/express";

			let shell = {
				let verbosity = Verbosity::Quiet;
				let color_choice = ColorChoice::Auto;

				Shell::new(
					Output::stdout(color_choice),
					Output::stderr(color_choice),
					verbosity,
				)
			};

			let check = Check {
				check_type: CheckType::RepoSource,
				check_value: repo.into(),
				parent_command_value: CheckKind::Repo.name().to_string(),
			};

			let config_path = None;
			let data_path = None;
			let home_dir = None;
			let format = Format::Human;
			let raw_version = "3.0.0";

			run_with_shell(
				shell,
				check,
				config_path,
				data_path,
				home_dir,
				format,
				raw_version,
			)
		})
	});
}

criterion_group! {
	name = benches;
	config = Criterion::default()
		.sample_size(10)
		.measurement_time(Duration::from_secs(150))
		.plotting_backend(PlottingBackend::Plotters);
	targets = criterion_benchmark
}

criterion_main!(benches);
