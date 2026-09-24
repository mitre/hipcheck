import { error } from '@sveltejs/kit';
import type { PackageSourceSummary } from '$lib/api/generated';
import { nightVisionApi } from '$lib/server/night-vision-api';
import { pageErrorStatus } from '$lib/server/page-errors';
import type { PageServerLoad } from './$types';

// The table pages through rows in the browser, so read the API's pages up front,
// most recent activity first. The API returns at most 100 rows per page.
const MAX_ROWS = 500;

export const load: PageServerLoad = async () => {
	const packageSources: PackageSourceSummary[] = [];
	let cursor: string | null | undefined;
	do {
		const result = await nightVisionApi.listPackageSources({ limit: 100, cursor });
		if (!result.ok) error(pageErrorStatus(result.error), result.error.message);
		packageSources.push(...result.data.items);
		cursor = result.data.nextCursor;
	} while (cursor && packageSources.length < MAX_ROWS);

	return { packageSources };
};
