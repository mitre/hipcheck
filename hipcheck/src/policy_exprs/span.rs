use std::{cmp::Ordering, fmt::Display, str::FromStr};

use jiff::{Span, SpanArithmetic, SpanCompare, SpanRound, ZonedArithmetic};

/// Wrapper around `jiff::Span` that implements our required semantics for days and weeks.
///
/// `jiff`, starting in version 0.2, updated the handling of comparisons for `Span`s to
/// by-default disallow certain operations when a span includes "calendar units" like
/// days or weeks. This is because, when being maximally accurate, the meaning of "day"
/// or "week" is actually dependent on the specific reference timeframe you're using.
/// For example, leap seconds or daylight savings may intervene to make the duration of
/// a "day" or a "week" different than the normal assumption that days are 24 hours and
/// weeks are 7 24-hour days long.
///
/// While this is maximally accurate, it means that to have span-math that always
/// succeeds you'd need to constrain spans to only use non-calendrical units. So hours,
/// minutes, and seconds would be fine, but days or weeks would be excluded. For us,
/// this would be awkward, since we want policy expressions to be able to easily express
/// thresholds like "52 weeks".
///
/// An alternative is to use a setting `jiff` exposes to ensure that days are always
/// treated as being 24 hours, and weeks are always treated as 7 24-hour days. This
/// will sometimes producing surprising results in edge cases, but it makes the
/// API we expose to users simpler, and is probably a worthwhile trade-off in our case.
///
/// This type ensures that any `Span` operations in Hipcheck always use these semantics,
/// so days are always treated as being 24 hours long, and weeks are made of 7 24-hour
/// long dyas.
#[derive(Debug, Default, Clone, Copy)]
pub struct HcSpan(Span);

impl PartialEq for HcSpan {
	fn eq(&self, other: &Self) -> bool {
		self.cmp(other) == Ordering::Equal
	}
}

impl Eq for HcSpan {}

impl PartialOrd for HcSpan {
	fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
		Some(self.cmp(other))
	}
}

impl Ord for HcSpan {
	fn cmp(&self, other: &Self) -> Ordering {
		self.0
			.compare(SpanCompare::from(other.0).days_are_24_hours())
			.expect("should not error with days set to 24 hours long")
	}
}

impl Display for HcSpan {
	fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
		write!(f, "{}", self.0)
	}
}

impl FromStr for HcSpan {
	type Err = jiff::Error;

	fn from_str(s: &str) -> Result<Self, Self::Err> {
		s.parse().map(HcSpan)
	}
}

impl From<Span> for HcSpan {
	fn from(value: Span) -> Self {
		HcSpan(value)
	}
}

impl From<HcSpan> for ZonedArithmetic {
	fn from(value: HcSpan) -> Self {
		ZonedArithmetic::from(value.0)
	}
}

impl<'a> From<HcSpan> for SpanArithmetic<'a> {
	fn from(value: HcSpan) -> Self {
		SpanArithmetic::from(value.0)
	}
}

impl HcSpan {
	pub fn checked_add<'a, A: Into<SpanArithmetic<'a>>>(
		&self,
		options: A,
	) -> Result<HcSpan, jiff::Error> {
		let options: SpanArithmetic<'a> = options.into();
		self.0.checked_add(options.days_are_24_hours()).map(HcSpan)
	}

	pub fn checked_sub<'a, A: Into<SpanArithmetic<'a>>>(
		&self,
		options: A,
	) -> Result<HcSpan, jiff::Error> {
		let options: SpanArithmetic<'a> = options.into();
		self.0.checked_sub(options.days_are_24_hours()).map(HcSpan)
	}

	pub fn round<'a, R: Into<SpanRound<'a>>>(self, options: R) -> Result<Span, jiff::Error> {
		let options: SpanRound<'a> = options.into();
		self.0.round(options.days_are_24_hours())
	}

	pub fn get_years(&self) -> i16 {
		self.0.get_years()
	}

	pub fn get_months(&self) -> i32 {
		self.0.get_months()
	}

	pub fn get_weeks(&self) -> i32 {
		self.0.get_weeks()
	}

	pub fn get_days(&self) -> i32 {
		self.0.get_days()
	}

	pub fn try_weeks<I: Into<i64>>(self, weeks: I) -> Result<HcSpan, jiff::Error> {
		self.0.try_weeks(weeks).map(HcSpan)
	}

	pub fn try_days<I: Into<i64>>(self, days: I) -> Result<HcSpan, jiff::Error> {
		self.0.try_days(days).map(HcSpan)
	}
}
