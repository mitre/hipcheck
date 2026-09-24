import { error } from '@sveltejs/kit';
import { nightVisionApi } from '$lib/server/night-vision-api';
import type { PageServerLoad } from './$types';

export const load: PageServerLoad = async () => {
	const result = await nightVisionApi.listPackageSources();
	if (!result.ok) error(result.error.status ?? 503, result.error.message);

	return {
		packageSources: result.data.items
	};
};
