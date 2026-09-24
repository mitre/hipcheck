import { recentAssessments } from '$lib/server/assessments';
import { nightVisionApi } from '$lib/server/night-vision-api';
import type { PageServerLoad } from './$types';

export const load: PageServerLoad = async ({ cookies }) => {
	const rows = await Promise.all(
		recentAssessments(cookies).map(async (entry) => {
			const status = await nightVisionApi.getUpgradeAssessmentStatus(entry.id);
			// Skip assessments the API no longer knows about, for example after a database reset.
			if (!status.ok) return null;
			const result =
				status.data.status === 'completed'
					? await nightVisionApi.getUpgradeAssessmentResult(entry.id)
					: null;
			return {
				id: entry.id,
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
