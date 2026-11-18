import type { Cookies } from '@sveltejs/kit';
import type { UpgradeAssessmentInput } from '$lib/api/generated';
import { manifestName } from '$lib/format';
import { nightVisionApi } from './night-vision-api';

// The API has no endpoint for listing upgrade assessments yet, so the Assessments
// page lists the ones started from this browser, remembered in a cookie. Replace
// this with the API once it can list assessments.
const COOKIE = 'nv_recent_assessments';
const MAX_REMEMBERED = 15;

export type RecentAssessment = {
	id: string;
	sourceId: string | null;
	/** The manifest's name, so rows can name their source even while running. */
	sourceName?: string | null;
	/** The source's file name, such as package.json. */
	fileName?: string;
	/** "name@version" of the vulnerable package, for labelling rows before a result exists. */
	pkg: string;
	cve: string | null;
	candidate: string | null;
};

const isRecentAssessment = (value: unknown): value is RecentAssessment =>
	typeof value === 'object' &&
	value !== null &&
	typeof (value as { id?: unknown }).id === 'string' &&
	typeof (value as { pkg?: unknown }).pkg === 'string';

/** Assessments started from this browser, newest first. */
export const recentAssessments = (cookies: Cookies): RecentAssessment[] => {
	const raw = cookies.get(COOKIE);
	if (!raw) return [];
	try {
		const parsed: unknown = JSON.parse(atob(raw));
		return Array.isArray(parsed) ? parsed.filter(isRecentAssessment) : [];
	} catch {
		return [];
	}
};

/** Starts an upgrade assessment and remembers it for the Assessments page. */
export const startAssessment = async (
	input: UpgradeAssessmentInput,
	sourceId: string | null,
	cookies: Cookies,
	url: URL
) => {
	const result = await nightVisionApi.submitUpgradeAssessment(input);
	if (result.ok) {
		const entry: RecentAssessment = {
			id: result.data.id,
			sourceId,
			sourceName: manifestName(input.packageSource.contents),
			fileName: input.packageSource.fileName,
			pkg: input.vulnerablePackage
				? `${input.vulnerablePackage.name}@${input.vulnerablePackage.version}`
				: input.packageSource.fileName,
			cve: input.cveLinkage?.[0] ?? null,
			candidate: input.candidateVersion ?? null
		};
		const remembered = [entry, ...recentAssessments(cookies).filter((item) => item.id !== entry.id)];
		// Everything stored is ASCII (ids, npm names and versions, CVE ids), so btoa is safe.
		cookies.set(COOKIE, btoa(JSON.stringify(remembered.slice(0, MAX_REMEMBERED))), {
			path: '/',
			httpOnly: true,
			sameSite: 'lax',
			secure: url.protocol === 'https:',
			maxAge: 60 * 60 * 24 * 30
		});
	}
	return result;
};
