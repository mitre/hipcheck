import { error } from '@sveltejs/kit';
import type { PackageSourceExposure, VersionedPackage } from '$lib/api/generated';
import { nightVisionApi } from '$lib/server/night-vision-api';
import { formatPurl } from '../format';
import { pageErrorStatus } from '../page-errors';
import type { PageServerLoad } from './$types';

// Every derivation starts at "<root>", so a two-step path is a direct dependency.
const reachability = ({ derivations }: VersionedPackage): string => {
	if (derivations.length === 0) return '—';
	const shortest = derivations.reduce((a, b) => (b.length < a.length ? b : a));
	return shortest.length <= 2 ? 'Direct dependency' : `Through ${formatPurl(shortest[1])}`;
};

// KEV dates are calendar dates ("2022-01-18"); format them in UTC so they don't shift a day.
const formatKevDate = (value: string | null | undefined): string =>
	value
		? new Date(value).toLocaleDateString('en-US', {
				month: 'short',
				day: 'numeric',
				year: 'numeric',
				timeZone: 'UTC'
			})
		: '—';

const toExposureRow = (exposure: PackageSourceExposure) => ({
	package: formatPurl(exposure.package.purl),
	cveId: exposure.cveId,
	vulnerabilityName: exposure.kev.vulnerabilityName ?? null,
	kevDateAdded: formatKevDate(exposure.kev.dateAdded),
	reachability: reachability(exposure.package)
});

export const load: PageServerLoad = async ({ params }) => {
	const [detail, list] = await Promise.all([
		nightVisionApi.getPackageSourceStatus(params.sourceId),
		// The detail response carries the file name only once resolution completes.
		nightVisionApi.listPackageSources({ limit: 500 })
	]);
	if (!detail.ok) error(pageErrorStatus(detail.error), detail.error.message);
	const source = detail.data;
	const completed = source.status === 'completed' || source.status === 'completed-with-warnings';

	let exposures: ReturnType<typeof toExposureRow>[] | null = null;
	let exposuresError: string | null = null;
	if (completed) {
		const result = await nightVisionApi.getPackageSourceExposures(source.id);
		if (result.ok) exposures = result.data.exposures.map(toExposureRow);
		else exposuresError = result.error.message;
	}

	// Only counts and small fields go to the browser: versioned packages can carry
	// tens of thousands of derivation paths.
	return {
		source: {
			id: source.id,
			fileName:
				(list.ok ? list.data.items.find((item) => item.id === source.id)?.fileName : undefined) ??
				(completed ? source.source.fileName : 'Package source'),
			status: source.status,
			createdAt: source.createdAt,
			finishedAt: completed
				? source.completedAt
				: source.status === 'failed'
					? source.finishedAt
					: source.status === 'cancelled'
						? source.cancelledAt
						: null,
			attempt: source.attempt,
			reachablePackages: completed ? source.versionedPackages.length : null,
			warnings: source.status === 'completed-with-warnings' ? source.warnings : [],
			failure:
				source.status === 'failed'
					? { kind: source.kind, diagnostic: source.diagnostic, retryable: source.retryable }
					: null
		},
		exposures,
		exposuresError
	};
};
