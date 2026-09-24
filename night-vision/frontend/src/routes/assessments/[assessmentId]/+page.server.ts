import { error, fail, redirect } from '@sveltejs/kit';
import type { UpgradeAssessmentResult } from '$lib/api/generated';
import { manifestName } from '$lib/format';
import { recentAssessments, startAssessment } from '$lib/server/assessments';
import { nightVisionApi } from '$lib/server/night-vision-api';
import { pageErrorStatus } from '$lib/server/page-errors';
import type { Actions, PageServerLoad } from './$types';

// Discovery can return hundreds of candidates; show the nearest ones plus counts.
const CANDIDATES_SHOWN = 12;

export const load: PageServerLoad = async ({ params, cookies }) => {
	const status = await nightVisionApi.getUpgradeAssessmentStatus(params.assessmentId);
	if (!status.ok) error(pageErrorStatus(status.error), status.error.message);
	const remembered = recentAssessments(cookies).find((entry) => entry.id === params.assessmentId);

	let result: UpgradeAssessmentResult | null = null;
	if (status.data.status === 'completed') {
		const fetched = await nightVisionApi.getUpgradeAssessmentResult(params.assessmentId);
		if (!fetched.ok) error(pageErrorStatus(fetched.error), fetched.error.message);
		result = fetched.data;
	}

	const input = result?.input;
	const candidates = result?.candidateVersions ?? [];
	const verdictCounts = new Map<string, number>();
	for (const candidate of candidates) {
		const verdict = candidate.verdict ?? 'unknown';
		verdictCounts.set(verdict, (verdictCounts.get(verdict) ?? 0) + 1);
	}

	return {
		assessment: {
			id: params.assessmentId,
			status: status.data.status,
			createdAt: status.data.createdAt,
			completedAt: status.data.completedAt ?? null,
			error: status.data.error?.message ?? null,
			pkg: input?.vulnerablePackage
				? `${input.vulnerablePackage.name}@${input.vulnerablePackage.version}`
				: (remembered?.pkg ?? 'Upgrade assessment'),
			cves: input?.cveLinkage ?? (remembered?.cve ? [remembered.cve] : []),
			requestedCandidate: input ? (input.candidateVersion ?? null) : (remembered?.candidate ?? null),
			manifestName: input ? manifestName(input.packageSource.contents) : null,
			fileName: input?.packageSource.fileName ?? remembered?.fileName ?? null,
			sourceId: remembered?.sourceId ?? null,
			verdict: result?.verdict ?? null,
			summary: result?.summary ?? null,
			candidates: candidates.slice(0, CANDIDATES_SHOWN).map((candidate) => ({
				version: candidate.version,
				upgradeDistance: candidate.upgradeDistance ?? null,
				verdict: candidate.verdict ?? null
			})),
			candidateCount: candidates.length,
			verdictCounts: [...verdictCounts.entries()],
			caveats: result?.caveats ?? [],
			findings: (result?.findings ?? []).map(({ id, title, summary, effect }) => ({ id, title, summary, effect })),
			evidence: (result?.evidence ?? []).map(({ id, title, summary, url }) => ({ id, title, summary, url }))
		}
	};
};

export const actions: Actions = {
	// Checks one discovered candidate on its own, which adds supply-chain analysis.
	check: async ({ params, request, cookies, url }) => {
		const candidate = (await request.formData()).get('candidate');
		if (typeof candidate !== 'string' || !candidate) {
			return fail(400, { checkError: 'Choose a candidate to check.' });
		}
		const current = await nightVisionApi.getUpgradeAssessmentResult(params.assessmentId);
		if (!current.ok) return fail(502, { checkError: current.error.message });

		const remembered = recentAssessments(cookies).find((entry) => entry.id === params.assessmentId);
		const result = await startAssessment(
			{ ...current.data.input, candidateVersion: candidate },
			remembered?.sourceId ?? null,
			cookies,
			url
		);
		if (!result.ok) {
			return fail(result.error.status === 400 ? 400 : 502, {
				checkError:
					result.error.status === 400
						? `Night Vision would not check ${candidate}; it may still be affected by a known exploited vulnerability.`
						: result.error.message
			});
		}
		redirect(303, `/assessments/${result.data.id}`);
	}
};
