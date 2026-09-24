import { error } from '@sveltejs/kit';
import { nightVisionApi } from '$lib/server/night-vision-api';
import type { PackageSourceRow } from './columns';
import { pageErrorStatus } from './page-errors';
import type { PageServerLoad } from './$types';

export const load: PageServerLoad = async () => {
	const result = await nightVisionApi.listPackageSources();
	if (!result.ok) error(pageErrorStatus(result.error), result.error.message);

	// The list endpoint has no exposure counts yet, so look them up for each
	// completed source. Fine at demo scale; a count on the list endpoint would
	// avoid one request per row.
	const packageSources: PackageSourceRow[] = await Promise.all(
		result.data.items.map(async (source) => {
			if (source.status !== 'completed' && source.status !== 'completed-with-warnings') {
				return { ...source, kevExposures: null };
			}
			const exposures = await nightVisionApi.getPackageSourceExposures(source.id);
			return { ...source, kevExposures: exposures.ok ? exposures.data.exposures.length : null };
		})
	);

	return { packageSources };
};
