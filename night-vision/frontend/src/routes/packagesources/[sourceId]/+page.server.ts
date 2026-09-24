import { error, fail, redirect } from '@sveltejs/kit';
import type { PackageSourceExposure, VersionedPackage } from '$lib/api/generated';
import { startAssessment } from '$lib/server/assessments';
import { nightVisionApi } from '$lib/server/night-vision-api';
import { formatPurl } from '$lib/format';
import { pageErrorStatus } from '$lib/server/page-errors';
import type { Actions, PageServerLoad } from './$types';

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
	name: exposure.package.name,
	version: exposure.package.version,
	package: formatPurl(exposure.package.purl),
	cveId: exposure.cveId,
	vulnerabilityName: exposure.kev.vulnerabilityName ?? null,
	kevDateAdded: formatKevDate(exposure.kev.dateAdded),
	reachability: reachability(exposure.package)
});

export const load: PageServerLoad = async ({ params }) => {
	const [detail, list] = await Promise.all([
		nightVisionApi.getPackageSourceStatus(params.sourceId),
		// The detail response carries the source's name only once resolution completes.
		nightVisionApi.listPackageSources({ limit: 100 })
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

	const listed = list.ok ? list.data.items.find((item) => item.id === source.id) : undefined;
	const title = (completed ? source.source.displayName : listed?.displayName) ?? 'Package source';
	const fileName = completed ? source.source.fileName : null;

	// Only counts and small fields go to the browser: versioned packages can carry
	// tens of thousands of derivation paths.
	return {
		source: {
			id: source.id,
			fileName,
			title,
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

export const actions: Actions = {
	// Runs an upgrade assessment for one KEV exposure, letting Night Vision discover the candidates.
	assess: async ({ params, request, cookies, url }) => {
		const form = await request.formData();
		const name = form.get('name');
		const version = form.get('version');
		const cve = form.get('cve');
		if (typeof name !== 'string' || typeof version !== 'string' || !name || !version) {
			return fail(400, { assessError: 'Choose an exposure to assess.' });
		}

		// Use the stored manifest rather than anything the browser sends.
		const detail = await nightVisionApi.getPackageSourceStatus(params.sourceId);
		if (!detail.ok) return fail(502, { assessError: detail.error.message });
		const source = detail.data;
		if (source.status !== 'completed' && source.status !== 'completed-with-warnings') {
			return fail(409, { assessError: 'Assessments need a package source that finished resolving.' });
		}

		const result = await startAssessment(
			{
				packageSource: source.source,
				vulnerablePackage: { ecosystem: 'npm', name, version },
				cveLinkage: typeof cve === 'string' && cve ? [cve] : null
			},
			params.sourceId,
			cookies,
			url
		);
		if (!result.ok) {
			return fail(result.error.status === 400 ? 400 : 502, {
				assessError:
					result.error.status === 400
						? 'Night Vision could not start an assessment for this exposure.'
						: result.error.message
			});
		}
		redirect(303, `/assessments/${result.data.id}`);
	}
};
