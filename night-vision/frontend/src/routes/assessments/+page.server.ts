import { manifestName } from '$lib/format';
import { recentAssessments } from '$lib/server/assessments';
import { nightVisionApi } from '$lib/server/night-vision-api';
import type { PageServerLoad } from './$types';

export const load: PageServerLoad = async ({ cookies }) => {
	// Only link sources that still exist; a removed source is shown by name alone.
	const sources = await nightVisionApi.listPackageSources({ limit: 500 });
	const liveSourceIds = new Set(sources.ok ? sources.data.items.map((source) => source.id) : []);

	const rows = await Promise.all(
		recentAssessments(cookies).map(async (entry) => {
			const status = await nightVisionApi.getUpgradeAssessmentStatus(entry.id);
			// Skip assessments the API no longer knows about, for example after a database reset.
			if (!status.ok) return null;
			const result =
				status.data.status === 'completed'
					? await nightVisionApi.getUpgradeAssessmentResult(entry.id)
					: null;
			const fileName =
				entry.fileName ?? (result?.ok ? result.data.input.packageSource.fileName : 'package.json');
			return {
				id: entry.id,
				sourceId: entry.sourceId && liveSourceIds.has(entry.sourceId) ? entry.sourceId : null,
				sourceName:
					entry.sourceName ??
					(result?.ok ? manifestName(result.data.input.packageSource.contents) : null) ??
					fileName,
				fileName,
				pkg: entry.pkg,
				cve: entry.cve,
				candidate: entry.candidate,
				status: status.data.status,
				verdict: result?.ok ? result.data.verdict : null,
				summary: result?.ok ? result.data.summary : (status.data.error?.message ?? null),
				updatedAt: status.data.updatedAt
			};
		})
	);
	return { assessments: rows.filter((row) => row !== null) };
};
